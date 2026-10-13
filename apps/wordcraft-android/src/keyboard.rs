//! The on-screen keyboard, over GameActivity's text input state (see [`crate::ime`] for why).
//!
//! A small thread reads `AndroidApp::text_input_state` while the editor wants text (about thirty
//! times a second), and when the buffer changes it stores it and wakes the UI, which hands it to
//! [`Bridge`] on its next frame and adds the resulting input to that frame's events. Only that
//! thread reads the state (the glue says it isn't safe to read from several), and the UI thread
//! writes it only to empty the buffer.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use android_activity::AndroidApp;
use android_activity::input::{ImeOptions, InputType, TextInputAction, TextInputState};

use crate::ime::{Bridge, Output, Snapshot};

/// How often the keyboard's buffer is read while the editor wants text.
const POLL: Duration = Duration::from_millis(30);
/// The gap between the two reads that must agree.
const CONFIRM: Duration = Duration::from_millis(8);
/// How often the thread looks again when the editor doesn't want text.
const IDLE: Duration = Duration::from_millis(250);

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

/// The keyboard's input, as events for the UI.
pub struct Keyboard {
    android: AndroidApp,
    shared: Arc<Shared>,
    bridge: Bridge,
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
        Keyboard { android, shared, bridge: Bridge::default() }
    }

    /// Whether the editor wants typed text now; the keyboard is read only then.
    pub fn want(&self, wanted: bool) {
        self.shared.wanted.store(wanted, Ordering::Relaxed);
    }

    /// The input to add to this frame: what the keyboard typed since the last frame, after
    /// `incoming` (the frame's other events). A touch, a click or a cursor key first commits the
    /// word being composed and empties the keyboard's buffer, so it starts over at the new caret.
    pub fn events(&mut self, incoming: &[egui::Event]) -> Vec<egui::Event> {
        let mut outputs = Vec::new();
        if incoming.iter().any(moves_the_caret) {
            outputs.extend(self.bridge.reset());
            self.android.set_text_input_state(TextInputState::default());
            // What the thread saw before the buffer was emptied is stale.
            *self.shared.latest.lock().unwrap_or_else(PoisonError::into_inner) = None;
        } else if let Some(snapshot) = self.shared.latest.lock().unwrap_or_else(PoisonError::into_inner).take() {
            outputs.extend(self.bridge.observe(&snapshot));
            if self.bridge.should_reset() {
                outputs.extend(self.bridge.reset());
                self.android.set_text_input_state(TextInputState::default());
            }
        }
        outputs.into_iter().flat_map(events_for).collect()
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
        if !shared.wanted.load(Ordering::Relaxed) {
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
    Snapshot::from_utf16_spans(state.text, compose)
}

/// An event that puts the caret somewhere else than where the typed text was going.
fn moves_the_caret(event: &egui::Event) -> bool {
    use egui::Key::{ArrowDown, ArrowLeft, ArrowRight, ArrowUp, End, Home, PageDown, PageUp};
    match event {
        egui::Event::Touch { phase: egui::TouchPhase::Start, .. } | egui::Event::PointerButton { pressed: true, .. } => true,
        egui::Event::Key { key, pressed: true, .. } => matches!(key, ArrowLeft | ArrowRight | ArrowUp | ArrowDown | Home | End | PageUp | PageDown),
        _ => false,
    }
}

/// The egui events for one piece of keyboard input.
fn events_for(output: Output) -> Vec<egui::Event> {
    let key = |key, pressed| egui::Event::Key { key, physical_key: Some(key), pressed, repeat: false, modifiers: egui::Modifiers::NONE };
    match output {
        Output::Backspace(n) => {
            (0..n.min(MAX_BACKSPACES)).flat_map(|_| [key(egui::Key::Backspace, true), key(egui::Key::Backspace, false)]).collect()
        }
        Output::Enter => vec![key(egui::Key::Enter, true), key(egui::Key::Enter, false)],
        Output::Commit(text) => vec![egui::Event::Ime(egui::ImeEvent::Commit(text))],
        Output::Preedit(text) => vec![egui::Event::Ime(egui::ImeEvent::Preedit { text, active_range_chars: None })],
    }
}

/// At most this many Backspace presses from one change (the buffer holds a few hundred characters).
const MAX_BACKSPACES: usize = 512;
