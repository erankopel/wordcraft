//! Words WordCraft writes into a document on its own (titles, caption labels, "Page X of Y",
//! placeholders, sample items, dates) are in the language of the text that receives them: Hebrew
//! in a Hebrew document, English in English text, whatever the interface's language.

use serde_json::json;
use wordcraft_doc::para::InlineObject;
use wordcraft_doc::{Document, Path, StoryRef};

use crate::Session;

fn run(s: &mut Session, id: &str, v: serde_json::Value) -> serde_json::Value {
    s.run(id, &v).unwrap_or_else(|e| panic!("{id}: {e}"))
}

/// A new blank document as a Hebrew interface starts one: right to left, Hebrew as the language
/// of complex-script text, English as the other text's.
fn hebrew_document() -> Session {
    let mut s = Session::new(Document::new());
    s.set_new_documents_rtl(true);
    s
}

fn hebrew(text: &str) -> bool {
    text.chars().any(|c| ('\u{05D0}'..='\u{05EA}').contains(&c))
}

fn latin(text: &str) -> bool {
    text.chars().any(|c| c.is_ascii_alphabetic())
}

fn body_text(s: &Session, i: usize) -> String {
    s.doc.para(StoryRef::Body, &Path::top(i)).map(|p| p.plain_text()).unwrap_or_default()
}

/// The field codes of a paragraph.
fn fields(p: &wordcraft_doc::Paragraph) -> Vec<String> {
    p.objects
        .iter()
        .filter_map(|o| match o {
            InlineObject::Field { instr, .. } | InlineObject::FieldStart { instr, .. } => Some(instr.clone()),
            _ => None,
        })
        .collect()
}

/// The first story part's paragraphs' text (a header's or a footer's).
fn part_text(s: &Session, story: &serde_json::Value) -> String {
    let id = story["story"].as_u64().and_then(|i| u32::try_from(i).ok()).expect("a part");
    s.doc.plain_text(StoryRef::Part(id))
}

#[test]
fn a_table_of_contents_is_titled_in_the_language_of_the_text_it_goes_into() {
    let mut s = hebrew_document();
    run(&mut s, "references.toc", json!({"levels": 3}));
    let title = body_text(&s, 0);
    assert!(hebrew(&title) && !latin(&title), "a Hebrew title in a Hebrew document: {title:?}");
    // The gallery's second table names its title itself; it is WordCraft's word, so it is Hebrew
    // too. A title of the user's own is kept as it is.
    let mut s = hebrew_document();
    run(&mut s, "references.toc", json!({"levels": 2, "title": "Table of Contents"}));
    let title = body_text(&s, 0);
    assert!(hebrew(&title) && !latin(&title), "the gallery's title in a Hebrew document: {title:?}");
    let mut s = hebrew_document();
    run(&mut s, "references.toc", json!({"levels": 2, "title": "Overview"}));
    assert!(body_text(&s, 0).starts_with("Overview") && !hebrew(&body_text(&s, 0)), "{:?}", body_text(&s, 0));
    // The same commands in English text give the English titles.
    for params in [json!({"levels": 3}), json!({"levels": 2, "title": "Table of Contents"})] {
        let mut s = Session::new(Document::new());
        run(&mut s, "references.toc", params);
        assert!(latin(&body_text(&s, 0)) && !hebrew(&body_text(&s, 0)));
    }
}

/// A caption in Hebrew text is labelled and numbered as a Hebrew Word document's are: the label
/// in Hebrew, and its SEQ identifier the same word, so a table of figures for that label (the
/// default in Hebrew text) lists it, and a Figure cross-reference finds it.
#[test]
fn a_caption_in_hebrew_text_is_labelled_and_numbered_by_its_hebrew_label() {
    let mut s = hebrew_document();
    run(&mut s, "text.insert", json!({"text": "שלום"}));
    run(&mut s, "references.caption", json!({"text": "מפה"}));
    let caption = s.doc.para(StoryRef::Body, &Path::top(1)).expect("the caption").clone();
    let text = caption.plain_text();
    let label = text.split_whitespace().next().unwrap_or_default().to_string();
    assert!(hebrew(&label) && !latin(&text), "{text:?}");
    let seq = fields(&caption).into_iter().find(|f| f.starts_with("SEQ")).expect("a SEQ field");
    assert_eq!(seq.split_whitespace().nth(1), Some(label.as_str()), "numbered by its label: {seq}");
    assert!(text.contains('1'), "numbered: {text:?}");
    // A table of figures without a label lists the Hebrew captions.
    run(&mut s, "caret.docEnd", json!({}));
    run(&mut s, "text.newParagraph", json!({}));
    run(&mut s, "references.tableOfFigures", json!({}));
    let listed = s.doc.plain_text(StoryRef::Body);
    assert_eq!(listed.matches(text.as_str()).count(), 2, "the caption, then its entry in the table of figures: {listed:?}");
    let targets = crate::cmd::citations::ref_targets(&s, "figure");
    assert_eq!(targets.len(), 1, "a Figure cross-reference finds the Hebrew caption: {targets:?}");
}

#[test]
fn page_x_of_y_in_a_hebrew_footer_reads_in_hebrew_and_is_replaced_not_repeated() {
    let mut s = hebrew_document();
    let footer = run(&mut s, "insert.pageNumber", json!({"position": "bottom", "align": "center", "format": "x of y"}));
    let text = part_text(&s, &footer);
    assert!(hebrew(&text) && !latin(&text), "{text:?}");
    let id = footer["story"].as_u64().and_then(|i| u32::try_from(i).ok()).unwrap_or_default();
    let paras = |s: &Session| s.doc.story(StoryRef::Part(id)).map_or(0, |b| b.len());
    let before = paras(&s);
    // The gallery recognises its own Hebrew paragraph: a new choice replaces it.
    run(&mut s, "insert.pageNumber", json!({"position": "bottom", "align": "right"}));
    assert_eq!(paras(&s), before, "replaced, not added");
    assert!(!hebrew(&part_text(&s, &footer)), "the plain page number replaced the Hebrew words");
    // In English text the words are English.
    let mut s = Session::new(Document::new());
    let footer = run(&mut s, "insert.pageNumber", json!({"position": "bottom", "format": "x of y"}));
    assert!(latin(&part_text(&s, &footer)));
}

#[test]
fn header_placeholders_are_in_the_documents_language() {
    let mut s = hebrew_document();
    for (preset, header) in [("blank", true), ("title", true), ("blank", false)] {
        let part = run(&mut s, if header { "insert.header" } else { "insert.footer" }, json!({"preset": preset}));
        let text = part_text(&s, &part);
        assert!(hebrew(&text) && !latin(&text) && text.starts_with('['), "{preset}: {text:?}");
    }
    let mut s = Session::new(Document::new());
    let part = run(&mut s, "insert.header", json!({"preset": "blank"}));
    assert!(latin(&part_text(&s, &part)));
}

#[test]
fn index_bibliography_and_authorities_titles_follow_the_text() {
    for (id, params) in [("references.index", json!({})), ("references.bibliography", json!({})), ("references.tableOfAuthorities", json!({}))] {
        let mut s = hebrew_document();
        run(&mut s, id, params.clone());
        let title = body_text(&s, 0);
        assert!(hebrew(&title) && !latin(&title), "{id}: {title:?}");
        let mut s = Session::new(Document::new());
        run(&mut s, id, params);
        assert!(latin(&body_text(&s, 0)), "{id} in English text");
    }
}

#[test]
fn a_cover_page_in_a_hebrew_document_has_hebrew_placeholders_and_date() {
    let mut s = hebrew_document();
    // The author is the user's name, not a word of WordCraft's.
    run(&mut s, "insert.coverPage", json!({"author": "דנה כהן"}));
    let text = s.doc.plain_text(StoryRef::Body);
    assert!(hebrew(&text) && !latin(&text), "{text:?}");
    assert!(text.contains(&crate::cmd::words::long_date(Some("he-IL"))), "dated in Hebrew: {text:?}");
}

/// Text inserted into English words of a Hebrew document is English: the words follow the text,
/// not the document's direction or the interface.
#[test]
fn english_text_in_a_hebrew_document_gets_english_words() {
    let mut s = hebrew_document();
    run(&mut s, "text.insert", json!({"text": "Results "}));
    run(&mut s, "references.caption", json!({}));
    assert!(body_text(&s, 1).starts_with("Figure"), "{:?}", body_text(&s, 1));
}

#[test]
fn smart_art_and_charts_in_hebrew_text_get_hebrew_sample_items() {
    let mut s = hebrew_document();
    let g = run(&mut s, "insert.smartArt", json!({"layout": "process"}));
    let items = g["smartArt"]["items"].as_array().cloned().unwrap_or_default();
    assert!(!items.is_empty() && items.iter().all(|i| i["text"].as_str().is_some_and(hebrew)), "{items:?}");
    let mut s = hebrew_document();
    run(&mut s, "insert.chart", json!({"type": "column"}));
    let chart = crate::cmd::chart::selected_chart(&s).expect("the new chart is selected");
    assert!(chart.categories.iter().chain(chart.series.iter().map(|x| &x.name)).all(|t| hebrew(t)), "{chart:?}");
}

// ---------------------------------------------------------------------------------------------
// Dates and times

/// The pieces of a date written `dd/MM/yyyy`, or `MM/dd/yyyy` ([`format_date_in`]).
fn numbers(text: &str) -> Vec<u32> {
    text.split(|c: char| !c.is_ascii_digit()).filter(|n| !n.is_empty()).filter_map(|n| n.parse().ok()).collect()
}

/// A date in a Hebrew document is Hebrew: the day before the month by default, and the month and
/// weekday by their Hebrew names, as fixed text and as a field that stays Hebrew when updated, also
/// after a DOCX round trip.
#[test]
fn a_date_in_a_hebrew_document_is_day_first_with_hebrew_names() {
    use crate::cmd::insert::{format_date_in, format_date_parts_in};
    let mut s = hebrew_document();
    run(&mut s, "insert.dateTime", json!({}));
    let text = body_text(&s, 0);
    let (y, m, d, _, _) = crate::cmd::local_now();
    assert_eq!(numbers(&text), [d, m, u32::try_from(y).unwrap_or_default()], "day, month, year: {text:?}");
    // Names in Hebrew, in a field that keeps them through an update and a save.
    let mut s = hebrew_document();
    run(&mut s, "insert.dateTime", json!({"format": "dddd d MMMM yyyy", "update": true}));
    let named = body_text(&s, 0);
    assert!(hebrew(&named) && !latin(&named), "{named:?}");
    crate::cmd::references::refresh_date_fields(&mut s.doc);
    assert_eq!(body_text(&s, 0), named, "an update keeps the Hebrew");
    let bytes = wordcraft_docx::write(&s.doc).unwrap();
    let mut doc = wordcraft_docx::read(&bytes).unwrap();
    crate::cmd::references::refresh_date_fields(&mut doc);
    let reread = doc.para(StoryRef::Body, &Path::top(0)).map(|p| p.plain_text()).unwrap_or_default();
    assert!(hebrew(&reread) && !latin(&reread), "still Hebrew after a reopen: {reread:?}");
    // Hebrew is accepted by name, in English text too.
    let mut s = Session::new(Document::new());
    run(&mut s, "insert.dateTime", json!({"format": "d MMMM yyyy", "language": "he-IL"}));
    assert!(hebrew(&body_text(&s, 0)));
    // The formatter: Hebrew names and abbreviations, the 24-hour clock's default.
    let he = |pic: &str| format_date_parts_in(pic, Some("he-IL"), 2026, 11, 27, (15, 4, 0), 20_784);
    let en = |pic: &str| format_date_parts_in(pic, Some("en-US"), 2026, 11, 27, (15, 4, 0), 20_784);
    for pic in ["MMMM", "MMM", "dddd", "ddd", "am/pm"] {
        assert!(hebrew(&he(pic)) && !latin(&he(pic)) && he(pic) != en(pic), "{pic}: {}", he(pic));
    }
    assert_eq!(numbers(&he(crate::cmd::insert::default_date_picture(Some("he")))), [27, 11, 2026], "day, month, year");
    assert_eq!(numbers(&he(crate::cmd::insert::default_time_picture(Some("he")))), [15, 4], "a 24-hour clock");
    let iw = format_date_parts_in("MMMM", Some("iw-IL"), 2026, 11, 27, (15, 4, 0), 20_784);
    assert_eq!(iw, he("MMMM"), "iw is Hebrew");
    assert!(format_date_in("yyyy", Some("he")).chars().all(|c| c.is_ascii_digit()), "a year in Western digits");
}

/// A date field from Insert › Field in Hebrew text is Hebrew too.
#[test]
fn a_date_field_inserted_in_hebrew_text_is_hebrew() {
    let mut s = hebrew_document();
    run(&mut s, "text.insert", json!({"text": "היום "}));
    run(&mut s, "insert.field", json!({"instr": "DATE \\@ \"d MMMM yyyy\""}));
    let text = body_text(&s, 0);
    assert!(!latin(&text) && numbers(&text).len() == 2, "the day and the year around a Hebrew month: {text:?}");
    let mut s = Session::new(Document::new());
    run(&mut s, "insert.field", json!({"instr": "DATE \\@ \"d MMMM yyyy\""}));
    assert!(latin(&body_text(&s, 0)), "English text keeps English");
}

/// A date picker in Hebrew text writes its dates in Hebrew, day first, and names Hebrew as its
/// language in the saved file, so Word writes the dates picked there in Hebrew too; in English
/// text it is as it was.
#[test]
fn a_date_picker_in_hebrew_text_writes_hebrew_dates() {
    use wordcraft_doc::control::ControlKind;
    let mut s = hebrew_document();
    run(&mut s, "developer.datePicker", json!({"tag": "when"}));
    run(&mut s, "developer.setDate", json!({"control": "when", "date": "2026-11-27"}));
    let text = s.doc.plain_text(StoryRef::Body);
    assert_eq!(numbers(&text), [27, 11, 2026], "day first: {text:?}");
    run(&mut s, "developer.properties", json!({"control": "when", "format": "d MMMM yyyy"}));
    let named = s.doc.plain_text(StoryRef::Body);
    assert!(hebrew(&named) && !latin(&named) && numbers(&named) == [27, 2026], "{named:?}");
    let saved = wordcraft_docx::read(&wordcraft_docx::write(&s.doc).unwrap()).unwrap();
    let lid = saved.control_ranges(StoryRef::Body).iter().find_map(|r| match &r.control.kind {
        ControlKind::Date { lid, .. } => Some(lid.clone()),
        _ => None,
    });
    assert!(crate::cmd::words::is_hebrew(lid.as_deref()), "{lid:?}");
    let mut s = Session::new(Document::new());
    run(&mut s, "developer.datePicker", json!({"date": "2026-11-27", "format": "MMMM d, yyyy"}));
    let text = s.doc.plain_text(StoryRef::Body);
    assert!(latin(&text) && !hebrew(&text), "{text:?}");
    let mut s = Session::new(Document::new());
    run(&mut s, "developer.datePicker", json!({"date": "2026-11-27"}));
    assert_eq!(numbers(&s.doc.plain_text(StoryRef::Body)), [11, 27, 2026], "month first in English text");
}

/// A content control inserted in Hebrew text shows its placeholder in Hebrew, in the text and in
/// the saved file: a date picker's, a plain text control's and a drop-down list's. In English
/// text it is the English one.
#[test]
fn content_controls_in_hebrew_text_have_hebrew_placeholders() {
    let placeholder = |s: &mut Session| {
        let c = run(s, "developer.control", json!({"control": "c"}));
        assert_eq!(c["showingPlaceholder"], true, "{c}");
        c["placeholder"].as_str().unwrap_or_default().to_string()
    };
    for command in ["developer.datePicker", "developer.plainText", "developer.dropDown"] {
        let mut s = hebrew_document();
        run(&mut s, command, json!({"tag": "c"}));
        let shown = placeholder(&mut s);
        assert!(hebrew(&shown) && !latin(&shown), "{command}: {shown:?}");
        assert!(body_text(&s, 0).contains(&shown), "{command}: in the text: {:?}", body_text(&s, 0));
        let saved = wordcraft_docx::read(&wordcraft_docx::write(&s.doc).unwrap()).unwrap();
        assert!(saved.plain_text(StoryRef::Body).contains(&shown), "{command}: in the saved file");
        let mut s = Session::new(Document::new());
        run(&mut s, command, json!({"tag": "c"}));
        let english = placeholder(&mut s);
        assert!(latin(&english) && !hebrew(&english), "{command}: {english:?}");
    }
}

/// Decorative text inserted without text of its own holds a placeholder in the language of the
/// text it goes into.
#[test]
fn decorative_text_in_hebrew_text_has_a_hebrew_placeholder() {
    let mut s = hebrew_document();
    let part = run(&mut s, "insert.wordArt", json!({}));
    let text = part_text(&s, &part);
    assert!(hebrew(&text) && !latin(&text), "{text:?}");
    let mut s = Session::new(Document::new());
    let part = run(&mut s, "insert.wordArt", json!({}));
    assert!(latin(&part_text(&s, &part)));
}

/// The words of Design › Watermark in a Hebrew document are Hebrew (טיוטה for DRAFT). Such a
/// watermark keeps its text through a DOCX round trip, and every letter of it is drawn: the default
/// watermark font, Aptos, is drawn in Carlito, which has no Hebrew letters, so they come from the
/// faces body text takes them from, and strict export finds nothing missing (each letter was an
/// empty box). English text keeps the English words.
#[test]
fn a_hebrew_watermark_keeps_its_text_and_draws_its_letters() {
    use crate::cmd::words::{WATERMARKS, caret_lang, word};
    let mut s = hebrew_document();
    let words: Vec<&str> = WATERMARKS.iter().map(|w| word(caret_lang(&s).as_deref(), w)).collect();
    assert!(words.iter().all(|w| hebrew(w) && !latin(w)), "{words:?}");
    let english = Session::new(Document::new());
    assert!(WATERMARKS.iter().all(|w| word(caret_lang(&english).as_deref(), w) == *w), "English text, English words");
    let draft = words.first().copied().unwrap_or_default();
    run(&mut s, "design.watermark", json!({"text": draft}));
    let saved = wordcraft_docx::read(&wordcraft_docx::write(&s.doc).unwrap()).unwrap();
    assert_eq!(saved.settings.watermark.map(|w| w.text).as_deref(), Some(draft), "the text after a reopen");
    if wordcraft_fonts::FontDb::global().fallback_for('ט', 0).is_none() {
        eprintln!("skipped the drawing: no font with Hebrew letters installed");
        return;
    }
    let report = run(&mut s, "file.strictReport", json!({}));
    assert_eq!(report["strict"], "passed", "{report}");
}

/// `design.watermark` without a text, in a document that has no watermark, says DRAFT in the
/// language of the text at the caret: טיוטה in a Hebrew document (it was the English DRAFT), DRAFT
/// in English text. A watermark that is there keeps its text.
#[test]
fn a_new_watermark_without_text_says_draft_in_the_language_of_the_text() {
    let text = |s: &Session| s.doc.settings.watermark.as_ref().map(|w| w.text.clone()).unwrap_or_default();
    let mut s = hebrew_document();
    run(&mut s, "design.watermark", json!({"diagonal": false}));
    assert!(hebrew(&text(&s)) && !latin(&text(&s)), "{:?}", text(&s));
    let mut english = Session::new(Document::new());
    run(&mut english, "design.watermark", json!({}));
    assert_eq!(text(&english), "DRAFT");
    run(&mut english, "design.watermark", json!({"text": "SAMPLE"}));
    run(&mut english, "design.watermark", json!({"diagonal": true}));
    assert_eq!(text(&english), "SAMPLE", "a watermark that is there keeps its text");
}
