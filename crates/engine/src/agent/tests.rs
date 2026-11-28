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
    assert_eq!(ids, ["select.range", "format.bold"], "reads are not steps; a selection by words is recorded as its exact place");
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

/// The steps are replayed from the selection the workspace started with, wherever the person has clicked
/// since: a step that acts on the given selection lands where the assistant made it.
#[test]
fn steps_replay_from_where_the_workspace_started() {
    let mut s = session("Alpha beta");
    s.run("select.text", &json!({"text": "beta"})).unwrap();
    let mut tools = Toolbox::new(Seed::of(&s, ""));
    assert!(!tools.call("type_text", &json!({"text": "gamma"})).is_error);
    let (steps, start) = (tools.steps().to_vec(), tools.start().clone());
    s.run("caret.docStart", &json!({})).unwrap();
    s.run("assistant.runSteps", &json!({"steps": steps, "start": start})).unwrap();
    assert_eq!(body(&s), "Alpha gamma");
}

/// A command that is read-only in name but moves the selection (Spelling selects the next issue) is a step,
/// so what is done at that selection is replayed there.
#[test]
fn a_command_that_moves_the_selection_is_a_step() {
    if crate::tests::no_language_data("a_command_that_moves_the_selection_is_a_step") {
        return;
    }
    let s = session("The colour and the flavour.");
    let mut tools = Toolbox::new(Seed::of(&s, ""));
    for (word, fix) in [("colour", "color"), ("flavour", "flavor")] {
        let found = tools.call("execute", &json!({"command": "review.spelling"}));
        assert!(found.content.contains(word), "{}", found.content);
        assert!(!tools.call("execute", &json!({"command": "review.applySuggestion", "params": {"text": fix}})).is_error);
    }
    assert_eq!(body(tools.workspace()), "The color and the flavor.");
    let ids: Vec<&str> = tools.steps().iter().filter_map(|st| st["command"].as_str()).collect();
    assert!(!ids.contains(&"review.spelling"), "the moves are the places they reached: {ids:?}");
    let replayed = replay(Seed::of(&s, ""), tools.steps()).unwrap();
    assert_eq!(body(&replayed), body(tools.workspace()), "the replay gives what the assistant saw");
}

/// A call that fails leaves nothing behind that a later step depends on: a bad pattern does not leave Find
/// set to patterns, so what follows runs in the workspace as it is replayed.
#[test]
fn a_failed_call_leaves_no_trace() {
    let s = session("Mr. Smith and Mrs. Jones");
    let mut tools = Toolbox::new(Seed::of(&s, ""));
    let bad = tools.call("find", &json!({"text": "(?<=Mr)\\.", "regex": true}));
    assert!(bad.is_error, "{}", bad.content);
    assert!(!tools.call("replace_all", &json!({"text": "Mr.", "with": "Mr"})).is_error);
    assert_eq!(body(tools.workspace()), "Mr Smith and Mrs. Jones", "a plain replacement, not a pattern");
    let replayed = replay(Seed::of(&s, ""), tools.steps()).unwrap();
    assert_eq!(body(&replayed), body(tools.workspace()));
}

/// A call that only opens a dialog leaves the selection as it was, and a command that only asks the window
/// to scroll is a step like any other.
#[test]
fn a_dialog_leaves_no_trace_and_a_scroll_is_no_dialog() {
    let s = session("One\nTwo");
    let mut tools = Toolbox::new(Seed::of(&s, ""));
    tools.call("select_text", &json!({"text": "Two"}));
    let before = tools.workspace().sel.clone();
    assert!(tools.call("execute", &json!({"command": "edit.goto"})).is_error);
    assert_eq!(tools.workspace().sel, before, "the dialog-only call is put back");
    let goto = tools.call("execute", &json!({"command": "edit.goto", "params": {"page": 1}}));
    assert!(!goto.is_error, "{}", goto.content);
    assert_eq!(tools.steps().last().and_then(|st| st["command"].as_str()), Some("select.range"), "a move is recorded as the place it reached");
    let replayed = replay(Seed::of(&s, ""), tools.steps()).unwrap();
    assert_eq!(replayed.sel, tools.workspace().sel, "the replay goes where the assistant went");
}

/// With Track Changes on, a change that cannot be tracked is refused and leaves nothing; tracked typing,
/// new paragraphs and replacements go through.
#[test]
fn with_track_changes_on_an_untracked_change_is_refused() {
    let mut s = session("Alpha beta");
    s.run("review.trackChanges", &json!({"value": true})).unwrap();
    let mut tools = Toolbox::new(Seed::of(&s, "Helper"));
    tools.call("select_text", &json!({"text": "beta"}));
    let table = tools.call("insert_table", &json!({"values": [["a", "b"], ["c", "d"]]}));
    assert!(table.is_error && table.content.contains("Track Changes"), "{}", table.content);
    assert!(!tools.changed(), "the refused table left nothing");
    for (tool, input) in [
        ("type_text", json!({"text": "gamma"})),
        ("type_text", json!({"text": "one\ntwo", "paragraphs": true})),
        ("replace_all", json!({"text": "Alpha", "with": "Omega"})),
    ] {
        let done = tools.call(tool, &input);
        assert!(!done.is_error, "{tool}: {}", done.content);
    }
    assert!(tools.changed());
    let replayed = replay(Seed::of(&s, "Helper"), tools.steps()).unwrap();
    assert_eq!(body(&replayed), body(tools.workspace()));
}

/// One batch runs at most its limit, and one request records at most its limit of changes: the model is
/// told before anything is lost, not when the changes are applied.
#[test]
fn the_limits_are_told_while_the_assistant_works() {
    let s = session("Hello");
    let mut tools = Toolbox::new(Seed::of(&s, ""));
    let many = vec![json!({"command": "select.all"}); MAX_BATCH + 1];
    let refused = tools.call("batch", &json!({"commands": many}));
    assert!(refused.is_error && refused.content.contains("at most"), "{}", refused.content);
    let batch = vec![json!({"command": "select.all"}); MAX_BATCH];
    while tools.steps().len() < MAX_STEPS {
        assert!(!tools.call("batch", &json!({"commands": batch})).is_error);
    }
    let over = tools.call("execute", &json!({"command": "select.all"}));
    assert!(over.is_error && over.content.contains("most changes"), "{}", over.content);
    assert_eq!(tools.steps().len(), MAX_STEPS);
    assert!(replay(Seed::of(&s, ""), tools.steps()).is_ok(), "what was recorded can be applied");
}

/// A runaway copy and paste stops at the bound on how far one request may grow the document.
#[test]
fn the_document_cannot_grow_without_bound() {
    let s = session(&"x".repeat(1000));
    let mut tools = Toolbox::new(Seed::of(&s, ""));
    let doubling =
        [json!({"command": "select.all"}), json!({"command": "edit.copy"}), json!({"command": "caret.docEnd"}), json!({"command": "edit.paste"})];
    let commands: Vec<Value> = doubling.iter().cycle().take(48).cloned().collect();
    tools.call("batch", &json!({"commands": commands}));
    let limit = 1000 + 2_000_000;
    let size = body(tools.workspace()).len();
    assert!(size > 1000 && size <= limit, "it grew, and stopped at the bound: {size}");
    let again = tools.call("execute", &json!({"command": "edit.paste"}));
    assert!(again.is_error && again.content.contains("more than this request may grow"), "{}", again.content);
    assert!(tools.hit_limit(), "a limit stopped the work");
    assert_eq!(body(tools.workspace()).len(), size, "the refused paste left nothing");
}

/// Fields that read a file or a web address, or start a program, are refused; hyperlinks and other fields are
/// not. HTML and RTF are not pasted.
#[test]
fn fields_that_reach_out_and_rich_pastes_are_refused() {
    for code in [
        "INCLUDEPICTURE \"https://example.test/p.png\" \\d",
        "includetext \"C:\\\\docs\\\\x.docx\"",
        "DDEAUTO cmd \"/c calc\"",
        "QUOTE \"https://example.test/?d=x\"",
    ] {
        assert!(refusal("insert.field", &json!({"instr": code})).is_some(), "{code}");
        assert!(refusal("fields.setCode", &json!({"code": code})).is_some(), "{code}");
    }
    for code in ["HYPERLINK \"https://example.test\"", "DATE \\@ \"d MMMM yyyy\"", "PAGE"] {
        assert!(refusal("insert.field", &json!({"instr": code})).is_none(), "{code}");
    }
    assert!(refusal("edit.paste", &json!({"html": "<img src=\"https://example.test/p.png\">"})).is_some());
    assert!(refusal("edit.pasteSpecial", &json!({"rtf": "{\\rtf1}"})).is_some());
    assert!(refusal("edit.paste", &json!({"text": "plain"})).is_none());
}

/// Parameters named like files that are not files (a direction, an original size) do not refuse a command.
#[test]
fn names_like_files_that_are_not_files_are_allowed() {
    assert!(refusal("shape.textDirection", &json!({"dir": "vert"})).is_none());
    assert!(refusal("equation.move", &json!({"dir": "left"})).is_none());
    assert!(refusal("picture.size", &json!({"original": true})).is_none());
    assert!(refusal("format.bold", &json!({"dir": "/tmp"})).is_some());
}

/// The assistant reads the document as it reads with every tracked change accepted: deleted text is in no
/// tool's result, the readers that show it are closed, and a phrase that spans a deletion can be selected.
#[test]
fn the_assistant_never_reads_tracked_deletions() {
    let mut s = session("The cap is $2M now");
    s.run("review.trackChanges", &json!({"value": true})).unwrap();
    s.run("select.text", &json!({"text": "$2M"})).unwrap();
    s.run("text.insert", &json!({"text": "$3M"})).unwrap();
    assert!(s.doc.plain_text(StoryRef::Body).contains("$2M"), "the deletion is tracked");
    let mut tools = Toolbox::new(Seed::of(&s, ""));
    let mut seen = tools.overview();
    for (tool, input) in [
        ("get_text", json!({})),
        ("inspect_document", json!({"parts": true})),
        ("get_selection", json!({})),
        ("select_text", json!({"text": "cap is $3M"})),
        ("get_selection", json!({})),
    ] {
        let out = tools.call(tool, &input);
        assert!(!out.is_error, "{tool}: {}", out.content);
        seen.push_str(&out.content);
    }
    assert!(!seen.contains("$2M"), "{seen}");
    assert!(seen.contains("cap is $3M"), "the phrase across the deletion was selected: {seen}");
    for id in ["document.paragraph", "review.changes"] {
        assert!(tools.call("execute", &json!({"command": id, "params": {"path": [0]}})).is_error, "{id}");
    }
}

/// Repeat and macro recording leave the assistant's changes out: they were made for one document as it was.
#[test]
fn repeat_does_not_apply_the_assistant_s_changes_again() {
    let mut s = session("Hello world");
    // The person's own last command, which Repeat is for.
    s.run("format.bold", &json!({})).unwrap();
    let mut tools = Toolbox::new(Seed::of(&s, ""));
    tools.call("execute", &json!({"command": "caret.docEnd"}));
    tools.call("type_text", &json!({"text": "!"}));
    s.run("assistant.runSteps", &json!({"steps": tools.steps(), "start": tools.start()})).unwrap();
    assert_eq!(body(&s), "Hello world!");
    let _ = s.run("edit.repeat", &json!({}));
    assert_eq!(body(&s), "Hello world!", "Repeat does not type the assistant's text again");
}

/// A failed call between building a selection of several shapes and acting on it leaves that selection as it
/// was: what the assistant fills is what the replay fills.
#[test]
fn a_failed_call_keeps_a_selection_of_several_objects() {
    let mut s = Session::new(wordcraft_doc::Document::new());
    s.run("insert.shape", &json!({"kind": "rectangle", "width": 80, "height": 40})).unwrap();
    s.run("select.collapse", &json!({"end": true})).unwrap();
    s.run("insert.shape", &json!({"kind": "ellipse", "width": 80, "height": 40})).unwrap();
    s.run("select.collapse", &json!({"end": true})).unwrap();
    let mut tools = Toolbox::new(Seed::of(&s, ""));
    assert!(!tools.call("execute", &json!({"command": "select.objects", "params": {"index": 0}})).is_error);
    assert!(!tools.call("execute", &json!({"command": "select.addObject", "params": {"index": 1}})).is_error);
    let both = tools.workspace().also_selected.clone();
    assert!(!both.is_empty(), "two objects are selected");
    assert!(tools.call("execute", &json!({"command": "shape.fill", "params": {"color": "red"}})).is_error, "not a colour it reads");
    assert_eq!(tools.workspace().also_selected, both, "the failed call left the selection as it was");
    assert!(!tools.call("execute", &json!({"command": "shape.fill", "params": {"color": "FF0000"}})).is_error);
    assert!(tools.verify().is_ok(), "the replay fills what the assistant filled");
}

/// A command that only moves the selection is recorded as the place it reached, so the replay lands there
/// whatever the word lists or the layout say by then.
#[test]
fn a_move_is_recorded_as_the_place_it_reached() {
    let s = session("One two\nThree four");
    let mut tools = Toolbox::new(Seed::of(&s, ""));
    assert!(!tools.call("execute", &json!({"command": "caret.docEnd"})).is_error);
    assert_eq!(tools.steps().last().and_then(|st| st["command"].as_str()), Some("select.range"));
}

/// The start of a replay keeps the layout geometry the view does not save (the Web Layout width).
#[test]
fn the_start_keeps_the_web_layout_width() {
    let mut s = session("Text");
    s.view.web_width = 1234.0;
    let start = Seed::of(&s, "").start();
    let other = session("Text");
    let replayed = Seed::of(&other, "").starting_at(&start).unwrap().workspace();
    assert_eq!(replayed.view.web_width, 1234.0);
}

/// Readers that return the stored text, notes and headings as stored, copies in markup: none of them
/// carries text deleted with Track Changes on to the model.
#[test]
fn no_reader_carries_tracked_deletions() {
    let mut s = session("The cap is $2M now");
    s.run("review.trackChanges", &json!({"value": true})).unwrap();
    s.run("select.text", &json!({"text": "$2M"})).unwrap();
    s.run("text.insert", &json!({"text": "$3M"})).unwrap();
    let mut tools = Toolbox::new(Seed::of(&s, ""));
    let mut seen = String::new();
    for (tool, input) in [
        ("execute", json!({"command": "document.inspect"})),
        ("execute", json!({"command": "document.selection"})),
        ("execute", json!({"command": "document.text"})),
        ("execute", json!({"command": "references.notes"})),
        ("execute", json!({"command": "link.targets"})),
        ("batch", json!({"commands": [{"command": "document.inspect"}], "keepGoing": true})),
        ("execute", json!({"command": "select.all"})),
        ("execute", json!({"command": "edit.copy"})),
        ("get_selection", json!({})),
    ] {
        seen.push_str(&tools.call(tool, &input).content);
    }
    assert!(!seen.contains("$2M"), "{seen}");
}

/// With Track Changes on, comments and bookmarks are added (they are not tracked changes, in Word either),
/// and commands whose changes WordCraft does not track (a field's code, an equation) are refused.
#[test]
fn with_track_changes_on_comments_go_in_and_field_changes_do_not() {
    let mut s = session("Alpha beta");
    s.run("review.trackChanges", &json!({"value": true})).unwrap();
    let mut tools = Toolbox::new(Seed::of(&s, ""));
    tools.call("select_text", &json!({"text": "beta"}));
    for (id, params) in [("review.newComment", json!({"text": "Check this"})), ("insert.bookmark", json!({"name": "here"}))] {
        let done = tools.call("execute", &json!({"command": id, "params": params}));
        assert!(!done.is_error, "{id}: {}", done.content);
    }
    let field = tools.call("execute", &json!({"command": "fields.setCode", "params": {"code": "DATE"}}));
    assert!(field.is_error && field.content.contains("Track Changes"), "{}", field.content);
    assert!(tools.verify().is_ok(), "comment times aside, the replay gives the same document");
}

/// RTF and HTML are refused however a paste passes them.
#[test]
fn rich_pastes_are_refused_however_they_are_passed() {
    assert!(refusal("edit.pasteSpecial", &json!({"as": "rtf", "text": "{\\rtf1 x}"})).is_some());
    assert!(refusal("edit.pasteSpecial", &json!({"as": "html", "text": "plain"})).is_some());
    assert!(refusal("edit.paste", &json!({"text": "<p>x</p>"})).is_some());
    assert!(refusal("edit.paste", &json!({"text": "1 < 2 and 3 > 2"})).is_none(), "plain text that mentions brackets is text");
}

/// The bound on growth counts paragraphs, not only their letters: a document of empty paragraphs is not
/// free to copy.
#[test]
fn empty_paragraphs_count_towards_the_growth_bound() {
    let s = session(&"\n".repeat(99));
    assert!(text_bytes(&s.doc) >= 100 * PARAGRAPH_COST);
}

/// A save meanwhile does not count as a change; a new title does.
#[test]
fn a_save_is_no_change_and_a_new_title_is() {
    let s = session("Text");
    let mut saved = s.doc.clone();
    saved.core.modified = "2026-11-27T12:00:00Z".into();
    saved.core.revision += 1;
    saved.settings.zoom = Some(wordcraft_doc::Zoom { percent: 150, fit: None });
    assert!(unchanged_since(&saved, &s.doc));
    let mut retitled = s.doc.clone();
    retitled.core.title = "Quarterly Report".into();
    assert!(!unchanged_since(&retitled, &s.doc));
}

/// Hidden text the person does not show is in no tool's result, and a phrase that spans it is found and
/// selected by the text the model read, at the right place in the stored text: an edit made there lands
/// where the model meant, around the hidden text, on the workspace and in the replay.
#[test]
fn the_assistant_does_not_read_hidden_text_and_edits_around_it() {
    let mut s = session("The fee is confidential-figure due now\nLast line");
    s.run("select.text", &json!({"text": "confidential-figure "})).unwrap();
    s.run("format.hidden", &json!({"value": true})).unwrap();
    let mut tools = Toolbox::new(Seed::of(&s, ""));
    let mut seen = tools.overview();
    for (tool, input) in [
        ("get_text", json!({})),
        ("inspect_document", json!({"parts": true})),
        ("execute", json!({"command": "select.all"})),
        ("get_selection", json!({})),
        ("execute", json!({"command": "document.inspect"})),
        ("execute", json!({"command": "document.text"})),
        ("execute", json!({"command": "edit.copy"})),
    ] {
        seen.push_str(&tools.call(tool, &input).content);
    }
    assert!(!seen.contains("confidential-figure"), "{seen}");
    assert!(seen.contains("The fee is due now"), "the text around it is read as one: {seen}");

    // "is due" is two words apart in the stored text: the selection covers the hidden text between them.
    assert!(!tools.call("select_text", &json!({"text": "is due"})).is_error);
    assert!(!tools.call("type_text", &json!({"text": "was paid"})).is_error);
    assert_eq!(body(tools.workspace()), "The fee was paid now\nLast line", "the edit replaced exactly the words read");
    let replayed = replay(Seed::of(&s, ""), tools.steps()).unwrap();
    assert_eq!(body(&replayed), body(tools.workspace()), "the replay places it the same way");
    // A word after the hidden text is placed past it.
    let mut tools = Toolbox::new(Seed::of(&s, ""));
    assert!(!tools.call("select_text", &json!({"text": "due"})).is_error);
    assert!(!tools.call("type_text", &json!({"text": "owed"})).is_error);
    assert!(tools.workspace().doc.plain_text(StoryRef::Body).contains("confidential-figure owed now"));
}

/// With hidden text shown on screen (Show/Hide ¶ or Options › Display › Hidden text), the assistant reads
/// it as the person sees it.
#[test]
fn shown_hidden_text_is_read() {
    let mut s = session("The fee is confidential-figure due now");
    s.run("select.text", &json!({"text": "confidential-figure "})).unwrap();
    s.run("format.hidden", &json!({"value": true})).unwrap();
    s.prefs.marks.hidden = true;
    let mut tools = Toolbox::new(Seed::of(&s, ""));
    assert!(tools.overview().contains("confidential-figure"));
    assert!(tools.call("get_text", &json!({})).content.contains("confidential-figure"));
}
