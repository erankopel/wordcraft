//! OpenDocument Text (`.odt`, `.ott`, `.fodt`) import and export via `craft-odf`.

use wordcraft_doc::{Document, SectionProps};

pub use craft_odf::text::{CryptError, TEMPLATE_MIME, TEXT_MIME, decrypt, encrypt, is_encrypted};

fn shared_section(page: &SectionProps) -> craft_wordmodel::SectionProps {
    let mut d = Document::new();
    d.last_section = page.clone();
    wordcraft_docx::to_shared_model(&d).map(|s| s.last_section).unwrap_or_default()
}

/// ODF has a complex-script size, weight and posture of its own (`style:font-size-complex`,
/// `style:font-weight-complex`, `style:font-style-complex`), and `craft-odf` reads them. But it
/// drops a complex size that is the same as the text's, so that size goes back beside it.
fn sizes_for_all_scripts(mut doc: Document) -> Document {
    doc.pair_complex(false);
    doc
}

pub fn import(bytes: &[u8]) -> Result<Document, String> {
    let shared = craft_odf::text::import(bytes)?;
    wordcraft_docx::from_shared_model(shared).map(sizes_for_all_scripts).map_err(|e| e.to_string())
}

pub fn import_on(bytes: &[u8], page: &SectionProps) -> Result<Document, String> {
    let shared = craft_odf::text::import_on(bytes, &shared_section(page))?;
    wordcraft_docx::from_shared_model(shared).map(sizes_for_all_scripts).map_err(|e| e.to_string())
}

pub fn import_flat(bytes: &[u8]) -> Result<Document, String> {
    let shared = craft_odf::text::import_flat(bytes)?;
    wordcraft_docx::from_shared_model(shared).map(sizes_for_all_scripts).map_err(|e| e.to_string())
}

pub fn import_flat_on(bytes: &[u8], page: &SectionProps) -> Result<Document, String> {
    let shared = craft_odf::text::import_flat_on(bytes, &shared_section(page))?;
    wordcraft_docx::from_shared_model(shared).map(sizes_for_all_scripts).map_err(|e| e.to_string())
}

pub fn export(doc: &Document) -> Result<Vec<u8>, String> {
    let shared = wordcraft_docx::to_shared_model(&with_paragraph_directions(doc)).map_err(|e| e.to_string())?;
    craft_odf::text::export(&shared).and_then(|bytes| package_with_direction(doc, bytes))
}

pub fn export_template(doc: &Document) -> Result<Vec<u8>, String> {
    let shared = wordcraft_docx::to_shared_model(&with_paragraph_directions(doc)).map_err(|e| e.to_string())?;
    craft_odf::text::export_template(&shared).and_then(|bytes| package_with_direction(doc, bytes))
}

pub fn export_flat(doc: &Document) -> Result<Vec<u8>, String> {
    let shared = wordcraft_docx::to_shared_model(&with_paragraph_directions(doc)).map_err(|e| e.to_string())?;
    craft_odf::text::export_flat(&shared).map(|xml| with_direction(doc, xml))
}

/// The start of the default paragraph style's paragraph properties, as `craft-odf` writes them.
const DEFAULT_PARAGRAPH_PROPS: &[u8] = b"<style:default-style style:family=\"paragraph\"><style:paragraph-properties";

/// Whether `doc`'s paragraphs read right to left by default: its Normal paragraph, by the
/// document defaults or the Normal style (what `craft-odf` writes into the default style).
fn rtl_by_default(doc: &Document) -> bool {
    doc.styles.resolve_para(&wordcraft_doc::props::ParaProps::default()).bidi
}

/// `xml` (a `styles.xml`, or a flat document) whose default paragraph style reads right to left
/// (`style:writing-mode="rl-tb"`, ODF 1.3 §20.404) when `doc`'s paragraphs do by default, as the
/// DOCX (`w:bidi` in the document defaults) and RTF writers say. `craft-odf` writes each
/// paragraph's own direction ([`with_paragraph_directions`]) but not the default's, so a
/// paragraph added in another program would start left to right. Other documents' bytes are
/// unchanged.
fn with_direction(doc: &Document, xml: Vec<u8>) -> Vec<u8> {
    if !rtl_by_default(doc) {
        return xml;
    }
    let Some(at) = xml.windows(DEFAULT_PARAGRAPH_PROPS.len()).position(|w| w == DEFAULT_PARAGRAPH_PROPS) else { return xml };
    let end = at + DEFAULT_PARAGRAPH_PROPS.len();
    let mut out = Vec::with_capacity(xml.len() + 32);
    out.extend_from_slice(&xml[..end]);
    out.extend_from_slice(b" style:writing-mode=\"rl-tb\"");
    out.extend_from_slice(&xml[end..]);
    out
}

/// `doc` with the direction of each paragraph that reads right to left by default said on the
/// paragraph itself, when the document's paragraphs read right to left by default: `craft-odf`
/// writes a paragraph's own direction only, and reads none from the default style, so without it
/// the paragraphs of a Hebrew document would open left to right in WordCraft. Other documents
/// are written as they are.
fn with_paragraph_directions(doc: &Document) -> std::borrow::Cow<'_, Document> {
    use wordcraft_doc::StoryRef;
    if !rtl_by_default(doc) {
        return std::borrow::Cow::Borrowed(doc);
    }
    let mut d = doc.clone();
    let stories: Vec<StoryRef> = std::iter::once(StoryRef::Body).chain(d.parts.keys().map(|id| StoryRef::Part(*id))).collect();
    for story in stories {
        for path in d.para_paths(story) {
            let Some(p) = d.para(story, &path) else { continue };
            if p.props.bidi.is_some() || !d.styles.resolve_para(&p.props).bidi {
                continue;
            }
            if let Ok(p) = d.para_mut(story, &path) {
                p.props.bidi = Some(true);
                p.touch();
            }
        }
    }
    std::borrow::Cow::Owned(d)
}

/// [`with_direction`] for a package: its `styles.xml`, the other entries as they are, in the same
/// order and with the same compression (the `mimetype` first and stored).
fn package_with_direction(doc: &Document, bytes: Vec<u8>) -> Result<Vec<u8>, String> {
    if !rtl_by_default(doc) {
        return Ok(bytes);
    }
    // The package was written just now: its own size bounds what it expands to.
    let limits = craft_zip::Limits { max_entries: crate::MAX_ZIP_ENTRIES, max_total_bytes: u64::MAX };
    let mut zip = craft_zip::Archive::open(&bytes, limits).map_err(|e| e.to_string())?;
    let mut out = craft_zip::Writer::new();
    for i in 0..zip.len() {
        let Some(name) = zip.entry_name(i).map_err(|e| e.to_string())? else { continue };
        let stored = zip.is_stored(&name).unwrap_or(false);
        let data = zip.read(&name, u64::MAX).map_err(|e| e.to_string())?.unwrap_or_default();
        let data = if name == "styles.xml" { with_direction(doc, data) } else { data };
        let how = if stored { craft_zip::Compression::Stored } else { craft_zip::Compression::Deflated };
        out.add(&name, &data, how).map_err(|e| e.to_string())?;
    }
    out.finish().map_err(|e| e.to_string())
}
