//! Software updates, background check at startup, and startup prompt modal.

use crate::WordApp;

#[cfg(not(target_arch = "wasm32"))]
use craft_update::dialog::{CheckResultDialog, CheckResultResponse};
#[cfg(not(target_arch = "wasm32"))]
use craft_update::updater::{Action, Status, Updater};

pub(crate) struct Runtime {
    #[cfg(not(target_arch = "wasm32"))]
    updater: Updater,
    #[cfg(not(target_arch = "wasm32"))]
    result_dialog: CheckResultDialog,
    /// The desktop app turns the check at start on; tests and the web build leave it off.
    #[cfg(not(target_arch = "wasm32"))]
    startup_check: bool,
    #[cfg(not(target_arch = "wasm32"))]
    check_requested: bool,
    /// Why this process doesn't update itself, if it doesn't ([`WordApp::disable_self_update`]).
    #[cfg(not(target_arch = "wasm32"))]
    off: Option<UpdatesOff>,
    /// The context of the last frame, to wake the UI when a check is queued from a menu callback.
    #[cfg(not(target_arch = "wasm32"))]
    repaint: Option<egui::Context>,
    pub(crate) manual_window_open: bool,
    #[cfg(target_arch = "wasm32")]
    pub(crate) status_message: Option<String>,
}

impl Default for Runtime {
    fn default() -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            updater: Updater::new(&crate::UPDATE_APP, env!("CARGO_PKG_VERSION")).with_translator(translate),
            #[cfg(not(target_arch = "wasm32"))]
            result_dialog: CheckResultDialog::new(&crate::UPDATE_APP, env!("CARGO_PKG_VERSION")).with_translator(translate),
            #[cfg(not(target_arch = "wasm32"))]
            startup_check: false,
            #[cfg(not(target_arch = "wasm32"))]
            check_requested: false,
            #[cfg(not(target_arch = "wasm32"))]
            off: None,
            #[cfg(not(target_arch = "wasm32"))]
            repaint: None,
            manual_window_open: false,
            #[cfg(target_arch = "wasm32")]
            status_message: None,
        }
    }
}

/// The update prompt's and the result window's words in the interface language: craft-update
/// passes its English text and the values of its `{name}` placeholders each time it draws.
#[cfg(not(target_arch = "wasm32"))]
fn translate(text: &str, args: &[(&str, &str)]) -> String {
    crate::i18n::fmt(crate::i18n::t(text), args)
}

impl WordApp {
    /// Whether the app asks for a newer release when it starts (off until the desktop app turns it on).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn set_startup_update_check(&mut self, on: bool) {
        self.updates_rt.startup_check = on;
    }

    /// Turns the self-update off for the rest of this process: no update is offered, nothing is
    /// fetched, and checking for updates says `why`, in the interface language of the moment it
    /// is asked. An edition ([`UpdatesOff::Edition`]) has no Help ▸ Check for Updates at all
    /// ([`WordApp::offers_update_check`]): craft-update's window would name the stock app's maker
    /// and releases.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn disable_self_update(&mut self, why: UpdatesOff) {
        let rt = &mut self.updates_rt;
        rt.startup_check = false;
        rt.off = Some(why);
        rt.updater = refusing_updater(why);
    }

    /// Whether Help ▸ Check for Updates is offered: not in an edition, which never updates itself.
    pub(crate) fn offers_update_check(&self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        return self.updates_rt.off != Some(UpdatesOff::Edition);
        #[cfg(target_arch = "wasm32")]
        return true;
    }

    /// Starts the helper of an update that is ready, so that it installs once the process has
    /// ended. Call it once the exit is certain, before a path that ends the process without
    /// dropping the app. Later calls do nothing.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn start_pending_update(&mut self) {
        self.updates_rt.updater.start_pending_update();
    }
}

/// Why a process doesn't update itself ([`WordApp::disable_self_update`]).
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdatesOff {
    /// It still carries another program's AppImage variables, which the update would take for
    /// this copy's file and replace (#5653). Starting WordCraft from its own entry fixes it.
    ForeignAppImage,
    /// It is an edition ([`crate::i18n::EDITION_LANGUAGE`], the Hebrew one): the releases the
    /// update reads are the stock English ones, and installing one would replace the edition.
    /// Always off.
    Edition,
}

#[cfg(not(target_arch = "wasm32"))]
impl UpdatesOff {
    /// Why a process built with `edition` ([`crate::i18n::EDITION_LANGUAGE`]) that does or doesn't
    /// carry another program's AppImage variables can't update itself, if it can't. An edition's
    /// reason comes first: it holds however the program is started.
    pub fn of(edition: Option<&str>, foreign_appimage: bool) -> Option<UpdatesOff> {
        if edition.is_some() {
            Some(UpdatesOff::Edition)
        } else if foreign_appimage {
            Some(UpdatesOff::ForeignAppImage)
        } else {
            None
        }
    }

    /// The reason a check for updates gives, in English (the catalog key).
    fn reason(self) -> &'static str {
        match self {
            UpdatesOff::ForeignAppImage => {
                "Updates are off in this window: WordCraft was started with another program's AppImage settings. Start WordCraft from its own menu entry to update it."
            }
            UpdatesOff::Edition => {
                "This edition of WordCraft doesn't update itself: the updates it would find are the standard English releases, which would replace it. To update, install a newer build of this edition."
            }
        }
    }
}

/// An updater whose check never finds an update and gives `why` as its error, in the current
/// interface language, which Help ▸ Check for Updates shows. Nothing is fetched. The check runs
/// on another thread, where the interface language isn't known, so the reason is worded here.
#[cfg(not(target_arch = "wasm32"))]
fn refusing_updater(why: UpdatesOff) -> Updater {
    let why = tl!(why.reason()).to_string();
    let check: craft_update::updater::CheckSource = std::sync::Arc::new(move || Err(why.clone()));
    Updater::new(&crate::UPDATE_APP, env!("CARGO_PKG_VERSION")).with_translator(translate).with_source(check)
}

/// Help ▸ Check for Updates: checks now and shows the result. In an edition, nothing: the reason,
/// in English for scripts, as the error (the status bar translates it).
pub(crate) fn check(app: &mut WordApp) -> Result<(), String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        match app.updates_rt.off {
            Some(UpdatesOff::Edition) => return Err(UpdatesOff::Edition.reason().to_string()),
            // The reason in the interface language the window is in now: the self-update is
            // turned off before the first frame, which is when the app first sets the language.
            Some(why) => app.updates_rt.updater = refusing_updater(why),
            None => {}
        }
        app.updates_rt.check_requested = true;
        app.updates_rt.manual_window_open = true;
        if let Some(ctx) = &app.updates_rt.repaint {
            ctx.request_repaint();
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        app.updates_rt.manual_window_open = true;
        app.updates_rt.status_message = Some(tl!("Update check is not supported in the web browser.").to_string());
    }
    Ok(())
}

pub(crate) fn tick(app: &mut WordApp, ctx: &egui::Context) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let rt = &mut app.updates_rt;
        rt.repaint = Some(ctx.clone());
        if std::mem::take(&mut rt.check_requested) {
            rt.updater.check_now(ctx);
        } else if rt.startup_check {
            rt.updater.check_at_start(ctx);
        }
        rt.updater.tick(ctx);
    }
    #[cfg(target_arch = "wasm32")]
    let _ = (app, ctx);
}

pub(crate) fn show(app: &mut WordApp, ctx: &egui::Context) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        // An update starts, and a ready one asks the app to close, only while no work is unsaved.
        let clean = !(app.session.dirty || app.save_unconfirmed() || app.printing.busy() || crate::panes::has_comment_drafts(app));
        match app.updates_rt.updater.show_if(ctx, clean) {
            Action::OpenUrl(url) => ctx.open_url(egui::OpenUrl::new_tab(url)),
            // The whole app quits, every window included, and first asks about each document with
            // unsaved changes.
            Action::Close => {
                let _ = app.ask_before_quit();
            }
            Action::None => {}
        }
        // The dialog answers Help ▸ Check for Updates; a found update leaves the status idle and the prompt above takes over.
        // It also reports what the last update came to, which the check at start leaves as the status.
        let rt = &mut app.updates_rt;
        if !matches!(rt.updater.status(), Status::Idle) {
            rt.manual_window_open = true;
        }
        if rt.manual_window_open {
            match rt.result_dialog.show(ctx, rt.updater.status()) {
                CheckResultResponse::None => {}
                CheckResultResponse::Close => {
                    rt.manual_window_open = false;
                    rt.updater.dismiss_status();
                }
                CheckResultResponse::CheckAgain => rt.updater.check_now(ctx),
                // The window closes, and the next frame's `show_if` asks the app to quit once the
                // shown document has no unsaved work.
                CheckResultResponse::RestartToUpdate => {
                    rt.updater.restart_to_update();
                    rt.manual_window_open = false;
                    rt.updater.dismiss_status();
                }
            }
            // An update was found: the prompt replaces the dialog.
            if matches!(rt.updater.status(), Status::Idle) {
                rt.manual_window_open = false;
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    if app.updates_rt.manual_window_open {
        let mut open = true;
        egui::Window::new(tl!("Check for Updates"))
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                crate::rtl::root(ui, |ui| {
                    if let Some(msg) = &app.updates_rt.status_message {
                        ui.label(msg);
                    }
                })
            });
        if !open {
            app.updates_rt.manual_window_open = false;
            app.updates_rt.status_message = None;
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use craft_update::updater::Status;

    /// With the self-update off, a startup check is never made, and Help ▸ Check for Updates (the
    /// command's own path) ends in the reason instead of a release to install (#5653).
    #[test]
    fn with_the_self_update_off_a_check_finds_nothing_and_says_why() {
        let mut app = crate::WordApp::new(wordcraft_engine::Session::new(wordcraft_doc::Document::new()), crate::Services::default());
        app.set_startup_update_check(true);
        app.disable_self_update(super::UpdatesOff::ForeignAppImage);
        assert!(!app.updates_rt.startup_check, "no check at start");
        let ctx = egui::Context::default();
        super::check(&mut app).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !matches!(app.updates_rt.updater.status(), Status::Failed(_)) {
            assert!(std::time::Instant::now() < deadline, "the check never ended: {:?}", app.updates_rt.updater.status());
            super::tick(&mut app, &ctx);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let status = app.updates_rt.updater.status();
        assert!(matches!(status, Status::Failed(why) if why.contains("another program's AppImage")), "{status:?}");
        assert!(!app.updates_rt.updater.prompt_open(), "no update offered");
    }

    /// An edition never updates itself: the stock releases it would find are English and would
    /// replace it. No check at start, no Help ▸ Check for Updates, and asking for one anyway (the
    /// command search, a script) says why on the status line and to the caller, starts nothing and
    /// opens no window.
    #[test]
    fn an_edition_never_checks_for_updates() {
        let mut app = crate::WordApp::new(wordcraft_engine::Session::new(wordcraft_doc::Document::new()), crate::Services::default());
        app.set_startup_update_check(true);
        app.disable_self_update(super::UpdatesOff::Edition);
        assert!(!app.offers_update_check());
        let why = app.run("ui.checkForUpdates", serde_json::json!({})).unwrap_err();
        assert!(why.contains("English releases"), "{why}");
        assert_eq!(app.status_msg.as_ref().map(|(shown, _)| shown.as_str()), Some(why.as_str()), "the status line says it too");
        let ctx = egui::Context::default();
        for _ in 0..3 {
            super::tick(&mut app, &ctx);
        }
        assert_eq!(app.updates_rt.updater.status(), &Status::Idle, "nothing checked");
        assert!(!app.updates_rt.manual_window_open, "no update window");
    }

    /// The self-update is turned off before the first frame sets the interface language; Help ▸
    /// Check for Updates gives the reason in the language the window is in when it is asked.
    #[test]
    fn the_reason_is_in_the_interface_language_of_the_check() {
        use super::UpdatesOff;
        use crate::i18n::{Lang, fixture_lang, set_current};
        let mut app = crate::WordApp::new(wordcraft_engine::Session::new(wordcraft_doc::Document::new()), crate::Services::default());
        set_current(Lang::EN);
        app.disable_self_update(UpdatesOff::ForeignAppImage);
        let row: &'static str = Box::leak(format!("\t{}\tהעדכונים כבויים\n", UpdatesOff::ForeignAppImage.reason()).into_boxed_str());
        set_current(fixture_lang("he", row));
        let checked = super::check(&mut app);
        set_current(Lang::EN);
        checked.unwrap();
        let ctx = egui::Context::default();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !matches!(app.updates_rt.updater.status(), Status::Failed(_)) {
            assert!(std::time::Instant::now() < deadline, "the check never ended: {:?}", app.updates_rt.updater.status());
            super::tick(&mut app, &ctx);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(app.updates_rt.updater.status(), &Status::Failed("העדכונים כבויים".into()));
    }

    /// Help ▸ Check for Updates runs while the ribbon draws, after the frame's `tick`: the result
    /// window shows the check from the next frame on.
    #[test]
    fn a_check_asked_for_while_drawing_opens_the_result_window() {
        let mut app = crate::WordApp::new(wordcraft_engine::Session::new(wordcraft_doc::Document::new()), crate::Services::default());
        let pins = std::env::temp_dir().join(format!("wordcraft-no-pins-{}.toml", std::process::id()));
        app.updates_rt.updater = craft_update::updater::Updater::new(&crate::UPDATE_APP, env!("CARGO_PKG_VERSION"))
            .with_source(std::sync::Arc::new(|| Err("offline".into())))
            .with_pins_path(pins);
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            super::tick(&mut app, ui.ctx());
            super::check(&mut app).unwrap();
            super::show(&mut app, ui.ctx());
        });
        out.textures_delta.clear();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            super::tick(&mut app, ui.ctx());
            super::show(&mut app, ui.ctx());
        });
        out.textures_delta.clear();
        let status = app.updates_rt.updater.status();
        assert!(matches!(status, Status::Checking | Status::Failed(_)), "{status:?}");
        assert!(app.updates_rt.manual_window_open, "the result window shows the check ({status:?})");
    }

    /// The first start after an update reports what it came to without a menu click: the check at
    /// start reads the record, the result window opens, and Esc closes it and clears the status.
    #[test]
    fn the_last_update_opens_the_result_window_until_it_is_closed() {
        let version = env!("CARGO_PKG_VERSION");
        // The files craft-update writes for an installed update and for a failed one.
        let cases: [(&str, &str, fn(&Status) -> bool); 2] = [
            ("data/self-update-done.txt", version, |s| matches!(s, Status::Updated(v) if v == env!("CARGO_PKG_VERSION"))),
            ("data/logs/self-update-error.txt", "disk full", |s| matches!(s, Status::InstallFailed(why) if why == "disk full")),
        ];
        for (n, (file, text, expected)) in cases.into_iter().enumerate() {
            let root = std::env::temp_dir().join(format!("wordcraft-last-update-{n}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            let record = root.join(file);
            std::fs::create_dir_all(record.parent().unwrap()).unwrap();
            std::fs::write(&record, text).unwrap();
            let mut app = crate::WordApp::new(wordcraft_engine::Session::new(wordcraft_doc::Document::new()), crate::Services::default());
            app.updates_rt.updater = craft_update::updater::Updater::new(&crate::UPDATE_APP, version)
                .with_source(std::sync::Arc::new(|| Ok(None)))
                .with_paths(craft_update::Paths::under(&root))
                .with_pins_path(root.join("no-pins.toml"));
            app.set_startup_update_check(true);
            let ctx = egui::Context::default();
            let frame = |app: &mut crate::WordApp, events: Vec<egui::Event>| {
                let input = egui::RawInput { events, ..Default::default() };
                let mut out = ctx.run_ui(input, |ui| {
                    super::tick(app, ui.ctx());
                    super::show(app, ui.ctx());
                });
                out.textures_delta.clear();
            };
            frame(&mut app, Vec::new());
            let status = app.updates_rt.updater.status();
            assert!(expected(status), "{file}: {status:?}");
            assert!(app.updates_rt.manual_window_open, "{file}: the result window opens by itself");
            assert!(!record.exists(), "{file}: the record is read once");
            frame(&mut app, Vec::new());
            assert!(app.updates_rt.manual_window_open, "{file}: the window stays open");
            let escape =
                egui::Event::Key { key: egui::Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE };
            frame(&mut app, vec![escape]);
            assert!(!app.updates_rt.manual_window_open, "{file}: Esc closes the window");
            assert_eq!(app.updates_rt.updater.status(), &Status::Idle, "{file}: closing clears the status");
            frame(&mut app, Vec::new());
            assert!(!app.updates_rt.manual_window_open, "{file}: the window stays closed");
            let _ = std::fs::remove_dir_all(&root);
        }
    }
}
