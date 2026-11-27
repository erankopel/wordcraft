//! The AI assistant's engine commands: what would be shared, and putting a reply into the document.

use serde_json::json;
use wordcraft_doc::StoryRef;

use crate::Session;

fn run(s: &mut Session, id: &str, v: serde_json::Value) -> serde_json::Value {
    s.run(id, &v).unwrap_or_else(|e| panic!("{id}: {e}"))
}

/// A session whose body is `text` (paragraphs split on newlines).
fn session(text: &str) -> Session {
    let mut s = Session::new(wordcraft_doc::Document::new());
    run(&mut s, "document.setText", json!({"text": text}));
    s
}

fn body(s: &Session) -> String {
    s.doc.final_text(StoryRef::Body)
}

fn select(s: &mut Session, text: &str) {
    run(s, "select.text", json!({"text": text}));
}

#[test]
fn context_is_the_selected_text_or_the_whole_body_and_changes_nothing() {
    let mut s = session("Hello brave world\nSecond paragraph");
    select(&mut s, "brave");
    let before = serde_json::to_value(&s.doc).unwrap();
    let selection = run(&mut s, "assistant.context", json!({}));
    assert_eq!(selection["scope"], "selection", "the selection is the default");
    assert_eq!(
        (selection["text"].as_str(), selection["hasSelection"].as_bool(), selection["empty"].as_bool()),
        (Some("brave"), Some(true), Some(false))
    );
    let document = run(&mut s, "assistant.context", json!({"scope": "document"}));
    assert_eq!(document["text"], "Hello brave world\nSecond paragraph");
    assert_eq!(document["paragraphs"], 2);
    assert!(document["bytes"].as_u64() >= document["chars"].as_u64());
    assert_eq!(serde_json::to_value(&s.doc).unwrap(), before, "reading what would be shared changes nothing");
}

#[test]
fn with_nothing_selected_the_selection_scope_is_empty_and_says_so() {
    let mut s = session("Hello world");
    run(&mut s, "caret.docStart", json!({}));
    let ctx = run(&mut s, "assistant.context", json!({"scope": "selection"}));
    assert_eq!((ctx["empty"].as_bool(), ctx["hasSelection"].as_bool(), ctx["chars"].as_u64()), (Some(true), Some(false), Some(0)));
}

#[test]
fn text_deleted_by_track_changes_is_not_shared() {
    let mut s = session("keep drop keep");
    run(&mut s, "review.trackChanges", json!({"value": true}));
    select(&mut s, "drop ");
    run(&mut s, "text.delete", json!({}));
    assert!(s.doc.plain_text(StoryRef::Body).contains("drop"), "the deleted text is still in the file, marked deleted");
    let shared = run(&mut s, "assistant.context", json!({"scope": "document"}));
    assert_eq!(shared["text"], "keep keep", "a tracked deletion is not shared: {shared}");
}

/// The same rule for a selection: text deleted with Track Changes on is still in the file, so reading the
/// selection as plain text would have shared it.
#[test]
fn text_deleted_by_track_changes_is_not_shared_from_a_selection_either() {
    let mut s = session("keep drop keep\nsecond paragraph");
    run(&mut s, "review.trackChanges", json!({"value": true}));
    select(&mut s, "drop ");
    run(&mut s, "text.delete", json!({}));
    assert!(s.doc.plain_text(StoryRef::Body).contains("drop"), "the deleted text is still in the file");
    run(&mut s, "select.all", json!({}));
    let selection = run(&mut s, "assistant.context", json!({"scope": "selection"}));
    let document = run(&mut s, "assistant.context", json!({"scope": "document"}));
    assert!(!selection["text"].as_str().unwrap_or_default().contains("drop"), "a tracked deletion is not shared: {selection}");
    assert_eq!(selection["text"], document["text"], "with everything selected both scopes read the same text");
    // A selection that covers only part of the text is read the same way.
    select(&mut s, "keep drop");
    let part = run(&mut s, "assistant.context", json!({"scope": "selection"}));
    assert!(!part["text"].as_str().unwrap_or_default().contains("drop"), "{part}");
}

#[test]
fn an_unknown_scope_is_refused() {
    let mut s = session("text");
    assert!(s.run("assistant.context", &json!({"scope": "everything"})).is_err());
}

#[test]
fn a_reply_replaces_the_selection_in_one_undo_step() {
    let mut s = session("Hello brave world");
    select(&mut s, "brave");
    run(&mut s, "assistant.apply", json!({"text": "kind"}));
    assert_eq!(body(&s), "Hello kind world");
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(body(&s), "Hello brave world", "one undo puts the original back");
}

#[test]
fn with_nothing_selected_a_reply_is_inserted_at_the_caret() {
    let mut s = session("Hello world");
    run(&mut s, "caret.docStart", json!({}));
    run(&mut s, "assistant.apply", json!({"text": "Oh, "}));
    assert_eq!(body(&s), "Oh, Hello world");
}

#[test]
fn a_reply_with_several_lines_becomes_several_paragraphs() {
    let mut s = session("one two three");
    select(&mut s, "two");
    run(&mut s, "assistant.apply", json!({"text": "A\nB\r\nC"}));
    assert_eq!(body(&s), "one A\nB\nC three");
    assert_eq!(s.doc.paragraph_count(), 3);
}

/// A reply is placed as written. Typing the same text would turn a line of dashes into a border,
/// `=rand()` into sample text and straight quotes into curly ones.
#[test]
fn a_reply_is_inserted_literally_with_no_autocorrect_or_autoformat() {
    let mut s = session("placeholder");
    select(&mut s, "placeholder");
    let reply = "---\n=rand()\n\"quoted\" -- and 1/2 (c)\n* not a bullet";
    run(&mut s, "assistant.apply", json!({"text": reply}));
    assert_eq!(body(&s), reply, "every character as the model wrote it");
    assert_eq!(s.doc.paragraph_count(), 4, "one paragraph per line, no border, table or sample text added");
}

#[test]
fn insert_after_adds_paragraphs_after_the_paragraph_the_selection_ends_in() {
    let mut s = session("Hello brave world\nNext");
    select(&mut s, "brave");
    run(&mut s, "assistant.apply", json!({"text": "Answer line 1\nAnswer line 2", "mode": "insertAfter"}));
    assert_eq!(body(&s), "Hello brave world\nAnswer line 1\nAnswer line 2\nNext", "the original paragraph is not split");
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(body(&s), "Hello brave world\nNext", "one undo removes the whole insertion");
}

#[test]
fn applying_with_track_changes_on_is_tracked_and_can_be_rejected() {
    let mut s = session("Hello brave world");
    run(&mut s, "review.trackChanges", json!({"value": true}));
    select(&mut s, "brave");
    run(&mut s, "assistant.apply", json!({"text": "kind"}));
    assert_eq!(body(&s), "Hello kind world", "read as accepted");
    assert!(s.doc.plain_text(StoryRef::Body).contains("brave"), "the replaced text is still there, marked deleted");
    run(&mut s, "review.rejectAll", json!({}));
    assert_eq!(body(&s), "Hello brave world", "rejecting the change restores the original");
}

#[test]
fn input_that_cannot_be_applied_is_refused_and_changes_nothing() {
    let mut s = session("Hello brave world");
    select(&mut s, "brave");
    let before = serde_json::to_value(&s.doc).unwrap();
    for params in [
        json!({}),
        json!({"text": ""}),
        json!({"text": "  \n "}),
        json!({"text": 5}),
        json!({"text": "ok", "mode": "sideways"}),
        json!({"text": "x".repeat(crate::cmd::assistant::MAX_APPLY_BYTES + 1)}),
    ] {
        assert!(s.run("assistant.apply", &params).is_err(), "{params}");
        assert_eq!(serde_json::to_value(&s.doc).unwrap(), before, "a refused reply leaves the document as it was");
    }
}
