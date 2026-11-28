//! New blank documents that start right to left (a Hebrew interface): the document defaults carry
//! the direction, so the caret starts at the right margin before a letter is typed, while a
//! headless session, templates (unless asked for right to left: their Hebrew versions), opened
//! files and documents already begun stay as they are.

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
    // A template keeps its own direction; asked for right to left, it is its Hebrew version,
    // with a right-to-left section, in a headless session too.
    let mut headless = Session::new(Document::new());
    for template in ["sample", "letter", "resume", "report"] {
        run(&mut s, "file.new", json!({"template": template}));
        assert!(!reads_rtl(&s), "the {template} template is left to right");
        for session in [&mut s, &mut headless] {
            run(session, "file.new", json!({"template": template, "direction": "rtl"}));
            assert!(reads_rtl(session) && session.doc.last_section.rtl, "the {template} template right to left");
            assert!(wordcraft_doc::bidi::has_rtl(&session.doc.plain_text(StoryRef::Body)), "the {template} template is written in Hebrew");
        }
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
    // The section says its own direction (`w:bidi` in `w:sectPr`); the paragraph doesn't repeat its.
    let paragraphs = body.split("<w:sectPr").next().unwrap_or_default();
    assert!(!paragraphs.contains("<w:bidi/>"), "no direction is repeated on the paragraph: {body}");
    assert!(sect_pr(&bytes).contains("<w:bidi/>"), "the section is right to left: {body}");
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

/// The x of the first line of each body paragraph on page `page`, by paragraph index.
fn para_xs(s: &mut Session, page: usize) -> Vec<(usize, f32)> {
    let l = s.layout();
    l.pages
        .get(page)
        .map(|p| {
            p.items
                .iter()
                .filter_map(|it| match it {
                    wordcraft_layout::Placed::Lines { story: StoryRef::Body, path, x, .. } => path.0.first().map(|i| (*i as usize, *x)),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The `w:sectPr` of a saved package's body.
fn sect_pr(bytes: &[u8]) -> String {
    let body = part(bytes, "word/document.xml");
    body.rsplit("<w:sectPr").next().and_then(|s| s.split("</w:sectPr>").next()).unwrap_or_default().to_string()
}

/// HE-01, RTL-05, UWB-17: a new right-to-left document's section is right to left too (`w:bidi` in
/// `w:sectPr`), so text in columns starts in the right-hand column; the untouched first document
/// follows the setting both ways, and a left-to-right document keeps a left-to-right section.
#[test]
fn a_new_right_to_left_document_has_a_right_to_left_section() {
    let mut s = hebrew_session();
    assert!(s.doc.last_section.rtl, "the untouched first document");
    s.set_new_documents_rtl(false);
    assert!(!s.doc.last_section.rtl, "taken back off with the setting");
    s.set_new_documents_rtl(true);
    assert!(s.doc.last_section.rtl);
    run(&mut s, "text.insert", json!({"text": "x"}));
    run(&mut s, "file.new", json!({"direction": "ltr"}));
    assert!(!s.doc.last_section.rtl, "a left-to-right blank document");
    run(&mut s, "file.new", json!({}));
    assert!(s.doc.last_section.rtl, "File > New in a Hebrew interface");
    assert!(sect_pr(&crate::io::save_bytes("x.docx", &s.doc).unwrap()).contains("<w:bidi/>"), "and the file says so");
    // Two columns: the first paragraphs stand in the right-hand column, the later ones to their left.
    let text = vec![SHALOM; 80].join("\n");
    run(&mut s, "text.insert", json!({"text": text, "paragraphs": true}));
    run(&mut s, "layout.columns", json!({"count": 2}));
    let xs = para_xs(&mut s, 0);
    let first = xs.iter().find(|(i, _)| *i == 0).map(|(_, x)| *x).unwrap();
    let last = xs.iter().map(|(i, x)| (*i, *x)).max_by_key(|(i, _)| *i).map(|(_, x)| x).unwrap();
    assert!(first > last + 72.0, "column 1 is the right one: paragraph 1 at {first}, the page's last at {last}");
}

/// The `w:sz` and `w:szCs` values of a `w:rPr` (or docDefaults) fragment, in order.
fn sizes(xml: &str) -> Vec<(String, String)> {
    let val = |tag: &str, s: &str| s.split(tag).nth(1).and_then(|v| v.split('"').nth(1)).map(str::to_string);
    xml.split("<w:sz ").skip(1).map(|s| (val("w:val=", s).unwrap_or_default(), val("<w:szCs w:val=", s).unwrap_or_default())).collect()
}

/// HE-04, RTL-01: Word sizes Hebrew text only from `w:szCs`, so a saved new Hebrew document has a
/// complex-script size beside every size its defaults and styles set, the same size.
#[test]
fn a_saved_hebrew_document_sizes_its_hebrew_text_as_its_latin_text() {
    let mut s = hebrew_session();
    run(&mut s, "text.insert", json!({"text": SHALOM}));
    run(&mut s, "para.style", json!({"style": "Heading 1"}));
    let styles = part(&crate::io::save_bytes("x.docx", &s.doc).unwrap(), "word/styles.xml");
    let all = sizes(&styles);
    assert!(all.len() > 5, "the defaults and the styles that set a size: {all:?}");
    for (sz, sz_cs) in &all {
        assert_eq!(sz, sz_cs, "w:szCs beside w:sz: {all:?}");
    }
    let heading = styles.split("w:styleId=\"Heading1\"").nth(1).and_then(|h| h.split("</w:style>").next()).unwrap();
    assert!(heading.contains("<w:szCs w:val=\"40\"/>"), "Heading 1 is 20 pt for Hebrew text too: {heading}");
    // Reopened, the Hebrew heading is as large as a Latin one.
    let d = crate::io::open_bytes("x.docx", &crate::io::save_bytes("x.docx", &s.doc).unwrap()).unwrap();
    let rc = d.styles.resolve_char(Some("Heading1"), &Default::default());
    assert!(rc.size_cs > d.styles.resolve_char(None, &Default::default()).size_cs, "{rc:?}");
}

/// The Design tab's style sets keep the complex-script values of the styles they change with the
/// others, so a style set reaches the Hebrew text of the headings and the title too.
#[test]
fn style_sets_format_complex_script_text_alike() {
    for set in ["basic", "lines", "shaded", "casual", "centered", "minimalist", "title"] {
        let mut s = hebrew_session();
        run(&mut s, "design.styleSet", json!({"name": set}));
        for st in &s.doc.styles.styles {
            let c = &st.chr;
            assert_eq!((c.size_cs, c.bold_cs, c.italic_cs), (c.size, c.bold, c.italic), "{set}: {}", st.id);
        }
    }
}

/// RTL-07: Modify Style in a right-to-left document changes the font and size of the style's
/// Hebrew text too, so Word shows them (`w:rFonts/@w:cs`, `w:szCs`); a left-to-right document's
/// complex-script font is left alone unless the style holds right-to-left text.
#[test]
fn modify_style_reaches_the_hebrew_text_of_a_right_to_left_document() {
    let mut s = hebrew_session();
    run(&mut s, "text.insert", json!({"text": SHALOM}));
    run(&mut s, "para.style", json!({"style": "Heading 1"}));
    run(&mut s, "styles.modify", json!({"style": "Heading 1", "chr": {"font": "Frank Ruehl", "size": 26}}));
    let rc = s.doc.styles.resolve_char(Some("Heading1"), &Default::default());
    assert_eq!((rc.font_cs.as_str(), rc.size_cs), ("Frank Ruehl", 26.0), "the Hebrew heading takes them");
    assert_eq!((rc.font.as_str(), rc.size), ("Frank Ruehl", 26.0), "and so does its Latin text");
    let styles = part(&crate::io::save_bytes("x.docx", &s.doc).unwrap(), "word/styles.xml");
    let heading = styles.split("w:styleId=\"Heading1\"").nth(1).and_then(|h| h.split("</w:style>").next()).unwrap();
    assert!(heading.contains("w:cs=\"Frank Ruehl\"") && heading.contains("<w:szCs w:val=\"52\"/>"), "{heading}");

    // Left to right, with no right-to-left text in the style: no complex-script font is written.
    let mut s = Session::new(Document::new());
    run(&mut s, "text.insert", json!({"text": "Chapter"}));
    run(&mut s, "para.style", json!({"style": "Heading 1"}));
    run(&mut s, "styles.modify", json!({"style": "Heading 1", "chr": {"font": "Georgia", "size": 26}}));
    let own = |s: &Session| s.doc.styles.get("Heading1").unwrap().chr.clone();
    assert_eq!(own(&s).font_cs, None, "a Latin font choice is the Latin text's");
    assert_eq!(own(&s).size_cs, Some(26.0), "the sizes stay paired, as the built-in styles pair them");
    // The same document with a Hebrew heading: the style is used for right-to-left text.
    run(&mut s, "text.newParagraph", json!({}));
    run(&mut s, "text.insert", json!({"text": SHALOM}));
    run(&mut s, "para.style", json!({"style": "Heading 1"}));
    run(&mut s, "styles.modify", json!({"style": "Heading 1", "chr": {"font": "Frank Ruehl"}}));
    assert_eq!(own(&s).font_cs.as_deref(), Some("Frank Ruehl"));
}

/// RTL-07: the Modify Style dialog sends the font and size it shows whether or not they were
/// changed. OK with only the spacing changed leaves the Hebrew text's own font and size alone.
#[test]
fn modify_style_keeps_the_hebrew_font_and_size_it_was_not_asked_to_change() {
    let mut s = hebrew_session();
    run(&mut s, "text.insert", json!({"text": SHALOM}));
    run(&mut s, "para.style", json!({"style": "Heading 1"}));
    // Heading 1's Hebrew text has a size of its own, as a Word document may give it.
    if let Some(h) = std::sync::Arc::make_mut(&mut s.doc.styles).get_mut("Heading1") {
        h.chr.size_cs = Some(24.0);
    }
    let before = s.doc.styles.resolve_char(Some("Heading1"), &Default::default());
    assert_ne!(before.font_cs, before.font, "the Hebrew text has a font of its own");
    run(&mut s, "styles.modify", json!({"style": "Heading 1", "chr": {"font": before.font, "size": before.size}, "para": {"spaceBefore": 30}}));
    let after = s.doc.styles.resolve_char(Some("Heading1"), &Default::default());
    assert_eq!((&after.font_cs, after.size_cs), (&before.font_cs, before.size_cs), "the Hebrew font and size stay");
    assert_eq!((&after.font, after.size), (&before.font, before.size));
}

/// RTL-08: the three-part header of a right-to-left document reads from the right: its first
/// item at the right margin, its last at the left; left to right, the other way round.
#[test]
fn the_three_part_header_reads_from_the_right_in_a_right_to_left_document() {
    for rtl in [false, true] {
        let mut s = Session::new(Document::new());
        s.set_new_documents_rtl(rtl);
        let r = run(&mut s, "insert.header", json!({"preset": "blankThree"}));
        let id = r["story"].as_u64().unwrap() as u32;
        let story = StoryRef::Part(id);
        let blocks = s.doc.story(story).unwrap();
        let table = blocks.iter().find_map(|b| match &**b {
            wordcraft_doc::Block::Table(t) => Some(t.clone()),
            _ => None,
        });
        assert_eq!(table.unwrap().props.bidi_visual, rtl, "a right-to-left table in a right-to-left document only");
        let l = s.layout();
        let x = |cell: u32| l.caret_on(&wordcraft_doc::Pos { story, path: wordcraft_doc::Path(vec![0, 0, cell, 0]), off: 0 }, 0).unwrap().x;
        let (first, last) = (x(0), x(2));
        if rtl {
            assert!(first > last, "the first item at the right: {first} vs {last}");
        } else {
            assert!(first < last, "the first item at the left: {first} vs {last}");
        }
    }
}

/// RTL-12: a blank document on A4, the Hebrew region's paper, has its Header and Footer tabs at
/// the middle and the end of A4's text width, not Letter's, so the last of three items ends at
/// the margin.
#[test]
fn header_tabs_follow_the_paper_of_a_new_document() {
    for region in ["he-IL", "en-US"] {
        let mut s = hebrew_session();
        s.prefs.region = Some(region.into());
        run(&mut s, "file.new", json!({}));
        let width = s.doc.last_section.text_width();
        for style in ["Header", "Footer"] {
            let tabs = s.doc.styles.resolve_para(&ParaProps { style: Some(style.into()), ..Default::default() }).tabs;
            let end = tabs.iter().map(|t| t.pos).fold(0.0, f32::max);
            assert!((end - width).abs() < 0.5, "{region} {style}: the right tab at the right margin ({end} of {width})");
            assert!(tabs.iter().any(|t| (t.pos - width / 2.0).abs() < 0.5), "{region} {style}: and a centre tab at the middle: {tabs:?}");
        }
    }
}

/// HE-08: Find looks past Hebrew points and cantillation marks unless Match diacritics is on, and a
/// typed " or ' finds the gershayim and geresh of an abbreviation.
#[test]
fn find_ignores_hebrew_points_unless_asked_and_matches_gershayim() {
    let mut s = hebrew_session();
    // בְּרֵאשִׁית בָּרָא שָׁלוֹם, צה״ל, ג׳ירפה
    let text = "\u{05D1}\u{05B0}\u{05BC}\u{05E8}\u{05B5}\u{05D0}\u{05E9}\u{05C1}\u{05B4}\u{05D9}\u{05EA} \u{05D1}\u{05BC}\u{05B8}\u{05E8}\u{05B8}\u{05D0} \u{05E9}\u{05C1}\u{05B8}\u{05DC}\u{05D5}\u{05B9}\u{05DD}, \u{05E6}\u{05D4}\u{05F4}\u{05DC}, \u{05D2}\u{05F3}\u{05D9}\u{05E8}\u{05E4}\u{05D4}";
    run(&mut s, "text.insert", json!({"text": text}));
    let count = |s: &mut Session, v: serde_json::Value| run(s, "edit.find", v)["count"].as_u64().unwrap();
    let shalom = "\u{05E9}\u{05DC}\u{05D5}\u{05DD}";
    let pointed = "\u{05E9}\u{05C1}\u{05B8}\u{05DC}\u{05D5}\u{05B9}\u{05DD}";
    assert_eq!(count(&mut s, json!({"text": shalom})), 1, "the unpointed word finds the pointed one");
    assert_eq!(count(&mut s, json!({"text": pointed})), 1, "and the pointed one itself");
    assert_eq!(count(&mut s, json!({"text": shalom, "matchDiacritics": true})), 0, "unless the points must match");
    assert_eq!(count(&mut s, json!({"text": pointed})), 1, "the option stays as set: the exact word still matches");
    assert_eq!(count(&mut s, json!({"text": "\u{05D1}\u{05E8}\u{05D0}", "matchDiacritics": false, "wholeWord": true})), 1, "ברא as a whole word");
    assert_eq!(count(&mut s, json!({"text": "\u{05E6}\u{05D4}\"\u{05DC}", "wholeWord": false})), 1, "צה\"ל finds צה״ל");
    assert_eq!(count(&mut s, json!({"text": "\u{05D2}'\u{05D9}\u{05E8}\u{05E4}\u{05D4}"})), 1, "ג'ירפה finds ג׳ירפה");
    // Replace takes the whole pointed word, points and all.
    run(&mut s, "edit.replaceAll", json!({"text": shalom, "with": "X", "wholeWord": false}));
    let body = s.doc.plain_text(StoryRef::Body);
    assert!(body.contains("X,") && !body.contains('\u{05DD}'), "{body}");
}

/// HE-09, UWB-15: Review > Language with Hebrew marks the text's complex-script language
/// (`w:lang/@w:bidi`), where Word and proofing read Hebrew, and leaves its Latin language; the
/// language state reports Hebrew in Hebrew text and English in Latin text of the same document.
#[test]
fn hebrew_is_the_language_of_complex_script_text() {
    let mut s = hebrew_session();
    run(&mut s, "text.insert", json!({"text": format!("{SHALOM} Hello")}));
    let lang = |s: &mut Session| run(s, "format.state", json!({}))["lang"].clone();
    run(&mut s, "caret.docStart", json!({}));
    assert_eq!(lang(&mut s), json!("he-IL"), "a caret in Hebrew text of a new Hebrew document");
    run(&mut s, "caret.docEnd", json!({}));
    assert_eq!(lang(&mut s), json!("en-US"), "a caret in its Latin word");
    run(&mut s, "select.all", json!({}));
    assert_eq!(lang(&mut s), serde_json::Value::Null, "both: mixed");

    // An English document: marking its Hebrew word Hebrew sets the complex-script language.
    let mut s = Session::new(Document::new());
    run(&mut s, "text.insert", json!({"text": SHALOM}));
    run(&mut s, "select.all", json!({}));
    run(&mut s, "review.language", json!({"lang": "he-IL"}));
    let c = s.doc.para(StoryRef::Body, &wordcraft_doc::Path::top(0)).unwrap().props_of_char(0).clone();
    assert_eq!((c.lang_bidi.as_deref(), c.lang.as_deref()), (Some("he-IL"), None), "the Hebrew language, not the Latin one");
    assert_eq!(lang(&mut s), json!("he-IL"), "and the language state says so");
    let body = part(&crate::io::save_bytes("x.docx", &s.doc).unwrap(), "word/document.xml");
    assert!(body.contains("w:bidi=\"he-IL\"") && !body.contains("w:val=\"he-IL\""), "{body}");
    // Set As Default with Hebrew: the default complex-script language; the Latin one stays.
    run(&mut s, "review.language", json!({"lang": "he-IL", "default": true}));
    assert_eq!((s.doc.styles.default_chr.lang_bidi.as_deref(), s.doc.styles.default_chr.lang.as_deref()), (Some("he-IL"), Some("en-US")));
}

/// HE-13: a caption numbered in Hebrew numerals or the Hebrew alphabet: א, ב… with `\* hebrew1`
/// or `\* hebrew2`, which Word reads alike.
#[test]
fn captions_can_be_numbered_in_hebrew() {
    let mut s = hebrew_session();
    run(&mut s, "text.insert", json!({"text": SHALOM}));
    for format in ["hebrew1", "hebrew2"] {
        run(&mut s, "references.caption", json!({"label": "Figure", "format": format}));
    }
    let fields: Vec<(String, String)> = s
        .doc
        .body
        .iter()
        .filter_map(|b| b.as_para())
        .flat_map(|p| p.objects.iter())
        .filter_map(|o| match o {
            wordcraft_doc::InlineObject::Field { instr, result, .. } if instr.starts_with("SEQ") => Some((instr.clone(), result.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(fields.len(), 2, "{fields:?}");
    assert!(fields.iter().any(|(i, _)| i.contains("\\* hebrew1")) && fields.iter().any(|(i, _)| i.contains("\\* hebrew2")), "{fields:?}");
    let results: Vec<&str> = fields.iter().map(|(_, r)| r.as_str()).collect();
    assert!(results.contains(&"\u{05D0}") && results.contains(&"\u{05D1}"), "the first and second figure are א and ב: {fields:?}");
}

/// HTML on the clipboard (a web page, Word's HTML clipboard) sizes Hebrew text with the one size
/// it has. Pasted into a new Hebrew document, whose defaults and styles set a complex-script size
/// beside every size, that text keeps its size rather than taking the document's.
#[test]
fn pasted_hebrew_keeps_its_size_in_a_new_hebrew_document() {
    let mut s = hebrew_session();
    let html = format!("<p><span style=\"font-size:20pt\">{SHALOM}</span> <span style=\"font-size:20pt\">Hello</span></p>");
    run(&mut s, "edit.pasteSpecial", json!({"as": "html", "html": html}));
    let (style, props) = s
        .doc
        .body
        .iter()
        .filter_map(|b| b.as_para())
        .find(|p| p.text.contains(SHALOM))
        .map(|p| (p.props.style.clone(), p.props_of_char(0).clone()))
        .unwrap();
    let rc = s.doc.styles.resolve_char(style.as_deref(), &props);
    assert!((rc.size - 20.0).abs() < 0.01, "the pasted size: {}", rc.size);
    assert!((rc.size_cs - rc.size).abs() < 0.01, "the Hebrew text is as large as its Latin text: {} and {}", rc.size_cs, rc.size);
}

/// The font Hebrew text in paragraphs of `style` (Normal when `None`) is drawn in.
fn hebrew_font(d: &Document, style: Option<&str>) -> String {
    d.styles.resolve_char(style, &Default::default()).complex().font
}

/// HE-15: Design › Fonts and Design › Themes reach the Hebrew text of a Hebrew document: each
/// theme's set has Hebrew heading and body fonts, the default set keeps the David of a new
/// document, and the saved styles name them (`w:rFonts/@w:cs`), so Word shows them too. A style set
/// keeps them; a document whose complex-script text isn't Hebrew keeps its own unless asked.
#[test]
fn theme_fonts_reach_the_hebrew_text_of_a_hebrew_document() {
    let mut s = hebrew_session();
    let new_body = hebrew_font(&s.doc, None);
    let r = run(&mut s, "design.themeFonts", json!({"heading": "Futura", "body": "Avenir Next"}));
    let (body, heading) = (hebrew_font(&s.doc, None), hebrew_font(&s.doc, Some("Heading1")));
    assert!(body != new_body && heading != body, "the set's own Hebrew fonts: {heading} / {body}");
    assert_eq!(r["complexScriptFonts"], json!({"heading": heading, "body": body}), "and the command says so");
    assert_eq!(hebrew_font(&s.doc, Some("Title")), heading, "the title takes the heading font");
    assert_eq!(s.doc.styles.resolve_char(None, &Default::default()).font, "Avenir Next", "the Latin fonts as before");
    // Design › Themes brings the same pair; a style set keeps it.
    let mut t = hebrew_session();
    run(&mut t, "design.theme", json!({"name": "Darkroom"}));
    run(&mut t, "design.styleSet", json!({"name": "basic"}));
    assert_eq!((hebrew_font(&t.doc, None), hebrew_font(&t.doc, Some("Heading1"))), (body.clone(), heading.clone()));
    // Saved and reopened, the styles still give the Hebrew text these fonts.
    let d = crate::io::open_bytes("x.docx", &crate::io::save_bytes("x.docx", &s.doc).unwrap()).unwrap();
    assert_eq!((hebrew_font(&d, None), hebrew_font(&d, Some("Heading1"))), (body, heading));
    // The default theme's set is David again, as in a new document.
    run(&mut s, "design.themeFonts", json!({"heading": "Aptos Display", "body": "Aptos"}));
    assert_eq!((hebrew_font(&s.doc, None), hebrew_font(&s.doc, Some("Heading1"))), (new_body.clone(), new_body));
    // Not a Hebrew document: its complex-script fonts are left alone, unless they are given.
    let mut e = Session::new(Document::new());
    let before = (hebrew_font(&e.doc, None), hebrew_font(&e.doc, Some("Heading1")));
    let r = run(&mut e, "design.themeFonts", json!({"heading": "Futura", "body": "Avenir Next"}));
    assert!(r["complexScriptFonts"].is_null());
    assert_eq!(e.doc.styles.default_chr.font_cs, None);
    assert_ne!(before.0, "Miriam");
    run(&mut e, "design.themeFonts", json!({"heading": "Futura", "headingCs": "Miriam"}));
    assert_eq!((hebrew_font(&e.doc, None), hebrew_font(&e.doc, Some("Heading1"))), ("Miriam".to_string(), "Miriam".to_string()));
    assert!(e.run("design.themeFonts", &json!({"heading": "Futura", "bodyCs": ""})).is_err());
}
