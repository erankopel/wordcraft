//! A document whose defaults read right to left, as a blank document does in a Hebrew interface:
//! `w:bidi` in the paragraph defaults (`w:pPrDefault`, ECMA-376 Part 1 §17.3.1.6 inside §17.7.5.3),
//! the complex-script font (`w:rFonts/@w:cs`) and language (`w:lang/@w:bidi`, §17.3.2.20) in the run
//! defaults. Synthetic packages, written here from the schema.

use std::sync::Arc;
use wordcraft_doc::props::{Align, ParaProps};
use wordcraft_doc::{Document, Paragraph, para_block};

const NS: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main""#;

/// A package whose paragraph defaults are `ppr_default`, with `styles` after them and `body`.
fn package(ppr_default: &str, styles: &str, body: &str) -> Vec<u8> {
    let root = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="r1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="s1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#;
    let document = format!(r#"<w:document {NS}><w:body>{body}</w:body></w:document>"#);
    let styles = format!(
        r#"<w:styles {NS}><w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="24"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr>{ppr_default}</w:pPr></w:pPrDefault></w:docDefaults>{styles}</w:styles>"#
    );
    let mut z = craft_zip::Writer::new();
    for (name, data) in [
        ("_rels/.rels", root),
        ("word/_rels/document.xml.rels", rels),
        ("word/document.xml", document.as_str()),
        ("word/styles.xml", styles.as_str()),
    ] {
        z.add(name, data.as_bytes(), craft_zip::Compression::Deflated).unwrap();
    }
    z.finish().unwrap()
}

fn part(bytes: &[u8], name: &str) -> String {
    let mut z = craft_zip::Archive::open(bytes, craft_zip::Limits { max_entries: 10_000, max_total_bytes: 64 << 20 }).unwrap();
    String::from_utf8(z.read(name, 16 << 20).unwrap().unwrap()).unwrap()
}

fn paras(d: &Document) -> Vec<&Paragraph> {
    d.body.iter().filter_map(|b| b.as_para()).collect()
}

fn reads_rtl(d: &Document, p: &Paragraph) -> bool {
    d.styles.resolve_para(&p.props).bidi
}

#[test]
fn a_right_to_left_default_is_written_in_the_defaults_and_read_back() {
    let mut d = Document::new();
    let styles = Arc::make_mut(&mut d.styles);
    styles.default_para.bidi = Some(true);
    styles.default_chr.lang_bidi = Some("he-IL".into());
    styles.default_chr.font_cs = Some("David".into());
    d.body = vec![para_block(Paragraph::with_text("שלום", Default::default()))];
    let bytes = wordcraft_docx::write(&d).unwrap();
    let xml = part(&bytes, "word/styles.xml");
    let defaults = xml.split("<w:docDefaults>").nth(1).and_then(|s| s.split("</w:docDefaults>").next()).unwrap();
    let (run_defaults, para_defaults) = defaults.split_once("<w:pPrDefault>").unwrap();
    assert!(run_defaults.contains(r#"w:cs="David""#) && run_defaults.contains(r#"w:bidi="he-IL""#), "{run_defaults}");
    // CT_PPrBase is a sequence: bidi comes before the spacing.
    let at = |t: &str| para_defaults.find(t).unwrap_or_else(|| panic!("{t} missing: {para_defaults}"));
    assert!(at("<w:bidi/>") < at("<w:spacing"), "{para_defaults}");
    assert!(!para_defaults.contains("<w:jc"), "no alignment is written: start is the default edge in a right-to-left paragraph");

    let r = wordcraft_docx::read(&bytes).unwrap();
    assert_eq!(r.styles.default_para.bidi, Some(true));
    assert_eq!((r.styles.default_chr.lang_bidi.as_deref(), r.styles.default_chr.font_cs.as_deref()), (Some("he-IL"), Some("David")));
    let p = paras(&r)[0];
    assert_eq!(p.props.bidi, None, "the paragraph inherits it");
    assert!(reads_rtl(&r, p));
    // The same after another round: nothing is copied onto the paragraphs, nothing is lost.
    let again = wordcraft_docx::read(&wordcraft_docx::write(&r).unwrap()).unwrap();
    assert_eq!(again.styles.default_para.bidi, Some(true));
    assert_eq!(paras(&again)[0].props.bidi, None);
}

#[test]
fn the_default_direction_reaches_every_paragraph_and_a_paragraph_can_override_it() {
    let body = r#"<w:p><w:r><w:t>one</w:t></w:r></w:p>
<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>two</w:t></w:r></w:p>
<w:p><w:pPr><w:bidi w:val="0"/></w:pPr><w:r><w:t>three</w:t></w:r></w:p>
<w:p><w:pPr><w:bidi/><w:jc w:val="left"/></w:pPr><w:r><w:t>four</w:t></w:r></w:p>"#;
    let heading = r#"<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/></w:style>"#;
    let d = wordcraft_docx::read(&package("<w:bidi/>", heading, body)).unwrap();
    let ps = paras(&d);
    let seen: Vec<bool> = ps.iter().map(|p| reads_rtl(&d, p)).collect();
    assert_eq!(
        seen,
        [true, true, false, true],
        "a style that says nothing, and an unstyled paragraph, follow the defaults; `w:bidi w:val=\"0\"` leaves them"
    );
    // Left (the start edge) is on the right in a right-to-left paragraph, and on the left in the one that left it.
    let shown = |i: usize| d.styles.resolve_para(&ps[i].props).align.visual(seen[i]);
    assert_eq!((shown(0), shown(2), shown(3)), (Align::Right, Align::Left, Align::Right));
    // The override and the paragraph that says it again are written back as they were.
    let r = wordcraft_docx::read(&wordcraft_docx::write(&d).unwrap()).unwrap();
    let back: Vec<Option<bool>> = paras(&r).iter().map(|p| p.props.bidi).collect();
    assert_eq!(back, [None, None, Some(false), Some(true)]);
}

#[test]
fn a_file_without_a_default_direction_stays_left_to_right() {
    let d = wordcraft_docx::read(&package("", "", "<w:p><w:r><w:t>שלום</w:t></w:r></w:p>")).unwrap();
    assert_eq!(d.styles.default_para.bidi, None);
    assert!(!reads_rtl(&d, paras(&d)[0]), "Hebrew text alone does not make an opened paragraph right to left");
    assert_eq!(d.styles.resolve_para(&ParaProps::default()).align.visual(false), Align::Left);
}
