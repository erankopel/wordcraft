//! The AI pane: ask for a change to the selection or the document, or a question about it, and put the
//! reply into the document.
//!
//! Text leaves the machine only through a provider, and only after the person agrees here: before a
//! request that would send more than has been agreed to in this run the pane says who receives the text,
//! which model reads it, how many characters go and how the text starts. No command and no setting can give
//! that agreement; only the button in the pane can. A reply is placed with `assistant.apply`, so it is one
//! undo step and tracked when Track Changes is on. Reading what would be shared (`assistant.context`) and
//! placing a reply (`assistant.apply`) are engine commands; sending is this pane's alone, because an agent
//! that drives WordCraft brings its own model. See `docs/assistant.md`.

use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::Duration;

use egui::accesskit::{Live, Role};
use egui::{Frame, Margin, RichText, Stroke, TextEdit, Ui};
use serde_json::json;
use wordcraft_assistant::{
    DEFAULT_MODEL, Disclosure, Error, MAX_INSTRUCTION_BYTES, Provider, Reply, Request, Role as Speaker, Scope, Stop, Turn, build_body,
};
use wordcraft_engine::Password;

use crate::WordApp;
use crate::i18n::fmt;
use crate::theme::semibold;

/// Whether this build can send a request at all: the browser version cannot.
const CAN_SEND: bool = cfg!(not(target_arch = "wasm32"));

/// How many characters of the shared text the agreement step shows the person.
const PREVIEW_CHARS: usize = 160;

/// How many frames a control asks for the keyboard focus: the page takes the keyboard back after a command
/// has run, so a single request in the frame the control appears can be taken away again.
const FOCUS_FRAMES: u8 = 3;

/// Makes the provider for a request from the key the person typed, if they typed one (a provider that needs
/// a key and has none looks for it in the environment). A function, so a test can put its own in the state.
type Factory = fn(Option<&str>) -> Result<Box<dyn Provider>, Error>;

/// The Claude provider, with the key typed in the pane, else the one in the environment.
#[cfg(not(target_arch = "wasm32"))]
fn claude(typed: Option<&str>) -> Result<Box<dyn Provider>, Error> {
    use wordcraft_assistant::claude::Claude;
    Ok(Box::new(match typed {
        Some(key) => Claude::from_key(Some(key))?,
        None => Claude::from_env()?,
    }))
}

/// The browser version cannot send requests.
#[cfg(target_arch = "wasm32")]
fn claude(_: Option<&str>) -> Result<Box<dyn Provider>, Error> {
    Err(Error::Unavailable)
}

/// The text a reply was asked about: the document revision and the selection when the request was made. A
/// reply to a selection is placed only while both are as they were, so it never lands in other text.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Basis {
    rev: u64,
    selection: wordcraft_engine::Selection,
}

/// A request that is ready to go and waits for the person's agreement.
struct Ready {
    provider: Box<dyn Provider>,
    request: Request,
    disclosure: Disclosure,
    /// How many characters of the document go with it.
    chars: usize,
    /// For a request about the selection: what it was asked about.
    basis: Option<Basis>,
    /// The key the request is made with, if the person typed or saved one: it is the one that may be kept
    /// when the service accepts it, whatever the key box holds by then.
    key: Option<Password>,
}

/// A request in flight on its worker thread.
struct Waiting {
    rx: Receiver<Result<Reply, Error>>,
    instruction: String,
    basis: Option<Basis>,
    key: Option<Password>,
}

/// Where keyboard focus should go the next time the pane is drawn: a control that appears or disappears
/// would otherwise drop the focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Focus {
    None,
    Send,
    Agree,
    Stop,
    Key,
}

/// How strongly a notice speaks, and so how it is drawn and announced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tone {
    Info,
    Warn,
    Bad,
}

/// What the person agreed to and typed for the whole run of the program, in every window: a second window
/// does not ask again, and the key is not typed again. The key is kept on this computer only when the person
/// asks for that, in a file only they can read ([`wordcraft_assistant::key_store`]).
#[derive(Default)]
pub(crate) struct AssistantRun {
    /// The widest scope the person agreed to send to a provider that leaves the machine: `None` until they
    /// agree. Agreeing to the document covers the selection too. Never saved.
    agreed: Option<Scope>,
    /// The key the person typed or that was saved: kept in memory, wiped when dropped, never shown.
    key: Password,
    /// Where a key can be kept. `None`: nowhere (the browser, or before the app has chosen a folder).
    #[cfg(not(target_arch = "wasm32"))]
    file: Option<wordcraft_assistant::key_store::KeyFile>,
    /// Keep the key on this computer: once it has worked, and from then on.
    #[cfg(not(target_arch = "wasm32"))]
    remember: bool,
    /// The key as it is in the file, when it is in the file.
    #[cfg(not(target_arch = "wasm32"))]
    on_disk: Option<Password>,
    /// Why the saved key could not be used, written or removed.
    #[cfg(not(target_arch = "wasm32"))]
    file_problem: Option<wordcraft_assistant::key_store::KeyFileError>,
}

impl AssistantRun {
    /// The key to give the provider: the one typed or saved, if any.
    fn typed_key(&self) -> Option<&str> {
        Some(self.key.as_str().trim()).filter(|k| !k.is_empty())
    }

    /// Can a key be kept on this computer?
    #[cfg(not(target_arch = "wasm32"))]
    fn can_keep(&self) -> bool {
        self.file.is_some()
    }

    /// Keep `used`, the key a request was made with, if the person asked for that and the file does not
    /// already have it. Called once the service has answered, so a key that does not work is not kept, and
    /// the key that is kept is the one that worked. Says whether the file was written.
    #[cfg(not(target_arch = "wasm32"))]
    fn keep_if_asked(&mut self, used: Option<&Password>) -> bool {
        let Some(file) = self.file.clone() else { return false };
        let Some(key) = used.map(|k| k.as_str().trim()).filter(|k| !k.is_empty()) else { return false };
        if !self.remember || self.on_disk.as_ref().is_some_and(|saved| saved.as_str() == key) {
            return false;
        }
        match file.save(key) {
            Ok(()) => {
                self.on_disk = Some(Password::new(key));
                self.file_problem = None;
                true
            }
            Err(e) => {
                self.file_problem = Some(e);
                false
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn keep_if_asked(&mut self, _used: Option<&Password>) -> bool {
        false
    }

    /// Delete the saved key, and with `also_typed` the one in memory. Says whether the file is gone: a file
    /// that could not be removed stays tracked (the tick, the button), so the person can try again, and the
    /// key in memory stays with it.
    #[cfg(not(target_arch = "wasm32"))]
    fn forget(&mut self, also_typed: bool) -> bool {
        let removed = match &self.file {
            Some(file) => match file.forget() {
                Ok(()) => {
                    self.file_problem = None;
                    true
                }
                Err(problem) => {
                    self.file_problem = Some(problem);
                    false
                }
            },
            None => true,
        };
        if removed {
            self.on_disk = None;
            self.remember = false;
            if also_typed {
                self.key = Password::default();
            }
        }
        removed
    }
}

/// The key box has one id in every window, because the key it edits is the run's, not a window's: what it
/// remembers for undo is then one thing to clear.
fn key_box_id() -> egui::Id {
    egui::Id::unique("assistant.key")
}

/// Clear what the key box remembers for undo, which would hold the key it showed.
#[cfg(not(target_arch = "wasm32"))]
fn wipe_key_history(ctx: &egui::Context) {
    let id = key_box_id();
    if let Some(mut state) = egui::text_edit::TextEditState::load(ctx, id) {
        state.clear_undoer();
        state.store(ctx, id);
    }
}

/// What the pane remembers for one window.
pub(crate) struct AssistantState {
    /// Share the whole document, not the selection.
    document_scope: bool,
    prompt: String,
    /// The earlier turns of this chat, oldest first.
    history: Vec<Turn>,
    reply: Option<Reply>,
    /// What the reply was asked about, when that was the selection.
    basis: Option<Basis>,
    error: Option<Error>,
    /// Why a reply could not be placed, in the engine's words.
    apply_error: Option<String>,
    /// The person stopped waiting for a request that had been sent.
    stopped: bool,
    ready: Option<Ready>,
    waiting: Option<Waiting>,
    want_focus: Focus,
    /// For how many more frames `want_focus` is asked for.
    focus_frames: u8,
    /// What a screen reader is told when something changes away from the keyboard: a live region.
    said: String,
    /// Whether the keyboard focus was on Stop Waiting when it was last drawn.
    stop_focused: bool,
    /// Flip at every new attempt, so an alert that repeats the last (the same error twice) is spoken again:
    /// one for a failed request, one for a reply that could not be placed.
    alert_mark: bool,
    apply_mark: bool,
    factory: Factory,
}

impl Default for AssistantState {
    fn default() -> AssistantState {
        AssistantState {
            document_scope: false,
            prompt: String::new(),
            history: Vec::new(),
            reply: None,
            basis: None,
            error: None,
            apply_error: None,
            stopped: false,
            ready: None,
            waiting: None,
            want_focus: Focus::None,
            focus_frames: 0,
            said: String::new(),
            stop_focused: false,
            alert_mark: false,
            apply_mark: false,
            factory: claude,
        }
    }
}

impl AssistantState {
    /// A new chat for a new document: what the person chose stays, the conversation goes. (What they agreed to
    /// and typed as the key is [`AssistantRun`]'s, so it stays anyway.)
    pub(crate) fn fresh_for_document(old: &AssistantState) -> AssistantState {
        AssistantState { document_scope: old.document_scope, factory: old.factory, ..AssistantState::default() }
    }

    /// Ask for the keyboard focus on a control for the next few frames.
    fn want(&mut self, focus: Focus) {
        self.want_focus = focus;
        self.focus_frames = FOCUS_FRAMES;
    }

    /// Tell a screen reader something: the status line is a live region, and a line that repeats the last
    /// gets an invisible mark so it counts as new.
    fn say(&mut self, text: String) {
        self.said = if text == self.said.trim_end_matches('\u{200b}') && !self.said.ends_with('\u{200b}') { format!("{text}\u{200b}") } else { text };
    }

    fn scope(&self) -> Scope {
        if self.document_scope { Scope::Document } else { Scope::Selection }
    }
}

/// Must the person agree before a request sharing `scope` goes to a provider? Only when the provider leaves
/// the machine, and they have not agreed already to that much or more in this run.
fn needs_agreement(agreed: Option<Scope>, scope: Scope, leaves_machine: bool) -> bool {
    leaves_machine && !agreed.is_some_and(|wide| wide == Scope::Document || wide == scope)
}

/// What a click in the pane asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Act {
    Send,
    Agree,
    Cancel,
    Stop,
    NewChat,
    Replace,
    InsertAfter,
    Copy,
    #[cfg(not(target_arch = "wasm32"))]
    ForgetKey,
}

impl WordApp {
    /// Keep the API key in `folder` (the settings folder) when the person asks for that, and use the key
    /// saved there by an earlier run. Called once, by the desktop app, after its settings are loaded.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn use_saved_assistant_key(&mut self, folder: &std::path::Path) {
        use wordcraft_assistant::key_store::KeyFile;
        let file = KeyFile::in_folder(folder);
        let run = &mut self.assistant_run;
        match file.load() {
            Ok(Some(key)) => {
                run.key = Password::new(&key);
                run.on_disk = Some(Password::new(&key));
                run.remember = true;
            }
            Ok(None) => {}
            Err(problem) => run.file_problem = Some(problem),
        }
        run.file = Some(file);
    }

    /// Carry out what a click asked for.
    pub(crate) fn assistant_act(&mut self, act: Act, ctx: &egui::Context) {
        match act {
            Act::Send => self.assistant_ask(ctx),
            Act::Agree => {
                if let Some(ready) = self.assistant.ready.take() {
                    let scope = ready.request.scope;
                    let run = &mut self.assistant_run;
                    run.agreed = Some(if run.agreed == Some(Scope::Document) { Scope::Document } else { scope });
                    self.assistant_send(ready, ctx);
                }
            }
            Act::Cancel => {
                self.assistant.ready = None;
                self.assistant.want(Focus::Send);
            }
            Act::Stop => {
                self.assistant.waiting = None;
                self.assistant.stopped = true;
                self.assistant.want(Focus::Send);
                self.assistant_say(tl!("You stopped waiting for the reply.").to_owned(), ctx);
            }
            Act::NewChat => self.assistant = AssistantState::fresh_for_document(&self.assistant),
            Act::Replace => self.assistant_apply("replaceSelection", ctx),
            Act::InsertAfter => self.assistant_apply("insertAfter", ctx),
            Act::Copy => {
                if let Some(reply) = &self.assistant.reply {
                    ctx.copy_text(reply.text.clone());
                    self.assistant_say(tl!("The reply was copied.").to_owned(), ctx);
                }
            }
            #[cfg(not(target_arch = "wasm32"))]
            Act::ForgetKey => {
                let said = if self.assistant_run.forget(true) {
                    wipe_key_history(ctx);
                    // The button that was pressed is gone: the keyboard goes to the key box.
                    self.assistant.want(Focus::Key);
                    tl!("The saved key was removed.")
                } else {
                    tl!("The saved key could not be removed.")
                };
                self.assistant_say(said.to_owned(), ctx);
            }
        }
    }

    /// Say something through the pane's live region.
    fn assistant_say(&mut self, text: String, ctx: &egui::Context) {
        self.assistant.say(text);
        ctx.request_repaint();
    }

    /// Gather what would be shared and make the provider. A provider that leaves the machine waits for the
    /// person's agreement, unless they have already agreed to this much in this run; every other request goes
    /// at once.
    fn assistant_ask(&mut self, ctx: &egui::Context) {
        if self.assistant.waiting.is_some() || self.assistant.ready.is_some() {
            return;
        }
        self.assistant.error = None;
        self.assistant.apply_error = None;
        self.assistant.stopped = false;
        // What was said about the last request is not true of this one.
        self.assistant.said.clear();
        self.assistant.alert_mark = !self.assistant.alert_mark;
        let instruction = self.assistant.prompt.trim().to_owned();
        if instruction.is_empty() {
            self.assistant.error = Some(Error::EmptyInstruction);
            return;
        }
        let scope = self.assistant.scope();
        let context = match self.run("assistant.context", json!({"scope": scope.as_str()})) {
            Ok(shared) => shared.get("text").and_then(|t| t.as_str()).unwrap_or_default().to_owned(),
            Err(_) => {
                self.assistant.error = Some(Error::NotStarted);
                return;
            }
        };
        // A reply to text that was selected is placed only while that selection is as it was. With nothing
        // selected the reply is not about any text, so there is nothing to guard.
        let basis =
            (scope == Scope::Selection && !context.trim().is_empty()).then(|| Basis { rev: self.session.rev(), selection: self.session.sel.clone() });
        let key = self.assistant_run.typed_key().map(Password::new);
        let provider = match (self.assistant.factory)(self.assistant_run.typed_key()) {
            Ok(provider) => provider,
            Err(error) => {
                self.assistant.error = Some(error);
                return;
            }
        };
        let request = Request { instruction, context, scope, history: self.assistant.history.clone() };
        // Too large or empty is found here, before anything is asked of the person or the network.
        if let Err(error) = build_body(&request, DEFAULT_MODEL) {
            self.assistant.error = Some(error);
            return;
        }
        let disclosure = provider.disclosure();
        let chars = request.context.chars().count();
        let ready = Ready { provider, request, disclosure, chars, basis, key };
        if needs_agreement(self.assistant_run.agreed, scope, ready.disclosure.leaves_machine) {
            self.assistant.ready = Some(ready);
            self.assistant.want(Focus::Agree);
        } else {
            self.assistant_send(ready, ctx);
        }
    }

    /// Send on a worker thread, so the window keeps drawing while the reply is written.
    fn assistant_send(&mut self, mut ready: Ready, ctx: &egui::Context) {
        let (tx, rx) = channel();
        let instruction = ready.request.instruction.clone();
        let basis = ready.basis.clone();
        let key = ready.key.clone();
        let repaint = ctx.clone();
        let started = std::thread::Builder::new().name("assistant".into()).spawn(move || {
            let result = ready.provider.send(&ready.request);
            let _ = tx.send(result);
            repaint.request_repaint();
        });
        match started {
            Ok(_) => {
                // The last reply is of the last request: it does not stay under this one's outcome.
                self.assistant.reply = None;
                self.assistant.waiting = Some(Waiting { rx, instruction, basis, key });
                self.assistant.want(Focus::Stop);
                self.assistant_say(tl!("Waiting for the reply…").to_owned(), ctx);
            }
            Err(_) => self.assistant.error = Some(Error::NotStarted),
        }
    }

    /// Take the reply of a finished request, or ask for another look soon while one is pending.
    pub(crate) fn assistant_poll(&mut self, ctx: &egui::Context) {
        let Some(waiting) = self.assistant.waiting.as_ref() else { return };
        let outcome = match waiting.rx.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Empty) => {
                ctx.request_repaint_after(Duration::from_millis(250));
                None
            }
            Err(TryRecvError::Disconnected) => Some(Err(Error::Interrupted)),
        };
        let Some(result) = outcome else { return };
        let instruction = waiting.instruction.clone();
        let basis = waiting.basis.clone();
        let key = waiting.key.clone();
        self.assistant.waiting = None;
        // Stop Waiting is gone: the keyboard focus it held goes to Send. A person who has moved on, to the
        // document for instance, is not pulled back.
        if std::mem::take(&mut self.assistant.stop_focused) {
            self.assistant.want(Focus::Send);
        } else {
            self.assistant.want_focus = Focus::None;
            self.assistant.focus_frames = 0;
        }
        match result {
            Ok(mut reply) => {
                // A declined reply and a blank one have no text to place, whatever else came with them.
                if matches!(reply.stop, Stop::Refusal { .. }) || reply.text.trim().is_empty() {
                    reply.text.clear();
                }
                if !reply.text.is_empty() {
                    self.assistant.history.push(Turn { role: Speaker::User, text: instruction });
                    self.assistant.history.push(Turn { role: Speaker::Assistant, text: reply.text.clone() });
                }
                let mut said = outcome_words(&reply);
                self.assistant.reply = Some(reply);
                self.assistant.basis = basis;
                // The service accepted the key: now it may be kept, if the person asked, and they are told.
                if self.assistant_run.keep_if_asked(key.as_ref()) {
                    said = format!("{said} {}", tl!("The key was saved on this computer."));
                }
                self.assistant_say(said, ctx);
            }
            // An error shows in an alert, which a screen reader reads out when it appears; what the status
            // line said while waiting is no longer true.
            Err(error) => {
                self.assistant.said.clear();
                self.assistant.error = Some(error);
            }
        }
    }

    /// Put the reply into the document through `assistant.apply`.
    fn assistant_apply(&mut self, mode: &str, ctx: &egui::Context) {
        let Some(text) = self.assistant.reply.as_ref().map(|reply| reply.text.clone()).filter(|text| !text.trim().is_empty()) else { return };
        self.assistant.apply_error = None;
        self.assistant.apply_mark = !self.assistant.apply_mark;
        // What was said about the last placement is not true of this one.
        self.assistant.said.clear();
        let current = Basis { rev: self.session.rev(), selection: self.session.sel.clone() };
        if self.assistant.basis.as_ref().is_some_and(|asked| *asked != current) {
            self.assistant.apply_error = Some(
                tl!("The document or the selection changed after the request, so the reply was not placed. Select the text again and ask again.")
                    .to_owned(),
            );
            return;
        }
        match self.run("assistant.apply", json!({"text": text, "mode": mode})) {
            Ok(_) => self.assistant_say(tl!("The reply was placed in the document.").to_owned(), ctx),
            Err(why) => self.assistant.apply_error = Some(fmt(tl!("The reply could not be placed: {why}"), &[("why", &why.to_string())])),
        }
    }
}

/// What a screen reader is told when a reply arrives: what it is, not only that it came.
fn outcome_words(reply: &Reply) -> String {
    match (&reply.stop, reply.text.is_empty()) {
        (Stop::Refusal { .. }, _) => tl!("The model declined this request, so there is no reply to place."),
        (Stop::MaxTokens, true) => tl!("The reply ran out of length before any text was written. Select less text."),
        (Stop::MaxTokens, false) => tl!("The AI reply is ready, but it was cut off at its length limit."),
        (Stop::Other(_), _) => tl!("The AI reply is ready, but it may be incomplete."),
        (Stop::End, true) => tl!("The model sent no text, so there is no reply to place."),
        (Stop::End, false) => tl!("The AI reply is ready."),
    }
    .to_owned()
}

/// What a failed request says, in words for the person and never with the key.
fn error_text(error: &Error) -> String {
    // The size is rounded up and the limit is a whole number of KB, so the two never read as equal.
    let size_kb = |bytes: usize| bytes.div_ceil(1000).to_string();
    let limit_kb = |bytes: usize| (bytes / 1000).to_string();
    match error {
        Error::MissingKey => {
            tl!("No API key. Type one in the API key box, or set the ANTHROPIC_API_KEY environment variable and start WordCraft again.").to_owned()
        }
        Error::EmptyInstruction => tl!("Write what you want first.").to_owned(),
        Error::TooLarge { what: "the instruction", bytes, limit } => {
            fmt(tl!("The instruction is too long ({size} KB; the most is {limit} KB)."), &[("size", &size_kb(*bytes)), ("limit", &limit_kb(*limit))])
        }
        Error::TooLarge { bytes, limit, .. } => fmt(
            tl!("The text is too large to send ({size} KB; the most is {limit} KB). Select less text."),
            &[("size", &size_kb(*bytes)), ("limit", &limit_kb(*limit))],
        ),
        Error::Network(why) => fmt(tl!("The service could not be reached: {why}"), &[("why", why)]),
        Error::Http { status: 401, .. } => {
            tl!("The API key was not accepted. Check the key you typed, or the ANTHROPIC_API_KEY environment variable.").to_owned()
        }
        Error::Http { status: 403, message, .. } if !message.is_empty() => {
            fmt(tl!("The service did not allow this request: {message}"), &[("message", message)])
        }
        Error::Http { status: 403, .. } => tl!("The service did not allow this request.").to_owned(),
        Error::Http { status: 429, message, .. } if !message.is_empty() => {
            fmt(tl!("The service is busy, or the usage limit was reached ({message}). Try again later."), &[("message", message)])
        }
        Error::Http { status: 429, .. } => tl!("The service is busy, or the usage limit was reached. Try again later.").to_owned(),
        Error::Http { status, .. } if *status >= 500 => {
            fmt(tl!("The service had a problem ({status}). Try again."), &[("status", &status.to_string())])
        }
        Error::Http { status, message, .. } => {
            fmt(tl!("The service refused the request ({status}): {message}"), &[("status", &status.to_string()), ("message", message)])
        }
        Error::BadReply(why) => fmt(tl!("The reply could not be read: {why}"), &[("why", why)]),
        Error::Unavailable => tl!("The AI assistant is not available in this version of WordCraft.").to_owned(),
        Error::NotStarted => tl!("The request could not be started. Nothing was sent. Try again.").to_owned(),
        Error::Interrupted => tl!("The request ended without an answer. Try again.").to_owned(),
    }
}

/// Where the key comes from and whether it is kept, said before the person writes anything.
#[cfg(not(target_arch = "wasm32"))]
fn key_note(run: &AssistantRun) -> String {
    if run.on_disk.is_some() && run.on_disk.as_ref().map(|k| k.as_str()) == run.typed_key() {
        return tl!("This key is saved on this computer, in WordCraft's settings folder, unencrypted.").to_owned();
    }
    if run.typed_key().is_some() {
        return if run.remember {
            tl!("This key will be saved on this computer once it works, in WordCraft's settings folder, unencrypted.").to_owned()
        } else {
            tl!("The key you typed is used only in this run. It is not saved.").to_owned()
        };
    }
    if wordcraft_assistant::claude::Claude::key_present() {
        return tl!("Using the key in the ANTHROPIC_API_KEY environment variable.").to_owned();
    }
    tl!("No API key yet. Type one in the API key box, or set the ANTHROPIC_API_KEY environment variable and start WordCraft again.").to_owned()
}

/// The browser version cannot send, so there is no key to speak of.
#[cfg(target_arch = "wasm32")]
fn key_note(_: &AssistantRun) -> String {
    tl!("The AI assistant is not available in the browser version.").to_owned()
}

/// Why the saved key could not be used, if that is so.
#[cfg(not(target_arch = "wasm32"))]
fn file_problem_text(run: &AssistantRun) -> Option<String> {
    use wordcraft_assistant::key_store::KeyFileError;
    Some(match run.file_problem.as_ref()? {
        KeyFileError::TooOpen => {
            tl!("The saved key was not used because other users can read its file. Remove the file, or type the key again.").to_owned()
        }
        KeyFileError::NotAKey => tl!("The file with the saved key does not hold a key, so it was not used.").to_owned(),
        KeyFileError::Io(_) => tl!("The saved key could not be read, written or removed.").to_owned(),
    })
}

/// The start of the shared text on one line, for the person to see what goes before it goes.
fn preview(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = flat.chars();
    let head: String = chars.by_ref().take(PREVIEW_CHARS).collect();
    if chars.next().is_some() { format!("{head}…") } else { head }
}

/// Draws the pane's body.
pub fn show(app: &mut WordApp, ui: &mut Ui) {
    app.assistant_poll(ui.ctx());
    let mut act = None;
    body(app, ui, &mut act);
    if let Some(act) = act {
        let ctx = ui.ctx().clone();
        app.assistant_act(act, &ctx);
    }
}

/// Muted explanatory text.
fn muted(ui: &mut Ui, text: impl Into<String>) {
    ui.label(RichText::new(text.into()).weak());
}

/// A boxed message. A warning or an error also starts with its word, so colour is not the only signal, and an
/// error is an alert; any notice can be a live region.
fn notice(ui: &mut Ui, tone: Tone, text: &str, live: Option<(Live, bool)>) {
    let color = match tone {
        Tone::Info => ui.visuals().widgets.noninteractive.fg_stroke.color,
        Tone::Warn => ui.visuals().warn_fg_color,
        Tone::Bad => ui.visuals().error_fg_color,
    };
    let text = match tone {
        Tone::Info => text.to_owned(),
        Tone::Warn => format!("{}: {text}", tl!("Warning")),
        Tone::Bad => format!("{}: {text}", tl!("Error")),
    };
    Frame::new().stroke(Stroke::new(1.0, color)).inner_margin(Margin::same(6)).corner_radius(4).show(ui, |ui| {
        let label = ui.add(egui::Label::new(text.clone()).wrap());
        // `repeat` puts an invisible mark on the spoken text, so an alert that is the same as the last one is
        // still announced.
        if let Some((live, repeat)) = live {
            let spoken = if repeat { format!("{text}\u{200b}") } else { text.clone() };
            live_region(ui, label.id, &spoken, live);
        }
    });
}

/// Make the label `id` a live region that screen readers read out when it appears or changes: an alert when
/// it is urgent, else a status. egui describes a label by its value; a live region is read by its name.
fn live_region(ui: &Ui, id: egui::Id, text: &str, live: Live) {
    ui.ctx().accesskit_node_builder(id, |node| {
        node.set_live(live);
        node.set_role(if live == Live::Assertive { Role::Alert } else { Role::Status });
        node.set_label(text);
        node.clear_value();
    });
}

/// A button that takes the keyboard focus when the pane has asked for it.
fn button(ui: &mut Ui, app: &mut WordApp, label: &str, focus: Focus, enabled: bool) -> bool {
    let response = ui.add_enabled(enabled, egui::Button::new(label));
    if enabled {
        focus_if_asked(ui, app, &response, focus);
    }
    if focus == Focus::Stop {
        // egui's own focus, not the window's: a window that is not in front still holds its keyboard focus.
        app.assistant.stop_focused = ui.ctx().memory(|m| m.has_focus(response.id));
    }
    response.clicked()
}

/// Give the keyboard focus to `response` when the pane has asked for `focus`.
fn focus_if_asked(ui: &Ui, app: &mut WordApp, response: &egui::Response, focus: Focus) {
    if app.assistant.want_focus != focus {
        return;
    }
    app.assistant.focus_frames = app.assistant.focus_frames.saturating_sub(1);
    if app.assistant.focus_frames == 0 {
        app.assistant.want_focus = Focus::None;
    }
    response.request_focus();
    // The page asks for the keyboard after a command; this control asked later.
    app.canvas.want_focus = false;
    ui.ctx().request_repaint();
}

fn body(app: &mut WordApp, ui: &mut Ui, act: &mut Option<Act>) {
    muted(ui, tl!("Ask for a change to your text, or a question about it. A reply is placed only when you choose."));
    if !CAN_SEND {
        // Nothing here can send: say so and offer nothing that looks as if it could.
        notice(ui, Tone::Info, &key_note(&app.assistant_run), None);
        return;
    }
    let busy = app.assistant.waiting.is_some() || app.assistant.ready.is_some();
    muted(ui, key_note(&app.assistant_run));
    ui.add_space(2.0);
    let label = ui.label(tl!("API key:"));
    let key_box = ui
        .add_enabled(!busy, TextEdit::singleline(app.assistant_run.key.as_mut_string()).password(true).desired_width(f32::INFINITY).id(key_box_id()))
        .labelled_by(label.id);
    if !busy {
        focus_if_asked(ui, app, &key_box, Focus::Key);
    }
    keep_controls(app, ui, busy, act);
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(problem) = file_problem_text(&app.assistant_run) {
        notice(ui, Tone::Warn, &problem, Some((Live::Polite, false)));
        // Which file it is, so the person can act on what the notice says.
        if let Some(file) = &app.assistant_run.file {
            ui.add(egui::Label::new(RichText::new(file.path().display().to_string()).weak()).selectable(true).wrap());
        }
    }
    ui.add_space(4.0);
    ui.add_enabled(!busy, egui::Checkbox::new(&mut app.assistant.document_scope, tl!("Use the whole document instead of the selection")));
    ui.add_space(4.0);
    let label = ui.label(tl!("Instruction:"));
    ui.add_enabled(
        !busy,
        TextEdit::multiline(&mut app.assistant.prompt)
            .desired_rows(4)
            .desired_width(f32::INFINITY)
            .char_limit(MAX_INSTRUCTION_BYTES)
            .hint_text(tl!("For example: make this paragraph shorter"))
            .id_salt("assistant.prompt"),
    )
    .labelled_by(label.id);
    let waiting = app.assistant.waiting.is_some();
    let can_send = !busy && !app.assistant.prompt.trim().is_empty();
    let (mut send, mut new_chat, mut stop) = (false, false, false);
    ui.horizontal_wrapped(|ui| {
        send = button(ui, app, tl!("Send"), Focus::Send, can_send);
        if waiting {
            stop = button(ui, app, tl!("Stop Waiting"), Focus::Stop, true);
        }
        new_chat = ui.add_enabled(!busy, egui::Button::new(tl!("New Chat"))).clicked();
    });
    for (clicked, what) in [(send, Act::Send), (stop, Act::Stop), (new_chat, Act::NewChat)] {
        if clicked {
            *act = Some(what);
        }
    }
    status_line(app, ui);
    if app.assistant.stopped {
        notice(ui, Tone::Info, tl!("You stopped waiting. The request was already sent, and the service may still process it."), None);
    }
    consent(app, ui, act);
    if let Some(error) = &app.assistant.error {
        notice(ui, Tone::Bad, &error_text(error), Some((Live::Assertive, app.assistant.alert_mark)));
    }
    reply(app, ui, act);
}

/// The status line: a live region, so what happens away from the keyboard is spoken.
fn status_line(app: &WordApp, ui: &mut Ui) {
    if app.assistant.said.is_empty() {
        return;
    }
    let label = ui.add(egui::Label::new(RichText::new(app.assistant.said.clone()).weak()).wrap());
    live_region(ui, label.id, &app.assistant.said, Live::Polite);
}

/// The options for keeping the key on this computer.
#[cfg(not(target_arch = "wasm32"))]
fn keep_controls(app: &mut WordApp, ui: &mut Ui, busy: bool, act: &mut Option<Act>) {
    if !app.assistant_run.can_keep() {
        return;
    }
    let before = app.assistant_run.remember;
    if !before {
        // Said before the checkbox, so it is read before the person can turn it on.
        muted(ui, tl!("If you choose to remember the key, it is kept unencrypted in a file in WordCraft's settings folder."));
    }
    ui.add_enabled(!busy, egui::Checkbox::new(&mut app.assistant_run.remember, tl!("Remember this key on this computer")));
    if before && !app.assistant_run.remember && app.assistant_run.on_disk.is_some() && !app.assistant_run.forget(false) {
        // Turning it off removes the saved copy, and the key stays for this run. If the copy could not be
        // removed it is still there, so the box stays ticked.
        app.assistant_run.remember = true;
    }
    if app.assistant_run.on_disk.is_some() && ui.add_enabled(!busy, egui::Button::new(tl!("Forget Saved Key"))).clicked() {
        *act = Some(Act::ForgetKey);
    }
}

#[cfg(target_arch = "wasm32")]
fn keep_controls(_: &mut WordApp, _: &mut Ui, _: bool, _: &mut Option<Act>) {}

/// Who gets the text, said before it is sent, with the buttons that agree or decline. It describes the request
/// that is waiting, not the controls above it, which cannot change while it waits.
fn consent(app: &mut WordApp, ui: &mut Ui, act: &mut Option<Act>) {
    let Some(ready) = &app.assistant.ready else { return };
    let note = if ready.chars == 0 {
        let sentence = match ready.request.scope {
            Scope::Selection => tl!(
                "Your instruction (no document text goes with it: nothing is selected) will be sent over the internet to {recipient}, to be read by the model {model}. WordCraft keeps no copy. Nothing is sent until you agree."
            ),
            Scope::Document => tl!(
                "Your instruction (no document text goes with it: the document is empty) will be sent over the internet to {recipient}, to be read by the model {model}. WordCraft keeps no copy. Nothing is sent until you agree."
            ),
        };
        fmt(sentence, &[("recipient", &ready.disclosure.recipient), ("model", &ready.disclosure.model)])
    } else {
        let what = match ready.request.scope {
            Scope::Document => tl!("the whole document"),
            Scope::Selection => tl!("the selected text"),
        };
        fmt(
            tl!(
                "Your instruction and {what} ({chars} characters) will be sent over the internet to {recipient}, to be read by the model {model}. WordCraft keeps no copy. Nothing is sent until you agree."
            ),
            &[("what", what), ("chars", &ready.chars.to_string()), ("recipient", &ready.disclosure.recipient), ("model", &ready.disclosure.model)],
        )
    };
    let start = (ready.chars > 0).then(|| fmt(tl!("The text starts: {start}"), &[("start", &preview(&ready.request.context))]));
    // What agreeing covers, as the pane really treats it: the whole document covers a selection too, a
    // selection does not cover the whole document.
    let covers_document = ready.request.scope == Scope::Document || app.assistant_run.agreed == Some(Scope::Document);
    let covers = if covers_document {
        tl!("Agreeing covers later requests in this run, whether they send a selection or the whole document.")
    } else {
        tl!("Agreeing covers later requests in this run that send a selection. A request that sends the whole document asks again.")
    };
    // Everything the decision rests on is one announcement, said when it appears, so a screen reader reads
    // the whole of it before the person decides.
    let mut lines = vec![note];
    lines.extend(start);
    if ready.disclosure.fallback_possible {
        lines.push(tl!("If the model declines, the service may have another of its models read the text instead.").to_owned());
    }
    lines.push(covers.to_owned());
    notice(ui, Tone::Info, &lines.join("\n"), Some((Live::Assertive, false)));
    let (mut agree, mut cancel) = (false, false);
    ui.horizontal_wrapped(|ui| {
        agree = button(ui, app, tl!("Agree and Send"), Focus::Agree, true);
        cancel = ui.button(tl!("Cancel")).clicked();
    });
    if agree {
        *act = Some(Act::Agree);
    } else if cancel {
        *act = Some(Act::Cancel);
    }
}

/// The reply, what ended it, and the buttons that place it.
fn reply(app: &mut WordApp, ui: &mut Ui, act: &mut Option<Act>) {
    let Some(reply) = &app.assistant.reply else { return };
    ui.add_space(6.0);
    ui.label(RichText::new(tl!("Reply")).font(semibold(14.0)));
    match (&reply.stop, reply.text.is_empty()) {
        (Stop::Refusal { .. }, _) => {
            notice(ui, Tone::Warn, tl!("The model declined this request, so there is no reply to place."), None);
        }
        (Stop::MaxTokens, true) => {
            notice(ui, Tone::Warn, tl!("The reply ran out of length before any text was written. Select less text."), None);
        }
        (Stop::MaxTokens, false) => {
            notice(ui, Tone::Warn, tl!("The reply was cut off at its length limit. Check it before you place it."), None);
        }
        (Stop::Other(why), _) if why.is_empty() => {
            notice(ui, Tone::Warn, tl!("The reply may be incomplete: the service did not say why it ended."), None);
        }
        (Stop::Other(why), _) => {
            notice(ui, Tone::Warn, &fmt(tl!("The reply may be incomplete (the service ended it with: {why})."), &[("why", why)]), None);
        }
        (Stop::End, true) => {
            notice(ui, Tone::Warn, tl!("The model sent no text, so there is no reply to place."), None);
        }
        (Stop::End, false) => {}
    }
    if !reply.text.is_empty() {
        ui.add(egui::Label::new(reply.text.clone()).selectable(true).wrap());
        ui.horizontal_wrapped(|ui| {
            if ui.button(tl!("Replace Selection")).clicked() {
                *act = Some(Act::Replace);
            }
            if ui.button(tl!("Insert After")).clicked() {
                *act = Some(Act::InsertAfter);
            }
            if ui.button(tl!("Copy")).clicked() {
                *act = Some(Act::Copy);
            }
        });
    }
    if let Some(why) = &app.assistant.apply_error {
        notice(ui, Tone::Bad, why, Some((Live::Assertive, app.assistant.apply_mark)));
    }
    let usage = fmt(
        tl!("{model}: {input} tokens in, {output} tokens out"),
        &[("model", &reply.model), ("input", &reply.usage.input_tokens.to_string()), ("output", &reply.usage.output_tokens.to_string())],
    );
    muted(ui, usage);
    if reply.fallback_used {
        muted(ui, tl!("The first model declined, and a fallback model wrote this reply."));
    }
}

#[cfg(test)]
mod tests;
