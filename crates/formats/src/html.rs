//! HTML import and export via `craft-html`.

use std::borrow::Cow;

use wordcraft_doc::props::CharProps;
use wordcraft_doc::{Document, Paragraph, Run, SectionProps};

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
    match wordcraft_docx::to_shared_model(&by_script(doc)) {
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
    match wordcraft_docx::to_shared_model(&by_script(doc)) {
        Ok(shared) => craft_html::doc::export_fragment(&shared),
        Err(_) => String::new(),
    }
}

/// `doc` with its complex-script text (Hebrew and Arabic letters, every character of a `w:rtl` or
/// `w:cs` run) in runs of its own that say its complex-script font, size, bold and italic as the
/// others, where those differ: HTML has one set for every script, and `craft-html` writes the
/// others, so Hebrew in a run bold for Latin text only is written regular, as WordCraft and Word
/// show it. Borrowed unchanged when no run needs it (Latin documents, documents that format both
/// scripts alike).
fn by_script(doc: &Document) -> Cow<'_, Document> {
    let mut any = false;
    doc.for_each_para(&mut |p| any = any || script_runs(doc, p).is_some());
    if !any {
        return Cow::Borrowed(doc);
    }
    let mut out = doc.clone();
    out.for_each_para_mut(&mut |p| {
        if let Some(runs) = script_runs(doc, p) {
            p.runs = runs;
            p.touch();
        }
    });
    Cow::Owned(out)
}

/// Append `len` bytes in `props` to `out`, in its last run when that has the same.
fn push(out: &mut Vec<Run>, len: usize, props: &CharProps) {
    match out.last_mut() {
        Some(last) if last.props == *props => last.len += len,
        _ => out.push(Run { len, props: props.clone() }),
    }
}

/// `p`'s runs split by script where a run formats its complex-script text otherwise
/// ([`by_script`]); `None` when none does.
fn script_runs(doc: &Document, p: &Paragraph) -> Option<Vec<Run>> {
    let style = p.props.style.as_deref();
    let mut out: Vec<Run> = Vec::with_capacity(p.runs.len());
    let mut changed = false;
    for (range, props) in p.run_ranges() {
        let Some(text) = p
            .text
            .get(range.clone())
            .filter(|t| t.chars().any(wordcraft_doc::bidi::is_complex_script) || props.rtl == Some(true) || props.cs == Some(true))
        else {
            push(&mut out, range.len(), props);
            continue;
        };
        let rc = doc.styles.resolve_char(style, props);
        let cs = rc.complex();
        if (cs.font.as_str(), cs.size, cs.bold, cs.italic) == (rc.font.as_str(), rc.size, rc.bold, rc.italic) {
            push(&mut out, range.len(), props);
            continue;
        }
        let complex_props =
            CharProps { font: Some(cs.font.clone()), size: Some(cs.size), bold: Some(cs.bold), italic: Some(cs.italic), ..props.clone() };
        let mut piece: Option<(usize, bool)> = None;
        for (i, c) in text.char_indices() {
            let complex = rc.uses_complex(c);
            match piece {
                Some((_, k)) if k == complex => {}
                Some((from, k)) => {
                    push(&mut out, i - from, if k { &complex_props } else { props });
                    piece = Some((i, complex));
                }
                None => piece = Some((i, complex)),
            }
            changed |= complex;
        }
        if let Some((from, k)) = piece {
            push(&mut out, text.len() - from, if k { &complex_props } else { props });
        }
    }
    changed.then_some(out)
}
