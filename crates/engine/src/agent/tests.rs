use serde_json::json;
use wordcraft_doc::StoryRef;

use super::*;

/// A session whose body is `text` (paragraphs split on newlines).
fn session(text: &str) -> Session {
    let mut s = Session::new(wordcraft_doc::Document::new());
    s.run("document.setText", &json!({"text": text})).unwrap();
    s
}

fn body(s: &Session) -> String {
    s.doc.final_text(StoryRef::Body)
}

/// The commands that edit the document are open to an assistant; files, the network, preferences,
/// macros, the window and the person's control of Track Changes are not, and no command may name a
/// file.
#[test]
fn the_policy_opens_editing_and_closes_everything_else() {
    for id in [
        "format.bold",
        "para.style",
        "table.merge",
        "select.text",
        "text.insert",
        "styles.modify",
        "review.newComment",
        "edit.replaceAll",
        "insert.table",
    ] {
        assert!(may_run(id), "{id} edits the document");
    }
    for id in [
        "file.save",
        "file.open",
        "file.close",
        "tools.macros",
        "tools.recordMacro",
        "edit.repeat",
        "view.zoom",
        "ui.tab",
        "review.trackChanges",
        "review.acceptAll",
        "review.rejectAll",
        "review.restrict",
        "proof.installPack",
        "font.add",
        "insert.picture",
        "insert.textFromFile",
        "document.setText",
        "assistant.runSteps",
        "assistant.apply",
        "readAloud.playPause",
        "mailings.finish",
        "made.up",
    ] {
        assert!(!may_run(id), "{id} must stay closed to an assistant");
    }
    assert!(refusal("format.bold", &json!({"path": "/etc/passwd"})).is_some(), "no command may name a file");
    assert!(refusal("references.sources", &json!({"import": "x.xml"})).is_some());
    assert!(refusal("format.bold", &json!({})).is_none());
}

/// A workspace refuses what an assistant may not run, also when a command would run it for them.
#[test]
fn a_workspace_refuses_closed_commands_and_reads_no_files() {
    let s = session("Hello world");
    let mut ws = Seed::of(&s, "").workspace();
    assert!(ws.run("review.trackChanges", &json!({"value": false})).is_err());
    let macro_run =
        ws.run("tools.macros", &json!({"define": {"name": "m", "steps": [{"command": "file.save", "params": {"path": "/tmp/x.docx"}}]}, "run": "m"}));
    assert!(macro_run.is_err(), "a macro cannot carry a closed command past the workspace");
    assert!(ws.run("format.bold", &json!({})).is_ok(), "editing works");
    assert!(confined(|| crate::file_access::check_read("/etc/hostname")).is_err(), "no file can be read in the assistant's work");
    assert!(confined(|| crate::file_access::check_write("/tmp/anything")).is_err(), "nor written");
}

/// The tools run on the workspace, not on the open document, and record the calls that change
/// something as steps; reading calls are not steps.
#[test]
fn tool_calls_change_the_workspace_and_record_steps() {
    let s = session("Hello world\nSecond line");
    let undo_before = s.undo_depth();
    let mut tools = Toolbox::new(Seed::of(&s, ""));
    assert!(!tools.call("select_text", &json!({"text": "world"})).is_error);
    assert!(!tools.call("execute", &json!({"command": "format.bold"})).is_error);
    assert!(!tools.call("get_text", &json!({})).is_error);
    let inspected = tools.call("inspect_document", &json!({"from": 1, "count": 1}));
    assert!(!inspected.is_error && inspected.content.contains("Second line") && !inspected.content.contains("Hello world"), "{}", inspected.content);
    let ids: Vec<&str> = tools.steps().iter().filter_map(|st| st["command"].as_str()).collect();
    assert_eq!(ids, ["select.text", "format.bold"], "reads are not steps");
    assert!(tools.changed());
    assert_eq!(body(&s), "Hello world\nSecond line");
    assert_eq!(s.undo_depth(), undo_before, "the open document has no new undo step");
}

/// A refused command and a command that only opens a dialog are failures the model reads, never
/// successes, and leave no step.
#[test]
fn a_refused_or_dialog_only_call_is_an_error() {
    let s = session("Hello world");
    let mut tools = Toolbox::new(Seed::of(&s, ""));
    let refused = tools.call("execute", &json!({"command": "file.save", "params": {"path": "/tmp/out.docx"}}));
    assert!(refused.is_error && refused.content.contains("not available"), "{}", refused.content);
    let dialog = tools.call("execute", &json!({"command": "edit.goto"}));
    assert!(dialog.is_error, "edit.goto without parameters only opens a dialog: {}", dialog.content);
    assert!(tools.workspace().ui_requests.is_empty(), "no request reaches a window");
    let unknown = tools.call("no_such_tool", &json!({}));
    assert!(unknown.is_error);
    assert!(tools.steps().is_empty());
}

/// The list of commands offers only what the assistant may run.
#[test]
fn listed_commands_are_only_the_open_ones() {
    let s = session("Hello");
    let mut tools = Toolbox::new(Seed::of(&s, ""));
    let listed = tools.call("list_commands", &json!({"query": "save"}));
    assert!(!listed.is_error);
    assert!(!listed.content.contains("\"file.save\""), "{}", listed.content);
    let bold = tools.call("list_commands", &json!({"query": "bold"}));
    assert!(bold.content.contains("format.bold"), "{}", bold.content);
}

/// Replaying the recorded steps on a fresh workspace of the same document gives the same document.
#[test]
fn steps_replay_to_the_same_document() {
    let s = session("Alpha beta gamma\nDelta");
    let mut tools = Toolbox::new(Seed::of(&s, ""));
    tools.call("select_text", &json!({"text": "beta"}));
    tools.call("execute", &json!({"command": "format.italic"}));
    tools.call("select_text", &json!({"text": "Delta"}));
    tools.call("type_text", &json!({"text": "Epsilon"}));
    tools.call("replace_all", &json!({"text": "gamma", "with": "Gamma"}));
    let replayed = replay(Seed::of(&s, ""), tools.steps()).unwrap();
    assert_eq!(body(&replayed), body(tools.workspace()));
    assert!(replayed.doc.same_content(&tools.workspace().doc), "formatting replays too");
}

/// A tool call budget ends a runaway request.
#[test]
fn the_call_budget_ends_a_request() {
    let s = session("Hello");
    let mut tools = Toolbox::new(Seed::of(&s, ""));
    for _ in 0..MAX_CALLS {
        tools.call("get_text", &json!({}));
    }
    let over = tools.call("get_text", &json!({}));
    assert!(over.is_error && over.content.contains("tool calls"), "{}", over.content);
}

/// A long result is cut at a character boundary and says so, never cut inside a Hebrew letter.
#[test]
fn long_results_are_cut_at_a_character_and_say_so() {
    let text = "א".repeat(RESULT_BUDGET);
    let cut = within_budget(&text, RESULT_BUDGET);
    assert!(cut.len() < text.len() + 200 && cut.contains("[cut:"), "{}", &cut[cut.len() - 80..]);
}
