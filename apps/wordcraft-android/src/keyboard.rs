//! The on-screen keyboard, over GameActivity's text input state (see [`crate::ime`] for why).
//!
//! A small thread reads `AndroidApp::text_input_state` while the editor wants text and the app has
//! a window (about thirty times a second), and when the buffer changes it stores it and wakes the
//! UI, which hands it to [`Bridge`] on its next frame and adds the resulting input to that frame's
//! events. Only that thread reads the state (the glue says it isn't safe to read from several), and
//! the UI thread writes it only to fill it again.
//!
//! The keyboard's buffer is kept equal to the text before the caret. When the caret may have moved
//! (a press or a release on the screen, a cursor key) what the keyboard typed and the word being
//! composed are typed where they were (before the frame moves the caret), the buffer is emptied, and
//! two frames later (the frame that moved the caret has been drawn by then) it is filled with the
//! document's text before the new caret. A buffer that the system hasn't been given time to apply
//! yet can still be read as it was, so for a moment after emptying or filling it what is read is not
//! believed unless it is what was written. While a text field (a dialog, Find, the file name) has the
//! keyboard the buffer holds neutral filler instead, and what the keyboard types goes to the field.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use android_activity::AndroidApp;
use android_activity::input::{ImeOptions, InputType, TextInputAction, TextInputState, TextSpan};
use wordcraft_ui_egui::WordApp;

use crate::ime::{Bridge, Output, Snapshot, typing_pieces};

/// How often the keyboard's buffer is read while the editor wants text.
const POLL: Duration = Duration::from_millis(30);
/// The gap between the two reads that must agree.
const CONFIRM: Duration = Duration::from_millis(8);
/// How often the thread looks again when the editor doesn't want text.
const IDLE: Duration = Duration::from_millis(250);
/// After the buffer is filled, how long what is read is doubted (the system applies a new state a
/// little later, on its own thread).
const SETTLE: Duration = Duration::from_millis(300);
/// The buffer is looked after (refilled when it has grown long or run short) after this long without
/// the keyboard changing it.
const MAINTAIN_IDLE: Duration = Duration::from_millis(700);
/// A buffer shorter than this is refilled when the document has more text before the caret (the
/// keyboard deleted its way through the context).
const MIN_BUFFER_CHARS: usize = 8;
/// What the keyboard's buffer holds while a text field has the keyboard (the field's own text isn't
/// known here): enough for the keyboard to have something to delete.
pub const FIELD_CONTEXT: &str = "                                ";
/// At most this many Backspace presses from one change.
const MAX_BACKSPACES: usize = 512;

/// What the polling thread and the UI share.
#[derive(Default)]
struct Shared {
    /// The newest buffer the thread saw change, until the UI takes it.
    latest: Mutex<Option<Snapshot>>,
    /// The editor wants typed text (the page or a text field has focus).
    wanted: AtomicBool,
    /// The app is closing.
    stop: AtomicBool,
    /// Counts every time the buffer was written (emptied or filled): a read that began before is
    /// from the old buffer and is dropped.
    generation: AtomicU64,
    /// The thread publishes the buffer it reads next even if it hasn't changed.
    republish: AtomicBool,
}

/// A buffer was just written: what it must read as before it is believed (or until `until`).
struct Settle {
    expect: Snapshot,
    until: Instant,
    /// The texts the buffer held before the write: a read that equals one of them is the old buffer
    /// (not yet replaced); a read that starts with `expect` and is none of them is new typing.
    stale: Vec<String>,
}

/// The keyboard's input, as events for the UI.
pub struct Keyboard {
    android: AndroidApp,
    ctx: egui::Context,
    shared: Arc<Shared>,
    bridge: Bridge,
    /// Frames until the buffer is filled with the text before the caret (it is empty until then).
    seed_in: u8,
    settle: Option<Settle>,
    /// The text of the newest snapshot taken.
    last_text: String,
    last_change: Instant,
}

impl Keyboard {
    /// Tell the system what is being typed (several lines of ordinary text) and start reading the
    /// keyboard; `ctx` is woken when it types something.
    pub fn new(android: AndroidApp, ctx: &egui::Context) -> Keyboard {
        // Enter types a line break (no Done or Go button); the keyboard may correct and capitalize.
        android.set_ime_editor_info(
            InputType::TYPE_CLASS_TEXT
                | InputType::TYPE_TEXT_FLAG_MULTI_LINE
                | InputType::TYPE_TEXT_FLAG_AUTO_CORRECT
                | InputType::TYPE_TEXT_FLAG_CAP_SENTENCES,
            TextInputAction::None,
            ImeOptions::IME_FLAG_NO_ENTER_ACTION | ImeOptions::IME_FLAG_NO_FULLSCREEN,
        );
        let shared = Arc::new(Shared::default());
        let thread = {
            let (android, shared, ctx) = (android.clone(), Arc::clone(&shared), ctx.clone());
            std::thread::Builder::new().name("keyboard".into()).spawn(move || poll(&android, &shared, &ctx))
        };
        if let Err(e) = thread {
            log::error!("the keyboard can't be read: {e}");
        }
        Keyboard {
            android,
            ctx: ctx.clone(),
            shared,
            bridge: Bridge::default(),
            seed_in: 0,
            settle: None,
            last_text: String::new(),
            last_change: Instant::now(),
        }
    }

    /// Whether the editor wants typed text now; the keyboard is read only then.
    pub fn want(&self, wanted: bool) {
        self.shared.wanted.store(wanted, Ordering::Relaxed);
    }

    /// The input to add to the front of this frame's events: what the keyboard typed since the last
    /// frame. `incoming` is the frame's other events: a press, a release or a cursor key may move
    /// the caret, so then what was typed and the word being composed are typed into `app` right
    /// now, where they were typed (before the frame moves the caret), and the keyboard's buffer is
    /// emptied to be filled again ([`Keyboard::tick`]). `page_owns` says whether the page (not a text
    /// field) has the keyboard: if not, the input goes to the field as events instead.
    pub fn events(&mut self, incoming: &[egui::Event], app: &mut WordApp, page_owns: bool) -> Vec<egui::Event> {
        let touched = incoming.iter().any(|e| matches!(e, egui::Event::Touch { phase: egui::TouchPhase::Start, .. }));
        let mut restart = incoming.iter().any(moves_the_caret) || (touched && self.bridge.composing());
        let mut outputs = Vec::new();
        if let Some(snapshot) = self.take_snapshot() {
            if snapshot.at_end && !self.bridge.out_of_step(&snapshot) {
                outputs.extend(self.bridge.observe(&snapshot));
                self.last_change = Instant::now();
            } else {
                // The keyboard's cursor is inside its buffer (a swipe on the space bar), or the
                // buffer isn't an edit of what was given (it couldn't be read, or is an old one):
                // nothing of it is typed, and the buffer starts again at the editor's caret.
                restart = true;
            }
        }
        if restart {
            outputs.extend(self.bridge.reset());
            self.restart();
            if page_owns {
                type_now(app, outputs);
                return Vec::new();
            }
        }
        outputs.into_iter().flat_map(events_for).collect()
    }

    /// Once per frame, after the frame's input was handled: fills the emptied buffer when it is
    /// due, and looks after one that has run short, or that no longer matches the document after a
    /// quiet moment (an undo, a paste, the editor's own AutoCorrect). `context` is the text the
    /// buffer should hold, asked for only when it is needed.
    pub fn tick(&mut self, context: impl FnOnce() -> String) {
        if let Some(settle) = &self.settle {
            // A frame is wanted when the doubt ends; asked for in the pass, which egui keeps.
            self.ctx.request_repaint_after(settle.until.saturating_duration_since(Instant::now()));
        }
        if self.seed_in > 0 {
            self.seed_in -= 1;
            if self.seed_in == 0 {
                self.seed(&context());
            }
            return;
        }
        if self.settle.is_some() || self.bridge.composing() {
            return;
        }
        let have = self.bridge.given_len();
        let quiet = self.last_change.elapsed() >= MAINTAIN_IDLE;
        if !quiet && have >= MIN_BUFFER_CHARS {
            return;
        }
        let text = context();
        let wanted = text.chars().count();
        // After a quiet moment a buffer whose length is off is put right (an Undo, a Paste, a
        // correction that changed the length: the next Backspace would delete the wrong amount);
        // one that differs only in letters (a capital, a curly quote) is left, since rewriting it
        // would risk a key typed just then. A run short buffer is refilled at once when the
        // document has more before the caret (the keyboard deleted its way through).
        if (quiet && wanted != have) || (have < MIN_BUFFER_CHARS && wanted > have + 4) {
            self.seed(&text);
        }
        if quiet {
            // Look again after another quiet moment, not on every frame.
            self.last_change = Instant::now();
        }
    }

    /// The caret may have moved (or the keyboard's buffer is out of step with it): empty the buffer
    /// until [`Keyboard::tick`] fills it again.
    fn restart(&mut self) {
        // A press and its release (a tap) are two frames: the buffer is emptied once, and filled
        // after the second.
        if self.seed_in == 0 {
            self.write_state(TextInputState::default(), Snapshot { text: String::new(), compose: None, at_end: true });
        }
        self.seed_in = 2;
        self.ctx.request_repaint();
    }

    /// Fill the keyboard's buffer with `text`, the text before the caret (or the filler for a text
    /// field), with its cursor at the end and nothing composing.
    fn seed(&mut self, text: &str) {
        self.bridge.seed(text);
        let units = text.encode_utf16().count();
        let state = TextInputState { text: text.to_owned(), selection: TextSpan { start: units, end: units }, compose_region: None };
        self.write_state(state, Snapshot { text: text.to_owned(), compose: None, at_end: true });
    }

    /// Write the keyboard's buffer. What the polling thread read before is from the old buffer, and
    /// until the system has applied this one a read can still be the old one: so the next reads are
    /// not believed unless they are what was written (or `SETTLE` has passed).
    fn write_state(&mut self, state: TextInputState, expect: Snapshot) {
        self.shared.generation.fetch_add(1, Ordering::SeqCst);
        self.android.set_text_input_state(state);
        let pending = self.shared.latest.lock().unwrap_or_else(PoisonError::into_inner).take();
        let mut stale = vec![self.last_text.clone()];
        stale.extend(pending.map(|s| s.text));
        self.settle = Some(Settle { expect, until: Instant::now() + SETTLE, stale });
    }

    /// The newest buffer the thread stored, unless it is doubted (just after the buffer was
    /// written): then it must be what was written, or `SETTLE` has to have passed, after which the
    /// thread is asked to publish the buffer as it is (what was read in between may be stale).
    fn take_snapshot(&mut self) -> Option<Snapshot> {
        let latest = self.shared.latest.lock().unwrap_or_else(PoisonError::into_inner).take();
        let Some(settle) = self.settle.as_ref() else {
            if let Some(s) = &latest {
                self.last_text.clone_from(&s.text);
            }
            return latest;
        };
        let waited = Instant::now() >= settle.until;
        match latest {
            // What was written, or typing on top of it (a read that is none of the old buffer's texts).
            Some(s) if waited || (s.at_end && s.text.starts_with(&settle.expect.text) && !settle.stale.contains(&s.text)) => {
                self.settle = None;
                self.last_text.clone_from(&s.text);
                Some(s)
            }
            Some(_) => None,
            None => {
                if waited {
                    self.settle = None;
                    self.shared.republish.store(true, Ordering::Relaxed);
                }
                None
            }
        }
    }
}

impl Drop for Keyboard {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
    }
}

/// The thread: read the keyboard's buffer and store it when it changes.
fn poll(android: &AndroidApp, shared: &Shared, ctx: &egui::Context) {
    let mut last = Snapshot::default();
    while !shared.stop.load(Ordering::Relaxed) {
        // Frames, which update `wanted`, stop when the app is paused; the window going away
        // (the app moved to the background) is what stops the reading then.
        if !shared.wanted.load(Ordering::Relaxed) || android.native_window().is_none() {
            std::thread::sleep(IDLE);
            continue;
        }
        let generation = shared.generation.load(Ordering::SeqCst);
        let snapshot = read(android);
        // GameTextInput's buffer is shared with the thread that types into it, with no lock around
        // the read: only a state seen twice in a row is taken, so a half-written one never is.
        std::thread::sleep(CONFIRM);
        if read(android) != snapshot {
            continue;
        }
        if snapshot != last || shared.republish.swap(false, Ordering::Relaxed) {
            let mut latest = shared.latest.lock().unwrap_or_else(PoisonError::into_inner);
            // The buffer was written while this was being read: it is from the old buffer.
            if shared.generation.load(Ordering::SeqCst) == generation {
                last = snapshot.clone();
                *latest = Some(snapshot);
                drop(latest);
                ctx.request_repaint();
            }
        }
        std::thread::sleep(POLL);
    }
}

/// The keyboard's buffer now.
fn read(android: &AndroidApp) -> Snapshot {
    let state = android.text_input_state();
    let compose = state.compose_region.map(|span| (span.start, span.end));
    Snapshot::from_utf16_spans(state.text, compose, Some((state.selection.start, state.selection.end)))
}

/// An event that may put the caret somewhere else than where the typed text was going, or change
/// the text before it: a press, and the release, where a button's command runs (Undo, Paste, Find
/// Next). A finger that went down has not moved the caret yet (the page gets its press when the
/// touch is a tap), and a touch that begins while a word is composed is handled by the caller.
fn moves_the_caret(event: &egui::Event) -> bool {
    use egui::Key::{ArrowDown, ArrowLeft, ArrowRight, ArrowUp, End, Home, PageDown, PageUp};
    match event {
        egui::Event::PointerButton { .. } => true,
        egui::Event::Key { key, pressed: true, .. } => matches!(key, ArrowLeft | ArrowRight | ArrowUp | ArrowDown | Home | End | PageUp | PageDown),
        _ => false,
    }
}

/// Type what the keyboard did into the page now (before the frame): the same as its events would do
/// in the frame, for the one case where the frame is about to move the caret.
fn type_now(app: &mut WordApp, outputs: Vec<Output>) {
    for output in outputs {
        match output {
            Output::Commit(text) => {
                for piece in typing_pieces(&text) {
                    app.commit_text(&piece);
                }
            }
            Output::Backspace(n) => {
                // (In an equation the editing keys are the equation's own, as for the keys.)
                let id = if app.session.math.is_some() { "equation.backspace" } else { "text.backspace" };
                for _ in 0..n.min(MAX_BACKSPACES) {
                    let _ = app.run(id, serde_json::json!({}));
                }
            }
            Output::Enter => {
                let id = if app.session.math.is_some() { "equation.enter" } else { "text.newParagraph" };
                let _ = app.run(id, serde_json::json!({}));
            }
            // The word being composed is committed with the rest; the editor shows no preedit now.
            Output::Preedit(_) => {}
        }
    }
}

/// The egui events for one piece of keyboard input. Typed text goes in as text events one typing
/// piece at a time, the way keys type it, so AutoCorrect, smart quotes and list or dash AutoFormat
/// see each space, punctuation mark and quote.
fn events_for(output: Output) -> Vec<egui::Event> {
    let key = |key, pressed| egui::Event::Key { key, physical_key: Some(key), pressed, repeat: false, modifiers: egui::Modifiers::NONE };
    match output {
        Output::Backspace(n) => {
            (0..n.min(MAX_BACKSPACES)).flat_map(|_| [key(egui::Key::Backspace, true), key(egui::Key::Backspace, false)]).collect()
        }
        Output::Enter => vec![key(egui::Key::Enter, true), key(egui::Key::Enter, false)],
        Output::Commit(text) => typing_pieces(&text).into_iter().map(egui::Event::Text).collect(),
        Output::Preedit(text) => vec![egui::Event::Ime(egui::ImeEvent::Preedit { text, active_range_chars: None })],
    }
}
