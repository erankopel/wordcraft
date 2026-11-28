//! The AI pane: ask for a change to the selection or the document, or a question about it. The assistant
//! either makes the changes itself with WordCraft's commands (all of one request's changes apply together, and
//! one Undo reverts them; see [`wordcraft_engine::agent`]), or answers with text the person places.
//!
//! Text leaves the machine only through a provider, and only after the person agrees here, once: before the
//! first request the pane says who receives the text, which model reads it, everything a request may send in
//! either way of working, how many characters this one sends and how its text starts. The agreement is kept
//! with the pane's choices ([`saved`]), so later requests, documents and runs send without asking, each on the
//! person's own Send, until the person withdraws it under Settings. No command and no setting can give it; only
//! the button in the pane can. A reply is placed with `assistant.apply`, so it is one undo step and tracked
//! when Track Changes is on. Reading what would be shared (`assistant.context`) and placing a reply
//! (`assistant.apply`) are engine commands; sending is this pane's alone, because an agent that drives
//! WordCraft brings its own model. See `docs/assistant.md`.
//!
//! Until the one-time setup is done (a key and the agreement) the pane shows it: an introduction, the key and
//! the options. Then it shows only the conversation and a Settings button, which shows that part in place.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::Duration;

use egui::accesskit::{Live, Role};
use egui::{Frame, Margin, RichText, Stroke, TextEdit, Ui};
use serde_json::{Value, json};
use wordcraft_assistant::agent as conversation;
use wordcraft_assistant::{
    DEFAULT_MODEL, Disclosure, Error, MAX_INSTRUCTION_BYTES, Provider, Reply, Request, Scope, Shared, Stop, Turn, Usage, build_body, history_start,
};
use wordcraft_engine::Password;
use wordcraft_engine::agent as workspace;

use crate::WordApp;
use crate::i18n::fmt;
use crate::theme::semibold;

mod saved;
use saved::{Agreed, Choice, Choices};

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

/// A request in which the assistant changes the document itself: what it is given besides the instruction and
/// the document (the workspace is made from the document when the request is sent, and the selected text the
/// model is told about is read there, so the two agree).
struct EditJob {
    /// The earlier turns of this chat, oldest first.
    history: Vec<Turn>,
}

/// A request that is ready to go and waits for the person's agreement.
struct Ready {
    provider: Box<dyn Provider>,
    /// The instruction and what is shared. For changes made by the assistant, the whole document (its tools can
    /// read all of it), which the agreement describes.
    request: Request,
    /// Set when the assistant makes the changes itself.
    edit: Option<EditJob>,
    disclosure: Disclosure,
    /// How many characters of the document go with it.
    chars: usize,
    /// For a request about the selection: what it was asked about.
    basis: Option<Basis>,
    /// The key the request is made with, if the person typed or saved one: it is the one that may be kept
    /// when the service accepts it, whatever the key box holds by then.
    key: Option<Password>,
}

/// What the worker thread tells the pane.
enum Msg {
    /// A text request's reply.
    Reply(Result<Reply, Error>),
    /// What a request that changes the document is doing.
    Progress(conversation::Progress),
    /// How a request that changes the document ended: an error comes with what the rounds before it used.
    Edited(Result<Edited, conversation::Failed>),
}

/// The end of a request that changes the document: how the conversation ended, and the steps to apply.
struct Edited {
    outcome: conversation::Outcome,
    steps: Vec<Value>,
    changed: bool,
    /// A limit of the request (tool calls, commands, changes) stopped the assistant: its work may be partial.
    limited: bool,
    /// Replaying the steps did not give what the assistant worked on: they must not be applied.
    unrepeatable: bool,
}

/// The document the assistant's changes were made on, which they may be applied to only while it is unchanged.
struct EditBase {
    document: u64,
    /// Its content when the request began (sharing its storage with the open document).
    doc: wordcraft_engine::doc::Document,
    /// Track Changes was on: the changes were made, and are applied, tracked or not accordingly.
    tracking: bool,
    /// The name the assistant's tracked changes carry.
    author: String,
    /// The selection and the view the assistant started from, which the changes are replayed from, wherever
    /// the person has clicked since.
    start: Value,
}

/// Runs the assistant's tool calls on its workspace.
struct Host<'a>(&'a mut workspace::Toolbox);

impl conversation::ToolHost for Host<'_> {
    fn call(&mut self, name: &str, input: &Value) -> (String, bool) {
        let result = self.0.call(name, input);
        (result.content, result.is_error)
    }
}

/// A request in flight on its worker thread.
struct Waiting {
    rx: Receiver<Msg>,
    instruction: String,
    /// For a text request: the document text that went with the instruction, which the chat sends with it again.
    shared: Option<Shared>,
    basis: Option<Basis>,
    key: Option<Password>,
    /// Set when the request is let go (Stop Waiting, New Chat, another document): its worker stops before its
    /// next step.
    cancel: Arc<AtomicBool>,
    /// For a request that changes the document: the document it works on.
    edit: Option<EditBase>,
    /// The round the model is in, for the progress line.
    round: usize,
    /// Tool calls made so far.
    calls: usize,
}

impl Drop for Waiting {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
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
    /// The instruction box.
    Prompt,
    /// The Settings button.
    Settings,
}

/// What the last request used, as far as the pane knows: Settings says it.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Spent {
    /// The request ended with an answer: the tokens of all its rounds, and the model that answered.
    All { model: String, usage: Usage },
    /// It failed after some of its rounds were answered: what those used (the service counts them all the same).
    Before { model: String, usage: Usage },
    /// It got no answer, or the person stopped waiting for it: what it used is not known.
    Unknown,
}

/// How strongly a notice speaks, and so how it is drawn and announced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tone {
    Info,
    Warn,
    Bad,
}

/// What the person chose, agreed to and typed, for the whole run of the program, in every window: a second
/// window does not ask again, and the key is not typed again. The choices and the agreement are kept between
/// runs when the app gives a settings folder ([`saved`]); the key is kept on this computer only when the person
/// asks for that, in a file only they can read ([`wordcraft_assistant::key_store`]).
#[derive(Default)]
pub(crate) struct AssistantRun {
    /// How the assistant works, and what the person agreed to.
    choices: Choices,
    /// Where the choices are kept. `None`: nowhere, so they last for the run.
    #[cfg(not(target_arch = "wasm32"))]
    choices_file: Option<std::path::PathBuf>,
    /// The choices whose last change could not be written: they last for the run.
    unsaved: Vec<Choice>,
    /// The service answered a request made in this run (an answer that is not an authentication error), so
    /// the key works.
    accepted: bool,
    /// The service refused the key (an authentication error): the setup shows again.
    rejected: bool,
    /// The key in memory was read from the saved file, or saved there in this run: it needs no typing.
    from_file: bool,
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

    /// Is the one-time setup done, so the pane shows the conversation and keeps the rest under Settings? It is
    /// when a key needs no typing (a saved key that could be read, or `ANTHROPIC_API_KEY`) and the person has
    /// agreed, and once the service has accepted the key in this run. A refused key, or a saved key file with
    /// a problem, brings the setup back with its message.
    fn setup_done(&self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        if self.file_problem.is_some() {
            return false;
        }
        !self.rejected && (self.accepted || (self.choices.agreed.is_some() && (self.from_file || env_key())))
    }

    /// What an ended request said about the key: an answer, even an error other than an authentication error,
    /// means the service accepted it; an authentication error means it refused it. A request that got no
    /// answer says nothing.
    fn note_answer(&mut self, result: Result<(), &Error>) {
        let accepted = match result {
            Ok(()) => true,
            Err(Error::Http { status: 401, .. }) => false,
            Err(Error::Http { kind, .. }) if kind == "authentication_error" => false,
            Err(Error::Http { .. }) => true,
            Err(_) => return,
        };
        self.accepted = accepted;
        self.rejected = !accepted;
    }

    /// Keep `which` of the choices where the app said, if it said, over what is kept there now. A failure is said
    /// in the pane, outside Settings ([`unsaved_notices`]), and that choice lasts for the run.
    fn save_choices(&mut self, which: Choice) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(path) = &self.choices_file {
            let failed = saved::save(path, &self.choices, which).is_err();
            self.unsaved.retain(|choice| *choice != which);
            if failed {
                self.unsaved.push(which);
            }
        }
        #[cfg(target_arch = "wasm32")]
        let _ = which;
    }

    /// Read the agreement again where the choices are kept: another WordCraft process may have given or
    /// withdrawn it since this one read it. An agreement, or a withdrawal, this run could not save stays as it is
    /// in memory.
    fn refresh_agreement(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(path) = &self.choices_file
            && !self.unsaved.contains(&Choice::Agreement)
            && let Some(kept) = saved::load(path)
        {
            self.choices.agreed = kept.agreed;
        }
    }

    /// Are the choices kept between runs? Not when the last attempt to write one failed.
    #[cfg(not(target_arch = "wasm32"))]
    fn choices_kept(&self) -> bool {
        self.choices_file.is_some() && self.unsaved.is_empty()
    }

    #[cfg(target_arch = "wasm32")]
    fn choices_kept(&self) -> bool {
        false
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
                self.from_file = true;
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
                // The key that worked is gone: the setup shows again, unless the environment has one.
                self.key = Password::default();
                self.from_file = false;
                self.accepted = false;
            }
        }
        removed
    }
}

/// Is there a key in `ANTHROPIC_API_KEY`? The key itself is not read out. The browser version has none.
#[cfg(not(target_arch = "wasm32"))]
fn env_key() -> bool {
    wordcraft_assistant::claude::Claude::key_present()
}

#[cfg(target_arch = "wasm32")]
fn env_key() -> bool {
    false
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
    prompt: String,
    /// Settings (the key and the options, once the setup is done) is open.
    settings_open: bool,
    /// Whether the setup showed when the pane was last drawn: `None` before it was drawn.
    shown_setup: Option<bool>,
    /// The controls of the setup part drawn last: a keyboard focus on one of them when it goes moves on.
    setup_ids: Vec<egui::Id>,
    /// Whether the keyboard focus was on the Settings button when it was last drawn.
    settings_focused: bool,
    /// What the last request used, for Settings.
    last_usage: Option<Spent>,
    /// The earlier turns of this chat, oldest first.
    history: Vec<Turn>,
    reply: Option<Reply>,
    /// The reply ends a request that changed the document (no text to place, perhaps changes to undo).
    edit_result: bool,
    /// The assistant's last changes were applied: the document and where its undo history stood right after.
    /// Undo These Changes is offered while the history stands there, so it undoes those changes and no
    /// others.
    applied: Option<(u64, u64)>,
    /// A limit of the request stopped the assistant before it finished: its changes so far were applied.
    partial: bool,
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
            prompt: String::new(),
            settings_open: false,
            shown_setup: None,
            setup_ids: Vec::new(),
            settings_focused: false,
            last_usage: None,
            history: Vec::new(),
            reply: None,
            edit_result: false,
            applied: None,
            partial: false,
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
    /// A new chat for a new document: the conversation goes, and how the pane shows stays. (What the person
    /// chose, agreed to and typed as the key is [`AssistantRun`]'s, so it stays anyway.)
    pub(crate) fn fresh_for_document(old: &AssistantState) -> AssistantState {
        AssistantState {
            factory: old.factory,
            settings_open: old.settings_open,
            shown_setup: old.shown_setup,
            // A request still running is let go, and what it used will not be known: the use shown is not the
            // request's before it.
            last_usage: if old.waiting.is_some() { Some(Spent::Unknown) } else { old.last_usage.clone() },
            ..AssistantState::default()
        }
    }

    /// Add an instruction and its reply to the chat, and let go the turns no later request sends
    /// ([`wordcraft_assistant::history_start`]): with the document text an instruction carried, they would only
    /// take memory. They go as the requests let them go, a run of exchanges at a time, not one exchange with
    /// every request, so the requests after a cut start with the same turns and the service reads them from
    /// its prompt cache.
    fn add_exchange(&mut self, instruction: Turn, reply: Turn) {
        self.history.push(instruction);
        self.history.push(reply);
        let start = history_start(&self.history);
        self.history.drain(..start);
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
}

impl Choices {
    /// How much of the document a text answer shares.
    fn scope(&self) -> Scope {
        if self.whole_document { Scope::Document } else { Scope::Selection }
    }
}

impl WordApp {
    /// Are the assistant's last changes the newest step of the open document's undo history?
    fn assistant_changes_on_top(&self) -> bool {
        self.assistant.applied == Some((self.session.document_id(), self.session.undo_mark())) && self.session.undo_label() == Some(ASSISTANT_CHANGES)
    }
}

/// Must the person agree before a request goes to the provider `disclosure` describes? Only when the text
/// leaves the machine and the person has not agreed to this recipient and model.
fn needs_agreement(agreed: Option<&Agreed>, disclosure: &Disclosure) -> bool {
    disclosure.leaves_machine && !agreed.is_some_and(|a| a.recipient == disclosure.recipient && a.model == disclosure.model)
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
    UndoEdits,
    /// Show or hide Settings.
    Settings,
    /// Withdraw the agreement: the next request asks again.
    Withdraw,
    #[cfg(not(target_arch = "wasm32"))]
    ForgetKey,
}

impl WordApp {
    /// Keep the API key in `folder` (the settings folder) when the person asks for that, and use the key
    /// saved there by an earlier run; and keep the pane's choices and the person's agreement there
    /// ([`saved::FILE_NAME`]), reading what an earlier run kept. Called once, by the desktop app, after its
    /// settings are loaded, and only when it keeps settings.
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
                run.from_file = true;
            }
            Ok(None) => {}
            Err(problem) => run.file_problem = Some(problem),
        }
        run.file = Some(file);
        let choices = folder.join(saved::FILE_NAME);
        if let Some(saved) = saved::load(&choices) {
            run.choices = saved;
        }
        run.choices_file = Some(choices);
    }

    /// Carry out what a click asked for.
    pub(crate) fn assistant_act(&mut self, act: Act, ctx: &egui::Context) {
        match act {
            Act::Send => self.assistant_ask(ctx),
            Act::Agree => {
                if let Some(ready) = self.assistant.ready.take() {
                    // Once for every later request, document and run, until it is withdrawn.
                    let run = &mut self.assistant_run;
                    run.choices.agreed = Some(Agreed { recipient: ready.disclosure.recipient.clone(), model: ready.disclosure.model.clone() });
                    run.save_choices(Choice::Agreement);
                    self.assistant_send(ready, ctx);
                }
            }
            Act::Settings => self.assistant.settings_open = !self.assistant.settings_open,
            Act::Withdraw => {
                self.assistant_run.choices.agreed = None;
                self.assistant_run.save_choices(Choice::Agreement);
                // The button that was pressed is gone: the keyboard goes to Settings, or to the instruction box
                // when the setup shows again (Settings is part of it then).
                self.assistant.want(if self.assistant_run.setup_done() { Focus::Settings } else { Focus::Prompt });
                self.assistant_say(tl!("Your agreement was withdrawn. The next request asks for it again before anything is sent.").to_owned(), ctx);
            }
            Act::Cancel => {
                self.assistant.ready = None;
                self.assistant.want(Focus::Send);
            }
            Act::Stop => {
                let editing = self.assistant.waiting.as_ref().is_some_and(|w| w.edit.is_some());
                // Letting the request go stops its worker before its next step.
                self.assistant.waiting = None;
                self.assistant.stopped = true;
                // Its answer, and what it used, will not be read: Settings does not show the request before as this one.
                self.assistant.last_usage = Some(Spent::Unknown);
                self.assistant.want(Focus::Send);
                let said = if editing { tl!("You stopped the assistant. Nothing was changed.") } else { tl!("You stopped waiting for the reply.") };
                self.assistant_say(said.to_owned(), ctx);
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
            Act::UndoEdits => {
                if self.assistant_changes_on_top() {
                    self.assistant.applied = None;
                    // The button that was pressed is gone: the keyboard goes to Send, or to the instruction box
                    // when there is no instruction to send.
                    self.assistant.want(if self.assistant.prompt.trim().is_empty() { Focus::Prompt } else { Focus::Send });
                    match self.run("edit.undo", json!({})) {
                        Ok(_) => self.assistant_say(tl!("The assistant's changes were undone.").to_owned(), ctx),
                        Err(why) => self.assistant.apply_error = Some(why),
                    }
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
    /// person's agreement, unless they have agreed already (once, for every request); every other request goes
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
        if self.assistant_run.choices.edits {
            self.assistant_ask_edits(instruction, ctx);
            return;
        }
        let scope = self.assistant_run.choices.scope();
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
        let Some(provider) = self.assistant_provider() else { return };
        let request = Request { instruction, context, scope, history: self.assistant.history.clone() };
        // Too large or empty is found here, before anything is asked of the person or the network.
        if let Err(error) = build_body(&request, DEFAULT_MODEL) {
            self.assistant.error = Some(error);
            return;
        }
        let disclosure = provider.disclosure();
        let chars = request.context.chars().count();
        let ready = Ready { provider, request, edit: None, disclosure, chars, basis, key };
        self.assistant_agree_or_send(ready, ctx);
    }

    /// The provider for a request, made with the key typed or saved; `None` when it cannot be made, and the pane
    /// then says why. With no key at all, the key box holds no saved key any more (the person cleared or changed
    /// it) and no key was accepted, so the setup, with the key box, shows again.
    fn assistant_provider(&mut self) -> Option<Box<dyn Provider>> {
        match (self.assistant.factory)(self.assistant_run.typed_key()) {
            Ok(provider) => Some(provider),
            Err(error) => {
                if error == Error::MissingKey {
                    self.assistant_run.accepted = false;
                    self.assistant_run.from_file = false;
                }
                self.assistant.error = Some(error);
                None
            }
        }
    }

    /// Send `ready`, or hold it for the person's agreement when it needs one.
    fn assistant_agree_or_send(&mut self, ready: Ready, ctx: &egui::Context) {
        self.assistant_run.refresh_agreement();
        if needs_agreement(self.assistant_run.choices.agreed.as_ref(), &ready.disclosure) {
            self.assistant.ready = Some(ready);
            self.assistant.want(Focus::Agree);
        } else {
            self.assistant_send(ready, ctx);
        }
    }

    /// Prepare a request in which the assistant changes the document itself. Its tools can read all of the
    /// document, which the agreement covers.
    fn assistant_ask_edits(&mut self, instruction: String, ctx: &egui::Context) {
        let context = match self.run("assistant.context", json!({"scope": "document"})) {
            Ok(shared) => shared.get("text").and_then(|t| t.as_str()).unwrap_or_default().to_owned(),
            Err(_) => {
                self.assistant.error = Some(Error::NotStarted);
                return;
            }
        };
        if instruction.len() > MAX_INSTRUCTION_BYTES {
            self.assistant.error = Some(Error::TooLarge { what: "the instruction", bytes: instruction.len(), limit: MAX_INSTRUCTION_BYTES });
            return;
        }
        let key = self.assistant_run.typed_key().map(Password::new);
        let Some(provider) = self.assistant_provider() else { return };
        let history = self.assistant.history.clone();
        let request = Request { instruction, context, scope: Scope::Document, history: history.clone() };
        let disclosure = provider.disclosure();
        let chars = request.context.chars().count();
        let ready = Ready { provider, request, edit: Some(EditJob { history }), disclosure, chars, basis: None, key };
        self.assistant_agree_or_send(ready, ctx);
    }

    /// Send on a worker thread, so the window keeps drawing while the reply is written. A request that changes
    /// the document runs its whole conversation there, on a workspace made from the document as it is now.
    fn assistant_send(&mut self, ready: Ready, ctx: &egui::Context) {
        let (tx, rx) = channel();
        let Ready { mut provider, request, edit, basis, key, .. } = ready;
        let instruction = request.instruction.clone();
        // A text request's document text, which the chat sends again with its instruction.
        let shared = (edit.is_none() && !request.context.trim().is_empty()).then(|| Shared { scope: request.scope, text: request.context.clone() });
        let repaint = ctx.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let stop = cancel.clone();
        // Once the request is let go the provider sends nothing again, also while it waits to retry.
        provider.set_cancel(cancel.clone());
        let (started, edit_base) = match edit {
            Some(job) => {
                let author = tl!("AI Assistant").to_owned();
                let seed = workspace::Seed::of(&self.session, &author);
                let base = EditBase {
                    document: self.session.document_id(),
                    doc: seed.document().clone(),
                    tracking: self.session.doc.settings.track_changes,
                    author,
                    start: seed.start(),
                };
                let started = std::thread::Builder::new().name("assistant".into()).spawn(move || {
                    let mut tools = workspace::Toolbox::new(seed);
                    tools.set_cancel(stop.clone());
                    let overview = tools.overview();
                    let selection = tools.selection_text();
                    let definitions = workspace::Toolbox::definitions();
                    let result = {
                        let mut host = Host(&mut tools);
                        let task = conversation::Task {
                            tools: &definitions,
                            instruction: &request.instruction,
                            overview: &overview,
                            selection: &selection,
                            history: &job.history,
                        };
                        let progress_tx = tx.clone();
                        let progress_repaint = repaint.clone();
                        conversation::run(provider.as_mut(), &mut host, &task, &stop, &mut |p| {
                            let _ = progress_tx.send(Msg::Progress(p));
                            progress_repaint.request_repaint();
                        })
                    };
                    let edited = result.map(|outcome| {
                        let changed = tools.changed();
                        // What will be applied must be what the model saw and described.
                        let unrepeatable = changed && outcome.stop == Stop::End && tools.verify().is_err();
                        Edited { outcome, steps: tools.steps().to_vec(), changed, limited: tools.hit_limit(), unrepeatable }
                    });
                    let _ = tx.send(Msg::Edited(edited));
                    repaint.request_repaint();
                });
                (started, Some(base))
            }
            None => {
                let started = std::thread::Builder::new().name("assistant".into()).spawn(move || {
                    let result = provider.send(&request);
                    let _ = tx.send(Msg::Reply(result));
                    repaint.request_repaint();
                });
                (started, None)
            }
        };
        match started {
            Ok(_) => {
                // The last reply is of the last request: it does not stay under this one's outcome.
                self.assistant.reply = None;
                self.assistant.edit_result = false;
                let said = if edit_base.is_some() { tl!("The assistant is working…") } else { tl!("Waiting for the reply…") };
                self.assistant.waiting = Some(Waiting { rx, instruction, shared, basis, key, cancel, edit: edit_base, round: 0, calls: 0 });
                self.assistant.want(Focus::Stop);
                self.assistant_say(said.to_owned(), ctx);
            }
            Err(_) => self.assistant.error = Some(Error::NotStarted),
        }
    }

    /// Take what the worker said: progress, or the end of the request. While one is pending, ask for another
    /// look soon. Called every frame, also while the pane is hidden, so changes are applied without it.
    pub(crate) fn assistant_poll(&mut self, ctx: &egui::Context) {
        loop {
            let Some(waiting) = self.assistant.waiting.as_mut() else { return };
            let msg = match waiting.rx.try_recv() {
                Ok(msg) => msg,
                Err(TryRecvError::Empty) => {
                    ctx.request_repaint_after(Duration::from_millis(250));
                    return;
                }
                Err(TryRecvError::Disconnected) if waiting.edit.is_some() => Msg::Edited(Err(Error::Interrupted.into())),
                Err(TryRecvError::Disconnected) => Msg::Reply(Err(Error::Interrupted)),
            };
            match msg {
                Msg::Progress(progress) => {
                    match progress {
                        // The status line is a live region: it changes once a round, so a screen reader is not
                        // read every step. The steps between rounds take no time anyway; the rounds do.
                        conversation::Progress::Round(round) => {
                            waiting.round = round;
                            let calls = waiting.calls;
                            let text = fmt(
                                tl!("The assistant is working: round {round}, {calls} steps so far…"),
                                &[("round", &round.to_string()), ("calls", &calls.to_string())],
                            );
                            self.assistant_say(text, ctx);
                        }
                        conversation::Progress::Tool { .. } => waiting.calls += 1,
                    }
                }
                Msg::Reply(result) => return self.assistant_reply(result, ctx),
                Msg::Edited(result) => return self.assistant_edited(result, ctx),
            }
        }
    }

    /// The pending request is over: let it go, and put the keyboard where it belongs.
    fn assistant_end_wait(&mut self) -> Option<Waiting> {
        let waiting = self.assistant.waiting.take()?;
        // Stop Waiting is gone: the keyboard focus it held goes to Send. A person who has moved on, to the
        // document for instance, is not pulled back, and a pane that was closed (its last focus is stale)
        // takes nothing when it opens again.
        let held_stop = std::mem::take(&mut self.assistant.stop_focused) && self.session.view.assistant_pane;
        if held_stop {
            self.assistant.want(Focus::Send);
        } else {
            self.assistant.want_focus = Focus::None;
            self.assistant.focus_frames = 0;
        }
        Some(waiting)
    }

    /// A text request ended.
    fn assistant_reply(&mut self, result: Result<Reply, Error>, ctx: &egui::Context) {
        let Some(waiting) = self.assistant_end_wait() else { return };
        let instruction = waiting.instruction.clone();
        let basis = waiting.basis.clone();
        let key = waiting.key.clone();
        self.assistant_run.note_answer(result.as_ref().map(|_| ()));
        match result {
            Ok(mut reply) => {
                self.assistant.last_usage = Some(Spent::All { model: reply.model.clone(), usage: reply.usage });
                // A declined reply and a blank one have no text to place, whatever else came with them.
                if matches!(reply.stop, Stop::Refusal { .. }) || reply.text.trim().is_empty() {
                    reply.text.clear();
                }
                if !reply.text.is_empty() {
                    // The instruction goes again with the text it carried, so the next request repeats this one.
                    self.assistant.add_exchange(Turn::user(instruction, waiting.shared.clone()), Turn::assistant(reply.text.clone()));
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
            // line said while waiting is no longer true. An error has no token count, and the request before's is
            // not this one's.
            Err(error) => {
                self.assistant.said.clear();
                self.assistant.last_usage = Some(Spent::Unknown);
                self.assistant.error = Some(error);
            }
        }
    }

    /// A request that changes the document ended. Its changes are applied, as one undo step, only when the
    /// assistant finished and the document is as it was when it began; otherwise nothing changes, and the
    /// person is told why.
    fn assistant_edited(&mut self, result: Result<Edited, conversation::Failed>, ctx: &egui::Context) {
        let Some(waiting) = self.assistant_end_wait() else { return };
        let Some(base) = waiting.edit.as_ref() else { return };
        // What the status line said while the assistant worked is no longer true, whatever happens now.
        self.assistant.said.clear();
        let edited = match result {
            Ok(edited) => edited,
            Err(failed) => {
                self.assistant_run.note_answer(Err(&failed.error));
                // The rounds answered before the error were counted all the same: Settings shows them.
                self.assistant.last_usage =
                    Some(if failed.rounds > 0 { Spent::Before { model: failed.model, usage: failed.usage } } else { Spent::Unknown });
                let text = error_text(&failed.error);
                self.assistant.error = Some(failed.error);
                self.assistant_show_outcome(&text);
                return;
            }
        };
        if edited.outcome.rounds > 0 {
            self.assistant_run.note_answer(Ok(()));
            self.assistant.last_usage = Some(Spent::All { model: edited.outcome.model.clone(), usage: edited.outcome.usage });
        } else {
            self.assistant.last_usage = Some(Spent::Unknown);
        }
        let mut said = String::new();
        self.assistant.applied = None;
        self.assistant.apply_error = None;
        self.assistant.apply_mark = !self.assistant.apply_mark;
        let outcome = edited.outcome;
        // A limit made the assistant stop: what it did is applied, and said to be partial.
        self.assistant.partial = outcome.stop == Stop::End && (outcome.cut_short || edited.limited);
        let mut on_document = false;
        if outcome.stop == Stop::End {
            if edited.changed && !edited.steps.is_empty() {
                // The same document with the same content: what the person did meanwhile that changed nothing (a
                // click, a save, an Undo and its Redo) does not cost them the request.
                let unchanged = self.session.document_id() == base.document
                    && self.session.doc.settings.track_changes == base.tracking
                    && workspace::unchanged_since(&self.session.doc, &base.doc);
                if edited.unrepeatable {
                    self.assistant.apply_error =
                        Some(tl!("The assistant's changes could not be repeated exactly, so they were not applied. Ask again.").to_owned());
                } else if !unchanged {
                    self.assistant.apply_error =
                        Some(tl!("The document changed while the assistant worked, so its changes were not applied. Ask again.").to_owned());
                } else {
                    let params = json!({"steps": edited.steps, "author": base.author, "start": base.start});
                    match self.run("assistant.runSteps", params) {
                        Ok(_) => {
                            self.assistant.applied = Some((self.session.document_id(), self.session.undo_mark()));
                            on_document = true;
                            said = if self.assistant.partial {
                                tl!("The assistant reached a limit of this request before it finished. The changes it made so far were applied, and its reply says what is left. Undo reverts all of them in one step.")
                            } else {
                                tl!("The assistant's changes were applied. Undo reverts all of them in one step.")
                            }
                            .to_owned();
                        }
                        Err(why) => {
                            self.assistant.apply_error = Some(fmt(tl!("The assistant's changes could not be applied: {why}"), &[("why", &why)]));
                        }
                    }
                }
            } else {
                said = tl!("The assistant answered without changing the document.").to_owned();
                on_document = true;
            }
            // The chat goes on from what the document has: the summary of changes that were not applied would
            // tell the model's next turn about changes the document does not have.
            if on_document && !outcome.text.trim().is_empty() {
                self.assistant.add_exchange(Turn::user(waiting.instruction.clone(), None), Turn::assistant(outcome.text.clone()));
            }
        } else {
            said = edit_stop_words(&outcome.stop).to_owned();
        }
        // The service answered, so it accepted the key: now it may be kept, if the person asked, and they are told.
        if outcome.rounds > 0 && self.assistant_run.keep_if_asked(waiting.key.as_ref()) {
            said = format!("{said} {}", tl!("The key was saved on this computer."));
        }
        let mut text = outcome.text;
        if matches!(outcome.stop, Stop::Refusal { .. }) {
            text.clear();
        }
        self.assistant.reply =
            Some(Reply { text, stop: outcome.stop, usage: outcome.usage, model: outcome.model, fallback_used: outcome.fallback_used });
        self.assistant.edit_result = true;
        self.assistant.basis = None;
        let shown = self.assistant.apply_error.clone().unwrap_or_else(|| said.trim().to_owned());
        self.assistant_show_outcome(&shown);
        if !said.trim().is_empty() {
            self.assistant_say(said.trim().to_owned(), ctx);
        }
    }

    /// A request that changes the document ended while the pane was closed: what happened to the document must
    /// not go unsaid. The pane opens again, without taking the keyboard, so its live region speaks and it
    /// shows; the status bar says it too.
    fn assistant_show_outcome(&mut self, text: &str) {
        if self.session.view.assistant_pane || text.is_empty() {
            return;
        }
        let _ = self.run("view.assistantPane", json!({"value": true}));
        self.status_long(text.to_owned());
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

/// The undo label of the assistant's changes (`assistant.runSteps`).
const ASSISTANT_CHANGES: &str = "Apply Assistant Changes";

/// Why a request that changes the document applied nothing, when it did not finish.
fn edit_stop_words(stop: &Stop) -> &'static str {
    match stop {
        Stop::End => "",
        Stop::Refusal { .. } => tl!("The model declined this request, so nothing was changed."),
        Stop::MaxTokens => tl!("The assistant's answer reached its length limit before it finished, so nothing was changed."),
        Stop::Other(why) if why.contains("rounds") || why.contains("too long") => {
            tl!("The assistant used all the steps one request may take before it finished, so nothing was changed. Ask for less at a time.")
        }
        Stop::Other(_) => tl!("The assistant stopped before it finished, so nothing was changed."),
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
        Error::NoAnswerInTime => tl!(
            "No whole answer came within 10 minutes. The request reached the service, which may still process it. Try again, perhaps with less text."
        )
        .to_owned(),
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

/// Runs `add` in a child ui whose identity is `name` and nothing else. In egui a widget's id follows from its
/// place among the widgets drawn before it, so a line that appears or goes above a control would give it a new
/// identity and the keyboard focus would be lost; a control that can hold the focus is drawn in one of these.
fn stable<R>(ui: &mut Ui, name: &str, add: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<R> {
    ui.scope_builder(egui::UiBuilder::new().scope_id(egui::Id::unique(("assistant", name))), add)
}

/// A button that takes the keyboard focus when the pane has asked for it.
fn button(ui: &mut Ui, app: &mut WordApp, label: &str, focus: Focus, enabled: bool) -> bool {
    let response = stable(ui, &format!("button{}", focus as u8), |ui| ui.add_enabled(enabled, egui::Button::new(label))).inner;
    if enabled {
        focus_if_asked(ui, app, &response, focus);
    } else if app.assistant.want_focus == focus {
        // A disabled control cannot take the keyboard; the request must not fire later, when it is
        // enabled while the person types elsewhere.
        app.assistant.want_focus = Focus::None;
        app.assistant.focus_frames = 0;
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

/// The introduction, for the way the assistant works.
fn intro(edits: bool) -> &'static str {
    if edits {
        tl!("Ask for a change to your document, or a question about it. The assistant makes the changes itself, and Undo reverts them in one step.")
    } else {
        tl!("Ask for a change to your text, or a question about it. A reply is placed only when you choose.")
    }
}

fn body(app: &mut WordApp, ui: &mut Ui, act: &mut Option<Act>) {
    if !CAN_SEND {
        muted(ui, intro(app.assistant_run.choices.edits));
        // Nothing here can send: say so and offer nothing that looks as if it could.
        notice(ui, Tone::Info, &key_note(&app.assistant_run), None);
        return;
    }
    let busy = app.assistant.waiting.is_some() || app.assistant.ready.is_some();
    // Until the one-time setup is done it shows first, as it always did; then it waits under Settings.
    let setup = !app.assistant_run.setup_done();
    setup_changed(app, ui, setup);
    if setup {
        muted(ui, intro(app.assistant_run.choices.edits));
        settings(app, ui, busy, act, true);
        ui.add_space(4.0);
    }
    let label = ui.label(tl!("Instruction:"));
    // Laid out as the document lays out its paragraphs, from the right in a right-to-left interface.
    let mut layouter = |ui: &Ui, text: &dyn egui::TextBuffer, wrap_width: f32| crate::rtl::field_galley(ui, text.as_str(), wrap_width);
    let prompt = ui
        .add_enabled(
            !busy,
            TextEdit::multiline(&mut app.assistant.prompt)
                .desired_rows(4)
                .desired_width(f32::INFINITY)
                .char_limit(MAX_INSTRUCTION_BYTES)
                .hint_text(tl!("For example: make this paragraph shorter"))
                .align(crate::rtl::field_align())
                .layouter(&mut layouter)
                .id_salt("assistant.prompt"),
        )
        .labelled_by(label.id);
    if !busy {
        focus_if_asked(ui, app, &prompt, Focus::Prompt);
    }
    let waiting = app.assistant.waiting.is_some();
    let can_send = !busy && !app.assistant.prompt.trim().is_empty();
    let (mut send, mut new_chat, mut stop, mut toggle) = (false, false, false, false);
    ui.horizontal_wrapped(|ui| {
        send = button(ui, app, tl!("Send"), Focus::Send, can_send);
        if waiting {
            stop = button(ui, app, tl!("Stop Waiting"), Focus::Stop, true);
        }
        new_chat = stable(ui, "newChat", |ui| ui.add_enabled(!busy, egui::Button::new(tl!("New Chat")))).inner.clicked();
        if !setup {
            toggle = settings_button(ui, app);
        }
    });
    for (clicked, what) in [(send, Act::Send), (stop, Act::Stop), (new_chat, Act::NewChat), (toggle, Act::Settings)] {
        if clicked {
            *act = Some(what);
        }
    }
    if !setup && app.assistant.settings_open {
        settings(app, ui, busy, act, false);
    }
    status_line(app, ui);
    unsaved_notices(app, ui);
    if app.assistant.stopped {
        notice(ui, Tone::Info, tl!("You stopped waiting. The request was already sent, and the service may still process it."), None);
    }
    consent(app, ui, act);
    if let Some(error) = &app.assistant.error {
        notice(ui, Tone::Bad, &error_text(error), Some((Live::Assertive, app.assistant.alert_mark)));
    }
    reply(app, ui, act);
}

/// When the setup part goes, or comes back: Settings starts closed, a keyboard focus on one of the controls that
/// go moves to the instruction box (or, when the Settings button goes, to the key box), and a screen reader is
/// told where the part went.
fn setup_changed(app: &mut WordApp, ui: &Ui, setup: bool) {
    let before = app.assistant.shown_setup.replace(setup);
    match before {
        Some(true) if !setup => {
            app.assistant.settings_open = false;
            let focused = ui.ctx().memory(|m| m.focused());
            if focused.is_some_and(|id| app.assistant.setup_ids.contains(&id)) {
                app.assistant.want(Focus::Prompt);
            }
            let done = tl!("The setup is done. The key and the options are now under Settings.");
            let said = app.assistant.said.trim_end_matches('\u{200b}');
            let text = if said.is_empty() { done.to_owned() } else { format!("{said} {done}") };
            app.assistant.say(text);
            ui.ctx().request_repaint();
        }
        Some(false) if setup && std::mem::take(&mut app.assistant.settings_focused) => app.assistant.want(Focus::Key),
        _ => {}
    }
}

/// The key and the options: shown before the instruction box until the setup is done, then under Settings. Its
/// controls keep their identity in both places, so the keyboard stays on them when they move.
fn settings(app: &mut WordApp, ui: &mut Ui, busy: bool, act: &mut Option<Act>, setup: bool) {
    let mut ids = Vec::new();
    muted(ui, key_note(&app.assistant_run));
    ui.add_space(2.0);
    let label = ui.label(tl!("API key:"));
    let key_box = ui
        .add_enabled(!busy, TextEdit::singleline(app.assistant_run.key.as_mut_string()).password(true).desired_width(f32::INFINITY).id(key_box_id()))
        .labelled_by(label.id);
    ids.push(key_box.id);
    if !busy {
        focus_if_asked(ui, app, &key_box, Focus::Key);
    }
    keep_controls(app, ui, busy, act, &mut ids);
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(problem) = file_problem_text(&app.assistant_run) {
        notice(ui, Tone::Warn, &problem, Some((Live::Polite, false)));
        // Which file it is, so the person can act on what the notice says.
        if let Some(file) = &app.assistant_run.file {
            ui.add(egui::Label::new(RichText::new(file.path().display().to_string()).weak()).selectable(true).wrap());
        }
    }
    ui.add_space(4.0);
    let choices = &mut app.assistant_run.choices;
    let edits = stable(ui, "edits", |ui| {
        ui.add_enabled(!busy, egui::Checkbox::new(&mut choices.edits, tl!("Let the assistant make changes in the document")))
    })
    .inner;
    ids.push(edits.id);
    let mut changed = edits.changed().then_some(Choice::Edits);
    if !choices.edits {
        let scope = stable(ui, "scope", |ui| {
            ui.add_enabled(!busy, egui::Checkbox::new(&mut choices.whole_document, tl!("Use the whole document instead of the selection")))
        })
        .inner;
        ids.push(scope.id);
        if scope.changed() {
            changed = Some(Choice::WholeDocument);
        }
    }
    if let Some(which) = changed {
        app.assistant_run.save_choices(which);
    }
    agreement_state(app, ui, busy, act, setup, &mut ids);
    if let Some(spent) = &app.assistant.last_usage {
        muted(ui, spent_words(spent));
    }
    app.assistant.setup_ids = ids;
}

/// What the person agreed to, with the button that withdraws it. Before the setup is done there is nothing to
/// say until they agree: the first request asks.
fn agreement_state(app: &mut WordApp, ui: &mut Ui, busy: bool, act: &mut Option<Act>, setup: bool, ids: &mut Vec<egui::Id>) {
    match app.assistant_run.choices.agreed.clone() {
        Some(agreed) => {
            let text = fmt(
                tl!("You agreed that requests may be sent to {recipient}, to be read by the model {model}."),
                &[("recipient", &agreed.recipient), ("model", &agreed.model)],
            );
            muted(ui, text);
            let withdraw = stable(ui, "withdraw", |ui| ui.add_enabled(!busy, egui::Button::new(tl!("Withdraw Agreement")))).inner;
            ids.push(withdraw.id);
            if withdraw.clicked() {
                *act = Some(Act::Withdraw);
            }
        }
        None if !setup => muted(ui, tl!("You have not agreed yet: the next request asks before anything is sent.")),
        None => {}
    }
}

/// What Settings says the last request used.
fn spent_words(spent: &Spent) -> String {
    match spent {
        Spent::All { model, usage } => usage_words(model, usage),
        Spent::Before { model, usage } => {
            format!(
                "{} {}",
                usage_words(model, usage),
                tl!("The request failed before it finished: these are the tokens of its rounds that got an answer.")
            )
        }
        Spent::Unknown => tl!("The last request ended without an answer, so its token use is not known.").to_owned(),
    }
}

/// The last request's token use: the tokens in, of them those the cache served and those written to it, and
/// the tokens out.
fn usage_words(model: &str, usage: &Usage) -> String {
    fmt(
        tl!(
            "Last request ({model}): {input} tokens in, of which {read} were read from the cache and {written} were written to it; {output} tokens out."
        ),
        &[
            ("model", model),
            ("input", &usage.input_tokens.to_string()),
            ("read", &usage.cache_read_tokens.to_string()),
            ("written", &usage.cache_write_tokens.to_string()),
            ("output", &usage.output_tokens.to_string()),
        ],
    )
}

/// The Settings button: shows or hides the key and the options in place. Its state is exposed to assistive
/// technology (expanded or collapsed) and drawn, not by colour: an arrow at its start that points the way
/// reading goes while closed and down while open.
fn settings_button(ui: &mut Ui, app: &mut WordApp) -> bool {
    let open = app.assistant.settings_open;
    let rtl = crate::rtl::on();
    let icon = egui::IdSalt::new("assistant.settingsArrow");
    let side = ui.spacing().icon_width * 0.6;
    let arrow = egui::Atom::custom(icon, egui::vec2(side, side));
    let shown = stable(ui, "settings", |ui| {
        // A button lays its parts out from the left: the arrow goes after the text to sit at the right, where a
        // right-to-left row starts.
        let button = if rtl { egui::Button::new((tl!("Settings"), arrow)) } else { egui::Button::new((arrow, tl!("Settings"))) };
        button.atom_ui(ui)
    })
    .inner;
    let response = shown.response.clone();
    if let Some(rect) = shown.rect(icon) {
        use std::f32::consts::TAU;
        let rotation = if open {
            0.0
        } else if rtl {
            TAU * 0.25
        } else {
            -TAU * 0.25
        };
        let color = ui.style().interact(&response).fg_stroke.color;
        ui.painter().add(egui::Shape::rotated_triangle(rect, rotation, color));
    }
    ui.ctx().accesskit_node_builder(response.id, |node| node.set_expanded(open));
    focus_if_asked(ui, app, &response, Focus::Settings);
    app.assistant.settings_focused = ui.ctx().memory(|m| m.has_focus(response.id));
    response.clicked()
}

/// What of the pane's choices could not be saved, said where the person sees it whatever shows (Settings, where
/// it was chosen, may be closed by now: agreeing ends the setup) and announced when it appears. The agreement step
/// said the agreement is kept for later runs; when it could not be, this says so.
fn unsaved_notices(app: &WordApp, ui: &mut Ui) {
    let run = &app.assistant_run;
    if run.unsaved.contains(&Choice::Agreement) {
        let text = if run.choices.agreed.is_some() {
            tl!("Your agreement could not be saved in WordCraft's settings, so it lasts for this run only: the next run asks for it again.")
        } else {
            tl!(
                "Your withdrawal could not be saved in WordCraft's settings, so a later run may still send requests without asking first. Withdraw your agreement again in that run."
            )
        };
        notice(ui, Tone::Warn, text, Some((Live::Polite, false)));
    }
    if run.unsaved.iter().any(|choice| *choice != Choice::Agreement) {
        notice(ui, Tone::Warn, tl!("The pane's settings could not be saved, so they last for this run only."), Some((Live::Polite, false)));
    }
}

/// The status line: a live region, so what happens away from the keyboard is spoken.
fn status_line(app: &WordApp, ui: &mut Ui) {
    if app.assistant.said.is_empty() {
        return;
    }
    let label = ui.add(egui::Label::new(RichText::new(app.assistant.said.clone()).weak()).wrap());
    live_region(ui, label.id, &app.assistant.said, Live::Polite);
}

/// The options for keeping the key on this computer. Their ids go into `ids`.
#[cfg(not(target_arch = "wasm32"))]
fn keep_controls(app: &mut WordApp, ui: &mut Ui, busy: bool, act: &mut Option<Act>, ids: &mut Vec<egui::Id>) {
    if !app.assistant_run.can_keep() {
        return;
    }
    let before = app.assistant_run.remember;
    if !before {
        // Said before the checkbox, so it is read before the person can turn it on.
        muted(ui, tl!("If you choose to remember the key, it is kept unencrypted in a file in WordCraft's settings folder."));
    }
    let remember = stable(ui, "remember", |ui| {
        ui.add_enabled(!busy, egui::Checkbox::new(&mut app.assistant_run.remember, tl!("Remember this key on this computer")))
    })
    .inner;
    ids.push(remember.id);
    if before && !app.assistant_run.remember && app.assistant_run.on_disk.is_some() && !app.assistant_run.forget(false) {
        // Turning it off removes the saved copy, and the key stays for this run. If the copy could not be
        // removed it is still there, so the box stays ticked.
        app.assistant_run.remember = true;
    }
    if app.assistant_run.on_disk.is_some() {
        let forget = stable(ui, "forget", |ui| ui.add_enabled(!busy, egui::Button::new(tl!("Forget Saved Key")))).inner;
        ids.push(forget.id);
        if forget.clicked() {
            *act = Some(Act::ForgetKey);
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn keep_controls(_: &mut WordApp, _: &mut Ui, _: bool, _: &mut Option<Act>, _: &mut Vec<egui::Id>) {}

/// Who gets the text, said once before the first request is sent, with the buttons that agree or decline. It
/// covers everything a later request may send, in either way of working, since agreeing is for all of them; and
/// it describes the request that is waiting, not the controls above it, which cannot change while it waits.
fn consent(app: &mut WordApp, ui: &mut Ui, act: &mut Option<Act>) {
    let Some(ready) = &app.assistant.ready else { return };
    let edits = ready.edit.is_some();
    let note = fmt(
        tl!(
            "Requests from this pane are sent over the internet to {recipient}, to be read by the model {model}. A request sends your instruction, the earlier messages of this chat, and the selected text, or the whole document when you choose that. When the assistant makes changes in the document, a request sends the whole document, and the assistant can also read the rest of it (headers, footers, notes, comments with their authors' names, and the document's properties; not text deleted with Track Changes on) and change it with WordCraft's editing commands. WordCraft keeps no copy. Nothing is sent until you agree."
        ),
        &[("recipient", &ready.disclosure.recipient), ("model", &ready.disclosure.model)],
    );
    // What this request sends: the scope of a text answer, the whole document when the assistant makes changes.
    let this = if ready.chars == 0 {
        match ready.request.scope {
            Scope::Selection => tl!("This request sends only your instruction: nothing is selected.").to_owned(),
            Scope::Document => tl!("This request sends only your instruction: the document is empty.").to_owned(),
        }
    } else {
        let what = match ready.request.scope {
            Scope::Document => tl!("the whole document"),
            Scope::Selection => tl!("the selected text"),
        };
        fmt(tl!("This request sends your instruction and {what} ({chars} characters)."), &[("what", what), ("chars", &ready.chars.to_string())])
    };
    let start = (ready.chars > 0).then(|| {
        let words = if edits { tl!("The document starts: {start}") } else { tl!("The text starts: {start}") };
        fmt(words, &[("start", &preview(&ready.request.context))])
    });
    // What agreeing covers, as the pane really treats it: every later request, until it is withdrawn.
    let covers = if app.assistant_run.choices_kept() {
        tl!(
            "Your agreement is kept in WordCraft's settings: later requests, in any document and in later runs, are sent when you press Send, without asking again. You can withdraw it under Settings."
        )
    } else {
        tl!(
            "Your agreement lasts for this run: later requests, in any document, are sent when you press Send, without asking again. You can withdraw it under Settings."
        )
    };
    // Everything the decision rests on is one announcement, said when it appears, so a screen reader reads
    // the whole of it before the person decides.
    let mut lines = vec![note, this];
    lines.extend(start);
    if ready.disclosure.fallback_possible {
        lines.push(tl!("If the model declines, the service may have another of its models read the text instead.").to_owned());
    }
    lines.push(covers.to_owned());
    notice(ui, Tone::Info, &lines.join("\n"), Some((Live::Assertive, false)));
    let (mut agree, mut cancel) = (false, false);
    ui.horizontal_wrapped(|ui| {
        agree = button(ui, app, tl!("Agree and Send"), Focus::Agree, true);
        cancel = stable(ui, "cancel", |ui| ui.button(tl!("Cancel"))).inner.clicked();
    });
    if agree {
        *act = Some(Act::Agree);
    } else if cancel {
        *act = Some(Act::Cancel);
    }
}

/// Text the model wrote, laid out paragraph by paragraph in each paragraph's own direction (a Hebrew paragraph
/// from the right, an English one from the left, in either interface), and selectable.
fn model_text(ui: &mut Ui, text: &str) {
    let galley = crate::rtl::paragraphs(ui, text, ui.available_width());
    ui.add(egui::Label::new(galley).selectable(true));
}

/// The reply, what ended it, and the buttons that place it.
fn reply(app: &mut WordApp, ui: &mut Ui, act: &mut Option<Act>) {
    let Some(reply) = &app.assistant.reply else { return };
    ui.add_space(6.0);
    ui.label(RichText::new(tl!("Reply")).font(semibold(14.0)));
    if app.assistant.edit_result {
        edit_result(app, ui, act);
        return;
    }
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
        model_text(ui, &reply.text);
        ui.horizontal_wrapped(|ui| {
            if stable(ui, "replace", |ui| ui.button(tl!("Replace Selection"))).inner.clicked() {
                *act = Some(Act::Replace);
            }
            if stable(ui, "insertAfter", |ui| ui.button(tl!("Insert After"))).inner.clicked() {
                *act = Some(Act::InsertAfter);
            }
            if stable(ui, "copy", |ui| ui.button(tl!("Copy"))).inner.clicked() {
                *act = Some(Act::Copy);
            }
        });
    }
    if let Some(why) = &app.assistant.apply_error {
        notice(ui, Tone::Bad, why, Some((Live::Assertive, app.assistant.apply_mark)));
    }
    if reply.fallback_used {
        muted(ui, tl!("The first model declined, and a fallback model wrote this reply."));
    }
}

/// The end of a request that changed the document: why nothing changed when it did not finish, the
/// assistant's words, and Undo These Changes while its changes are the last step.
fn edit_result(app: &mut WordApp, ui: &mut Ui, act: &mut Option<Act>) {
    let Some(reply) = &app.assistant.reply else { return };
    let why = edit_stop_words(&reply.stop);
    if !why.is_empty() {
        notice(ui, Tone::Warn, why, None);
    }
    if app.assistant.partial && app.assistant.applied.is_some() {
        notice(ui, Tone::Warn, tl!("The assistant reached a limit of this request before it finished: only part of the work may be done."), None);
    }
    if !reply.text.is_empty() {
        model_text(ui, &reply.text);
    }
    let undo = app.assistant_changes_on_top();
    ui.horizontal_wrapped(|ui| {
        if undo && stable(ui, "undoEdits", |ui| ui.button(tl!("Undo These Changes"))).inner.clicked() {
            *act = Some(Act::UndoEdits);
        }
        if !reply.text.is_empty() && stable(ui, "copy", |ui| ui.button(tl!("Copy"))).inner.clicked() {
            *act = Some(Act::Copy);
        }
    });
    if let Some(why) = &app.assistant.apply_error {
        notice(ui, Tone::Bad, why, Some((Live::Assertive, app.assistant.apply_mark)));
    }
    if reply.fallback_used {
        muted(ui, tl!("The first model declined, and a fallback model wrote this reply."));
    }
}

#[cfg(test)]
mod tests;
