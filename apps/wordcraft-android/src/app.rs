//! The eframe app on Android: [`WordApp`] plus what only a phone needs (keeping preferences and a
//! recovery copy of unsaved work on disk as it changes, since Android may end the app without
//! asking, and without drawing another frame).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use android_activity::AndroidApp;
use android_activity::ndk::configuration::{Keyboard, UiModeNight};
use wordcraft_engine::Session;
use wordcraft_ui_egui::{UiState, WordApp};

use crate::host;
use crate::keyboard::{FIELD_CONTEXT, Keyboard as SoftKeyboard};
use crate::storage::{self, Dirs, Plan, Recovery, Restore};

/// The interface size (percent) before the user has chosen one, on a screen 600 dp wide or more
/// (the unfolded Galaxy Z Fold, a tablet); a phone's narrower screen starts at 100.
const FIRST_RUN_INTERFACE_PERCENT: u32 = 125;

/// Run the editor until the activity ends.
pub fn run(android: AndroidApp) -> Result<(), String> {
    let files = android.internal_data_path().ok_or_else(|| "the system gave the app no private folder".to_owned())?;
    let dirs = Dirs::new(&files);
    // Before anything asks where the settings are (fonts, preferences, the dictionary).
    wordcraft_control_key::set_settings_dir(dirs.root().to_path_buf());
    // The log goes where a person can reach it without a computer's adb: Documents/WordCraft/logs
    // on shared storage when All files access is granted (any file manager can open and share it),
    // else the app's folder there, else the private folder.
    let shared_logs = PathBuf::from(storage::SHARED_LOG_DIR);
    let app_logs = android.external_data_path().map(|p| p.join("logs"));
    let private_logs = dirs.logs();
    let mut candidates: Vec<&Path> = vec![&shared_logs];
    candidates.extend(app_logs.as_deref());
    candidates.push(&private_logs);
    crate::logging::attach_dirs(&candidates);
    log::info!("settings folder {}", dirs.root().display());
    load_user_fonts(&dirs);

    let options = eframe::NativeOptions { android_app: Some(android.clone()), ..Default::default() };
    eframe::run_native(
        "WordCraft",
        options,
        Box::new(move |cc| {
            // Fingers are less exact than a mouse: a tap may wander further and a double tap
            // may take longer than the desktop defaults.
            cc.egui_ctx.options_mut(|o| {
                o.input_options.max_click_dist = 12.0;
                o.input_options.max_double_click_delay = 0.5;
                // A press and hold opens the context menu this long after the page got the press
                // (which a resting finger gets after `touch::HOLD`, 0.2 s); a button takes a press
                // that lasts up to this long as a click.
                o.input_options.max_click_duration = 0.5;
            });
            let region = wordcraft_ui_egui::i18n::system_region();
            let mut session = Session::new(wordcraft_engine::blank_document(&region));
            session.system_region = region;
            let mut app = WordApp::new(session, host::services());
            if !load_prefs(&mut app, &dirs) {
                // First start: a phone or a tablet is used with fingers, so the interface starts a
                // step larger than the desktop's 100 % (File › Options › Interface size changes it).
                let wide = android.config().screen_width_dp().is_none_or(|dp| dp >= 600);
                let percent = if wide { FIRST_RUN_INTERFACE_PERCENT } else { 100 };
                if let Err(e) = app.run("ui.interfaceSize", serde_json::json!({"percent": percent})) {
                    log::warn!("interface size: {e}");
                }
            }
            load_user_dictionary(&mut app, &dirs);
            let mut recovery = Recovery::default();
            if let Some(stamp) = start_document(&mut app, &dirs) {
                recovery.restored(stamp);
            }
            let keyboard = SoftKeyboard::new(android.clone(), &cc.egui_ctx);
            let prefs_saved = prefs_bytes(&app).unwrap_or_default();
            let shell = Shell {
                android,
                app,
                dirs,
                recovery,
                keyboard,
                left: false,
                dark: None,
                wake: None,
                started: Instant::now(),
                prefs_saved,
                prefs_checked_ms: 0.0,
            };
            Ok(Box::new(shell))
        }),
    )
    .map_err(|e| e.to_string())
}

/// How often a changed preferences file is written (no more often than this).
const PREFS_INTERVAL_MS: f64 = 2_000.0;

/// The editor and what keeps its work safe.
struct Shell {
    android: AndroidApp,
    app: WordApp,
    dirs: Dirs,
    recovery: Recovery,
    keyboard: SoftKeyboard,
    /// The app lost focus since the last frame: Android may end it next.
    left: bool,
    /// The system appearance last reported to the editor.
    dark: Option<bool>,
    /// A frame the touch gate wants (a finger that becomes a hold, a tap's release).
    wake: Option<Duration>,
    /// The clock the planners use: milliseconds since start, which never goes backwards.
    started: Instant,
    /// The preferences file as last written (or read), to write only when something changed.
    prefs_saved: Vec<u8>,
    prefs_checked_ms: f64,
}

impl eframe::App for Shell {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Follow the system's light or dark appearance (winit doesn't report it on Android); a
        // change draws a frame, since nothing else may.
        let dark = match self.android.config().ui_mode_night() {
            UiModeNight::Yes => Some(true),
            UiModeNight::No => Some(false),
            _ => None,
        };
        if dark != self.dark {
            self.dark = dark;
            ctx.request_repaint();
        }
        self.app.set_system_dark(dark);
        if let Some(wake) = self.wake.take() {
            ctx.request_repaint_after(wake);
        }
        self.app.logic(ctx);
        // The keyboard is read while the page or a text field has focus.
        self.keyboard.want(self.app.canvas.focused || ctx.egui_wants_keyboard_input());
        if self.app.quit_requested {
            // The person chose to close (and to save or discard); on_exit removes the recovery copy.
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        let page_owns = self.page_owns_keyboard(ctx);
        self.keyboard.tick(|| if page_owns { context_before_caret(&self.app) } else { FIELD_CONTEXT.to_owned() });
        let leaving = std::mem::take(&mut self.left);
        self.keep_prefs(leaving);
        self.keep_recovery(ctx, leaving);
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.app.raw_input_hook(raw);
        // A finger's press waits until it is known to be a tap or a hold; a scroll never reaches the
        // page as a press. A finger that rests asks for a frame when it becomes a hold.
        // (The frame is requested in `logic`: egui drops a delayed request made before its pass.)
        self.wake = self.app.gate_touches(ctx, raw);
        // What the keyboard typed goes first: it was typed before this frame's touches and keys.
        let page_owns = self.page_owns_keyboard(ctx);
        let typed = self.keyboard.events(&raw.events, &mut self.app, page_owns);
        raw.events.splice(0..0, typed);
        self.left |= raw.events.iter().any(|e| matches!(e, egui::Event::WindowFocused(false)));
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.app.ui(ui);
        // With a hardware keyboard the page doesn't raise the on-screen one by itself: while it is
        // up, the system keeps the arrow, Home and End keys for the keyboard's own buffer. The
        // keyboard button and a tap on the page still raise it.
        if !matches!(self.android.config().keyboard(), Keyboard::NoKeys | Keyboard::Any) {
            ui.ctx().output_mut(|o| o.ime = None);
        }
    }

    /// Reached only when the person closed the app (File, Close) after saving or discarding: the
    /// system ending the app doesn't come through here, which is what the recovery copy is for.
    fn on_exit(&mut self) {
        self.keep_prefs(true);
        storage::remove_quietly(&self.dirs.recovery());
    }
}

impl Shell {
    /// The page (not a text field of a dialog, Find or the file browser) has the keyboard: what the
    /// keyboard types goes into the document, and the buffer holds its text before the caret.
    fn page_owns_keyboard(&self, ctx: &egui::Context) -> bool {
        self.app.canvas.focused && !ctx.egui_wants_keyboard_input() && !self.app.session.view.read_mode
    }

    fn now_ms(&self) -> f64 {
        self.started.elapsed().as_secs_f64() * 1000.0
    }

    /// Write the preferences when they changed (at most every [`PREFS_INTERVAL_MS`]), or at once
    /// when `force`.
    fn keep_prefs(&mut self, force: bool) {
        let now = self.now_ms();
        if !force && now - self.prefs_checked_ms < PREFS_INTERVAL_MS {
            return;
        }
        self.prefs_checked_ms = now;
        let Some(bytes) = prefs_bytes(&self.app) else { return };
        if bytes == self.prefs_saved {
            return;
        }
        match storage::write_atomic(&self.dirs.prefs(), &bytes) {
            Ok(()) => self.prefs_saved = bytes,
            Err(e) => log::warn!("preferences not saved: {e}"),
        }
    }

    /// Keep the recovery copy of unsaved work in step with the document ([`Recovery`]).
    fn keep_recovery(&mut self, ctx: &egui::Context, leaving: bool) {
        let session = &self.app.session;
        let stamp = (session.document_id(), session.rev());
        let now = self.now_ms();
        match self.recovery.plan(session.dirty, stamp, now, leaving) {
            Plan::Nothing => {}
            // A paused app draws no frames, so a copy that is due has to ask for one.
            Plan::Wait(ms) => ctx.request_repaint_after(Duration::from_millis(ms.clamp(0.0, 60_000.0).ceil() as u64)),
            Plan::Clear => storage::remove_quietly(&self.dirs.recovery()),
            Plan::Write => {
                let written = storage::guarded(|| wordcraft_engine::io::save_bytes(storage::RECOVERY_FILE, &session.doc))
                    .and_then(|bytes| storage::write_atomic(&self.dirs.recovery(), &bytes).map_err(|e| e.to_string()));
                match written {
                    Ok(()) => self.recovery.wrote(stamp),
                    Err(e) => {
                        log::warn!("recovery copy: {e}");
                        self.recovery.failed(now);
                    }
                }
            }
        }
    }
}

/// The text before the caret (the start of the selection) that the on-screen keyboard's buffer is
/// filled with, so what it sees is what the document holds there ([`crate::ime::context`]).
fn context_before_caret(app: &WordApp) -> String {
    let (start, _) = app.session.sel.ordered();
    let before = app.session.doc.para_at(&start).and_then(|p| p.text.get(..start.off)).unwrap_or("");
    crate::ime::context(before)
}

/// The preferences as the file holds them.
fn prefs_bytes(app: &WordApp) -> Option<Vec<u8>> {
    match serde_json::to_vec_pretty(&app.prefs()) {
        Ok(bytes) => Some(bytes),
        Err(e) => {
            log::warn!("preferences not saved: {e}");
            None
        }
    }
}

/// Open the recovery copy a previous run left, as an unsaved document, or else show File, Home
/// as the desktop app does. The result is the opened document's (identity, revision) when there was
/// a copy ([`Recovery::restored`]).
fn start_document(app: &mut WordApp, dirs: &Dirs) -> Option<(u64, u64)> {
    match storage::restore_with(dirs, |bytes| wordcraft_engine::io::open_bytes(storage::RECOVERY_FILE, bytes)) {
        Restore::Opened(doc) => {
            app.session.set_document(doc);
            app.session.path = None;
            app.session.dirty = true;
            app.status("Your unsaved work from the last time was recovered. Save it to keep it.");
            log::info!("restored the recovery copy");
            Some((app.session.document_id(), app.session.rev()))
        }
        Restore::Unreadable(kept) => {
            app.start_screen();
            // Never deleted: it may be the only copy. It is in the app's own folder.
            log::warn!("the recovery copy was kept as {}", kept.display());
            app.status("Your unsaved work from the last time could not be opened. The copy is kept in the app's folder.");
            None
        }
        Restore::Nothing => {
            app.start_screen();
            None
        }
    }
}

/// Fonts the user added live in `fonts` in the settings folder.
fn load_user_fonts(dirs: &Dirs) {
    let (loaded, skipped) = wordcraft_fonts::FontDb::global().load_user_fonts(&dirs.fonts());
    if !loaded.is_empty() || !skipped.is_empty() {
        log::info!("user fonts: {} loaded, {} skipped from {}", loaded.len(), skipped.len(), dirs.fonts().display());
    }
}

/// Apply the saved preferences; false when there are none yet (the first start).
fn load_prefs(app: &mut WordApp, dirs: &Dirs) -> bool {
    if let Some(bytes) = storage::read_limited(&dirs.prefs(), storage::MAX_FILE_BYTES)
        && let Some(ui) = UiState::from_json(&bytes)
    {
        app.apply_prefs(ui);
        return true;
    }
    false
}

/// The words Add to Dictionary kept.
fn load_user_dictionary(app: &mut WordApp, dirs: &Dirs) {
    let path = dirs.dictionary(Session::USER_DICT_FILE);
    wordcraft_proof::load_user_dictionary(&path);
    app.session.user_dict_path = Some(path);
}
