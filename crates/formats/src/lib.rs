//! WordCraft import/export for the "other" text formats: plain text, Markdown (CommonMark subset
//! plus GFM tables and strikethrough), HTML (and MHT web archives, read-only), RTF, OpenDocument
//! Text and LaTeX — and, read-only, spreadsheet cells for mail-merge recipient lists ([`sheet`]:
//! .xlsx/.xlsm/.xls/.ods).
//!
//! Every format maps to a small flow model ([`model::Flow`]: paragraphs with a kind, list
//! membership, alignment and formatted spans; tables of cells), which [`model::to_doc`] and
//! [`model::from_doc`] convert to and from the full [`Document`]. All parsers are written from
//! the public specifications, are lenient by design and never panic on hostile input: nesting is
//! depth-limited and sizes are capped.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod comments;
pub mod html;
pub mod latex;
pub mod markdown;
pub mod mathml;
pub mod mht;
pub mod model;
pub mod odt;
pub mod rtf;
pub mod sheet;
pub mod txt;

use wordcraft_doc::Document;

/// Most entries an ODT or spreadsheet package may declare, as many as a Word package
/// (`wordcraft_docx::package::MAX_ENTRIES`); checked before `zip` parses them (#3660).
pub const MAX_ZIP_ENTRIES: u64 = 20_000;

/// Extensions this crate handles.
pub const EXTENSIONS: &[&str] =
    &["txt", "text", "md", "markdown", "html", "htm", "xhtml", "mht", "mhtml", "rtf", "odt", "ott", "fodt", "tex", "latex", "ltx"];

/// Import `bytes` in the format named by `ext` (no dot, any case). `None` = not our format.
pub fn import(ext: &str, bytes: &[u8]) -> Option<Result<Document, String>> {
    let ext = ext.trim_start_matches('.').to_ascii_lowercase();
    let r = match ext.as_str() {
        "txt" | "text" => Ok(txt::import(bytes)),
        "md" | "markdown" => Ok(markdown::import(&txt::decode(bytes))),
        "html" | "htm" | "xhtml" => Ok(html::import_page(&html::decode(bytes), &|_| None)),
        "mht" | "mhtml" => Ok(mht::import(bytes)),
        "rtf" => rtf::import(bytes),
        "odt" | "ott" => odt::import(bytes),
        "fodt" => odt::import_flat(bytes),
        "tex" | "latex" | "ltx" => Ok(latex::import(bytes)),
        _ => return None,
    };
    Some(r.map(|mut d| {
        d.ensure_nonempty();
        d
    }))
}

/// [`import`] onto `page` (its size and margins) for the formats whose files give no page size:
/// plain text, Markdown, HTML and MHT start on it, and their tables size to it (#1381). The other
/// formats keep the page their file gives.
pub fn import_on(ext: &str, bytes: &[u8], page: &wordcraft_doc::SectionProps) -> Option<Result<Document, String>> {
    let ext = ext.trim_start_matches('.').to_ascii_lowercase();
    let mut doc = match ext.as_str() {
        "odt" | "ott" => return Some(odt::import_on(bytes, page)),
        "fodt" => return Some(odt::import_flat_on(bytes, page)),
        "txt" | "text" => txt::import_on(bytes, page),
        "md" | "markdown" => markdown::import_on(&txt::decode(bytes), page),
        "html" | "htm" | "xhtml" => html::import_on(&html::decode(bytes), &|_| None, page),
        "mht" | "mhtml" => mht::import_on(bytes, page),
        _ => return import(&ext, bytes),
    };
    doc.ensure_nonempty();
    Some(Ok(doc))
}

/// Plain text and Markdown say nothing of direction: give each paragraph whose first strong
/// letter is right to left (Hebrew, Arabic…) the right-to-left direction, as typing those letters
/// into a paragraph does, unless the paragraph, its style or the document defaults set one.
/// Paragraphs without such a letter (English, numbers, blank lines) stay as they are, and so do
/// a Markdown code block's lines (its `Code` paragraphs), whose code reads left to right.
pub(crate) fn rtl_by_first_letter(doc: &mut Document) {
    use wordcraft_doc::StoryRef;
    if doc.styles.default_para.bidi.is_some() {
        return;
    }
    let stories: Vec<StoryRef> = std::iter::once(StoryRef::Body).chain(doc.parts.keys().map(|id| StoryRef::Part(*id))).collect();
    for story in stories {
        for path in doc.para_paths(story) {
            let Some(p) = doc.para(story, &path) else { continue };
            let style = p.props.style.as_deref().unwrap_or("Normal");
            let styled = || {
                style == crate::model::CODE_STYLE
                    || doc.styles.chain(style).iter().any(|st| st.para.bidi.is_some() || st.id == crate::model::CODE_STYLE)
            };
            if p.props.bidi.is_some() || !wordcraft_doc::bidi::first_strong_is_rtl(&p.text) || styled() {
                continue;
            }
            if let Ok(p) = doc.para_mut(story, &path) {
                p.props.bidi = Some(true);
                p.touch();
            }
        }
    }
}

/// Export `doc` in the format named by `ext`. `None` = not our format.
pub fn export(ext: &str, doc: &Document) -> Option<Result<Vec<u8>, String>> {
    let ext = ext.trim_start_matches('.').to_ascii_lowercase();
    Some(match ext.as_str() {
        "txt" | "text" => Ok(txt::export(doc)),
        "md" | "markdown" => Ok(markdown::export(doc).into_bytes()),
        "html" | "htm" | "xhtml" => Ok(html::export(doc).into_bytes()),
        "rtf" => Ok(rtf::export(doc).into_bytes()),
        "odt" => odt::export(doc),
        "ott" => odt::export_template(doc),
        "fodt" => odt::export_flat(doc),
        "tex" | "latex" | "ltx" => Ok(latex::export(doc).into_bytes()),
        _ => return None,
    })
}

/// How [`export_with`] writes a copy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExportOptions {
    /// Print Markup: HTML, Markdown and plain text end with the document's comments (who wrote
    /// each, the text it comments on, what it says, its replies). Other formats keep comments
    /// their own way or not at all.
    pub comments: bool,
}

/// [`export`] with options.
pub fn export_with(ext: &str, doc: &Document, opts: &ExportOptions) -> Option<Result<Vec<u8>, String>> {
    let out = export(ext, doc)?;
    if !opts.comments {
        return Some(out);
    }
    let ext = ext.trim_start_matches('.').to_ascii_lowercase();
    Some(out.map(|bytes| comments::append(&ext, doc, bytes)))
}

#[cfg(test)]
mod test_log;
#[cfg(test)]
mod tests;
