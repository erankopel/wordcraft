use std::cell::RefCell;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use std::collections::VecDeque;

use serde_json::json;
use wordcraft_assistant::agent::{Step, StepStop, ToolRequest};
use wordcraft_assistant::{Disclosure, Error, MAX_CONTEXT_BYTES, Provider, Reply, Request, Scope, Stop, Usage};
use wordcraft_doc::StoryRef;

use super::{Act, Agreed, WordApp};

/// What the scripted provider answers and whether its text leaves the machine, and what it was asked.
#[derive(Clone)]
struct Script {
    remote: bool,
    result: Result<Reply, Error>,
    sent: Arc<Mutex<Vec<Request>>>,
    /// While true, `send` does not return: a request that takes a long time.
    hold: Arc<std::sync::atomic::AtomicBool>,
    /// The turns a request that changes the document gets, in order.
    turns: Arc<Mutex<VecDeque<Result<Step, Error>>>>,
    /// How many tool requests were sent.
    tool_requests: Arc<Mutex<usize>>,
    /// The first message of each tool request: what the model is told.
    first_messages: Arc<Mutex<Vec<String>>>,
}

thread_local! {
    /// The script the next provider made on this thread follows (the factory is a plain function).
    static NEXT: RefCell<Option<Script>> = const { RefCell::new(None) };
    /// The keys the factory was given, in order.
    static KEYS: RefCell<Vec<Option<String>>> = const { RefCell::new(Vec::new()) };
}

struct Scripted(Script);

impl Provider for Scripted {
    fn disclosure(&self) -> Disclosure {
        Disclosure { recipient: "example.test".into(), model: "test-model".into(), leaves_machine: self.0.remote, fallback_possible: self.0.remote }
    }

    fn send(&mut self, request: &Request) -> Result<Reply, Error> {
        self.0.sent.lock().unwrap().push(request.clone());
        let until = Instant::now() + Duration::from_secs(10);
        while self.0.hold.load(std::sync::atomic::Ordering::SeqCst) && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(2));
        }
        self.0.result.clone()
    }

    fn send_tools(&mut self, request: &ToolRequest) -> Result<Step, Error> {
        *self.0.tool_requests.lock().unwrap() += 1;
        self.0.first_messages.lock().unwrap().push(request.messages.first().cloned().unwrap_or_default());
        let until = Instant::now() + Duration::from_secs(10);
        while self.0.hold.load(std::sync::atomic::Ordering::SeqCst) && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(2));
        }
        self.0.turns.lock().unwrap().pop_front().unwrap_or_else(|| Err(Error::Network("no turn left".into())))
    }
}

fn scripted(key: Option<&str>) -> Result<Box<dyn Provider>, Error> {
    KEYS.with(|keys| keys.borrow_mut().push(key.map(str::to_owned)));
    match NEXT.with(|next| next.borrow().clone()) {
        Some(script) => Ok(Box::new(Scripted(script))),
        None => Err(Error::MissingKey),
    }
}

fn reply(text: &str) -> Reply {
    Reply {
        text: text.into(),
        stop: Stop::End,
        usage: Usage { input_tokens: 10, output_tokens: 3, ..Usage::default() },
        model: "test-model".into(),
        fallback_used: false,
    }
}

fn script(remote: bool, result: Result<Reply, Error>) -> Script {
    Script {
        remote,
        result,
        sent: Arc::default(),
        hold: Arc::default(),
        turns: Arc::default(),
        tool_requests: Arc::default(),
        first_messages: Arc::default(),
    }
}

/// What agreeing to the scripted provider that leaves the machine records.
fn test_agreement() -> Agreed {
    Agreed { recipient: "example.test".into(), model: "test-model".into() }
}

/// A script for a request that changes the document: the model's turns, in order.
fn edit_script(remote: bool, turns: Vec<Result<Step, Error>>) -> Script {
    let s = script(remote, Err(Error::Network("text mode is not used here".into())));
    *s.turns.lock().unwrap() = turns.into();
    s
}

/// An app showing `text` whose provider follows `script` (no script: no key). These tests are of text
/// answers, so the assistant does not change the document itself unless a test turns that on.
fn app(text: &str, script: Option<Script>) -> (WordApp, Arc<Mutex<Vec<Request>>>) {
    let sent = script.as_ref().map(|s| s.sent.clone()).unwrap_or_default();
    NEXT.with(|next| *next.borrow_mut() = script);
    let mut app = WordApp::new(wordcraft_engine::Session::new(wordcraft_doc::Document::from_text(text)), crate::Services::default());
    app.assistant.factory = scripted;
    app.assistant_run.choices.edits = false;
    (app, sent)
}

/// An app whose assistant changes the document itself, following `script`.
fn edit_app(text: &str, script: Script) -> WordApp {
    let (mut app, _) = app(text, Some(script));
    app.assistant_run.choices.edits = true;
    app
}

fn body(app: &WordApp) -> String {
    app.session.doc.final_text(StoryRef::Body)
}

/// Wait for the worker thread's answer and take it.
fn settle(app: &mut WordApp, ctx: &egui::Context) {
    let until = Instant::now() + Duration::from_secs(10);
    while app.assistant.waiting.is_some() && Instant::now() < until {
        app.assistant_poll(ctx);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(app.assistant.waiting.is_none(), "the request finished");
}

fn ask(app: &mut WordApp, ctx: &egui::Context, prompt: &str) {
    app.assistant.prompt = prompt.into();
    app.assistant_act(Act::Send, ctx);
}

#[test]
fn a_provider_that_stays_on_this_machine_is_asked_at_once_with_the_selection() {
    let ctx = egui::Context::default();
    let (mut app, sent) = app("Hello brave world", Some(script(false, Ok(reply("kind")))));
    app.run("select.text", json!({"text": "brave"})).unwrap();
    ask(&mut app, &ctx, "Use a gentler word");
    assert!(app.assistant.ready.is_none(), "nothing to agree to: the text stays here");
    settle(&mut app, &ctx);
    let seen = sent.lock().unwrap().clone();
    assert_eq!(seen.len(), 1);
    assert_eq!((seen[0].context.as_str(), seen[0].instruction.as_str()), ("brave", "Use a gentler word"), "the selection, not the document");
    assert_eq!(app.assistant.reply.as_ref().map(|r| r.text.as_str()), Some("kind"));
    app.assistant_act(Act::Replace, &ctx);
    assert_eq!(body(&app), "Hello kind world");
    assert!(app.assistant.apply_error.is_none());
}

/// The first request that would leave the machine waits for the person; nothing is sent before.
#[test]
fn a_provider_that_leaves_the_machine_waits_for_agreement_and_is_asked_once() {
    let ctx = egui::Context::default();
    let (mut app, sent) = app("Some text", Some(script(true, Ok(reply("Reply")))));
    app.run("select.all", json!({})).unwrap();
    ask(&mut app, &ctx, "Summarise");
    let ready = app.assistant.ready.as_ref().expect("the request waits");
    assert_eq!((ready.disclosure.recipient.as_str(), ready.disclosure.model.as_str(), ready.chars), ("example.test", "test-model", 9));
    assert!(app.assistant.waiting.is_none() && sent.lock().unwrap().is_empty(), "nothing is sent before the person agrees");
    app.assistant_act(Act::Agree, &ctx);
    settle(&mut app, &ctx);
    assert_eq!(sent.lock().unwrap().len(), 1);
    assert_eq!(app.assistant_run.choices.agreed, Some(test_agreement()));
    // Agreed once: the next request is not asked about again.
    ask(&mut app, &ctx, "Shorter");
    assert!(app.assistant.ready.is_none(), "no second question");
    settle(&mut app, &ctx);
    assert_eq!(sent.lock().unwrap().len(), 2);
}

#[test]
fn declining_sends_nothing_and_agrees_to_nothing() {
    let ctx = egui::Context::default();
    let (mut app, sent) = app("Some text", Some(script(true, Ok(reply("Reply")))));
    app.run("select.all", json!({})).unwrap();
    ask(&mut app, &ctx, "Summarise");
    app.assistant_act(Act::Cancel, &ctx);
    assert!(app.assistant.ready.is_none() && app.assistant.waiting.is_none());
    assert_eq!(app.assistant_run.choices.agreed, None);
    assert!(sent.lock().unwrap().is_empty());
    // Asking again still asks.
    ask(&mut app, &ctx, "Summarise");
    assert!(app.assistant.ready.is_some());
}

/// There is no way to agree except the Agree button: a request does not start by itself, and
/// no value in the state can stand in for the click.
#[test]
fn a_request_that_leaves_the_machine_never_starts_without_the_agree_click() {
    let ctx = egui::Context::default();
    let (mut app, sent) = app("Some text", Some(script(true, Ok(reply("Reply")))));
    app.run("select.all", json!({})).unwrap();
    ask(&mut app, &ctx, "Summarise");
    for _ in 0..50 {
        app.assistant_poll(&ctx);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(sent.lock().unwrap().is_empty(), "waiting is not agreeing");
    // Sending again while it waits does not slip past it either.
    app.assistant_act(Act::Send, &ctx);
    assert!(sent.lock().unwrap().is_empty());
}

#[test]
fn a_missing_key_is_reported_before_anything_is_asked() {
    let ctx = egui::Context::default();
    let (mut app, _) = app("Some text", None);
    ask(&mut app, &ctx, "Summarise");
    assert_eq!(app.assistant.error, Some(Error::MissingKey));
    assert!(app.assistant.ready.is_none() && app.assistant.waiting.is_none());
}

#[test]
fn an_empty_or_oversized_request_is_refused_before_any_agreement() {
    let ctx = egui::Context::default();
    let (mut app, sent) = app("Some text", Some(script(true, Ok(reply("Reply")))));
    ask(&mut app, &ctx, "   ");
    assert_eq!(app.assistant.error, Some(Error::EmptyInstruction));
    assert!(app.assistant.ready.is_none());
    let (mut big, sent_big) = self::app(&"x".repeat(MAX_CONTEXT_BYTES + 1), Some(script(true, Ok(reply("Reply")))));
    big.assistant_run.choices.whole_document = true;
    ask(&mut big, &ctx, "Summarise");
    assert!(matches!(big.assistant.error, Some(Error::TooLarge { .. })), "{:?}", big.assistant.error);
    assert!(big.assistant.ready.is_none(), "the person is not asked to agree to what cannot be sent");
    assert!(sent.lock().unwrap().is_empty() && sent_big.lock().unwrap().is_empty());
}

#[test]
fn the_whole_document_is_shared_only_when_chosen() {
    let ctx = egui::Context::default();
    let (mut app, sent) = app("First paragraph\nSecond paragraph", Some(script(false, Ok(reply("ok")))));
    app.run("select.text", json!({"text": "First"})).unwrap();
    app.assistant_run.choices.whole_document = true;
    ask(&mut app, &ctx, "Check spelling");
    settle(&mut app, &ctx);
    assert_eq!(sent.lock().unwrap()[0].context, "First paragraph\nSecond paragraph");
}

#[test]
fn a_chat_carries_its_earlier_turns_and_a_new_chat_drops_them() {
    let ctx = egui::Context::default();
    let (mut app, sent) = app("Some text", Some(script(true, Ok(reply("First answer")))));
    app.run("select.all", json!({})).unwrap();
    ask(&mut app, &ctx, "First question");
    app.assistant_act(Act::Agree, &ctx);
    settle(&mut app, &ctx);
    ask(&mut app, &ctx, "Second question");
    settle(&mut app, &ctx);
    let history: Vec<(String, String)> = sent.lock().unwrap()[1].history.iter().map(|t| (format!("{:?}", t.role), t.text.clone())).collect();
    assert_eq!(history, [("User".to_owned(), "First question".to_owned()), ("Assistant".to_owned(), "First answer".to_owned())]);
    app.assistant_act(Act::NewChat, &ctx);
    assert!(app.assistant.history.is_empty() && app.assistant.reply.is_none());
    assert!(app.assistant_run.choices.agreed.is_some(), "what was agreed stays agreed");
    ask(&mut app, &ctx, "Third question");
    settle(&mut app, &ctx);
    assert!(sent.lock().unwrap()[2].history.is_empty(), "a new chat starts empty");
}

#[test]
fn a_declined_reply_has_nothing_to_place_and_is_not_added_to_the_chat() {
    let ctx = egui::Context::default();
    let declined = Reply { text: String::new(), stop: Stop::Refusal { category: Some("cyber".into()) }, ..reply("") };
    let (mut app, _) = app("Hello brave world", Some(script(false, Ok(declined))));
    app.run("select.text", json!({"text": "brave"})).unwrap();
    ask(&mut app, &ctx, "Anything");
    settle(&mut app, &ctx);
    assert!(app.assistant.history.is_empty());
    app.assistant_act(Act::Replace, &ctx);
    assert_eq!(body(&app), "Hello brave world", "there was nothing to place");
}

#[test]
fn a_service_error_is_shown_in_words_and_never_with_a_key() {
    let ctx = egui::Context::default();
    let failure = Error::Http { status: 401, kind: "authentication_error".into(), message: "invalid x-api-key sk-ant-secret".into() };
    let (mut app, _) = app("Some text", Some(script(false, Err(failure))));
    app.run("select.all", json!({})).unwrap();
    ask(&mut app, &ctx, "Summarise");
    settle(&mut app, &ctx);
    let shown = super::error_text(app.assistant.error.as_ref().unwrap());
    assert!(shown.contains("API key") && !shown.contains("sk-ant-secret"), "{shown}");
    // A 403 is not a wrong key: the service's own reason is shown.
    let forbidden = Error::Http { status: 403, kind: "permission_error".into(), message: "This organisation may not use the model".into() };
    let shown = super::error_text(&forbidden);
    assert!(shown.contains("may not use the model") && !shown.contains("key was not accepted"), "{shown}");
}

#[test]
fn a_reply_can_be_added_after_the_paragraph_and_undone_in_one_step() {
    let ctx = egui::Context::default();
    let (mut app, _) = app("Hello brave world\nNext", Some(script(false, Ok(reply("Added line")))));
    app.run("select.text", json!({"text": "brave"})).unwrap();
    ask(&mut app, &ctx, "Add a line");
    settle(&mut app, &ctx);
    app.assistant_act(Act::InsertAfter, &ctx);
    assert_eq!(body(&app), "Hello brave world\nAdded line\nNext");
    app.run("edit.undo", json!({})).unwrap();
    assert_eq!(body(&app), "Hello brave world\nNext");
}

#[test]
fn a_new_document_starts_a_new_chat_but_keeps_what_was_agreed() {
    let ctx = egui::Context::default();
    let (mut app, _) = app("Some text", Some(script(true, Ok(reply("Answer")))));
    app.run("select.all", json!({})).unwrap();
    ask(&mut app, &ctx, "Question");
    app.assistant_act(Act::Agree, &ctx);
    settle(&mut app, &ctx);
    let fresh = super::AssistantState::fresh_for_document(&app.assistant);
    assert!(fresh.history.is_empty() && fresh.reply.is_none() && fresh.prompt.is_empty());
    assert!(app.assistant_run.choices.agreed.is_some(), "the agreement is not the chat's");
}

/// The pane opens from its command and draws its controls. (Its layout in the right-to-left interfaces is checked
/// by `i18n::audit::the_arabic_interface_is_mirrored_and_never_panics` and the Hebrew one, which open every
/// `view.*Pane`.)
#[test]
#[cfg(not(target_arch = "wasm32"))]
fn the_pane_opens_with_its_command_and_shows_its_controls() {
    use egui_kittest::kittest::Queryable;

    let (mut app, _) = app("Hello", Some(script(false, Ok(reply("ok")))));
    app.run("view.assistantPane", json!({"value": true})).unwrap();
    assert!(crate::panes::open(&app).contains(&crate::panes::Pane::Assistant));
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1440.0, 900.0)).build_ui_state(
        |ui, app: &mut WordApp| {
            let ctx = ui.ctx().clone();
            app.logic(&ctx);
            app.ui(ui);
        },
        app,
    );
    h.run_steps(4);
    for label in ["Send", "New Chat", "Instruction:", "API key:", "Use the whole document instead of the selection"] {
        h.get_by_label(label);
    }
    h.state_mut().run("view.assistantPane", json!({"value": false})).unwrap();
    h.run_steps(2);
    assert!(!crate::panes::open(h.state()).contains(&crate::panes::Pane::Assistant), "the command closes it");
}

/// One agreement covers every later request: a selection, the whole document, and changes made by the
/// assistant; each still goes only when the person presses Send.
#[test]
fn one_agreement_covers_every_later_request() {
    let ctx = egui::Context::default();
    let script = script(true, Ok(reply("Reply")));
    *script.turns.lock().unwrap() = vec![Ok(Step::text("Nothing to change."))].into();
    let tool_requests = script.tool_requests.clone();
    let (mut app, sent) = app("First paragraph\nSecond paragraph", Some(script));
    app.run("select.text", json!({"text": "First"})).unwrap();
    ask(&mut app, &ctx, "One");
    assert!(app.assistant.ready.is_some(), "the first request asks");
    app.assistant_act(Act::Agree, &ctx);
    settle(&mut app, &ctx);
    app.assistant_run.choices.whole_document = true;
    ask(&mut app, &ctx, "Two");
    assert!(app.assistant.ready.is_none(), "the whole document is covered");
    settle(&mut app, &ctx);
    assert_eq!(sent.lock().unwrap().len(), 2);
    app.assistant_run.choices.edits = true;
    ask(&mut app, &ctx, "Three");
    assert!(app.assistant.ready.is_none(), "so are changes made by the assistant");
    settle(&mut app, &ctx);
    assert_eq!(*tool_requests.lock().unwrap(), 1);
}

/// An agreement names the recipient and the model: a request to another model asks again.
#[test]
fn an_agreement_for_another_model_asks_again() {
    let ctx = egui::Context::default();
    let (mut app, sent) = app("Some text", Some(script(true, Ok(reply("Reply")))));
    app.assistant_run.choices.agreed = Some(Agreed { recipient: "example.test".into(), model: "an-older-model".into() });
    app.run("select.all", json!({})).unwrap();
    ask(&mut app, &ctx, "Summarise");
    assert!(app.assistant.ready.is_some() && sent.lock().unwrap().is_empty());
}

/// The key the person types goes to the provider (blank means: look in the environment), is never
/// printed, and stays for the run when a chat starts again.
#[test]
fn a_typed_key_reaches_the_provider_and_is_never_printed() {
    let ctx = egui::Context::default();
    KEYS.with(|keys| keys.borrow_mut().clear());
    let (mut app, _) = app("Some text", Some(script(false, Ok(reply("ok")))));
    app.run("select.all", json!({})).unwrap();
    ask(&mut app, &ctx, "One");
    settle(&mut app, &ctx);
    app.assistant_run.key = wordcraft_engine::Password::new("  sk-ant-typed-key \n");
    ask(&mut app, &ctx, "Two");
    settle(&mut app, &ctx);
    app.assistant_run.key = wordcraft_engine::Password::new("   ");
    ask(&mut app, &ctx, "Three");
    settle(&mut app, &ctx);
    let keys = KEYS.with(|keys| keys.borrow().clone());
    assert_eq!(keys, [None, Some("sk-ant-typed-key".to_owned()), None], "no key, a trimmed key, a blank one");
    app.assistant_run.key = wordcraft_engine::Password::new("sk-ant-typed-key");
    assert!(!format!("{:?}", app.assistant_run.key).contains("sk-ant"), "the key does not print");
    app.assistant_act(Act::NewChat, &ctx);
    assert_eq!(app.assistant_run.typed_key(), Some("sk-ant-typed-key"), "a new chat keeps the key for this run");
}

/// A request in flight can be given up on. Its answer, when it comes, changes nothing, and a new
/// request can be made at once.
#[test]
fn a_person_can_stop_waiting_and_a_late_reply_changes_nothing() {
    let ctx = egui::Context::default();
    let slow = script(false, Ok(reply("Late reply")));
    slow.hold.store(true, std::sync::atomic::Ordering::SeqCst);
    let (hold, sent_by_slow) = (slow.hold.clone(), slow.sent.clone());
    let (mut app, _) = app("Some text", Some(slow));
    app.run("select.all", json!({})).unwrap();
    ask(&mut app, &ctx, "Summarise");
    assert!(app.assistant.waiting.is_some());
    let until = Instant::now() + Duration::from_secs(10);
    while sent_by_slow.lock().unwrap().is_empty() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(2));
    }
    app.assistant_act(Act::Stop, &ctx);
    assert!(app.assistant.waiting.is_none() && app.assistant.stopped);
    hold.store(false, std::sync::atomic::Ordering::SeqCst);
    for _ in 0..40 {
        app.assistant_poll(&ctx);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(app.assistant.reply.is_none() && app.assistant.history.is_empty(), "a reply nobody waits for is dropped");
    assert!(app.assistant.said.contains("stopped"), "the stop is announced");
    ask(&mut app, &ctx, "Again");
    assert!(!app.assistant.stopped, "a new request starts clean");
    settle(&mut app, &ctx);
    assert_eq!(app.assistant.reply.as_ref().map(|r| r.text.as_str()), Some("Late reply"));
}

/// What a screen reader hears when a reply arrives says what arrived: a refusal is not announced as a reply
/// that is ready, and a cut-off or unfinished one says so.
#[test]
fn the_announcement_says_what_arrived() {
    let ctx = egui::Context::default();
    let with = |text: &str, stop: Stop| Reply { text: text.into(), stop, ..reply(text) };
    let cases = [
        (with("", Stop::Refusal { category: None }), "declined"),
        (with("Part of it", Stop::MaxTokens), "cut off"),
        (with("", Stop::MaxTokens), "ran out of length"),
        (with("Part", Stop::Other("model_context_window_exceeded".into())), "incomplete"),
        (with("Done", Stop::End), "is ready."),
    ];
    let mut heard = Vec::new();
    for (result, expected) in cases {
        let (mut app, _) = app("Hello", Some(script(false, Ok(result))));
        ask(&mut app, &ctx, "Anything");
        settle(&mut app, &ctx);
        let said = app.assistant.said.clone();
        assert!(said.contains(expected), "{expected}: {said}");
        heard.push(said);
    }
    assert!(!heard[0].contains("is ready") && !heard[2].contains("is ready"), "a reply that is not there is not announced as ready: {heard:?}");
}

/// A reply to a selection is placed only in that text: if the selection moved, or the document was edited,
/// since the request, nothing is placed and the person is told. Selecting the same text again is enough.
#[test]
fn a_reply_to_a_selection_is_not_placed_in_other_text() {
    let ctx = egui::Context::default();
    let (mut app, _) = app("Hello brave world and more words", Some(script(false, Ok(reply("kind")))));
    app.run("select.text", json!({"text": "brave"})).unwrap();
    ask(&mut app, &ctx, "A gentler word");
    settle(&mut app, &ctx);
    app.run("select.text", json!({"text": "more"})).unwrap();
    app.assistant_act(Act::Replace, &ctx);
    assert_eq!(body(&app), "Hello brave world and more words", "other text was selected: nothing was replaced");
    assert!(app.assistant.apply_error.is_some(), "and the person is told");
    app.assistant_act(Act::InsertAfter, &ctx);
    assert_eq!(body(&app), "Hello brave world and more words", "Insert After is held back the same way");
    app.run("select.text", json!({"text": "brave"})).unwrap();
    app.assistant_act(Act::Replace, &ctx);
    assert_eq!(body(&app), "Hello kind world and more words", "the same text selected again: placed");
    assert!(app.assistant.apply_error.is_none());
    // An edit elsewhere in between holds it back too, even with the same text selected again.
    let (mut edited, _) = self::app("Hello brave world and more words", Some(script(false, Ok(reply("kind")))));
    edited.run("select.text", json!({"text": "brave"})).unwrap();
    ask(&mut edited, &ctx, "A gentler word");
    settle(&mut edited, &ctx);
    edited.run("select.text", json!({"text": "more"})).unwrap();
    edited.run("text.insert", json!({"text": "extra"})).unwrap();
    edited.run("select.text", json!({"text": "brave"})).unwrap();
    edited.assistant_act(Act::Replace, &ctx);
    assert!(body(&edited).contains("brave") && edited.assistant.apply_error.is_some(), "{}", body(&edited));
}

/// A reply to the whole document is the person's to put wherever the caret is: no text was asked about.
#[test]
fn a_reply_to_the_whole_document_can_be_placed_at_any_selection() {
    let ctx = egui::Context::default();
    let (mut app, _) = app("First paragraph\nSecond paragraph", Some(script(false, Ok(reply("Summary")))));
    app.assistant_run.choices.whole_document = true;
    ask(&mut app, &ctx, "Summarise");
    settle(&mut app, &ctx);
    app.run("select.text", json!({"text": "Second"})).unwrap();
    app.assistant_act(Act::Replace, &ctx);
    assert_eq!(body(&app), "First paragraph\nSummary paragraph");
    assert!(app.assistant.apply_error.is_none());
}

/// A new document starts a new chat (in a new window of the desktop app, or in the same one), and what the
/// person agreed to and typed as the key stays for the run, in every window.
#[test]
fn a_new_document_in_the_window_starts_a_new_chat_and_keeps_the_run_s_agreement() {
    let ctx = egui::Context::default();
    let (mut app, _) = app("Some text", Some(script(true, Ok(reply("Answer")))));
    let frame = |app: &mut WordApp| {
        ctx.run_ui(egui::RawInput::default(), |ui| app.logic(ui.ctx())).drop_without_applying_deltas();
    };
    frame(&mut app);
    app.run("select.all", json!({})).unwrap();
    ask(&mut app, &ctx, "Question");
    app.assistant_act(Act::Agree, &ctx);
    settle(&mut app, &ctx);
    app.assistant_run.key = wordcraft_engine::Password::new("sk-ant-typed-key");
    assert!(!app.assistant.history.is_empty() && app.assistant.reply.is_some());
    app.run("file.new", json!({})).unwrap();
    frame(&mut app);
    assert!(app.assistant.history.is_empty() && app.assistant.reply.is_none(), "the chat of the other document is gone");
    assert!(app.assistant_run.choices.agreed.is_some(), "what was agreed stays agreed");
    assert_eq!(app.assistant_run.typed_key(), Some("sk-ant-typed-key"), "and so does the key");
}

/// A failure replaces what the status line said while the request was waiting, and a second failure that reads
/// the same is a new alert for a screen reader.
#[test]
fn a_failed_request_does_not_leave_waiting_in_the_status_line() {
    let ctx = egui::Context::default();
    let (mut app, _) = app("Some text", Some(script(false, Err(Error::Network("down".into())))));
    app.run("select.all", json!({})).unwrap();
    ask(&mut app, &ctx, "One");
    assert!(app.assistant.said.contains("Waiting"), "while it runs");
    settle(&mut app, &ctx);
    assert!(app.assistant.error.is_some());
    assert!(!app.assistant.said.contains("Waiting"), "the failure is not still waiting: {:?}", app.assistant.said);
    let mark = app.assistant.alert_mark;
    ask(&mut app, &ctx, "Again");
    assert_ne!(app.assistant.alert_mark, mark, "a repeat is marked as new");
}

/// With nothing selected the reply is about no text, so it can be placed wherever the caret has gone.
#[test]
fn a_reply_with_nothing_selected_is_not_tied_to_the_caret() {
    let ctx = egui::Context::default();
    let (mut app, _) = app("First line\nSecond line", Some(script(false, Ok(reply("Added")))));
    app.run("caret.docStart", json!({})).unwrap();
    ask(&mut app, &ctx, "Write a line");
    settle(&mut app, &ctx);
    assert!(app.assistant.basis.is_none(), "nothing was selected, so nothing to guard");
    app.run("caret.docEnd", json!({})).unwrap();
    app.assistant_act(Act::InsertAfter, &ctx);
    assert!(app.assistant.apply_error.is_none(), "{:?}", app.assistant.apply_error);
    assert!(body(&app).contains("Added"));
}

/// A size is rounded up so the message never says the text is too large at the size that is allowed.
#[test]
fn a_size_message_never_states_equal_numbers() {
    let text = super::error_text(&Error::TooLarge { what: "the instruction", bytes: 20_500, limit: 20_000 });
    assert!(text.contains("21 KB") && text.contains("20 KB"), "{text}");
}

/// What was said about the last request is not said of the next one: a new attempt starts with a quiet status
/// line, and the same line twice is a new announcement.
#[test]
fn a_new_attempt_clears_the_status_line_and_a_repeated_line_is_spoken_again() {
    let ctx = egui::Context::default();
    let (mut app, _) = app("Some text", Some(script(false, Ok(reply("ok")))));
    app.run("select.all", json!({})).unwrap();
    ask(&mut app, &ctx, "One");
    settle(&mut app, &ctx);
    assert!(app.assistant.said.contains("ready"), "{:?}", app.assistant.said);
    NEXT.with(|next| *next.borrow_mut() = None);
    ask(&mut app, &ctx, "Two");
    assert!(app.assistant.error.is_some() && app.assistant.said.is_empty(), "nothing from the last request is still said: {:?}", app.assistant.said);
    // Copy twice says the same words: the second is marked so a screen reader reads it again.
    app.assistant_act(Act::Copy, &ctx);
    let first = app.assistant.said.clone();
    app.assistant_act(Act::Copy, &ctx);
    assert_ne!(app.assistant.said, first);
    assert_eq!(app.assistant.said.trim_end_matches('\u{200b}'), first);
}

/// A declined reply and a blank one have no text to place, whatever came with them, and are not kept for the
/// chat.
#[test]
fn a_declined_or_blank_reply_has_no_text_to_place() {
    let ctx = egui::Context::default();
    let with = |text: &str, stop: Stop| Reply { text: text.into(), stop, ..reply(text) };
    for result in [with("Some partial words", Stop::Refusal { category: None }), with("  \n ", Stop::End)] {
        let (mut app, _) = app("Hello", Some(script(false, Ok(result))));
        ask(&mut app, &ctx, "Anything");
        settle(&mut app, &ctx);
        assert!(app.assistant.reply.as_ref().is_some_and(|r| r.text.is_empty()), "no text is offered for placing");
        assert!(app.assistant.history.is_empty(), "and none goes into the chat");
    }
}

/// The assistant changes the document with commands, and all of its changes are one undo step.
#[test]
fn the_assistant_changes_the_document_and_one_undo_takes_it_back() {
    let ctx = egui::Context::default();
    let turns = vec![
        Ok(Step::tools(&[("select_text", json!({"text": "world"})), ("type_text", json!({"text": "there"}))])),
        Ok(Step::tools(&[("execute", json!({"command": "format.bold"}))])),
        Ok(Step::text("I replaced the word and made it bold.")),
    ];
    let mut app = edit_app("Hello world", edit_script(false, turns));
    ask(&mut app, &ctx, "Say there instead of world, in bold");
    settle(&mut app, &ctx);
    assert_eq!(body(&app), "Hello there");
    assert_eq!(app.session.undo_label(), Some(super::ASSISTANT_CHANGES), "one step for all of them");
    assert!(app.assistant.applied.is_some() && app.assistant.apply_error.is_none());
    assert_eq!(app.assistant.reply.as_ref().map(|r| r.text.as_str()), Some("I replaced the word and made it bold."));
    assert!(app.assistant.said.contains("Undo"), "{:?}", app.assistant.said);
    app.assistant_act(Act::UndoEdits, &ctx);
    assert_eq!(body(&app), "Hello world", "one undo takes all of it back");
    assert_eq!(app.assistant.want_focus, super::Focus::Send, "the keyboard leaves the button that is gone");
}

/// A request that changes the document asks first, describing the whole document, and the agreement covers
/// later ones.
#[test]
fn changing_the_document_asks_for_the_agreement_once() {
    let ctx = egui::Context::default();
    let turns = vec![Ok(Step::text("Nothing to change.")), Ok(Step::text("Still nothing."))];
    let mut app = edit_app("First paragraph\nSecond paragraph", edit_script(true, turns));
    app.run("select.text", json!({"text": "First"})).unwrap();
    ask(&mut app, &ctx, "Check the spelling");
    let ready = app.assistant.ready.as_ref().expect("the request waits for agreement");
    assert_eq!((ready.request.scope, ready.chars), (Scope::Document, "First paragraph\nSecond paragraph".chars().count()), "the whole document");
    app.assistant_act(Act::Agree, &ctx);
    settle(&mut app, &ctx);
    assert_eq!(app.assistant_run.choices.agreed, Some(test_agreement()));
    ask(&mut app, &ctx, "Check it again");
    assert!(app.assistant.ready.is_none(), "agreed once for the run");
    settle(&mut app, &ctx);
    assert_eq!(body(&app), "First paragraph\nSecond paragraph", "an answer without changes changes nothing");
}

/// Changes made while the person edited the document are not applied over their edit.
#[test]
fn changes_are_not_applied_when_the_document_changed_meanwhile() {
    let ctx = egui::Context::default();
    let held = edit_script(
        false,
        vec![Ok(Step::tools(&[("select_text", json!({"text": "world"})), ("type_text", json!({"text": "there"}))])), Ok(Step::text("Done."))],
    );
    held.hold.store(true, std::sync::atomic::Ordering::SeqCst);
    let hold = held.hold.clone();
    let mut app = edit_app("Hello world", held);
    ask(&mut app, &ctx, "Change world");
    app.run("caret.docEnd", json!({})).unwrap();
    app.run("text.insert", json!({"text": "!"})).unwrap();
    hold.store(false, std::sync::atomic::Ordering::SeqCst);
    settle(&mut app, &ctx);
    assert_eq!(body(&app), "Hello world!", "the person's edit stays and the assistant's is not applied");
    assert!(app.assistant.apply_error.as_deref().is_some_and(|e| e.contains("changed")), "{:?}", app.assistant.apply_error);
    assert!(app.assistant.history.is_empty(), "the chat does not claim changes the document does not have");
    assert!(!app.assistant.said.contains("working"), "the status line no longer says it is working: {:?}", app.assistant.said);
}

/// A request that did not finish applies nothing, and says so.
#[test]
fn an_unfinished_request_changes_nothing() {
    let ctx = egui::Context::default();
    let mut cut = Step::text("partial");
    cut.stop = StepStop::MaxTokens;
    let turns = vec![Ok(Step::tools(&[("select_text", json!({"text": "world"})), ("type_text", json!({"text": "there"}))])), Ok(cut)];
    let mut app = edit_app("Hello world", edit_script(false, turns));
    ask(&mut app, &ctx, "Change world");
    settle(&mut app, &ctx);
    assert_eq!(body(&app), "Hello world");
    assert!(app.assistant.applied.is_none());
    assert!(app.assistant.said.contains("nothing was changed"), "{:?}", app.assistant.said);
}

/// Stopping the assistant ends its work before its next step and changes nothing.
#[test]
fn stopping_the_assistant_changes_nothing_and_sends_no_more() {
    let ctx = egui::Context::default();
    let held = edit_script(
        false,
        vec![Ok(Step::tools(&[("select_text", json!({"text": "world"})), ("type_text", json!({"text": "there"}))])), Ok(Step::text("Done."))],
    );
    held.hold.store(true, std::sync::atomic::Ordering::SeqCst);
    let (hold, requests) = (held.hold.clone(), held.tool_requests.clone());
    let mut app = edit_app("Hello world", held);
    ask(&mut app, &ctx, "Change world");
    // Stop while the first request is out.
    let until = Instant::now() + Duration::from_secs(10);
    while *requests.lock().unwrap() == 0 && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(2));
    }
    app.assistant_act(Act::Stop, &ctx);
    hold.store(false, std::sync::atomic::Ordering::SeqCst);
    std::thread::sleep(Duration::from_millis(200));
    for _ in 0..5 {
        app.assistant_poll(&ctx);
    }
    assert_eq!(body(&app), "Hello world");
    assert_eq!(*requests.lock().unwrap(), 1, "the worker stopped before a second request");
    assert!(app.assistant.said.contains("Nothing was changed"), "{:?}", app.assistant.said);
}

/// Wait until the scripted provider has been asked `n` times.
fn wait_for_requests(script: &Script, n: usize) {
    let until = Instant::now() + Duration::from_secs(10);
    while *script.tool_requests.lock().unwrap() < n && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// The changes land where the assistant made them, also when the person clicked elsewhere while it worked: a
/// change to the selection the assistant was given replaces that selection, not the text at the new caret.
#[test]
fn the_changes_land_where_the_assistant_made_them_after_a_click() {
    let ctx = egui::Context::default();
    let held = edit_script(false, vec![Ok(Step::tools(&[("type_text", json!({"text": "there"}))])), Ok(Step::text("Done."))]);
    held.hold.store(true, std::sync::atomic::Ordering::SeqCst);
    let mut app = edit_app("Hello world", held.clone());
    app.run("select.text", json!({"text": "world"})).unwrap();
    ask(&mut app, &ctx, "Say there instead");
    wait_for_requests(&held, 1);
    app.run("caret.docStart", json!({})).unwrap();
    held.hold.store(false, std::sync::atomic::Ordering::SeqCst);
    settle(&mut app, &ctx);
    assert_eq!(body(&app), "Hello there", "{:?}", app.assistant.apply_error);
}

/// What the person does meanwhile that leaves the document as it was (an edit and its Undo) does not cost them
/// the request.
#[test]
fn an_edit_undone_meanwhile_does_not_cost_the_request() {
    let ctx = egui::Context::default();
    let held = edit_script(
        false,
        vec![Ok(Step::tools(&[("select_text", json!({"text": "world"})), ("type_text", json!({"text": "there"}))])), Ok(Step::text("Done."))],
    );
    held.hold.store(true, std::sync::atomic::Ordering::SeqCst);
    let mut app = edit_app("Hello world", held.clone());
    ask(&mut app, &ctx, "Change world");
    wait_for_requests(&held, 1);
    app.run("caret.docEnd", json!({})).unwrap();
    app.run("text.insert", json!({"text": "!"})).unwrap();
    app.run("edit.undo", json!({})).unwrap();
    held.hold.store(false, std::sync::atomic::Ordering::SeqCst);
    settle(&mut app, &ctx);
    assert_eq!(body(&app), "Hello there", "{:?}", app.assistant.apply_error);
}

/// Undo These Changes undoes the assistant's last changes and nothing else: once they are undone another way,
/// it does not undo an earlier request's.
#[test]
fn undo_these_changes_undoes_only_the_last_request_s_own_step() {
    let ctx = egui::Context::default();
    let turns = vec![
        Ok(Step::tools(&[("select_text", json!({"text": "Hello"})), ("execute", json!({"command": "format.bold"}))])),
        Ok(Step::text("Bold.")),
        Ok(Step::tools(&[("select_text", json!({"text": "world"})), ("type_text", json!({"text": "there"}))])),
        Ok(Step::text("Done.")),
    ];
    let mut app = edit_app("Hello world", edit_script(false, turns));
    ask(&mut app, &ctx, "Make Hello bold");
    settle(&mut app, &ctx);
    ask(&mut app, &ctx, "Say there");
    settle(&mut app, &ctx);
    assert_eq!(body(&app), "Hello there");
    app.run("edit.undo", json!({})).unwrap();
    assert_eq!(body(&app), "Hello world");
    let depth = app.session.undo_depth();
    app.assistant_act(Act::UndoEdits, &ctx);
    assert_eq!(app.session.undo_depth(), depth, "the first request's changes stay");
}

/// The agreement covers other documents too: a new one does not ask again.
#[test]
fn the_agreement_covers_other_documents_too() {
    let ctx = egui::Context::default();
    let turns = vec![Ok(Step::text("Nothing to change.")), Ok(Step::text("Nothing here either."))];
    let mut app = edit_app("A draft", edit_script(true, turns));
    let frame = |app: &mut WordApp| {
        ctx.run_ui(egui::RawInput::default(), |ui| app.logic(ui.ctx())).drop_without_applying_deltas();
    };
    frame(&mut app);
    ask(&mut app, &ctx, "Check it");
    app.assistant_act(Act::Agree, &ctx);
    settle(&mut app, &ctx);
    app.run("file.new", json!({})).unwrap();
    frame(&mut app);
    // The new document's window state starts with the app's own provider: the test's again.
    app.assistant.factory = scripted;
    ask(&mut app, &ctx, "Check this one");
    assert!(app.assistant.ready.is_none(), "a new document does not ask again");
    settle(&mut app, &ctx);
}

/// The model is told the selection the assistant starts from: a selection changed before Agree is the one it
/// works on and the one it is told about.
#[test]
fn the_model_is_told_the_selection_it_starts_from() {
    let ctx = egui::Context::default();
    let script = edit_script(true, vec![Ok(Step::text("Nothing to change."))]);
    let told = script.first_messages.clone();
    let mut app = edit_app("Alpha\nGamma", script);
    app.run("select.text", json!({"text": "Alpha"})).unwrap();
    ask(&mut app, &ctx, "Shorten the selection");
    app.run("select.text", json!({"text": "Gamma"})).unwrap();
    app.assistant_act(Act::Agree, &ctx);
    settle(&mut app, &ctx);
    let first = told.lock().unwrap().first().cloned().unwrap_or_default();
    assert!(first.contains("<selection>\\nGamma\\n</selection>"), "{first}");
}

/// With the pane hidden, the status bar says what happened to the document.
#[test]
fn a_hidden_pane_reports_the_outcome_on_the_status_bar() {
    let ctx = egui::Context::default();
    let turns = vec![Ok(Step::tools(&[("select_text", json!({"text": "world"})), ("type_text", json!({"text": "there"}))])), Ok(Step::text("Done."))];
    let mut app = edit_app("Hello world", edit_script(false, turns));
    app.session.view.assistant_pane = false;
    ask(&mut app, &ctx, "Change world");
    settle(&mut app, &ctx);
    let status = app.status_msg.as_ref().map(|(m, _)| m.clone()).unwrap_or_default();
    assert!(status.contains("applied"), "{status:?}");
    assert!(app.session.view.assistant_pane, "the pane opens again, so what happened is shown and spoken");
}

/// When a limit of the request stops the assistant, what it did is applied and the pane says the work may be
/// partial; its reply says what is left.
#[test]
fn work_stopped_by_a_limit_is_applied_and_said_to_be_partial() {
    let ctx = egui::Context::default();
    let mut turns = vec![Ok(Step::tools(&[("select_text", json!({"text": "world"})), ("type_text", json!({"text": "there"}))]))];
    for _ in 0..wordcraft_assistant::agent::MAX_ROUNDS - 2 {
        turns.push(Ok(Step::tools(&[("get_text", json!({}))])));
    }
    turns.push(Ok(Step::text("I changed the first word; the rest is left.")));
    let mut app = edit_app("Hello world", edit_script(false, turns));
    ask(&mut app, &ctx, "Change every word");
    settle(&mut app, &ctx);
    assert_eq!(body(&app), "Hello there", "the work so far is applied");
    assert!(app.assistant.partial && app.assistant.applied.is_some());
    assert!(app.assistant.said.contains("limit"), "{:?}", app.assistant.said);
}

/// A request that ends after the person closed the pane opens it again without taking the keyboard, even
/// when Stop Waiting held it when the pane was closed.
#[test]
fn a_request_that_ends_with_the_pane_closed_takes_no_keyboard() {
    let ctx = egui::Context::default();
    let held = edit_script(
        false,
        vec![Ok(Step::tools(&[("select_text", json!({"text": "world"})), ("type_text", json!({"text": "there"}))])), Ok(Step::text("Done."))],
    );
    held.hold.store(true, std::sync::atomic::Ordering::SeqCst);
    let mut app = edit_app("Hello world", held.clone());
    app.session.view.assistant_pane = true;
    ask(&mut app, &ctx, "Change world");
    wait_for_requests(&held, 1);
    app.assistant.stop_focused = true;
    app.session.view.assistant_pane = false;
    held.hold.store(false, std::sync::atomic::Ordering::SeqCst);
    settle(&mut app, &ctx);
    assert!(app.session.view.assistant_pane, "it opened again to say what happened");
    assert_eq!(app.assistant.want_focus, super::Focus::None, "the keyboard stays where the person is");
}

/// With nothing to send, Undo These Changes hands the keyboard to the instruction box, not to a disabled Send.
#[test]
fn undoing_with_no_instruction_hands_the_keyboard_to_the_instruction_box() {
    let ctx = egui::Context::default();
    let turns = vec![Ok(Step::tools(&[("select_text", json!({"text": "world"})), ("type_text", json!({"text": "there"}))])), Ok(Step::text("Done."))];
    let mut app = edit_app("Hello world", edit_script(false, turns));
    ask(&mut app, &ctx, "Change world");
    settle(&mut app, &ctx);
    app.assistant.prompt.clear();
    app.assistant_act(Act::UndoEdits, &ctx);
    assert_eq!(body(&app), "Hello world");
    assert_eq!(app.assistant.want_focus, super::Focus::Prompt);
}

/// The service accepting the key finishes the setup, a refused key brings it back, and another failure the
/// service answered with still means the key works.
#[test]
fn a_refused_key_brings_the_setup_back() {
    let ctx = egui::Context::default();
    let (mut app, _) = app("Some text", Some(script(false, Ok(reply("ok")))));
    assert!(!app.assistant_run.setup_done(), "nothing agreed or answered yet");
    ask(&mut app, &ctx, "One");
    settle(&mut app, &ctx);
    assert!(app.assistant_run.setup_done(), "the service answered");
    let refused = Error::Http { status: 401, kind: "authentication_error".into(), message: "invalid x-api-key".into() };
    NEXT.with(|next| *next.borrow_mut() = Some(script(false, Err(refused))));
    ask(&mut app, &ctx, "Two");
    settle(&mut app, &ctx);
    assert!(!app.assistant_run.setup_done(), "a refused key shows the setup again");
    let busy = Error::Http { status: 529, kind: "overloaded_error".into(), message: "busy".into() };
    NEXT.with(|next| *next.borrow_mut() = Some(script(false, Err(busy))));
    ask(&mut app, &ctx, "Three");
    settle(&mut app, &ctx);
    assert!(app.assistant_run.setup_done(), "a busy service still accepted the key");
}

/// A follow-up about the same selection sends the first instruction again with its text, and that text once.
#[test]
fn a_follow_up_about_the_same_selection_does_not_send_it_again() {
    let ctx = egui::Context::default();
    let (mut app, sent) = app("Hello brave world", Some(script(false, Ok(reply("Hello kind world")))));
    app.run("select.text", json!({"text": "brave"})).unwrap();
    ask(&mut app, &ctx, "A gentler word");
    settle(&mut app, &ctx);
    ask(&mut app, &ctx, "Another one");
    settle(&mut app, &ctx);
    let second = sent.lock().unwrap()[1].clone();
    assert_eq!(second.history[0].shared.as_ref().map(|s| s.text.as_str()), Some("brave"), "the first instruction keeps its text");
    let body = wordcraft_assistant::build_body(&second, "m").unwrap();
    assert_eq!(body.matches("brave").count(), 1, "the text goes once: {body}");
}

/// A long chat lets its oldest turns go a run at a time, as the requests do: most follow-ups still start with the
/// request before them, which the service then reads from its prompt cache, and the pane keeps no more turns than a
/// request sends.
#[test]
fn a_long_chat_lets_its_oldest_turns_go_a_run_at_a_time() {
    let ctx = egui::Context::default();
    let (mut app, sent) = app("Hello brave world", Some(script(false, Ok(reply("kind")))));
    app.run("select.text", json!({"text": "brave"})).unwrap();
    for n in 0..24 {
        ask(&mut app, &ctx, &format!("Question {n}"));
        settle(&mut app, &ctx);
    }
    assert!(app.assistant.history.len() <= wordcraft_assistant::MAX_HISTORY_TURNS, "{} turns kept", app.assistant.history.len());
    let bodies: Vec<String> = sent.lock().unwrap().iter().map(|r| wordcraft_assistant::build_body(r, "m").unwrap()).collect();
    let starts: Vec<bool> = bodies.windows(2).map(|pair| pair[0].strip_suffix("]}").is_some_and(|before| pair[1].starts_with(before))).collect();
    let first_cut = starts.iter().position(|starts| !starts).expect("the chat reached its bound");
    let after = &starts[first_cut..];
    let kept = after.iter().filter(|&&starts| starts).count();
    assert!(kept * 2 > after.len(), "{kept} of {} follow-ups after the first cut start with the request before", after.len());
}

/// Settings never shows another request's token use as the last one's: a request that failed after some of its
/// rounds were answered shows what those used, and one that got no answer, or that the person stopped waiting for,
/// says that its use is not known.
#[test]
fn the_token_use_shown_is_the_last_request_s() {
    use super::Spent;
    let ctx = egui::Context::default();
    let earlier = || Some(Spent::All { model: "earlier".into(), usage: Usage { input_tokens: 1, output_tokens: 1, ..Usage::default() } });
    // A request that changes the document, answered once and then failing.
    let mut first = Step::tools(&[("get_text", json!({}))]);
    first.usage = Usage { input_tokens: 2000, cache_read_tokens: 0, cache_write_tokens: 1990, output_tokens: 40 };
    let busy = Error::Http { status: 529, kind: "overloaded_error".into(), message: "busy".into() };
    let mut app = edit_app("Hello world", edit_script(false, vec![Ok(first), Err(busy)]));
    app.assistant.last_usage = earlier();
    ask(&mut app, &ctx, "Change it");
    settle(&mut app, &ctx);
    assert!(app.assistant.error.is_some());
    assert!(
        matches!(&app.assistant.last_usage, Some(Spent::Before { usage, .. }) if usage.input_tokens == 2000 && usage.cache_write_tokens == 1990),
        "the answered round's tokens: {:?}",
        app.assistant.last_usage
    );
    // A text request that fails.
    let (mut failing, _) = self::app("Some text", Some(script(false, Err(Error::Network("down".into())))));
    failing.assistant.last_usage = earlier();
    ask(&mut failing, &ctx, "Summarise");
    settle(&mut failing, &ctx);
    assert_eq!(failing.assistant.last_usage, Some(Spent::Unknown));
    // A request the person stopped waiting for.
    let held = script(false, Ok(reply("Late")));
    held.hold.store(true, std::sync::atomic::Ordering::SeqCst);
    let (mut stopped, _) = self::app("Some text", Some(held.clone()));
    stopped.assistant.last_usage = earlier();
    ask(&mut stopped, &ctx, "Summarise");
    stopped.assistant_act(Act::Stop, &ctx);
    held.hold.store(false, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(stopped.assistant.last_usage, Some(Spent::Unknown));
}

/// Keeping the key on this computer: only when asked, only once the key has worked, in a file of its own that
/// the next run reads, and gone when the person forgets it.
#[cfg(not(target_arch = "wasm32"))]
mod saved_key {
    use std::sync::atomic::{AtomicU32, Ordering};

    use wordcraft_assistant::key_store::{KeyFile, KeyFileError};

    use super::*;

    const KEY: &str = "sk-ant-saved-key-0123";

    /// A settings folder of its own for one test, removed afterwards.
    struct Folder(std::path::PathBuf);

    impl Folder {
        fn new() -> Folder {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let path = std::env::temp_dir().join(format!("wordcraft-pane-key-test-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::SeqCst)));
            std::fs::create_dir_all(&path).unwrap();
            Folder(path)
        }

        fn file(&self) -> KeyFile {
            KeyFile::in_folder(&self.0)
        }
    }

    impl Drop for Folder {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn typed(app: &mut WordApp, key: &str) {
        app.assistant_run.key = wordcraft_engine::Password::new(key);
    }

    /// The choices an earlier run kept in `folder`: text answers, which these tests are about.
    fn chose_text_answers(folder: &Folder) {
        std::fs::write(folder.0.join(crate::assistant_pane::saved::FILE_NAME), r#"{"edits": false}"#).unwrap();
    }

    #[test]
    fn a_key_is_kept_only_when_asked_and_only_after_it_has_worked() {
        let ctx = egui::Context::default();
        let folder = Folder::new();
        // Not asked: a key that works is not kept.
        let (mut app, _) = app("Some text", Some(script(false, Ok(reply("ok")))));
        app.use_saved_assistant_key(&folder.0);
        app.run("select.all", json!({})).unwrap();
        typed(&mut app, KEY);
        ask(&mut app, &ctx, "One");
        settle(&mut app, &ctx);
        assert_eq!(folder.file().load(), Ok(None), "the person did not ask for it to be kept");
        // Asked, but the service did not accept it: nothing is kept.
        let rejected = Error::Http { status: 401, kind: "authentication_error".into(), message: "invalid".into() };
        let (mut app, _) = self::app("Some text", Some(script(false, Err(rejected))));
        app.use_saved_assistant_key(&folder.0);
        app.run("select.all", json!({})).unwrap();
        typed(&mut app, "sk-ant-wrong");
        app.assistant_run.remember = true;
        ask(&mut app, &ctx, "One");
        settle(&mut app, &ctx);
        assert_eq!(folder.file().load(), Ok(None), "a key that was refused is not kept");
        // Asked, and it worked: kept.
        let (mut app, _) = self::app("Some text", Some(script(false, Ok(reply("ok")))));
        app.use_saved_assistant_key(&folder.0);
        app.run("select.all", json!({})).unwrap();
        typed(&mut app, KEY);
        app.assistant_run.remember = true;
        assert_eq!(folder.file().load(), Ok(None), "not before the request has been answered");
        ask(&mut app, &ctx, "One");
        settle(&mut app, &ctx);
        assert_eq!(folder.file().load(), Ok(Some(KEY.to_owned())));
    }

    #[test]
    fn a_kept_key_is_used_by_the_next_run_without_typing() {
        let ctx = egui::Context::default();
        let folder = Folder::new();
        folder.file().save(KEY).unwrap();
        KEYS.with(|keys| keys.borrow_mut().clear());
        let (mut app, _) = app("Some text", Some(script(false, Ok(reply("ok")))));
        app.use_saved_assistant_key(&folder.0);
        assert!(app.assistant_run.remember, "a saved key is kept until it is forgotten");
        assert_eq!(app.assistant_run.typed_key(), Some(KEY));
        app.run("select.all", json!({})).unwrap();
        ask(&mut app, &ctx, "One");
        settle(&mut app, &ctx);
        assert_eq!(KEYS.with(|keys| keys.borrow().clone()), [Some(KEY.to_owned())], "the saved key reaches the provider");
    }

    #[test]
    fn forgetting_removes_the_file_and_the_key() {
        let ctx = egui::Context::default();
        let folder = Folder::new();
        folder.file().save(KEY).unwrap();
        let (mut app, _) = app("Some text", Some(script(false, Ok(reply("ok")))));
        app.use_saved_assistant_key(&folder.0);
        app.assistant_act(Act::ForgetKey, &ctx);
        assert_eq!(folder.file().load(), Ok(None));
        assert!(app.assistant_run.typed_key().is_none() && !app.assistant_run.remember);
        assert!(app.assistant.said.contains("removed"), "it is announced");
        // Turning the option off removes the file but keeps the key for this run.
        folder.file().save(KEY).unwrap();
        let (mut app, _) = self::app("Some text", Some(script(false, Ok(reply("ok")))));
        app.use_saved_assistant_key(&folder.0);
        app.assistant_run.forget(false);
        assert_eq!(folder.file().load(), Ok(None));
        assert_eq!(app.assistant_run.typed_key(), Some(KEY), "still usable in this run");
    }

    /// A key file that cannot be removed is not reported as removed: the pane still tracks it, the key and the
    /// tick stay, and the person is told, so they can try again.
    #[test]
    fn a_key_file_that_cannot_be_removed_is_not_reported_as_removed() {
        let ctx = egui::Context::default();
        let folder = Folder::new();
        // The key file is a folder with something in it, which removing a file cannot remove.
        std::fs::create_dir_all(folder.file().path().join("inside")).unwrap();
        let (mut app, _) = app("Some text", None);
        app.use_saved_assistant_key(&folder.0);
        app.assistant_run.on_disk = Some(wordcraft_engine::Password::new(KEY));
        app.assistant_run.remember = true;
        typed(&mut app, KEY);
        app.assistant_act(Act::ForgetKey, &ctx);
        assert!(app.assistant_run.on_disk.is_some() && app.assistant_run.remember, "the file is still there, and still tracked");
        assert_eq!(app.assistant_run.typed_key(), Some(KEY), "the key in the box stays with it");
        assert!(matches!(app.assistant_run.file_problem, Some(KeyFileError::Io(_))));
        assert!(app.assistant.said.contains("could not be removed"), "{:?}", app.assistant.said);
        assert!(!app.assistant_run.forget(false), "turning the option off does not claim it either");
        assert!(app.assistant_run.on_disk.is_some());
    }

    /// The key that was kept is the one the service accepted, not whatever the key box holds by then.
    #[test]
    fn the_key_that_worked_is_the_one_that_is_kept() {
        let ctx = egui::Context::default();
        let folder = Folder::new();
        let held = script(false, Ok(reply("ok")));
        held.hold.store(true, Ordering::SeqCst);
        let hold = held.hold.clone();
        let (mut app, _) = app("Some text", Some(held));
        app.use_saved_assistant_key(&folder.0);
        app.run("select.all", json!({})).unwrap();
        typed(&mut app, KEY);
        app.assistant_run.remember = true;
        ask(&mut app, &ctx, "One");
        // Another window changes the key while the request is out.
        typed(&mut app, "sk-ant-changed-meanwhile");
        hold.store(false, Ordering::SeqCst);
        settle(&mut app, &ctx);
        assert_eq!(folder.file().load(), Ok(Some(KEY.to_owned())), "the key the request was made with");
        assert!(app.assistant.said.contains("saved"), "and the person is told: {:?}", app.assistant.said);
    }

    /// After the Forget button has done its work it is gone, so the keyboard goes to the key box.
    #[test]
    fn after_forgetting_the_keyboard_goes_to_the_key_box() {
        let ctx = egui::Context::default();
        let folder = Folder::new();
        folder.file().save(KEY).unwrap();
        let (mut app, _) = app("Some text", None);
        app.use_saved_assistant_key(&folder.0);
        app.assistant_act(Act::ForgetKey, &ctx);
        assert_eq!(app.assistant.want_focus, super::super::Focus::Key);
    }

    /// The statement that a remembered key is not encrypted comes before the box that turns it on.
    #[test]
    fn the_unencrypted_notice_comes_before_the_checkbox() {
        use egui_kittest::kittest::Queryable;
        let folder = Folder::new();
        let (mut app, _) = app("Some text", None);
        app.use_saved_assistant_key(&folder.0);
        let h = super::drawn::open(app);
        let notice = h.get_by_label_contains("kept unencrypted").rect();
        let checkbox = h.get_by_label("Remember this key on this computer").rect();
        assert!(notice.max.y <= checkbox.min.y + 1.0, "{notice:?} then {checkbox:?}");
    }

    /// Turning the option on or off changes the lines around the checkbox, and the keyboard stays on it: its
    /// identity does not depend on how many lines come before it.
    #[test]
    fn ticking_remember_keeps_the_keyboard_on_the_checkbox() {
        use egui_kittest::kittest::Queryable;
        let folder = Folder::new();
        let (mut app, _) = app("Some text", None);
        app.use_saved_assistant_key(&folder.0);
        let mut h = super::drawn::open(app);
        // Control: a checkbox whose toggling changes no other line.
        let scope = "Use the whole document instead of the selection";
        h.get_by_label(scope).focus();
        h.run_steps(2);
        assert!(h.get_by_label(scope).is_focused(), "the scope checkbox has the keyboard");
        h.key_press(egui::Key::Space);
        h.run_steps(3);
        assert!(h.get_by_label(scope).is_focused(), "control: still on the scope checkbox after it was toggled");
        let label = "Remember this key on this computer";
        h.get_by_label(label).focus();
        h.run_steps(2);
        assert!(h.get_by_label(label).is_focused(), "the checkbox has the keyboard");
        for _ in 0..2 {
            h.key_press(egui::Key::Space);
            h.run_steps(3);
            assert!(h.get_by_label(label).is_focused(), "still on the checkbox after it was toggled (now {})", h.state().assistant_run.remember);
        }
    }

    /// A file other users can read is not used, and the person is told why.
    #[cfg(unix)]
    #[test]
    fn a_saved_key_that_other_users_can_read_is_not_used() {
        let folder = Folder::new();
        folder.file().save(KEY).unwrap();
        std::fs::set_permissions(folder.file().path(), std::os::unix::fs::PermissionsExt::from_mode(0o644)).unwrap();
        let (mut app, _) = app("Some text", None);
        app.use_saved_assistant_key(&folder.0);
        assert!(app.assistant_run.typed_key().is_none(), "the key was not read");
        assert_eq!(app.assistant_run.file_problem, Some(KeyFileError::TooOpen));
        let h = super::drawn::open(app);
        use egui_kittest::kittest::Queryable;
        assert!(h.query_all_by_label_contains("other users can read").count() >= 1, "the pane says why");
    }

    /// The agreement is kept for later runs, which send without asking, until the person withdraws it; then the
    /// next request asks again, in this run and the next.
    #[test]
    fn the_agreement_is_kept_for_later_runs_until_it_is_withdrawn() {
        let ctx = egui::Context::default();
        let folder = Folder::new();
        chose_text_answers(&folder);
        let (mut app, _) = app("Some text", Some(script(true, Ok(reply("ok")))));
        app.use_saved_assistant_key(&folder.0);
        app.run("select.all", json!({})).unwrap();
        ask(&mut app, &ctx, "One");
        app.assistant_act(Act::Agree, &ctx);
        settle(&mut app, &ctx);
        let (mut later, sent) = self::app("Other text", Some(script(true, Ok(reply("ok")))));
        later.use_saved_assistant_key(&folder.0);
        later.run("select.all", json!({})).unwrap();
        ask(&mut later, &ctx, "Two");
        assert!(later.assistant.ready.is_none(), "a later run does not ask again");
        settle(&mut later, &ctx);
        assert_eq!(sent.lock().unwrap().len(), 1);
        later.assistant_act(Act::Withdraw, &ctx);
        assert!(later.assistant.said.contains("withdrawn"), "{:?}", later.assistant.said);
        ask(&mut later, &ctx, "Three");
        assert!(later.assistant.ready.is_some(), "withdrawn: the next request asks");
        let (mut next, _) = self::app("Text", None);
        next.use_saved_assistant_key(&folder.0);
        assert_eq!(next.assistant_run.choices.agreed, None, "and so does the next run");
    }

    /// An agreement withdrawn in another WordCraft process holds in this one, which read it when it started:
    /// the next request asks again, and nothing more is sent.
    #[test]
    fn an_agreement_withdrawn_in_another_process_holds_here_too() {
        let ctx = egui::Context::default();
        let folder = Folder::new();
        chose_text_answers(&folder);
        let (mut this, sent) = app("Some text", Some(script(true, Ok(reply("ok")))));
        this.use_saved_assistant_key(&folder.0);
        this.run("select.all", json!({})).unwrap();
        ask(&mut this, &ctx, "One");
        this.assistant_act(Act::Agree, &ctx);
        settle(&mut this, &ctx);
        let (mut other, sent_later) = self::app("Other text", Some(script(true, Ok(reply("ok")))));
        other.use_saved_assistant_key(&folder.0);
        assert_eq!(other.assistant_run.choices.agreed, Some(test_agreement()), "the other process starts agreed");
        other.assistant_act(Act::Withdraw, &ctx);
        ask(&mut this, &ctx, "Two");
        assert!(this.assistant.ready.is_some(), "this process asks again");
        assert_eq!(sent.lock().unwrap().len() + sent_later.lock().unwrap().len(), 1, "nothing more was sent");
    }

    /// The choices that wait under Settings are kept for later runs.
    #[test]
    fn the_choices_are_kept_for_later_runs() {
        use egui_kittest::kittest::Queryable;
        let folder = Folder::new();
        let (mut app, _) = app("Some text", None);
        app.use_saved_assistant_key(&folder.0);
        // As a first run starts: the assistant makes the changes.
        app.assistant_run.choices = super::super::saved::Choices::default();
        let mut h = super::drawn::open(app);
        h.get_by_label("Let the assistant make changes in the document").click();
        h.run_steps(2);
        h.get_by_label("Use the whole document instead of the selection").click();
        h.run_steps(2);
        let (mut later, _) = self::app("Other text", None);
        later.assistant_run.choices = super::super::saved::Choices::default();
        later.use_saved_assistant_key(&folder.0);
        assert!(later.assistant_run.choices.whole_document, "the whole document stays chosen");
        assert!(!later.assistant_run.choices.edits, "and so do text answers");
    }

    /// A saved key and an agreement from an earlier run: the pane opens set up, its setup under Settings.
    #[test]
    fn a_saved_key_and_an_agreement_open_the_pane_set_up() {
        use egui_kittest::kittest::Queryable;
        let folder = Folder::new();
        folder.file().save(KEY).unwrap();
        std::fs::write(folder.0.join(crate::assistant_pane::saved::FILE_NAME), r#"{"agreed": {"recipient": "example.test", "model": "test-model"}}"#)
            .unwrap();
        let (mut app, _) = app("Some text", None);
        app.use_saved_assistant_key(&folder.0);
        let h = super::drawn::open(app);
        assert!(h.query_by_label("API key:").is_none() && h.query_by_label_contains("Ask for a change").is_none(), "no setup");
        h.get_by_role_and_label(egui::accesskit::Role::Button, "Settings");
    }

    /// A saved key cleared from the key box is no longer a key that needs no typing: the request finds no key, and
    /// the setup, with the key box, shows again (unless the environment has a key).
    #[test]
    fn a_saved_key_cleared_from_the_key_box_brings_the_setup_back() {
        let ctx = egui::Context::default();
        let folder = Folder::new();
        folder.file().save(KEY).unwrap();
        std::fs::write(
            folder.0.join(crate::assistant_pane::saved::FILE_NAME),
            r#"{"edits": false, "agreed": {"recipient": "example.test", "model": "test-model"}}"#,
        )
        .unwrap();
        let (mut app, _) = app("Some text", None);
        app.use_saved_assistant_key(&folder.0);
        assert!(app.assistant_run.setup_done(), "a saved key and an agreement");
        typed(&mut app, "");
        ask(&mut app, &ctx, "Summarise");
        assert_eq!(app.assistant.error, Some(Error::MissingKey));
        assert_eq!(app.assistant_run.setup_done(), super::super::env_key(), "the setup shows again unless the environment has a key");
    }

    /// When the agreement cannot be saved, the pane says so where it is seen. The agreement step said it is kept for
    /// later runs, and once it is given the setup ends and Settings is closed.
    #[test]
    fn an_agreement_that_could_not_be_saved_is_said_outside_settings() {
        use egui::accesskit::Role;
        use egui_kittest::kittest::{NodeT, Queryable};
        let ctx = egui::Context::default();
        let folder = Folder::new();
        folder.file().save(KEY).unwrap();
        // The choices cannot be written: a folder with something in it is where their file would go.
        std::fs::create_dir_all(folder.0.join(crate::assistant_pane::saved::FILE_NAME).join("inside")).unwrap();
        let (mut app, _) = app("Some text", Some(script(true, Ok(reply("Done")))));
        app.use_saved_assistant_key(&folder.0);
        app.assistant_run.choices.edits = false;
        app.run("select.all", json!({})).unwrap();
        let mut h = super::drawn::open(app);
        h.state_mut().assistant.prompt = "Summarise".into();
        h.state_mut().assistant_act(Act::Send, &ctx);
        h.run_steps(2);
        assert!(h.query_by_label_contains("Your agreement is kept in WordCraft's settings").is_some(), "the promise the step makes");
        h.state_mut().assistant_act(Act::Agree, &ctx);
        settle(h.state_mut(), &ctx);
        h.run_steps(3);
        assert!(h.state().assistant_run.setup_done(), "the setup is done");
        assert_eq!(h.get_by_role_and_label(Role::Button, "Settings").accesskit_node().data().is_expanded(), Some(false), "Settings is closed");
        let said = h.query_all_by_label_contains("Your agreement could not be saved").count();
        assert!(said >= 1, "the pane says the agreement lasts for this run only");
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod drawn {
    use egui_kittest::kittest::{By, NodeT, Queryable};

    use super::*;

    /// The window with the pane open, drawn for a few frames.
    pub(super) fn open(app: WordApp) -> egui_kittest::Harness<'static, WordApp> {
        open_in(app, None)
    }

    /// The same in an interface language: it takes effect in a frame, and the pane then opens on the side it
    /// reads from.
    fn open_in(app: WordApp, language: Option<&str>) -> egui_kittest::Harness<'static, WordApp> {
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1440.0, 900.0)).build_ui_state(
            |ui, app: &mut WordApp| {
                let ctx = ui.ctx().clone();
                app.logic(&ctx);
                app.ui(ui);
            },
            app,
        );
        h.run_steps(2);
        if let Some(code) = language {
            h.state_mut().run("ui.language", json!({"value": code})).unwrap();
            h.run_steps(2);
        }
        h.state_mut().run("view.assistantPane", json!({"value": true})).unwrap();
        h.run_steps(4);
        h
    }

    /// The agreement describes the request that waits, whatever the controls above it say, and
    /// those controls cannot change while it waits.
    #[test]
    fn the_agreement_describes_the_request_that_waits_and_locks_the_controls_above_it() {
        let ctx = egui::Context::default();
        let (mut app, sent) = app("First paragraph\nSecond paragraph", Some(script(true, Ok(reply("Reply")))));
        app.run("select.text", json!({"text": "First"})).unwrap();
        app.assistant_run.choices.whole_document = true;
        ask(&mut app, &ctx, "Summarise");
        assert_eq!(app.assistant.ready.as_ref().map(|r| r.request.scope), Some(Scope::Document));
        let mut h = open(app);
        let (whole, selected) =
            ("This request sends your instruction and the whole document (", "This request sends your instruction and the selected text");
        assert!(h.query_by_label_contains(whole).is_some(), "the agreement names the whole document");
        assert!(h.query_by_label_contains("The text starts: First paragraph Second paragraph").is_some(), "and shows how the text starts");
        assert!(h.query_by_label_contains("another of its models").is_some(), "and says another model may read it if the first declines");
        assert!(h.query_by_label_contains("later requests, in any document").is_some(), "and what agreeing covers");
        for label in ["Use the whole document instead of the selection", "Instruction:", "API key:"] {
            assert!(h.get_by_label(label).accesskit_node().is_disabled(), "{label} cannot change while the agreement waits");
        }
        // Were the setting changed behind the pane's back, the agreement still says what would be sent.
        h.state_mut().assistant_run.choices.whole_document = false;
        h.run_steps(2);
        assert!(h.query_by_label_contains(whole).is_some() && h.query_by_label_contains(selected).is_none());
        h.state_mut().assistant_act(Act::Agree, &ctx);
        settle(h.state_mut(), &ctx);
        let seen = sent.lock().unwrap().clone();
        assert_eq!((seen[0].scope, seen[0].context.as_str()), (Scope::Document, "First paragraph\nSecond paragraph"));
    }

    /// With nothing selected only the instruction goes, and the agreement says so.
    #[test]
    fn the_agreement_says_when_no_document_text_goes() {
        let ctx = egui::Context::default();
        let (mut app, _) = app("Some text", Some(script(true, Ok(reply("Reply")))));
        app.run("caret.docStart", json!({})).unwrap();
        ask(&mut app, &ctx, "Write a haiku");
        let h = open(app);
        assert!(h.query_by_label_contains("only your instruction: nothing is selected").is_some());
        assert!(h.query_by_label_contains("The text starts").is_none());
        assert!(h.query_by_label_contains("or the whole document when you choose that").is_some(), "the agreement covers the whole document too");
    }

    /// An empty document says it is empty, not that nothing is selected.
    #[test]
    fn the_agreement_names_an_empty_document_as_empty() {
        let ctx = egui::Context::default();
        let (mut app, _) = app("", Some(script(true, Ok(reply("Reply")))));
        app.assistant_run.choices.whole_document = true;
        ask(&mut app, &ctx, "Write a haiku");
        let h = open(app);
        assert!(h.query_by_label_contains("the document is empty").is_some());
        assert!(h.query_by_label_contains("nothing is selected").is_none());
    }

    /// The same failure twice is announced twice: the alert's spoken text differs by an invisible mark.
    #[test]
    fn a_repeated_failure_is_spoken_again() {
        let ctx = egui::Context::default();
        let (mut app, _) = app("Some text", None);
        app.assistant.prompt = "Summarise".into();
        app.assistant_act(Act::Send, &ctx);
        let mut h = open(app);
        let spoken =
            |h: &egui_kittest::Harness<'static, WordApp>| h.get_by_role(egui::accesskit::Role::Alert).accesskit_node().label().map(|l| l.to_string());
        let first = spoken(&h);
        h.state_mut().assistant_act(Act::Send, &ctx);
        h.run_steps(3);
        let second = spoken(&h);
        assert!(first.is_some() && second.is_some());
        assert_ne!(first, second, "a screen reader hears the second failure");
    }

    /// When a request ends the focus goes back to Send only if it was on Stop Waiting; a person who went on to
    /// the document is left where they are.
    #[test]
    fn a_finished_request_takes_the_focus_only_from_stop_waiting() {
        let ctx = egui::Context::default();
        // The person stays on Stop Waiting; goes back to the document; or the window is not in front, which
        // does not move egui's keyboard focus away from Stop Waiting.
        for case in ["stays", "goes to the document", "window not in front"] {
            let stays_on_stop = case != "goes to the document";
            let held = script(false, Ok(reply("Done")));
            held.hold.store(true, std::sync::atomic::Ordering::SeqCst);
            let hold = held.hold.clone();
            let (mut app, _) = app("Some text", Some(held));
            app.run("select.all", json!({})).unwrap();
            let mut h = open(app);
            h.state_mut().assistant.prompt = "Shorter".into();
            h.state_mut().assistant_act(Act::Send, &ctx);
            h.run_steps(6);
            assert!(h.get_by_role_and_label(egui::accesskit::Role::Button, "Stop Waiting").is_focused(), "Stop Waiting has the focus while it waits");
            if case == "window not in front" {
                h.input_mut().focused = false;
                h.run_steps(2);
                assert!(!h.ctx.input(|i| i.focused), "the window is not in front");
            }
            if !stays_on_stop {
                // The person goes back to the document.
                let focused = h.ctx.memory(|m| m.focused());
                if let Some(id) = focused {
                    h.ctx.memory_mut(|m| m.surrender_focus(id));
                }
                h.run_steps(2);
            }
            hold.store(false, std::sync::atomic::Ordering::SeqCst);
            let until = Instant::now() + Duration::from_secs(10);
            while h.state().assistant.waiting.is_some() && Instant::now() < until {
                h.run_steps(1);
                std::thread::sleep(Duration::from_millis(5));
            }
            h.run_steps(4);
            assert!(h.state().assistant.waiting.is_none() && h.state().assistant.reply.is_some());
            let send = h.get_by_role_and_label(egui::accesskit::Role::Button, "Send");
            assert_eq!(send.is_focused(), stays_on_stop, "{case}: Send has the focus after Stop Waiting only if Stop Waiting had it");
        }
    }

    /// In a right-to-left interface the pane docks at the window's right, as in every interface, and its rows
    /// read from the right.
    #[test]
    fn in_a_right_to_left_interface_the_pane_docks_at_the_right_and_its_buttons_start_on_the_right() {
        let ctx = egui::Context::default();
        let (mut app, _) = app("Some text", Some(script(true, Ok(reply("Reply")))));
        app.run("select.all", json!({})).unwrap();
        ask(&mut app, &ctx, "Summarise");
        let h = open_in(app, Some("he"));
        let (agree, cancel) = (tl!("Agree and Send"), tl!("Cancel"));
        let agree = h.get_by_role_and_label(egui::accesskit::Role::Button, agree).rect();
        let cancel = h.get_by_role_and_label(egui::accesskit::Role::Button, cancel).rect();
        assert!(agree.min.x > cancel.min.x, "the first button of the row is the rightmost: {agree:?} {cancel:?}");
        assert!(agree.center().x > 1440.0 / 2.0, "the pane is at the window's right: {agree:?}");
        assert!(agree.max.x <= 1440.0 && cancel.min.x >= 0.0, "and stays inside the window");
    }

    /// A reply that ended oddly is shown as such, and a finished one shows no warning.
    #[test]
    fn a_reply_that_did_not_end_normally_says_so() {
        let ctx = egui::Context::default();
        let with = |text: &str, stop: Stop| Reply { text: text.into(), stop, ..reply(text) };
        for (result, expected) in [
            (with("Part", Stop::Other("model_context_window_exceeded".into())), Some("may be incomplete")),
            (with("Part", Stop::Other(String::new())), Some("may be incomplete")),
            (with("", Stop::MaxTokens), Some("ran out of length")),
            (with("Part", Stop::MaxTokens), Some("cut off")),
            (with("Done", Stop::End), None),
        ] {
            let (mut app, _) = app("Hello", Some(script(false, Ok(result))));
            ask(&mut app, &ctx, "Anything");
            settle(&mut app, &ctx);
            let h = open(app);
            // The notice and the announcement of the same state are both in the tree; only the notice starts
            // with the word Warning, so it is counted by that.
            let mentions = |words: &str| h.query_all_by_label_contains(words).count();
            match expected {
                Some(words) => assert!(mentions(words) >= 2 && mentions("Warning:") == 1, "{words}: the notice and the announcement"),
                None => assert!(mentions("Warning:") == 0 && mentions("cut off") == 0 && mentions("ran out of length") == 0),
            }
        }
    }

    /// A failure is an alert a screen reader reads out, not a silent change.
    #[test]
    fn a_failure_is_shown_as_an_alert() {
        let ctx = egui::Context::default();
        let (mut app, _) = app("Some text", None);
        ask(&mut app, &ctx, "Summarise");
        let h = open(app);
        let alert = h.get_by_role(egui::accesskit::Role::Alert);
        assert!(alert.accesskit_node().label().is_some_and(|l| l.contains("No API key")), "{:?}", alert.accesskit_node().label());
    }

    /// The agreement is announced when it appears (a live region), and takes the keyboard focus on its button.
    #[test]
    fn the_agreement_is_announced_and_takes_the_focus() {
        let ctx = egui::Context::default();
        let (mut app, _) = app("Some text", Some(script(true, Ok(reply("Reply")))));
        app.run("select.all", json!({})).unwrap();
        let mut h = open(app);
        // Press Send in the open pane, as a person does.
        h.state_mut().assistant.prompt = "Summarise".into();
        h.state_mut().assistant_act(Act::Send, &ctx);
        h.run_steps(4);
        let live = h.query_all(By::new().role(egui::accesskit::Role::Alert)).any(|n| {
            n.accesskit_node().label().is_some_and(|l| l.contains("Nothing is sent until you agree"))
                && n.accesskit_node().live() != egui::accesskit::Live::Off
        });
        assert!(live, "the agreement is a live region");
        let agree = h.get_by_role_and_label(egui::accesskit::Role::Button, "Agree and Send");
        assert!(agree.is_focused(), "the focus is on the button that decides");
    }

    /// Each char of the text `text` as the last frame drew it: the char, its left and right screen x, and its row.
    fn drawn_chars(h: &egui_kittest::Harness<'static, WordApp>, text: &str) -> Vec<(char, f32, f32, usize)> {
        fn collect(shape: &egui::Shape, text: &str, out: &mut Vec<(char, f32, f32, usize)>) {
            match shape {
                egui::Shape::Text(t) if t.galley.text() == text => {
                    for (i, row) in t.galley.rows.iter().enumerate() {
                        for glyph in &row.glyphs {
                            let x = t.pos.x + row.pos.x + glyph.pos.x;
                            out.push((glyph.chr, x, x + glyph.advance_width, i));
                        }
                    }
                }
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, text, out)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        for clipped in &h.output().shapes {
            collect(&clipped.shape, text, &mut out);
        }
        assert!(!out.is_empty(), "{text:?} is drawn");
        out
    }

    /// The middle of the first `c` drawn on `row`.
    fn x_of(chars: &[(char, f32, f32, usize)], c: char, row: usize) -> f32 {
        chars.iter().find(|g| g.0 == c && g.3 == row).map(|g| (g.1 + g.2) / 2.0).unwrap_or(f32::NAN)
    }

    /// The leftmost and rightmost ink of `row`'s letters.
    fn row_span(chars: &[(char, f32, f32, usize)], row: usize) -> (f32, f32) {
        let letters = chars.iter().filter(|g| g.3 == row && !g.0.is_whitespace());
        letters.fold((f32::MAX, f32::MIN), |(l, r), g| (l.min(g.1), r.max(g.2)))
    }

    /// The AI pane opens at the window's right in an English and in a Hebrew interface, and a place the person
    /// chose stays chosen; a pane that was only resized has chosen none.
    #[test]
    fn the_pane_opens_on_the_right_and_keeps_a_place_the_person_chose() {
        let send =
            |h: &egui_kittest::Harness<'static, WordApp>| h.get_by_role_and_label(egui::accesskit::Role::Button, tl!("Send")).rect().center().x;
        let (english, _) = app("Some text", None);
        assert!(send(&open(english)) > 720.0, "the right in English");
        let (mut resized, _) = app("Some text", None);
        resized.ui.pane_places.entry("assistant".into()).or_default().width = Some(300.0);
        let mut h = open_in(resized, Some("he"));
        assert!(send(&h) > 720.0, "the right in Hebrew, also after a resize");
        h.state_mut().run("ui.paneDock", json!({"pane": "assistant", "dock": "left"})).unwrap();
        h.run_steps(3);
        assert!(send(&h) < 720.0, "where the person docked it");
    }

    /// Once the service has accepted the key, the pane shows the conversation and a Settings button, collapsed;
    /// the button shows the key and the options in place and says it is expanded.
    #[test]
    fn once_set_up_the_pane_shows_the_conversation_and_a_settings_button() {
        use egui::accesskit::Role;
        let ctx = egui::Context::default();
        let (mut app, _) = app("Some text", Some(script(false, Ok(reply("Done")))));
        ask(&mut app, &ctx, "Anything");
        settle(&mut app, &ctx);
        let mut h = open(app);
        let setup = ["API key:", "Let the assistant make changes in the document", "Use the whole document instead of the selection"];
        for gone in setup {
            assert!(h.query_by_label(gone).is_none(), "{gone} waits under Settings");
        }
        assert!(h.query_by_label_contains("Ask for a change").is_none(), "no introduction");
        for shown in ["Instruction:", "Send", "New Chat", "Done"] {
            h.get_by_label(shown);
        }
        let settings = h.get_by_role_and_label(Role::Button, "Settings");
        assert_eq!(settings.accesskit_node().data().is_expanded(), Some(false));
        settings.click();
        h.run_steps(2);
        assert_eq!(h.get_by_role_and_label(Role::Button, "Settings").accesskit_node().data().is_expanded(), Some(true));
        for shown in setup {
            h.get_by_label(shown);
        }
    }

    /// When the setup goes at the end of a request, the keyboard stays with the person (Send, after Stop Waiting
    /// held it), and the status line, a live region, says where the setup went.
    #[test]
    fn when_the_setup_goes_the_keyboard_stays_and_a_screen_reader_is_told() {
        use egui::accesskit::Role;
        let ctx = egui::Context::default();
        let held = script(false, Ok(reply("Done")));
        held.hold.store(true, std::sync::atomic::Ordering::SeqCst);
        let hold = held.hold.clone();
        let (mut app, _) = app("Some text", Some(held));
        app.run("select.all", json!({})).unwrap();
        let mut h = open(app);
        assert!(h.query_by_label("API key:").is_some(), "the setup shows first");
        h.state_mut().assistant.prompt = "Shorter".into();
        h.state_mut().assistant_act(Act::Send, &ctx);
        h.run_steps(6);
        assert!(h.get_by_role_and_label(Role::Button, "Stop Waiting").is_focused());
        hold.store(false, std::sync::atomic::Ordering::SeqCst);
        let until = Instant::now() + Duration::from_secs(10);
        while h.state().assistant.waiting.is_some() && Instant::now() < until {
            h.run_steps(1);
            std::thread::sleep(Duration::from_millis(5));
        }
        h.run_steps(4);
        assert!(h.query_by_label("API key:").is_none(), "the setup went");
        assert!(h.get_by_role_and_label(Role::Button, "Send").is_focused(), "the keyboard is on Send");
        let told = h.query_all(By::new().role(Role::Status)).any(|n| n.accesskit_node().label().is_some_and(|l| l.contains("The setup is done")));
        assert!(told, "the change is said");
    }

    /// A control of the setup that held the keyboard when the setup went hands it to the instruction box.
    #[test]
    fn a_setup_control_that_held_the_keyboard_hands_it_to_the_instruction_box() {
        let (app, _) = app("Some text", None);
        let mut h = open(app);
        let label = "Let the assistant make changes in the document";
        h.get_by_label(label).focus();
        h.run_steps(2);
        assert!(h.get_by_label(label).is_focused());
        // As when the service accepts the key at the end of a request.
        h.state_mut().assistant_run.accepted = true;
        h.run_steps(4);
        assert!(h.query_by_label(label).is_none(), "the setup went");
        assert!(h.get_by_label("Instruction:").is_focused(), "the keyboard went to the instruction box");
    }

    /// In the Hebrew interface the instruction box reads from the right: its hint, a short line and a line that
    /// starts with an English word all end at the box's right edge, and that line is right to left, the English
    /// word first, at the right.
    #[test]
    fn in_the_hebrew_interface_the_instruction_box_reads_from_the_right() {
        let (app, _) = app("Some text", None);
        let mut h = open_in(app, Some("he"));
        let field = h.get_by_label(tl!("Instruction:")).rect();
        let hint = drawn_chars(&h, tl!("For example: make this paragraph shorter"));
        let (left, right) = row_span(&hint, 0);
        assert!(field.max.x - right < 16.0 && left > field.center().x - 40.0, "the hint at the right: {left}..{right} in {field:?}");
        for text in ["שלום", "Word היא תוכנה"] {
            h.state_mut().assistant.prompt = text.into();
            h.run_steps(2);
            let chars = drawn_chars(&h, text);
            let (_, right) = row_span(&chars, 0);
            assert!(field.max.x - right < 16.0 && right <= field.max.x, "{text:?} ends at the right edge: {right} in {field:?}");
        }
        let chars = drawn_chars(&h, "Word היא תוכנה");
        let (word, hebrew, last) = (x_of(&chars, 'W', 0), x_of(&chars, 'י', 0), x_of(&chars, 'ת', 0));
        assert!(word > hebrew && hebrew > last, "right to left, the English word first: {word} {hebrew} {last}");
    }

    /// In the Hebrew interface the empty instruction box, its hint showing, has its caret at its right edge, inside
    /// the box, where the typing starts: where the keyboard focus shows and an input method puts its window.
    #[test]
    fn in_the_hebrew_interface_the_empty_instruction_box_has_its_caret_at_its_right_edge() {
        let (app, _) = app("Some text", None);
        let mut h = open_in(app, Some("he"));
        let label = tl!("Instruction:");
        h.get_by_label(label).focus();
        h.run_steps(3);
        assert!(h.get_by_label(label).is_focused(), "the box has the keyboard");
        assert!(h.state().assistant.prompt.is_empty() && !drawn_chars(&h, tl!("For example: make this paragraph shorter")).is_empty());
        let field = h.get_by_label(label).rect();
        // Where the box draws its caret, and the part of the box the text and the caret are drawn in.
        let (caret, inside) =
            h.output().platform_output.ime.as_ref().map(|ime| (ime.cursor_rect, ime.rect)).expect("the focused box says where its caret is");
        let x = caret.center().x;
        assert!(x <= inside.max.x + 1.0 && x <= field.max.x, "the caret inside the box: {caret:?} in {inside:?}, {field:?}");
        assert!(field.max.x - x < 16.0, "the caret at the box's right edge: {caret:?} in {field:?}");
    }

    /// In an English interface each line of the instruction box starts at its own side: an English line at the
    /// left, a Hebrew one at the right edge, also when short.
    #[test]
    fn in_an_english_interface_each_line_of_the_instruction_box_starts_at_its_own_side() {
        let (app, _) = app("Some text", None);
        let mut h = open(app);
        let field = h.get_by_label("Instruction:").rect();
        let text = "Hello there\nשלום עולם";
        h.state_mut().assistant.prompt = text.into();
        h.run_steps(2);
        let chars = drawn_chars(&h, text);
        let (english, _) = row_span(&chars, 0);
        let (_, hebrew) = row_span(&chars, 1);
        assert!(english - field.min.x < 16.0, "the English line at the left: {english} in {field:?}");
        assert!(field.max.x - hebrew < 16.0 && hebrew <= field.max.x, "the Hebrew line at the right: {hebrew} in {field:?}");
    }

    /// A reply is laid out paragraph by paragraph in each one's own direction, in the Hebrew interface too: a
    /// paragraph that starts in Hebrew ends at the right edge and reads right to left, and one that starts in
    /// English starts at the left.
    #[test]
    fn a_reply_lays_out_each_paragraph_in_its_own_direction() {
        let ctx = egui::Context::default();
        let text = "שלום Word עולם\nWord is a program";
        let (mut app, _) = app("Some text", Some(script(false, Ok(reply(text)))));
        ask(&mut app, &ctx, "Anything");
        settle(&mut app, &ctx);
        let h = open_in(app, Some("he"));
        let field = h.get_by_label(tl!("Instruction:")).rect();
        let chars = drawn_chars(&h, text);
        let (_, right) = row_span(&chars, 0);
        assert!(field.max.x - right < 16.0 && right <= field.max.x + 1.0, "the Hebrew paragraph at the right: {right} in {field:?}");
        let (shalom, word, olam) = (x_of(&chars, 'ש', 0), x_of(&chars, 'W', 0), x_of(&chars, 'ע', 0));
        assert!(shalom > word && word > olam, "right to left: {shalom} {word} {olam}");
        let (left, _) = row_span(&chars, 1);
        assert!(left - field.min.x < 16.0 && left >= field.min.x - 1.0, "the English paragraph at the left: {left} in {field:?}");
    }

    /// Settings shows the last request's token use: the tokens in, what the cache served and took of them, and
    /// the tokens out.
    #[test]
    fn settings_shows_the_last_request_s_token_use() {
        let ctx = egui::Context::default();
        let used =
            Reply { usage: Usage { input_tokens: 1234, cache_read_tokens: 1000, cache_write_tokens: 200, output_tokens: 56 }, ..reply("Done") };
        let (mut app, _) = app("Some text", Some(script(false, Ok(used))));
        ask(&mut app, &ctx, "Anything");
        settle(&mut app, &ctx);
        let mut h = open(app);
        assert!(h.query_by_label_contains("1234 tokens in").is_none(), "it waits under Settings");
        h.get_by_role_and_label(egui::accesskit::Role::Button, "Settings").click();
        h.run_steps(2);
        // A label's text is its accessibility value.
        let line = h.get_by_label_contains("1234 tokens in").accesskit_node().value().unwrap_or_default();
        assert!(line.contains("1000 were read from the cache") && line.contains("200 were written") && line.contains("56 tokens out"), "{line}");
    }
}
