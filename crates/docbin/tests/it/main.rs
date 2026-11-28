//! Integration tests of the `.doc` reader: documents written with the craft-doc-binary writer
//! and read back through WordCraft's adapter.

use craft_wordmodel::props::CharProps;
use wordcraft_doc::para::Paragraph;
use wordcraft_doc::resolve::ResolvedChar;

/// Write one body paragraph of `(text, formatting)` runs as a `.doc` and read it back.
fn round_trip(runs: &[(&str, &CharProps)]) -> wordcraft_doc::Document {
    let mut para = craft_wordmodel::Paragraph::new();
    for (text, props) in runs {
        para.push_text(text, props);
    }
    let mut src = craft_wordmodel::Document::new();
    src.body = vec![craft_wordmodel::para_block(para)];
    let bytes = craft_doc_binary::write(&src).expect("write .doc");
    wordcraft_docbin::read(&bytes).expect("read .doc")
}

fn body_para(doc: &wordcraft_doc::Document) -> &Paragraph {
    doc.body.iter().find_map(|b| b.as_para()).expect("a body paragraph")
}

/// The run that holds the first character of `text` in `p`, resolved through the styles.
fn resolved_at(doc: &wordcraft_doc::Document, p: &Paragraph, text: &str) -> (wordcraft_doc::props::CharProps, ResolvedChar) {
    let at = p.text.find(text).expect("the run's text");
    let mut start = 0;
    let run = p
        .runs
        .iter()
        .find(|r| {
            let hit = at >= start && at < start + r.len;
            start += r.len;
            hit
        })
        .expect("a run at the text");
    (run.props.clone(), doc.styles.resolve_char(p.props.style.as_deref(), &run.props))
}

/// A Word file can format a run's Hebrew letters apart from its Latin ones (sprmCFBoldBi,
/// sprmCFItalicBi, sprmCHpsBi): each keeps its own bold, italic and size, and a run that is bold
/// only for Hebrew stays regular for Latin text. The run direction (sprmCFBiDi) is kept too.
#[test]
fn complex_script_bold_italic_and_size_are_read_apart_from_the_latin_ones() {
    let latin = CharProps { bold: Some(true), size: Some(16.0), bold_cs: Some(false), size_cs: Some(12.0), ..Default::default() };
    let hebrew = CharProps { bold_cs: Some(true), italic_cs: Some(true), rtl: Some(true), ..Default::default() };
    let doc = round_trip(&[("Latin שלום ", &latin), ("עולם", &hebrew)]);
    let p = body_para(&doc);

    let (_, a) = resolved_at(&doc, p, "Latin");
    let a_cs = a.complex();
    assert!(a.bold && !a_cs.bold, "Latin bold, Hebrew not bold");
    assert!(a_cs.size < a.size, "the Hebrew size is its own, smaller than the Latin one");
    assert_eq!((a.size, a_cs.size), (latin.size.unwrap(), latin.size_cs.unwrap()), "both sizes as written");

    let (b_props, b) = resolved_at(&doc, p, "עולם");
    let b_cs = b.complex();
    assert!(b_cs.bold && b_cs.italic, "the Hebrew letters are bold and italic");
    assert!(!b.bold && !b.italic, "Latin text in the same run is not");
    assert_eq!(b_props.rtl, Some(true), "the run stays right to left");
    assert!(!a.rtl, "the first run is not right to left");
}

/// Word's bold (sprmCFBold) is the Latin bold only: a run that sets it alone shows its Hebrew
/// letters regular, at the document's base complex-script size, not the run's Latin size. The
/// base sizes are one, as in Word ([MS-DOC] sprmCHps, sprmCHpsBi: 10 pt).
#[test]
fn latin_bold_and_size_alone_leave_the_complex_script_values_unset() {
    let props = CharProps { bold: Some(true), italic: Some(true), size: Some(16.0), ..Default::default() };
    let doc = round_trip(&[("שלום world", &props)]);
    let p = body_para(&doc);
    let (run, c) = resolved_at(&doc, p, "שלום");
    assert_eq!((run.bold_cs, run.italic_cs, run.size_cs), (None, None, None), "nothing sets the complex-script values");
    let cs = c.complex();
    assert!(c.bold && c.italic, "the Latin text is bold and italic");
    assert!(!cs.bold && !cs.italic, "the Hebrew letters are not");
    assert!(cs.size < c.size, "the Hebrew letters keep the base size");
    let defaults = &doc.styles.default_chr;
    assert!(defaults.size.is_some() && defaults.size_cs == defaults.size, "one base size for every script");
}
