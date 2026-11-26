use std::cell::RefCell;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::json;
use wordcraft_assistant::{Disclosure, Error, MAX_CONTEXT_BYTES, Provider, Reply, Request, Stop, Usage};
use wordcraft_doc::StoryRef;

use super::{Act, WordApp};

/// What the scripted provider answers and whether its text leaves the machine, and what it was asked.
#[derive(Clone)]
struct Script {
    remote: bool,
    result: Result<Reply, Error>,
    sent: Arc<Mutex<Vec<Request>>>,
}

thread_local! {
    /// The script the next provider made on this thread follows (the factory is a plain function).
    static NEXT: RefCell<Option<Script>> = const { RefCell::new(None) };
}

struct Scripted(Script);

impl Provider for Scripted {
    fn disclosure(&self) -> Disclosure {
        Disclosure { recipient: "example.test".into(), model: "test-model".into(), leaves_machine: self.0.remote }
    }

    fn send(&mut self, request: &Request) -> Result<Reply, Error> {
        self.0.sent.lock().unwrap().push(request.clone());
        self.0.result.clone()
    }
}

fn scripted() -> Result<Box<dyn Provider>, Error> {
    match NEXT.with(|next| next.borrow().clone()) {
        Some(script) => Ok(Box::new(Scripted(script))),
        None => Err(Error::MissingKey),
    }
}

fn reply(text: &str) -> Reply {
    Reply {
        text: text.into(),
        stop: Stop::End,
        usage: Usage { input_tokens: 10, output_tokens: 3 },
        model: "test-model".into(),
        fallback_used: false,
    }
}

fn script(remote: bool, result: Result<Reply, Error>) -> Script {
    Script { remote, result, sent: Arc::default() }
}

/// An app showing `text` whose provider follows `script` (no script: no key).
fn app(text: &str, script: Option<Script>) -> (WordApp, Arc<Mutex<Vec<Request>>>) {
    let sent = script.as_ref().map(|s| s.sent.clone()).unwrap_or_default();
    NEXT.with(|next| *next.borrow_mut() = script);
    let mut app = WordApp::new(wordcraft_engine::Session::new(wordcraft_doc::Document::from_text(text)), crate::Services::default());
    app.assistant.factory = scripted;
    (app, sent)
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
    assert!(app.assistant.consented);
    // Agreed once for this run: the next request is not asked about again.
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
    assert!(!app.assistant.consented);
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
    big.assistant.document_scope = true;
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
    app.assistant.document_scope = true;
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
    assert!(app.assistant.consented, "what was agreed this run stays agreed");
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
    assert!(fresh.consented && fresh.history.is_empty() && fresh.reply.is_none() && fresh.prompt.is_empty());
}

/// The pane opens from its command and draws its controls, in a left-to-right and a right-to-left interface.
#[test]
#[cfg(not(target_arch = "wasm32"))]
fn the_pane_opens_with_its_command_and_shows_its_controls() {
    use egui_kittest::kittest::Queryable;

    let (mut app, _) = app("Hello", Some(script(false, Ok(reply("ok")))));
    app.run("view.assistantPane", json!({"value": true})).unwrap();
    assert!(crate::panes::open(&app).contains(&crate::panes::Pane::Assistant));
    let mut h = crate::test_harness::builder().with_size(egui::vec2(1440.0, 900.0)).build_ui_state(
        |ui, app: &mut WordApp| {
            let ctx = ui.ctx().clone();
            app.logic(&ctx);
            app.ui(ui);
        },
        app,
    );
    h.run_steps(4);
    for label in ["Send", "New Chat", "Instruction", "Use the whole document instead of the selection"] {
        h.get_by_label(label);
    }
    h.state_mut().run("view.assistantPane", json!({"value": false})).unwrap();
    h.run_steps(2);
    assert!(!crate::panes::open(h.state()).contains(&crate::panes::Pane::Assistant), "the command closes it");
}
