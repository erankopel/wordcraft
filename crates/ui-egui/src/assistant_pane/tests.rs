use std::cell::RefCell;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::json;
use wordcraft_assistant::{Disclosure, Error, MAX_CONTEXT_BYTES, Provider, Reply, Request, Scope, Stop, Usage};
use wordcraft_doc::StoryRef;

use super::{Act, WordApp};

/// What the scripted provider answers and whether its text leaves the machine, and what it was asked.
#[derive(Clone)]
struct Script {
    remote: bool,
    result: Result<Reply, Error>,
    sent: Arc<Mutex<Vec<Request>>>,
    /// While true, `send` does not return: a request that takes a long time.
    hold: Arc<std::sync::atomic::AtomicBool>,
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
        usage: Usage { input_tokens: 10, output_tokens: 3 },
        model: "test-model".into(),
        fallback_used: false,
    }
}

fn script(remote: bool, result: Result<Reply, Error>) -> Script {
    Script { remote, result, sent: Arc::default(), hold: Arc::default() }
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
    assert_eq!(app.assistant_run.agreed, Some(Scope::Selection));
    // Agreed once for this run: the next request, sharing no more, is not asked about again.
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
    assert_eq!(app.assistant_run.agreed, None);
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
    assert_eq!(app.assistant_run.agreed, Some(Scope::Selection), "what was agreed this run stays agreed");
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
    assert!(app.assistant_run.agreed.is_some(), "the agreement is the run's, not the chat's");
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

/// Agreeing to the selection says nothing about the whole document: a wider request asks again, and
/// agreeing to the document covers the selection too.
#[test]
fn agreeing_to_the_selection_does_not_cover_the_whole_document() {
    let ctx = egui::Context::default();
    let (mut app, sent) = app("First paragraph\nSecond paragraph", Some(script(true, Ok(reply("Reply")))));
    app.run("select.text", json!({"text": "First"})).unwrap();
    ask(&mut app, &ctx, "One");
    app.assistant_act(Act::Agree, &ctx);
    settle(&mut app, &ctx);
    assert_eq!(app.assistant_run.agreed, Some(Scope::Selection));
    // The same amount again: no question.
    ask(&mut app, &ctx, "Two");
    assert!(app.assistant.ready.is_none());
    settle(&mut app, &ctx);
    assert_eq!(sent.lock().unwrap().len(), 2);
    // More text: the question comes back, and nothing is sent until it is answered.
    app.assistant.document_scope = true;
    ask(&mut app, &ctx, "Three");
    assert!(app.assistant.ready.is_some(), "the whole document was not agreed to");
    assert_eq!(sent.lock().unwrap().len(), 2, "nothing is sent while it waits");
    app.assistant_act(Act::Agree, &ctx);
    settle(&mut app, &ctx);
    assert_eq!(app.assistant_run.agreed, Some(Scope::Document));
    assert_eq!(sent.lock().unwrap().len(), 3);
    // Having agreed to the document covers the selection too.
    app.assistant.document_scope = false;
    ask(&mut app, &ctx, "Four");
    assert!(app.assistant.ready.is_none());
    settle(&mut app, &ctx);
    assert_eq!(sent.lock().unwrap().len(), 4);
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
    assert!(app.announcement.as_ref().is_some_and(|a| a.text().contains("stopped")), "the stop is announced");
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
        let said = app.announcement.as_ref().map(|a| a.text().to_owned()).unwrap_or_default();
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
    app.assistant.document_scope = true;
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
    assert_eq!(app.assistant_run.agreed, Some(Scope::Selection), "what was agreed this run stays agreed");
    assert_eq!(app.assistant_run.typed_key(), Some("sk-ant-typed-key"), "and so does the key");
}

#[cfg(not(target_arch = "wasm32"))]
mod drawn {
    use egui_kittest::kittest::{By, NodeT, Queryable};

    use super::*;

    /// The window with the pane open, drawn for a few frames.
    fn open(app: WordApp) -> egui_kittest::Harness<'static, WordApp> {
        open_in(app, None)
    }

    /// The same in an interface language: it takes effect in a frame, and the pane then opens on the side it
    /// reads from.
    fn open_in(app: WordApp, language: Option<&str>) -> egui_kittest::Harness<'static, WordApp> {
        let mut h = crate::test_harness::builder().with_size(egui::vec2(1440.0, 900.0)).build_ui_state(
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
        app.assistant.document_scope = true;
        ask(&mut app, &ctx, "Summarise");
        assert_eq!(app.assistant.ready.as_ref().map(|r| r.request.scope), Some(Scope::Document));
        let mut h = open(app);
        assert!(h.query_by_label_contains("the whole document (").is_some(), "the agreement names the whole document");
        assert!(h.query_by_label_contains("The text starts: First paragraph Second paragraph").is_some(), "and shows how the text starts");
        assert!(h.query_by_label_contains("another of its models").is_some(), "and says another model may read it if the first declines");
        for label in ["Use the whole document instead of the selection", "Instruction", "API key"] {
            assert!(h.get_by_label(label).accesskit_node().is_disabled(), "{label} cannot change while the agreement waits");
        }
        // Were the setting changed behind the pane's back, the agreement still says what would be sent.
        h.state_mut().assistant.document_scope = false;
        h.run_steps(2);
        assert!(h.query_by_label_contains("the whole document (").is_some() && h.query_by_label_contains("the selected text").is_none());
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
        assert!(h.query_by_label_contains("no document text goes with it").is_some());
        assert!(h.query_by_label_contains("The text starts").is_none());
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
            // The notice and the announcement of the same state are both in the tree.
            let mentions = |words: &str| h.query_all_by_label_contains(words).count();
            match expected {
                Some(words) => assert!(mentions(words) >= 1, "{words}"),
                None => assert!(mentions("incomplete") == 0 && mentions("cut off") == 0 && mentions("ran out of length") == 0),
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
        let live = h.query_all(By::new().role(egui::accesskit::Role::Status)).any(|n| {
            n.accesskit_node().label().is_some_and(|l| l.contains("Nothing is sent until you agree"))
                && n.accesskit_node().live() != egui::accesskit::Live::Off
        });
        assert!(live, "the agreement is a live region");
        let agree = h.get_by_role_and_label(egui::accesskit::Role::Button, "Agree and Send");
        assert!(agree.is_focused(), "the focus is on the button that decides");
    }
}
