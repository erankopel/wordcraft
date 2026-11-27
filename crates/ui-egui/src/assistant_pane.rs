//! The AI pane: ask for a change to the selection or the document, or a question about it, and put the
//! reply into the document.
//!
//! Text leaves the machine only through a provider, and only after the person agrees here: before a
//! request that would send more than has been agreed to in this run the pane says who receives the text,
//! which model reads it, how much of it goes and how it starts. No command and no setting can give that
//! agreement; only the button in the pane can. A reply is placed with `assistant.apply`, so it is one
//! undo step and tracked when Track Changes is on. Reading what would be shared (`assistant.context`)
//! and placing a reply (`assistant.apply`) are engine commands; sending is this pane's alone, because
//! an agent that drives WordCraft brings its own model. See `docs/assistant.md`.

use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::Duration;

use craft::theme::variant::Tone;
use craft::ui::buttons::Button;
use craft::ui::choice::Checkbox;
use craft::ui::contract::{Bind, Politeness, Props as _, Show as _, ShowContainer as _};
use craft::ui::display::{Shade, Text};
use craft::ui::layout_components::{Row, Stack};
use craft::ui::notices::Notice;
use craft::ui::runtime::focus::FocusRequest;
use craft::ui::style::TypeRole;
use craft::ui::text::TextField;
use egui::Ui;
use serde_json::json;
use wordcraft_assistant::{DEFAULT_MODEL, Disclosure, Error, MAX_INSTRUCTION_BYTES, Provider, Reply, Request, Role, Scope, Stop, Turn, build_body};
use wordcraft_engine::Password;

use crate::i18n::fmt;
use crate::{WordApp, announce};

/// Whether this build can send a request at all: the browser version cannot.
const CAN_SEND: bool = cfg!(not(target_arch = "wasm32"));

/// How many characters of the shared text the agreement step shows the person.
const PREVIEW_CHARS: usize = 160;

/// Makes the provider for a request from the key the person typed, if they typed one (a provider
/// that needs a key and has none looks for it in the environment). A function, so a test can put its
/// own in the state.
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

/// The text a reply was asked about: the document revision and the selection when the request was made.
/// A reply to a selection is placed only while both are as they were, so it never lands in other text.
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
}

/// A request in flight on its worker thread.
struct Waiting {
    rx: Receiver<Result<Reply, Error>>,
    instruction: String,
    basis: Option<Basis>,
}

/// Where keyboard focus should go the next time the pane is drawn, once: a control that appears or
/// disappears would otherwise drop the focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Focus {
    None,
    Send,
    Agree,
    Stop,
}

/// What the person agreed to and typed for the whole run of the program, in every window: a second window
/// does not ask again, and the key is not typed again. Never saved.
#[derive(Default)]
pub(crate) struct AssistantRun {
    /// The widest scope the person agreed to send to a provider that leaves the machine: `None` until they
    /// agree. Agreeing to the document covers the selection too.
    agreed: Option<Scope>,
    /// The key the person typed: kept in memory for the run, wiped when dropped, never saved or shown.
    key: Password,
}

impl AssistantRun {
    /// The key to give the provider: the one typed, if any.
    fn typed_key(&self) -> Option<&str> {
        Some(self.key.as_str().trim()).filter(|k| !k.is_empty())
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
    /// For how many more frames `want_focus` is asked for: the page takes the keyboard back after a command
    /// has run, so a single request in the frame the control appears can be taken away again.
    focus_frames: u8,
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
        self.focus_frames = 3;
    }

    fn scope(&self) -> Scope {
        if self.document_scope { Scope::Document } else { Scope::Selection }
    }
}

/// Must the person agree before a request sharing `scope` goes to a provider? Only when the provider
/// leaves the machine, and they have not agreed already to that much or more in this run.
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
}

impl WordApp {
    /// Carry out what a click asked for.
    pub(crate) fn assistant_act(&mut self, act: Act, ctx: &egui::Context) {
        match act {
            Act::Send => self.assistant_ask(ctx),
            Act::Agree => {
                if let Some(ready) = self.assistant.ready.take() {
                    let scope = ready.request.scope;
                    self.assistant_run.agreed = Some(if self.assistant_run.agreed == Some(Scope::Document) { Scope::Document } else { scope });
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
                announce::say(self, tl!("You stopped waiting for the reply.").to_owned(), ctx);
            }
            Act::NewChat => self.assistant = AssistantState::fresh_for_document(&self.assistant),
            Act::Replace => self.assistant_apply("replaceSelection", ctx),
            Act::InsertAfter => self.assistant_apply("insertAfter", ctx),
            Act::Copy => {
                if let Some(reply) = &self.assistant.reply {
                    ctx.copy_text(reply.text.clone());
                    announce::say(self, tl!("The reply was copied.").to_owned(), ctx);
                }
            }
        }
    }

    /// Gather what would be shared and make the provider. A provider that leaves the machine waits for
    /// the person's agreement, unless they have already agreed to this much in this run; every other
    /// request goes at once.
    fn assistant_ask(&mut self, ctx: &egui::Context) {
        if self.assistant.waiting.is_some() || self.assistant.ready.is_some() {
            return;
        }
        self.assistant.error = None;
        self.assistant.apply_error = None;
        self.assistant.stopped = false;
        let instruction = self.assistant.prompt.trim().to_owned();
        if instruction.is_empty() {
            self.assistant.error = Some(Error::EmptyInstruction);
            return;
        }
        let scope = self.assistant.scope();
        let basis = (scope == Scope::Selection).then(|| Basis { rev: self.session.rev(), selection: self.session.sel.clone() });
        let context = match self.run("assistant.context", json!({"scope": scope.as_str()})) {
            Ok(shared) => shared.get("text").and_then(|t| t.as_str()).unwrap_or_default().to_owned(),
            Err(_) => {
                self.assistant.error = Some(Error::NotStarted);
                return;
            }
        };
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
        let ready = Ready { provider, request, disclosure, chars, basis };
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
                self.assistant.waiting = Some(Waiting { rx, instruction, basis });
                self.assistant.want(Focus::Stop);
                announce::say(self, tl!("Waiting for the reply…").to_owned(), ctx);
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
        self.assistant.waiting = None;
        self.assistant.want(Focus::Send);
        match result {
            Ok(reply) => {
                if !reply.text.is_empty() {
                    self.assistant.history.push(Turn { role: Role::User, text: instruction });
                    self.assistant.history.push(Turn { role: Role::Assistant, text: reply.text.clone() });
                }
                let said = outcome_words(&reply);
                self.assistant.reply = Some(reply);
                self.assistant.basis = basis;
                announce::say(self, said, ctx);
            }
            // An error shows in an alert notice, which a screen reader reads out when it appears.
            Err(error) => self.assistant.error = Some(error),
        }
    }

    /// Put the reply into the document through `assistant.apply`.
    fn assistant_apply(&mut self, mode: &str, ctx: &egui::Context) {
        let Some(text) = self.assistant.reply.as_ref().map(|reply| reply.text.clone()).filter(|text| !text.trim().is_empty()) else { return };
        self.assistant.apply_error = None;
        let current = Basis { rev: self.session.rev(), selection: self.session.sel.clone() };
        if self.assistant.basis.as_ref().is_some_and(|asked| *asked != current) {
            self.assistant.apply_error = Some(
                tl!("The document or the selection changed after the request, so the reply was not placed. Select the text again and ask again.")
                    .to_owned(),
            );
            return;
        }
        match self.run("assistant.apply", json!({"text": text, "mode": mode})) {
            Ok(_) => announce::say(self, tl!("The reply was placed in the document.").to_owned(), ctx),
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
        (Stop::End, _) => tl!("The AI reply is ready."),
    }
    .to_owned()
}

/// What a failed request says, in words for the person and never with the key.
fn error_text(error: &Error) -> String {
    let kb = |bytes: usize| (bytes / 1000).to_string();
    match error {
        Error::MissingKey => {
            tl!("No API key. Type one in the API key box, or set the ANTHROPIC_API_KEY environment variable and start WordCraft again.").to_owned()
        }
        Error::EmptyInstruction => tl!("Write what you want first.").to_owned(),
        Error::TooLarge { what: "the instruction", bytes, limit } => {
            fmt(tl!("The instruction is too long ({size} KB; the most is {limit} KB)."), &[("size", &kb(*bytes)), ("limit", &kb(*limit))])
        }
        Error::TooLarge { bytes, limit, .. } => fmt(
            tl!("The text is too large to send ({size} KB; the most is {limit} KB). Select less text."),
            &[("size", &kb(*bytes)), ("limit", &kb(*limit))],
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

/// Where the key comes from, said before the person writes anything.
fn key_note(typed: bool) -> String {
    if !CAN_SEND {
        return tl!("The AI assistant is not available in the browser version.").to_owned();
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        if typed {
            return tl!("The key you typed is used only in this run. It is not saved.").to_owned();
        }
        if wordcraft_assistant::claude::Claude::key_present() {
            return tl!("Using the key in the ANTHROPIC_API_KEY environment variable.").to_owned();
        }
    }
    let _ = typed;
    tl!("No API key yet. Type one in the API key box, or set the ANTHROPIC_API_KEY environment variable and start WordCraft again.").to_owned()
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
    Stack::new().show(ui, |ui, _| body(app, ui, &mut act));
    if let Some(act) = act {
        let ctx = ui.ctx().clone();
        app.assistant_act(act, &ctx);
    }
}

/// A button that takes the keyboard focus once when the pane has asked for it.
fn button(ui: &mut Ui, app: &mut WordApp, label: &'static str, focus: Focus, enabled: bool) -> bool {
    let mut button = Button::new(label).enabled(enabled);
    if app.assistant.want_focus == focus && enabled {
        app.assistant.focus_frames = app.assistant.focus_frames.saturating_sub(1);
        if app.assistant.focus_frames == 0 {
            app.assistant.want_focus = Focus::None;
        }
        button = button.request_focus(FocusRequest::Take);
        ui.ctx().request_repaint();
    }
    button.show(ui).clicked()
}

fn body(app: &mut WordApp, ui: &mut Ui, act: &mut Option<Act>) {
    Text::new(tl!("Ask for a change to your text, or a question about it. A reply is placed only when you choose.")).shade(Shade::Muted).show(ui);
    if !CAN_SEND {
        // Nothing here can send: say so and offer nothing that looks as if it could.
        Notice::new(Tone::Info, key_note(false)).show(ui);
        return;
    }
    let busy = app.assistant.waiting.is_some() || app.assistant.ready.is_some();
    Text::new(key_note(app.assistant_run.typed_key().is_some())).shade(Shade::Muted).show(ui);
    TextField::new(tl!("API key"), Bind::bound(app.assistant_run.key.as_mut_string()))
        .masked()
        .placeholder(tl!("API key"))
        .enabled(!busy)
        .id_salt("assistant.key")
        .show(ui);
    Checkbox::new(Bind::bound(&mut app.assistant.document_scope))
        .label(tl!("Use the whole document instead of the selection"))
        .enabled(!busy)
        .show(ui);
    TextField::new(tl!("Instruction"), Bind::bound(&mut app.assistant.prompt))
        .lines(4)
        .limit(MAX_INSTRUCTION_BYTES)
        .placeholder(tl!("For example: make this paragraph shorter"))
        .enabled(!busy)
        .id_salt("assistant.prompt")
        .show(ui);
    let waiting = app.assistant.waiting.is_some();
    let can_send = !busy && !app.assistant.prompt.trim().is_empty();
    let (mut send, mut new_chat, mut stop) = (false, false, false);
    Row::new().show(ui, |ui, _| {
        send = button(ui, app, tl!("Send"), Focus::Send, can_send);
        if waiting {
            stop = button(ui, app, tl!("Stop Waiting"), Focus::Stop, true);
        }
        new_chat = Button::new(tl!("New Chat")).enabled(!busy).show(ui).clicked();
    });
    for (clicked, what) in [(send, Act::Send), (stop, Act::Stop), (new_chat, Act::NewChat)] {
        if clicked {
            *act = Some(what);
        }
    }
    if waiting {
        Text::new(tl!("Waiting for the reply…")).shade(Shade::Muted).show(ui);
    }
    if app.assistant.stopped {
        Notice::new(Tone::Info, tl!("You stopped waiting. The request was already sent, and the service may still process it.")).show(ui);
    }
    consent(app, ui, act);
    if let Some(error) = &app.assistant.error {
        Notice::new(Tone::Bad, error_text(error)).announce(Politeness::Assertive).show(ui);
    }
    reply(app, ui, act);
}

/// Who gets the text, said before it is sent, with the buttons that agree or decline. It describes the
/// request that is waiting, not the controls above it, which cannot change while it waits.
fn consent(app: &mut WordApp, ui: &mut Ui, act: &mut Option<Act>) {
    let Some(ready) = &app.assistant.ready else { return };
    let note = if ready.chars == 0 {
        fmt(
            tl!(
                "Your instruction (no document text goes with it: nothing is selected) will be sent over the internet to {recipient}, to be read by the model {model}. WordCraft keeps no copy. Nothing is sent until you agree."
            ),
            &[("recipient", &ready.disclosure.recipient), ("model", &ready.disclosure.model)],
        )
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
    let fallback = ready.disclosure.fallback_possible;
    // Said when it appears, so a screen reader reads the decision that is waiting.
    Notice::new(Tone::Info, note).announce(Politeness::Assertive).show(ui);
    if let Some(start) = start {
        Text::new(start).shade(Shade::Muted).show(ui);
    }
    if fallback {
        Text::new(tl!("If the model declines, the service may have another of its models read the text instead.")).shade(Shade::Muted).show(ui);
    }
    Text::new(tl!("Agreeing covers later requests in this run that share no more than this.")).shade(Shade::Muted).show(ui);
    let (mut agree, mut cancel) = (false, false);
    Row::new().show(ui, |ui, _| {
        agree = button(ui, app, tl!("Agree and Send"), Focus::Agree, true);
        cancel = Button::new(tl!("Cancel")).show(ui).clicked();
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
    Text::new(tl!("Reply")).role(TypeRole::Heading).show(ui);
    match (&reply.stop, reply.text.is_empty()) {
        (Stop::Refusal { .. }, _) => {
            Notice::new(Tone::Warn, tl!("The model declined this request, so there is no reply to place.")).show(ui);
        }
        (Stop::MaxTokens, true) => {
            Notice::new(Tone::Warn, tl!("The reply ran out of length before any text was written. Select less text.")).show(ui);
        }
        (Stop::MaxTokens, false) => {
            Notice::new(Tone::Warn, tl!("The reply was cut off at its length limit. Check it before you place it.")).show(ui);
        }
        (Stop::Other(why), _) if why.is_empty() => {
            Notice::new(Tone::Warn, tl!("The reply may be incomplete: the service did not say why it ended.")).show(ui);
        }
        (Stop::Other(why), _) => {
            Notice::new(Tone::Warn, fmt(tl!("The reply may be incomplete (the service ended it with: {why})."), &[("why", why)])).show(ui);
        }
        (Stop::End, _) => {}
    }
    if !reply.text.is_empty() {
        Text::new(reply.text.clone()).show(ui);
        Row::new().show(ui, |ui, _| {
            if Button::new(tl!("Replace Selection")).show(ui).clicked() {
                *act = Some(Act::Replace);
            }
            if Button::new(tl!("Insert After")).show(ui).clicked() {
                *act = Some(Act::InsertAfter);
            }
            if Button::new(tl!("Copy")).show(ui).clicked() {
                *act = Some(Act::Copy);
            }
        });
    }
    if let Some(why) = &app.assistant.apply_error {
        Notice::new(Tone::Bad, why.clone()).announce(Politeness::Assertive).show(ui);
    }
    let usage = fmt(
        tl!("{model}: {input} tokens in, {output} tokens out"),
        &[("model", &reply.model), ("input", &reply.usage.input_tokens.to_string()), ("output", &reply.usage.output_tokens.to_string())],
    );
    Text::new(usage).shade(Shade::Muted).show(ui);
    if reply.fallback_used {
        Text::new(tl!("The first model declined, and a fallback model wrote this reply.")).shade(Shade::Muted).show(ui);
    }
}

#[cfg(test)]
mod tests;
