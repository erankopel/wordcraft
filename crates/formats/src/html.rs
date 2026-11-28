//! HTML import and export via `craft-html`.

use wordcraft_doc::{Document, SectionProps};

pub use craft_html::doc::{ImageLoader, PAGE_FONT, css_len_pt, decode, decode_as, decode_entity, esc, export_flow, parse, parse_with, unescape};

fn shared_section(page: &SectionProps) -> craft_wordmodel::SectionProps {
    let mut d = Document::new();
    d.last_section = page.clone();
    wordcraft_docx::to_shared_model(&d).map(|s| s.last_section).unwrap_or_default()
}

pub fn import(html: &str) -> Document {
    crate::one_format_for_all_scripts(wordcraft_docx::from_shared_model(craft_html::doc::import(html)).unwrap_or_default())
}

pub fn import_page(html: &str, images: ImageLoader<'_>) -> Document {
    crate::one_format_for_all_scripts(wordcraft_docx::from_shared_model(craft_html::doc::import_page(html, images)).unwrap_or_default())
}

pub fn import_with(html: &str, images: ImageLoader<'_>) -> Document {
    crate::one_format_for_all_scripts(wordcraft_docx::from_shared_model(craft_html::doc::import_with(html, images)).unwrap_or_default())
}

pub fn import_on(html: &str, images: ImageLoader<'_>, page: &SectionProps) -> Document {
    crate::one_format_for_all_scripts(
        wordcraft_docx::from_shared_model(craft_html::doc::import_on(html, images, &shared_section(page))).unwrap_or_default(),
    )
}

pub fn export(doc: &Document) -> String {
    match wordcraft_docx::to_shared_model(doc) {
        Ok(shared) => with_direction(doc, craft_html::doc::export(&shared)),
        Err(_) => String::new(),
    }
}

/// `html` (a page `craft-html` wrote for `doc`) reading right to left when `doc`'s paragraphs do
/// by default (its Normal paragraph, by the document defaults or the Normal style), as the DOCX
/// and RTF writers say: `dir="rtl"` on the `html` element, whose `lang` is then the document's
/// language as a whole (`StyleSheet::document_lang`: the complex-script one, `he-IL` in a Hebrew
/// document, not its Latin text's `en-US`). Other pages are unchanged.
fn with_direction(doc: &Document, html: String) -> String {
    if !doc.styles.resolve_para(&wordcraft_doc::props::ParaProps::default()).bidi {
        return html;
    }
    const OPEN: &str = "<html lang=\"";
    let Some(at) = html.find(OPEN) else { return html };
    let Some(len) = html[at + OPEN.len()..].find('"') else { return html };
    let end = at + OPEN.len() + len;
    let lang = doc.styles.document_lang().map(|l| esc(&l)).unwrap_or_else(|| html[at + OPEN.len()..end].to_string());
    format!("{}{OPEN}{lang}\" dir=\"rtl{}", &html[..at], &html[end..])
}

pub fn export_fragment(doc: &Document) -> String {
    match wordcraft_docx::to_shared_model(doc) {
        Ok(shared) => craft_html::doc::export_fragment(&shared),
        Err(_) => String::new(),
    }
}
