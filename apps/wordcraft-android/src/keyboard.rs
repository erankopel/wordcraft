//! The on-screen keyboard, over GameActivity's text input state (see [`crate::ime`] for why).
//!
//! A small thread reads `AndroidApp::text_input_state` while the editor wants text and the app has
//! a window (about thirty times a second), and when the buffer changes it stores it and wakes the
//! UI, which hands it to [`Bridge`] on its next frame and adds the resulting input to that frame's
//! events. Only that thread reads the state (the glue says it isn't safe to read from several), and
//! the UI thread writes it only to fill it again.
//!
//! The keyboard's buffer is kept equal to the text before the caret. When the caret moves (a tap, a
//! cursor key) the word being composed is committed where it was typed, the buffer is emptied, and
//! two frames later (the frame that moved the caret has been drawn by then) it is filled with the
//! document's text before the new caret. A buffer that the system hasn't been given time to apply
//! yet can still be read as it was, so for a moment after filling it what is read is not believed
//! unless it is what was written.

use std::sync::atomic::{AtomicBool, Ordering};
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
/// A buffer longer than this is refilled with the real context.
const MAX_BUFFER_CHARS: usize = 400;
/// A buffer shorter than this is refilled when the document has more text before the caret (the
/// keyboard deleted its way through the context).
const MIN_BUFFER_CHARS: usize = 8;
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
}

/// A buffer was just filled: what it must read as before it is believed.
struct Settle {
    expect: Snapshot,
    until: Instant,
    /// What was read meanwhile, in case nothing newer comes before `until`.
    held: Option<Snapshot>,
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
        Keyboard { android, ctx: ctx.clone(), shared, bridge: Bridge::default(), seed_in: 0, settle: None, last_change: Instant::now() }
    }

    /// Whether the editor wants typed text now; the keyboard is read only then.
    pub fn want(&self, wanted: bool) {
        self.shared.wanted.store(wanted, Ordering::Relaxed);
    }

    /// The input to add to the front of this frame's events: what the keyboard typed since the last
    /// frame. `incoming` is the frame's other events: a touch, a click or a cursor key moves the
    /// caret, so first the word being composed is typed into `app` where it is (before the caret
    /// moves), and the keyboard's buffer is emptied to be filled again ([`Keyboard::tick`]).
    pub fn events(&mut self, incoming: &[egui::Event], app: &mut WordApp) -> Vec<egui::Event> {
        let mut outputs = Vec::new();
        let touched = incoming.iter().any(|e| matches!(e, egui::Event::Touch { phase: egui::TouchPhase::Start, .. }));
        if incoming.iter().any(moves_the_caret) || (touched && self.bridge.composing()) {
            self.restart(app);
        } else if let Some(snapshot) = self.take_snapshot() {
            if snapshot.at_end {
                outputs.extend(self.bridge.observe(&snapshot));
                self.last_change = Instant::now();
            } else {
                // The keyboard's cursor is inside its buffer (a swipe on the space bar): the editor's
                // caret doesn't follow, so start the buffer again at the caret.
                self.restart(app);
            }
        }
        outputs.into_iter().flat_map(events_for).collect()
    }

    /// Once per frame, after the frame's input was handled: fills the emptied buffer when it is
    /// due, and looks after one that has grown long or run short. `context` is the document's text
    /// before the caret, asked for only when it is needed.
    pub fn tick(&mut self, context: impl FnOnce() -> String) {
        if self.seed_in > 0 {
            self.seed_in -= 1;
            if self.seed_in == 0 {
                self.seed(&context());
            }
            return;
        }
        if self.settle.is_some() || self.bridge.composing() || self.last_change.elapsed() < MAINTAIN_IDLE {
            return;
        }
        let have = self.bridge.given_len();
        if !(MIN_BUFFER_CHARS..=MAX_BUFFER_CHARS).contains(&have) {
            let text = context();
            if (have > MAX_BUFFER_CHARS || text.chars().count() > have + 4) && text != self.bridge.given_text() {
                self.seed(&text);
            }
            // Look again after another quiet moment, not on every frame.
            self.last_change = Instant::now();
        }
    }

    /// The caret moved (or the keyboard's buffer is out of step with it): type what was being
    /// composed where it was, and empty the buffer until [`Keyboard::tick`] fills it again.
    fn restart(&mut self, app: &mut WordApp) {
        for output in self.bridge.reset() {
            if let Output::Commit(text) = output {
                app.commit_text(&text);
            }
        }
        self.android.set_text_input_state(TextInputState::default());
        // What the thread saw before the buffer was emptied is stale.
        *self.shared.latest.lock().unwrap_or_else(PoisonError::into_inner) = None;
        self.settle = None;
        self.seed_in = 2;
        self.ctx.request_repaint();
    }

    /// Fill the keyboard's buffer with `text`, the document's text before the caret, with its
    /// cursor at the end and nothing composing.
    fn seed(&mut self, text: &str) {
        self.bridge.seed(text);
        let units = text.encode_utf16().count();
        self.android.set_text_input_state(TextInputState {
            text: text.to_owned(),
            selection: TextSpan { start: units, end: units },
            compose_region: None,
        });
        *self.shared.latest.lock().unwrap_or_else(PoisonError::into_inner) = None;
        self.settle =
            Some(Settle { expect: Snapshot { text: text.to_owned(), compose: None, at_end: true }, until: Instant::now() + SETTLE, held: None });
        // Something may be typed in the meantime: come back to look at it.
        self.ctx.request_repaint_after(SETTLE);
    }

    /// The newest buffer the thread stored, unless it is doubted (just after the buffer was
    /// filled): then it must be what was written, or wait until the system had time to apply it.
    fn take_snapshot(&mut self) -> Option<Snapshot> {
        let latest = self.shared.latest.lock().unwrap_or_else(PoisonError::into_inner).take();
        let Some(settle) = self.settle.as_mut() else { return latest };
        let waited = Instant::now() >= settle.until;
        let seen = latest.or_else(|| if waited { settle.held.take() } else { None });
        match seen {
            Some(s) if waited || s == settle.expect => {
                self.settle = None;
                Some(s)
            }
            Some(s) => {
                settle.held = Some(s);
                None
            }
            None => {
                if waited {
                    self.settle = None;
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
        let snapshot = read(android);
        // GameTextInput's buffer is shared with the thread that types into it, with no lock around
        // the read: only a state seen twice in a row is taken, so a half-written one never is.
        std::thread::sleep(CONFIRM);
        if read(android) != snapshot {
            continue;
        }
        if snapshot != last {
            last = snapshot.clone();
            *shared.latest.lock().unwrap_or_else(PoisonError::into_inner) = Some(snapshot);
            ctx.request_repaint();
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

/// An event that puts the caret somewhere else than where the typed text was going. A finger that
/// went down has not moved it yet (the page gets its press when the touch is a tap), so only the
/// press counts, and a touch that begins while a word is composed is handled by the caller.
fn moves_the_caret(event: &egui::Event) -> bool {
    use egui::Key::{ArrowDown, ArrowLeft, ArrowRight, ArrowUp, End, Home, PageDown, PageUp};
    match event {
        egui::Event::PointerButton { pressed: true, .. } => true,
        egui::Event::Key { key, pressed: true, .. } => matches!(key, ArrowLeft | ArrowRight | ArrowUp | ArrowDown | Home | End | PageUp | PageDown),
        _ => false,
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
