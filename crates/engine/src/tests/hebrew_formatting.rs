//! Hebrew lists and the Font group in Hebrew text: Home › Numbering's Hebrew numbering, and the
//! font, size, bold and italic of Hebrew text, which Word keeps in the complex-script properties
//! (`w:cs`, `w:szCs`, `w:bCs`, `w:iCs`) apart from those of Latin text.

use serde_json::{Value, json};
use wordcraft_doc::{Document, StoryRef};
use wordcraft_layout::Placed;

use crate::Session;

/// שלום עולם
const SHALOM: &str = "\u{05E9}\u{05DC}\u{05D5}\u{05DD} \u{05E2}\u{05D5}\u{05DC}\u{05DD}";

fn run(s: &mut Session, id: &str, v: Value) -> Value {
    s.run(id, &v).unwrap_or_else(|e| panic!("{id}: {e}"))
}

/// A blank document that starts right to left, as the Hebrew interface makes it, with `paras`
/// typed into it, one paragraph each.
fn hebrew_document(paras: &[&str]) -> Session {
    let mut s = Session::new(Document::new());
    s.set_new_documents_rtl(true);
    for (i, t) in paras.iter().enumerate() {
        if i > 0 {
            run(&mut s, "text.newParagraph", json!({}));
        }
        run(&mut s, "text.insert", json!({"text": t}));
    }
    s
}

/// The list labels of the body's paragraphs, in order.
fn labels(s: &mut Session) -> Vec<String> {
    let l = s.layout();
    l.pages
        .iter()
        .flat_map(|p| &p.items)
        .filter_map(|i| if let Placed::Lines { para, l0: 0, .. } = i { para.label.as_ref().map(|lb| lb.text.clone()) } else { None })
        .collect()
}

/// The width of the first line of body paragraph 0: its characters' advances.
fn first_line_width(s: &mut Session) -> f32 {
    let l = s.layout();
    let para = l
        .pages
        .iter()
        .flat_map(|p| &p.items)
        .find_map(|i| match i {
            Placed::Lines { story: StoryRef::Body, path, para, l0: 0, .. } if path.0 == vec![0] => Some(para.clone()),
            _ => None,
        })
        .expect("paragraph 0 is laid out");
    let line = &para.lines[0];
    para.clusters[line.c0..line.c1].iter().map(|c| c.adv).sum()
}

fn state(s: &mut Session) -> Value {
    run(s, "format.state", json!({}))
}

/// HE-06, RTL-06: Numbering's Hebrew kinds number in Hebrew numerals, and a list of another
/// kind takes the Hebrew numbers instead of losing its numbers.
#[test]
fn numbering_offers_hebrew_numerals() {
    let mut s = hebrew_document(&["ראשון", "שני", "שלישי"]);
    run(&mut s, "select.all", json!({}));
    run(&mut s, "para.numbering", json!({}));
    assert_eq!(labels(&mut s), ["1.", "2.", "3."]);
    run(&mut s, "para.numbering", json!({"kind": "hebrew1"}));
    assert_eq!(labels(&mut s), ["\u{05D0}.", "\u{05D1}.", "\u{05D2}."], "א. ב. ג.");
    let list = run(&mut s, "list.get", json!({}));
    assert_eq!(list["levels"][0]["format"], "hebrew1");
    // The same kind again takes the numbers off, as Numbering does.
    run(&mut s, "para.numbering", json!({"kind": "hebrew1"}));
    assert!(labels(&mut s).is_empty());
    run(&mut s, "para.numbering", json!({"kind": "hebrew1Paren"}));
    assert_eq!(labels(&mut s), ["\u{05D0})", "\u{05D1})", "\u{05D2})"], "א) ב) ג)");
}

/// The Hebrew alphabet and Hebrew numerals part ways after the tenth item: the eleventh is כ in
/// the alphabet and יא in numerals.
#[test]
fn hebrew_alphabet_and_numerals_part_after_ten() {
    let items: Vec<String> = (1..=11).map(|i| format!("פריט {i}")).collect();
    let items: Vec<&str> = items.iter().map(String::as_str).collect();
    let eleventh = |kind: &str| {
        let mut s = hebrew_document(&items);
        run(&mut s, "select.all", json!({}));
        run(&mut s, "para.numbering", json!({"kind": kind}));
        labels(&mut s).pop().unwrap_or_default()
    };
    assert_eq!(eleventh("hebrew1"), "\u{05D9}\u{05D0}.", "יא.");
    assert_eq!(eleventh("hebrew2"), "\u{05DB}.", "כ.");
}

/// A list kind there isn't, or one of the other sort, is an error that changes nothing, not a
/// decimal list made in its place (AGENTS.md rule 15).
#[test]
fn an_unknown_list_kind_is_refused() {
    let mut s = hebrew_document(&["ראשון", "שני"]);
    run(&mut s, "select.all", json!({}));
    for (command, kind) in [("para.numbering", "hebrew"), ("para.numbering", "•"), ("para.bullets", "numbered"), ("para.multilevel", "nope")] {
        assert!(s.run(command, &json!({"kind": kind})).is_err(), "{command} {kind}");
        assert!(labels(&mut s).is_empty(), "{command} {kind} made no list");
    }
}

/// HE-02, RTL-03: in Hebrew text the Font group shows the complex-script font, the one Hebrew
/// letters are drawn in; in Latin text, the Latin font.
#[test]
fn the_font_group_shows_the_font_hebrew_text_is_drawn_in() {
    let mut s = hebrew_document(&[SHALOM]);
    let st = state(&mut s);
    assert_ne!(st["fontLatin"], st["fontCs"], "a new Hebrew document has a font of its own for Hebrew text: {st}");
    assert_eq!(st["font"], st["fontCs"], "a caret after Hebrew text shows the Hebrew font: {st}");
    assert_eq!(st["complex"], true);
    run(&mut s, "select.all", json!({}));
    assert_eq!(state(&mut s)["font"], st["fontCs"], "and so does the Hebrew text selected");
    run(&mut s, "caret.docEnd", json!({}));
    run(&mut s, "text.insert", json!({"text": " Word"}));
    let latin = state(&mut s);
    assert_eq!(latin["font"], latin["fontLatin"], "after a Latin word, the Latin font: {latin}");
    assert_eq!(latin["complex"], false);
    run(&mut s, "select.all", json!({}));
    assert_eq!(state(&mut s)["font"], Value::Null, "Hebrew and Latin text together show no one font");
    // An empty paragraph of a Hebrew document is Hebrew text to come.
    run(&mut s, "caret.docEnd", json!({}));
    run(&mut s, "text.newParagraph", json!({}));
    assert_eq!(state(&mut s)["font"], st["fontCs"]);
}

/// RTL-03: Hebrew text Word made bold (`w:bCs` alone) shows Bold on, and Ctrl+B takes the bold off
/// rather than setting it again.
#[test]
fn bold_reads_and_toggles_hebrew_text_by_its_complex_script_bold() {
    let mut s = hebrew_document(&[SHALOM]);
    run(&mut s, "select.all", json!({}));
    run(&mut s, "format.set", json!({"props": {"boldCs": true, "italicCs": true}}));
    let st = state(&mut s);
    assert_eq!((st["bold"].clone(), st["italic"].clone()), (json!(true), json!(true)), "{st}");
    run(&mut s, "format.bold", json!({}));
    run(&mut s, "format.italic", json!({}));
    let st = state(&mut s);
    assert_eq!((st["bold"].clone(), st["boldCs"].clone(), st["italic"].clone()), (json!(false), json!(false), json!(false)), "{st}");
}

/// The Font group reads the spaces and punctuation of a selection too, in Latin text as in Hebrew
/// text: two words made bold one at a time leave the plain ", " between them, so the selection is
/// not all bold and Bold makes all of it bold; a comma in another size leaves the size box empty.
#[test]
fn spaces_and_punctuation_between_formatted_words_count() {
    let hebrew_words = ("\u{05E9}\u{05DC}\u{05D5}\u{05DD}", "\u{05E2}\u{05D5}\u{05DC}\u{05DD}");
    for (document, (first, second)) in [(Session::new(Document::new()), ("Hello", "world")), (hebrew_document(&[]), hebrew_words)] {
        let mut s = document;
        run(&mut s, "text.insert", json!({"text": format!("{first}, {second}")}));
        for word in [first, second] {
            run(&mut s, "select.text", json!({"text": word}));
            run(&mut s, "format.bold", json!({}));
            run(&mut s, "format.italic", json!({}));
        }
        run(&mut s, "select.all", json!({}));
        let st = state(&mut s);
        assert_eq!((st["bold"].clone(), st["italic"].clone()), (json!(false), json!(false)), "{first}: the comma between is plain: {st}");
        run(&mut s, "format.bold", json!({}));
        let st = state(&mut s);
        assert_eq!(st["bold"], json!(true), "{first}: Bold made all of it bold, the comma too: {st}");
        run(&mut s, "select.text", json!({"text": ","}));
        run(&mut s, "format.size", json!({"size": 20}));
        run(&mut s, "select.all", json!({}));
        assert_eq!(state(&mut s)["size"], Value::Null, "{first}: a bigger comma makes the sizes mixed");
    }
}

/// RTL-03: Grow Font 1 Point makes Hebrew text one point bigger from its own (complex-script)
/// size, and the size box shows that size.
#[test]
fn grow_and_shrink_step_hebrew_text_from_its_own_size() {
    let mut s = hebrew_document(&[SHALOM]);
    run(&mut s, "select.all", json!({}));
    run(&mut s, "format.set", json!({"props": {"sizeCs": 16}, "pair": false}));
    let before = state(&mut s);
    assert_eq!(before["size"], before["sizeCs"], "{before}");
    assert_ne!(before["sizeCs"], before["sizeLatin"], "{before}");
    let size = |st: &Value, k: &str| st[k].as_f64().unwrap_or(f64::NAN);
    run(&mut s, "format.growFont1", json!({}));
    let grown = state(&mut s);
    assert!(size(&grown, "sizeCs") > size(&before, "sizeCs"), "bigger, not set to the Latin size plus one: {before} then {grown}");
    assert!(size(&grown, "sizeLatin") > size(&before, "sizeLatin"), "the Latin size grows from its own: {grown}");
    run(&mut s, "format.shrinkFont", json!({}));
    let shrunk = state(&mut s);
    assert!(size(&shrunk, "sizeCs") < size(&grown, "sizeCs") && size(&shrunk, "sizeCs") > size(&before, "sizeLatin"), "{shrunk}");
}

/// The Font dialog's complex script row: `pair: false` sets the Latin size alone, leaving Hebrew
/// text's size as it was.
#[test]
fn format_set_can_leave_the_complex_script_values_alone() {
    let mut s = hebrew_document(&[SHALOM]);
    run(&mut s, "select.all", json!({}));
    let before = state(&mut s);
    run(&mut s, "format.set", json!({"props": {"size": 30, "font": "Arial"}, "pair": false}));
    let after = state(&mut s);
    assert_eq!((after["sizeCs"].clone(), after["fontCs"].clone()), (before["sizeCs"].clone(), before["fontCs"].clone()), "{after}");
    assert_ne!(after["sizeLatin"], before["sizeLatin"]);
    run(&mut s, "format.set", json!({"props": {"size": 30}}));
    assert_eq!(state(&mut s)["sizeCs"], json!(30.0), "paired by default, as before");
}

/// RTL-04: punctuation typed in Hebrew text is saved in the text's right-to-left (`w:rtl`) runs,
/// whose every character Word sets in the complex-script font and size. It is drawn that way
/// before the save too, so the line is as wide after a save and a reopen.
#[test]
fn punctuation_in_hebrew_text_looks_the_same_after_a_save_and_a_reopen() {
    let mut s = hebrew_document(&["(\u{05E9}\u{05DC}\u{05D5}\u{05DD}), \u{05E2}\u{05D5}\u{05DC}\u{05DD}."]);
    run(&mut s, "select.all", json!({}));
    // Hebrew text bigger than Latin text, so the two sets of punctuation can't be told apart by
    // chance.
    run(&mut s, "format.set", json!({"props": {"sizeCs": 20}, "pair": false}));
    let typed = first_line_width(&mut s);
    let bytes = crate::io::save_bytes("x.docx", &s.doc).unwrap();
    let mut reopened = Session::new(crate::io::open_bytes("x.docx", &bytes).unwrap());
    let read = first_line_width(&mut reopened);
    assert!((typed - read).abs() < 0.01, "{typed} pt before the save, {read} pt after it");
}
