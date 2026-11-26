//! New blank documents that start right to left (a Hebrew interface): the document defaults carry
//! the direction, so the caret starts at the right margin before a letter is typed, while a
//! headless session, opened files, templates and documents already begun stay as they are.

use serde_json::json;
use wordcraft_doc::props::ParaProps;
use wordcraft_doc::{Document, StoryRef};

use crate::Session;

/// שלום עולם
const SHALOM: &str = "\u{05E9}\u{05DC}\u{05D5}\u{05DD} \u{05E2}\u{05D5}\u{05DC}\u{05DD}";

fn run(s: &mut Session, id: &str, v: serde_json::Value) -> serde_json::Value {
    s.run(id, &v).unwrap_or_else(|e| panic!("{id}: {e}"))
}

/// A session whose new documents start right to left, as a Hebrew interface sets it.
fn hebrew_session() -> Session {
    let mut s = Session::new(Document::new());
    s.set_new_documents_rtl(true);
    s
}

/// Does a paragraph with no direction of its own read right to left, as the defaults say?
fn reads_rtl(s: &Session) -> bool {
    s.doc.styles.resolve_para(&ParaProps::default()).bidi
}

fn para_props(s: &Session, i: usize) -> ParaProps {
    s.doc.para(StoryRef::Body, &wordcraft_doc::Path::top(i)).unwrap().props.clone()
}

fn caret_x(s: &mut Session) -> f32 {
    let l = s.layout();
    l.caret(&s.sel.focus).unwrap().x
}

/// The bytes of one part of a saved package.
fn part(bytes: &[u8], name: &str) -> String {
    let mut z = craft_zip::Archive::open(bytes, craft_zip::Limits { max_entries: 10_000, max_total_bytes: 64 << 20 }).unwrap();
    String::from_utf8(z.read(name, 16 << 20).unwrap().unwrap()).unwrap()
}

#[test]
fn a_session_starts_left_to_right_until_told_otherwise() {
    let mut s = Session::new(Document::new());
    assert!(!s.new_documents_rtl && !reads_rtl(&s));
    run(&mut s, "file.new", json!({}));
    assert!(!reads_rtl(&s), "file.new in a headless session (CLI, MCP, tests) is as it always was");
    assert_eq!(s.doc.styles.default_para.bidi, None);
    assert_eq!(s.doc.styles.default_chr.lang_bidi, None);
}

#[test]
fn an_untouched_blank_document_follows_the_setting_and_back() {
    let mut s = Session::new(Document::new());
    let left = caret_x(&mut s);
    s.set_new_documents_rtl(true);
    assert!(reads_rtl(&s), "the first document is right to left before anything is typed");
    assert!(s.is_untouched_blank() && !s.dirty && s.undo_depth() == 0, "not an edit");
    let right = caret_x(&mut s);
    assert!(right > left && right > s.doc.last_section.page_w / 2.0, "the caret starts at the right margin: {left} then {right}");
    s.set_new_documents_rtl(false);
    assert!(!reads_rtl(&s));
    assert_eq!(s.doc.styles.default_para.bidi, None, "taken off again, not left as `false`");
    assert!((caret_x(&mut s) - left).abs() < 0.01, "and the caret is back at the left margin");
}

#[test]
fn a_document_already_begun_keeps_its_direction() {
    let mut s = Session::new(Document::new());
    run(&mut s, "text.insert", json!({"text": "Hello"}));
    s.set_new_documents_rtl(true);
    assert!(!reads_rtl(&s), "typed text is not changed under the writer");
    let mut s = hebrew_session();
    run(&mut s, "text.insert", json!({"text": "x"}));
    s.set_new_documents_rtl(false);
    assert!(reads_rtl(&s), "nor is a right-to-left one when the interface changes");
}

#[test]
fn new_blank_documents_start_right_to_left_but_templates_and_opened_files_do_not() {
    let mut s = hebrew_session();
    run(&mut s, "text.insert", json!({"text": "x"}));
    run(&mut s, "file.new", json!({}));
    assert!(reads_rtl(&s), "File > New: Blank document");
    assert_eq!(s.doc.styles.default_chr.lang_bidi.as_deref(), Some("he-IL"));
    assert!(s.doc.styles.default_chr.font_cs.is_some(), "with a complex-script font of its own");
    // The direction does not depend on the language the user picked as the proofing default.
    s.prefs.default_language = Some("en-US".into());
    run(&mut s, "file.new", json!({}));
    assert!(reads_rtl(&s) && s.doc.styles.default_chr.lang.as_deref() == Some("en-US"));
    for template in ["sample", "letter", "resume", "report"] {
        run(&mut s, "file.new", json!({"template": template}));
        assert!(!reads_rtl(&s), "the {template} template is left to right");
    }
    // A file brings its own direction; plain text carries none.
    run(&mut s, "file.open", json!({"path": "a.txt", "data": "SGVsbG8="}));
    assert!(!reads_rtl(&s), "an opened text file");
    // An empty file opens as a new blank document, which starts as one does.
    run(&mut s, "file.open", json!({"path": "empty.docx", "data": ""}));
    assert!(reads_rtl(&s), "an empty file is a new blank document");
}

#[test]
fn closing_the_last_document_leaves_a_blank_one_in_the_same_direction() {
    let mut s = hebrew_session();
    run(&mut s, "text.insert", json!({"text": "x"}));
    run(&mut s, "file.closeDocument", json!({"force": true}));
    assert!(reads_rtl(&s) && s.doc.plain_text(StoryRef::Body).trim().is_empty());
}

#[test]
fn typed_text_keeps_the_direction_the_defaults_give() {
    let mut s = hebrew_session();
    run(&mut s, "text.insert", json!({"text": SHALOM}));
    assert_eq!(para_props(&s, 0).bidi, None, "inherited, so nothing is copied onto the paragraph");
    run(&mut s, "text.newParagraph", json!({}));
    run(&mut s, "text.insert", json!({"text": "Hello"}));
    assert_eq!(para_props(&s, 1).bidi, None, "Latin letters do not make it left to right");
    run(&mut s, "caret.docStart", json!({}));
    let start_of_hebrew = caret_x(&mut s);
    run(&mut s, "caret.docEnd", json!({}));
    run(&mut s, "caret.home", json!({}));
    let start_of_latin = caret_x(&mut s);
    let half = s.doc.last_section.page_w / 2.0;
    assert!(start_of_hebrew > half && start_of_latin > half, "both paragraphs start at the right margin: {start_of_hebrew}, {start_of_latin}");
}

#[test]
fn one_paragraph_can_be_turned_left_to_right_and_the_file_remembers_it() {
    let mut s = hebrew_session();
    run(&mut s, "text.insert", json!({"text": "Hello"}));
    run(&mut s, "text.newParagraph", json!({}));
    run(&mut s, "text.insert", json!({"text": SHALOM}));
    run(&mut s, "caret.docStart", json!({}));
    run(&mut s, "para.ltr", json!({}));
    assert_eq!((para_props(&s, 0).bidi, para_props(&s, 1).bidi), (Some(false), None));
    let start_of_english = caret_x(&mut s);
    assert!(start_of_english < s.doc.last_section.page_w / 2.0, "it starts at the left margin: {start_of_english}");
    let bytes = crate::io::save_bytes("x.docx", &s.doc).unwrap();
    let d = crate::io::open_bytes("x.docx", &bytes).unwrap();
    let bidi = |i| d.para(StoryRef::Body, &wordcraft_doc::Path::top(i)).unwrap().props.bidi;
    assert_eq!((bidi(0), bidi(1)), (Some(false), None), "the override survives, the inheritance is not copied down");
    assert!(d.styles.resolve_para(&para_props(&s, 1)).bidi);
}

#[test]
fn the_direction_and_complex_script_defaults_survive_a_save() {
    let s = hebrew_session();
    let bytes = crate::io::save_bytes("x.docx", &s.doc).unwrap();
    let styles = part(&bytes, "word/styles.xml");
    let defaults = styles.split("<w:docDefaults>").nth(1).and_then(|d| d.split("</w:docDefaults>").next()).unwrap().to_string();
    let (rpr, ppr) = (defaults.split("<w:pPrDefault>").next().unwrap(), defaults.split("<w:pPrDefault>").nth(1).unwrap());
    assert!(ppr.contains("<w:bidi/>"), "the paragraph direction is in the paragraph defaults: {ppr}");
    assert!(rpr.contains("w:cs=\"David\""), "the complex-script font is in the run defaults: {rpr}");
    assert!(rpr.contains("w:bidi=\"he-IL\""), "and so is the complex-script language: {rpr}");
    let d = crate::io::open_bytes("x.docx", &bytes).unwrap();
    assert!(d.styles.resolve_para(&ParaProps::default()).bidi, "a reopened file reads right to left");
    assert_eq!(d.styles.default_chr.lang_bidi.as_deref(), Some("he-IL"));
    assert_eq!(d.styles.default_chr.font_cs.as_deref(), Some("David"));
    // Hebrew typed into it is written as Hebrew whichever way the file is read.
    let mut s = s;
    run(&mut s, "text.insert", json!({"text": SHALOM}));
    let bytes = crate::io::save_bytes("x.docx", &s.doc).unwrap();
    let body = part(&bytes, "word/document.xml");
    assert!(!body.contains("<w:bidi/>"), "no direction is repeated on the paragraph: {body}");
    assert!(body.contains("<w:rtl/>"), "the Hebrew run is marked right to left");
    let again = crate::io::open_bytes("x.docx", &bytes).unwrap();
    assert!(again.styles.resolve_para(&again.para(StoryRef::Body, &wordcraft_doc::Path::top(0)).unwrap().props).bidi);
}

#[test]
fn file_new_can_ask_for_a_direction_whatever_the_setting() {
    // The automation interface reaches it too: no window is needed.
    let mut s = Session::new(Document::new());
    run(&mut s, "file.new", json!({"direction": "rtl"}));
    assert!(reads_rtl(&s) && s.doc.styles.default_chr.lang_bidi.is_some());
    run(&mut s, "file.new", json!({"direction": "ltr"}));
    assert!(!reads_rtl(&s));
    let mut s = hebrew_session();
    run(&mut s, "file.new", json!({"direction": "ltr"}));
    assert!(!reads_rtl(&s), "left to right on request in a Hebrew interface");
    run(&mut s, "file.new", json!({}));
    assert!(reads_rtl(&s), "and the setting is still what an omitted direction means");
    assert!(s.run("file.new", &json!({"direction": "sideways"})).is_err());
}

#[test]
fn a_page_number_asked_for_on_the_right_is_on_the_right_in_a_right_to_left_document() {
    use wordcraft_doc::props::Align;
    for rtl in [false, true] {
        let mut s = Session::new(Document::new());
        s.set_new_documents_rtl(rtl);
        run(&mut s, "insert.pageNumber", json!({"position": "top", "align": "right"}));
        let (id, _) = s.doc.parts.iter().next().unwrap();
        let blocks = s.doc.story(StoryRef::Part(*id)).unwrap();
        let align = blocks.iter().find_map(|b| b.as_para()).unwrap().props.align.unwrap();
        assert_eq!(align.visual(rtl), Align::Right, "as seen on the page (right to left: {rtl})");
    }
}
