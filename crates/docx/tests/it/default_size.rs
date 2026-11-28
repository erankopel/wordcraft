//! The document default font size (`w:docDefaults/w:rPrDefault/w:rPr/w:sz`) across a save and
//! reopen. An unset default is resolved as `DEFAULT_FONT_SIZE`; the writer must say so, because
//! the reader reads an omitted size as 10 pt.
use std::sync::Arc;
use wordcraft_doc::Document;
use wordcraft_doc::props::{CharProps, DEFAULT_FONT_SIZE};
fn resolved(d: &Document) -> f32 {
    d.styles.resolve_char(None, &CharProps::default()).size
}
fn styles_xml(bytes: &[u8]) -> String {
    let mut archive = craft_zip::Archive::open(bytes, craft_zip::Limits { max_entries: 10_000, max_total_bytes: 64 << 20 }).unwrap();

    String::from_utf8(archive.read("word/styles.xml", 16 << 20).unwrap().unwrap()).unwrap()
}
fn defaults(xml: &str) -> &str {
    xml.split("<w:docDefaults>").nth(1).unwrap().split("</w:docDefaults>").next().unwrap()
}
#[test]
fn an_unset_default_size_keeps_its_effective_size_across_save_and_reopen() {
    let mut d = Document::new();
    Arc::make_mut(&mut d.styles).default_chr.size = None;
    assert_eq!(resolved(&d), DEFAULT_FONT_SIZE);
    let bytes = wordcraft_docx::write(&d).unwrap();
    assert!(defaults(&styles_xml(&bytes)).contains(r#"<w:sz w:val="22"/>"#));
    let reopened = wordcraft_docx::read(&bytes).unwrap();
    assert_eq!(resolved(&reopened), DEFAULT_FONT_SIZE);
    // Saving again changes nothing further.
    let again = wordcraft_docx::read(&wordcraft_docx::write(&reopened).unwrap()).unwrap();
    assert_eq!(resolved(&again), DEFAULT_FONT_SIZE);
}
#[test]
fn an_explicit_default_size_is_written_as_it_is() {
    for size in [8.0, 10.0, 11.0, 12.5, 18.0] {
        let mut d = Document::new();
        Arc::make_mut(&mut d.styles).default_chr.size = Some(size);
        let bytes = wordcraft_docx::write(&d).unwrap();
        assert_eq!(resolved(&wordcraft_docx::read(&bytes).unwrap()), size);
    }
}
#[test]
fn a_file_that_omits_the_default_size_is_still_read_as_ten_points() {
    // The reader is unchanged: only what WordCraft writes differs. A package without a size
    // anywhere keeps its established reading.
    let root = r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let doc = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>x</w:t></w:r></w:p></w:body></w:document>"#;
    let styles = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:rPrDefault><w:rPr/></w:rPrDefault></w:docDefaults></w:styles>"#;
    let rels = r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#;
    let mut zw = craft_zip::Writer::new();
    for (name, body) in [("_rels/.rels", root), ("word/document.xml", doc), ("word/_rels/document.xml.rels", rels), ("word/styles.xml", styles)] {
        zw.add(name, body.as_bytes(), craft_zip::Compression::Deflated).unwrap();
    }
    let d = wordcraft_docx::read(&zw.finish().unwrap()).unwrap();
    assert_eq!(d.styles.default_chr.size, Some(10.0));
}
#[test]
fn an_unset_default_size_does_not_take_a_styles_size() {
    // The written default is the resolver's own fallback, not what the Normal style resolves to,
    // so a style that sets a size keeps it and one that does not still falls back to the default.
    let mut d = Document::new();
    Arc::make_mut(&mut d.styles).default_chr.size = None;
    if let Some(normal) = Arc::make_mut(&mut d.styles).get_mut("Normal") {
        normal.chr.size = Some(14.0);
    }
    let reopened = wordcraft_docx::read(&wordcraft_docx::write(&d).unwrap()).unwrap();
    assert_eq!(reopened.styles.default_chr.size, Some(DEFAULT_FONT_SIZE));
    assert_eq!(resolved(&reopened), resolved(&d));
}
/// An unset complex-script default size (`w:szCs`) is drawn at `DEFAULT_FONT_SIZE`, which the
/// writer writes as `w:sz` only: the saved file says it as `w:szCs` too, so Hebrew text keeps its
/// size after a reopen (the reader gives a missing `w:szCs` 10 pt).
#[test]
fn an_unset_complex_script_default_size_keeps_its_effective_size_across_save_and_reopen() {
    let complex = |d: &Document| d.styles.resolve_char(None, &CharProps::default()).complex().size;
    let mut d = Document::new();
    Arc::make_mut(&mut d.styles).default_chr.size_cs = None;
    assert_eq!(complex(&d), DEFAULT_FONT_SIZE);
    let reopened = wordcraft_docx::read(&wordcraft_docx::write(&d).unwrap()).unwrap();
    assert_eq!(complex(&reopened), DEFAULT_FONT_SIZE);
    assert_eq!(d.styles.default_chr.size_cs, None, "the document itself is unchanged");
}
/// A package without a styles part: Word sets its text at 10 pt, Latin and Hebrew alike, and the
/// built-in styles WordCraft keeps for it format both scripts the same.
#[test]
fn a_package_without_styles_sets_latin_and_hebrew_text_alike() {
    let root = r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let doc = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>x שלום</w:t></w:r></w:p><w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>x שלום</w:t></w:r></w:p></w:body></w:document>"#;
    let mut zw = craft_zip::Writer::new();
    for (name, body) in [("_rels/.rels", root), ("word/document.xml", doc)] {
        zw.add(name, body.as_bytes(), craft_zip::Compression::Deflated).unwrap();
    }
    let d = wordcraft_docx::read(&zw.finish().unwrap()).unwrap();
    let paras: Vec<_> = d.body.iter().filter_map(|b| b.as_para()).collect();
    let body = d.styles.resolve_char(paras[0].props.style.as_deref(), &CharProps::default());
    assert_eq!((body.size, body.complex().size), (10.0, 10.0), "body text at 10 pt in both scripts");
    let heading = d.styles.resolve_char(paras[1].props.style.as_deref(), &CharProps::default());
    let cs = heading.complex();
    assert_eq!((cs.size, cs.bold, cs.italic), (heading.size, heading.bold, heading.italic), "a heading's Hebrew as its Latin");
}
