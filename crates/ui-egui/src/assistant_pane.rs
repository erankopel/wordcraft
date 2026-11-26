//! The AI pane: ask for a change to the selection or the document, or a question about it, and put the
//! reply into the document.
//!
//! Text leaves the machine only through a provider, and only after the person agrees here: before the
//! first request of a run the pane says who receives the text, which model reads it and how much of it
//! goes. No command and no setting can give that agreement; only the button in the pane can. A reply is
//! placed with `assistant.apply`, so it is one undo step and tracked when Track Changes is on. Reading
//! what would be shared (`assistant.context`) and placing a reply (`assistant.apply`) are engine
//! commands; sending is this pane's alone. See `docs/assistant.md`.

use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::Duration;

use craft::theme::variant::Tone;
use craft::ui::buttons::Button;
use craft::ui::choice::Checkbox;
use craft::ui::contract::{Bind, Props as _, Show as _, ShowContainer as _};
use craft::ui::display::{Shade, Text};
use craft::ui::layout_components::{Row, Stack};
use craft::ui::notices::Notice;
use craft::ui::style::TypeRole;
use craft::ui::text::TextField;
use egui::Ui;
use serde_json::json;
use wordcraft_assistant::{DEFAULT_MODEL, Disclosure, Error, MAX_INSTRUCTION_BYTES, Provider, Reply, Request, Role, Scope, Stop, Turn, build_body};

use crate::i18n::fmt;
use crate::{WordApp, announce};

/// Makes the provider for a request. A function, so a test can put its own in the state.
type Factory = fn() -> Result<Box<dyn Provider>, Error>;

/// The Claude provider, with the key from the environment.
#[cfg(not(target_arch = "wasm32"))]
fn claude() -> Result<Box<dyn Provider>, Error> {
    Ok(Box::new(wordcraft_assistant::claude::Claude::from_env()?))
}

/// The browser version cannot send requests.
#[cfg(target_arch = "wasm32")]
fn claude() -> Result<Box<dyn Provider>, Error> {
    Err(Error::Unavailable)
}

/// A request that is ready to go and waits for the person's agreement.
struct Ready {
    provider: Box<dyn Provider>,
    request: Request,
    disclosure: Disclosure,
    /// How many characters of the document go with it.
    chars: usize,
}

/// A request in flight on its worker thread.
struct Waiting {
    rx: Receiver<Result<Reply, Error>>,
    instruction: String,
}

/// What the pane remembers for one window.
pub(crate) struct AssistantState {
    /// The person agreed, in this run, to text going to a provider that leaves the machine. Never saved.
    consented: bool,
    /// Share the whole document, not the selection.
    document_scope: bool,
    prompt: String,
    /// The earlier turns of this chat, oldest first.
    history: Vec<Turn>,
    reply: Option<Reply>,
    error: Option<Error>,
    /// Why a reply could not be placed, in the engine's words.
    apply_error: Option<String>,
    ready: Option<Ready>,
    waiting: Option<Waiting>,
    factory: Factory,
}

impl Default for AssistantState {
    fn default() -> AssistantState {
        AssistantState {
            consented: false,
            document_scope: false,
            prompt: String::new(),
            history: Vec::new(),
            reply: None,
            error: None,
            apply_error: None,
            ready: None,
            waiting: None,
            factory: claude,
        }
    }
}

impl AssistantState {
    /// A new chat for a new document: what the person agreed to and chose stays, the conversation goes.
    pub(crate) fn fresh_for_document(old: &AssistantState) -> AssistantState {
        AssistantState { consented: old.consented, document_scope: old.document_scope, factory: old.factory, ..AssistantState::default() }
    }
}

/// What a click in the pane asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Act {
    Send,
    Agree,
    Cancel,
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
                self.assistant.consented = true;
                if let Some(ready) = self.assistant.ready.take() {
                    self.assistant_send(ready, ctx);
                }
            }
            Act::Cancel => self.assistant.ready = None,
            Act::NewChat => self.assistant = AssistantState::fresh_for_document(&self.assistant),
            Act::Replace => self.assistant_apply("replaceSelection", ctx),
            Act::InsertAfter => self.assistant_apply("insertAfter", ctx),
            Act::Copy => {
                if let Some(reply) = &self.assistant.reply {
                    ctx.copy_text(reply.text.clone());
                }
            }
        }
    }

    /// Gather what would be shared and make the provider. A provider that leaves the machine waits for
    /// the person's agreement the first time; every other request goes at once.
    fn assistant_ask(&mut self, ctx: &egui::Context) {
        if self.assistant.waiting.is_some() {
            return;
        }
        self.assistant.error = None;
        self.assistant.apply_error = None;
        let instruction = self.assistant.prompt.trim().to_owned();
        if instruction.is_empty() {
            self.assistant.error = Some(Error::EmptyInstruction);
            return;
        }
        let scope = if self.assistant.document_scope { Scope::Document } else { Scope::Selection };
        let context = match self.run("assistant.context", json!({"scope": scope.as_str()})) {
            Ok(shared) => shared.get("text").and_then(|t| t.as_str()).unwrap_or_default().to_owned(),
            Err(why) => {
                self.assistant.error = Some(Error::BadReply(why.to_string()));
                return;
            }
        };
        let provider = match (self.assistant.factory)() {
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
        let ready = Ready { provider, request, disclosure, chars };
        if ready.disclosure.leaves_machine && !self.assistant.consented {
            self.assistant.ready = Some(ready);
        } else {
            self.assistant_send(ready, ctx);
        }
    }

    /// Send on a worker thread, so the window keeps drawing while the reply is written.
    fn assistant_send(&mut self, mut ready: Ready, ctx: &egui::Context) {
        let (tx, rx) = channel();
        let instruction = ready.request.instruction.clone();
        let repaint = ctx.clone();
        let started = std::thread::Builder::new().name("assistant".into()).spawn(move || {
            let result = ready.provider.send(&ready.request);
            let _ = tx.send(result);
            repaint.request_repaint();
        });
        match started {
            Ok(_) => self.assistant.waiting = Some(Waiting { rx, instruction }),
            Err(_) => self.assistant.error = Some(Error::Unavailable),
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
            Err(TryRecvError::Disconnected) => Some(Err(Error::Network("the request ended without an answer".into()))),
        };
        let Some(result) = outcome else { return };
        let instruction = waiting.instruction.clone();
        self.assistant.waiting = None;
        match result {
            Ok(reply) => {
                if !reply.text.is_empty() {
                    self.assistant.history.push(Turn { role: Role::User, text: instruction });
                    self.assistant.history.push(Turn { role: Role::Assistant, text: reply.text.clone() });
                }
                self.assistant.reply = Some(reply);
                announce::say(self, tl!("The AI reply is ready.").to_owned(), ctx);
            }
            Err(error) => {
                announce::say(self, error_text(&error), ctx);
                self.assistant.error = Some(error);
            }
        }
    }

    /// Put the reply into the document through `assistant.apply`.
    fn assistant_apply(&mut self, mode: &str, ctx: &egui::Context) {
        let Some(text) = self.assistant.reply.as_ref().map(|reply| reply.text.clone()).filter(|text| !text.trim().is_empty()) else { return };
        self.assistant.apply_error = None;
        match self.run("assistant.apply", json!({"text": text, "mode": mode})) {
            Ok(_) => announce::say(self, tl!("The reply was placed in the document.").to_owned(), ctx),
            Err(why) => self.assistant.apply_error = Some(why.to_string()),
        }
    }
}

/// What a failed request says, in words for the person and never with the key.
fn error_text(error: &Error) -> String {
    let kb = |bytes: usize| (bytes / 1000).to_string();
    match error {
        Error::MissingKey => tl!("No API key found. Set the ANTHROPIC_API_KEY environment variable and start WordCraft again.").to_owned(),
        Error::EmptyInstruction => tl!("Write what you want first.").to_owned(),
        Error::TooLarge { what: "the instruction", bytes, limit } => {
            fmt(tl!("The instruction is too long ({size} KB; the most is {limit} KB)."), &[("size", &kb(*bytes)), ("limit", &kb(*limit))])
        }
        Error::TooLarge { bytes, limit, .. } => fmt(
            tl!("The text is too large to send ({size} KB; the most is {limit} KB). Select less text."),
            &[("size", &kb(*bytes)), ("limit", &kb(*limit))],
        ),
        Error::Network(why) => fmt(tl!("The service could not be reached: {why}"), &[("why", why)]),
        Error::Http { status: 401 | 403, .. } => tl!("The API key was not accepted. Check the ANTHROPIC_API_KEY environment variable.").to_owned(),
        Error::Http { status: 429, .. } => tl!("The service is busy, or the usage limit was reached. Try again later.").to_owned(),
        Error::Http { status, .. } if *status >= 500 => {
            fmt(tl!("The service had a problem ({status}). Try again."), &[("status", &status.to_string())])
        }
        Error::Http { status, message, .. } => {
            fmt(tl!("The service refused the request ({status}): {message}"), &[("status", &status.to_string()), ("message", message)])
        }
        Error::BadReply(why) => fmt(tl!("The reply could not be read: {why}"), &[("why", why)]),
        Error::Unavailable => tl!("The AI assistant is not available in this version of WordCraft.").to_owned(),
    }
}

/// Whether a key is in place, said before the person writes anything.
fn key_note() -> String {
    #[cfg(not(target_arch = "wasm32"))]
    {
        if wordcraft_assistant::claude::Claude::key_present() {
            tl!("An API key was found in the ANTHROPIC_API_KEY environment variable.").to_owned()
        } else {
            tl!("No API key found. Set the ANTHROPIC_API_KEY environment variable and start WordCraft again.").to_owned()
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        tl!("The AI assistant is not available in the browser version.").to_owned()
    }
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

fn body(app: &mut WordApp, ui: &mut Ui, act: &mut Option<Act>) {
    Text::new(tl!("Ask for a change to your text, or a question about it. A reply is placed only when you choose.")).shade(Shade::Muted).show(ui);
    Text::new(key_note()).shade(Shade::Muted).show(ui);
    let waiting = app.assistant.waiting.is_some();
    Checkbox::new(Bind::bound(&mut app.assistant.document_scope))
        .label(tl!("Use the whole document instead of the selection"))
        .enabled(!waiting)
        .show(ui);
    TextField::new(tl!("Instruction"), Bind::bound(&mut app.assistant.prompt))
        .lines(4)
        .limit(MAX_INSTRUCTION_BYTES)
        .placeholder(tl!("For example: make this paragraph shorter"))
        .enabled(!waiting)
        .id_salt("assistant.prompt")
        .show(ui);
    let can_send = !waiting && app.assistant.ready.is_none() && !app.assistant.prompt.trim().is_empty();
    Row::new().show(ui, |ui, _| {
        if Button::new(tl!("Send")).enabled(can_send).show(ui).clicked() {
            *act = Some(Act::Send);
        }
        if Button::new(tl!("New Chat")).enabled(!waiting).show(ui).clicked() {
            *act = Some(Act::NewChat);
        }
    });
    if waiting {
        Text::new(tl!("Waiting for the reply…")).shade(Shade::Muted).show(ui);
    }
    consent(app, ui, act);
    if let Some(error) = &app.assistant.error {
        Notice::new(Tone::Bad, error_text(error)).show(ui);
    }
    reply(app, ui, act);
}

/// Who gets the text, said before it is sent, with the buttons that agree or decline.
fn consent(app: &mut WordApp, ui: &mut Ui, act: &mut Option<Act>) {
    let Some(ready) = &app.assistant.ready else { return };
    let what = if app.assistant.document_scope { tl!("the whole document") } else { tl!("the selected text") };
    let template = tl!(
        "Your instruction and {what} ({chars} characters) will be sent over the internet to {recipient}, to be read by the model {model}. WordCraft keeps no copy. Nothing is sent until you agree."
    );
    let note = fmt(
        template,
        &[("what", what), ("chars", &ready.chars.to_string()), ("recipient", &ready.disclosure.recipient), ("model", &ready.disclosure.model)],
    );
    Notice::new(Tone::Info, note).show(ui);
    Row::new().show(ui, |ui, _| {
        if Button::new(tl!("Agree and Send")).show(ui).clicked() {
            *act = Some(Act::Agree);
        }
        if Button::new(tl!("Cancel")).show(ui).clicked() {
            *act = Some(Act::Cancel);
        }
    });
}

/// The reply, what ended it, and the buttons that place it.
fn reply(app: &mut WordApp, ui: &mut Ui, act: &mut Option<Act>) {
    let Some(reply) = &app.assistant.reply else { return };
    Text::new(tl!("Reply")).role(TypeRole::Heading).show(ui);
    match &reply.stop {
        Stop::Refusal { .. } => {
            Notice::new(Tone::Warn, tl!("The model declined this request, so there is no reply to place.")).show(ui);
        }
        Stop::MaxTokens => {
            Notice::new(Tone::Warn, tl!("The reply was cut off at its length limit. Check it before you place it.")).show(ui);
        }
        _ => {}
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
        Notice::new(Tone::Bad, why.clone()).show(ui);
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
