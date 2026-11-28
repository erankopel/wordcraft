//! The language the words WordCraft writes into a document are in ([`wordcraft_doc::words`]): that
//! of the text that receives them, as Word files that text's letters. In Hebrew text, or in a
//! paragraph without letters that reads right to left, it is the language of complex-script text
//! (`w:lang/@w:bidi`), so a Hebrew document gets Hebrew words though its Latin text is English.
//! The interface language plays no part: a document's words belong to the document.

use wordcraft_doc::props::{CharProps, ParaProps};
use wordcraft_doc::{Document, Paragraph, Pos};

use crate::Session;

pub use wordcraft_doc::words::{caption_labels, greeting, is_hebrew, page_x_of_y, word};

/// The words Design › Watermark offers, in English. A watermark is the document's text, so the
/// gallery writes each in the language of the text at the caret ([`word`] of [`caret_lang`]): in
/// Hebrew text it writes and shows the Hebrew word (טיוטה for DRAFT), whatever the interface
/// language; in other text the English word, under the label's translation.
pub const WATERMARKS: [&str; 6] = ["DRAFT", "CONFIDENTIAL", "DO NOT COPY", "SAMPLE", "ASAP", "URGENT"];

/// The language of text inserted at the caret (at the start of a selection, which it replaces),
/// with the formatting typing gives it.
pub fn caret_lang(s: &Session) -> Option<String> {
    let at = s.sel.ordered().0;
    lang_with(s, &at, &s.typing_props())
}

/// The language of text inserted at `at`, with the formatting of the text there.
pub fn lang_at(s: &Session, at: &Pos) -> Option<String> {
    let props = s.doc.para_at(at).map(|p| p.props_at(at.off).clone()).unwrap_or_default();
    lang_with(s, at, &props)
}

/// The language of text formatted `props` at `at` ([`lang_in`]).
fn lang_with(s: &Session, at: &Pos, props: &CharProps) -> Option<String> {
    lang_in(&s.doc, s.doc.para_at(at), at.off, props)
}

/// The language of text formatted `props` at byte `off` of `para`, a paragraph of `doc` or a copy
/// of one (a merged letter's): of complex-script text where the text around it is
/// ([`super::format::complex_in`]). Hebrew letters are Hebrew (Israel's, as Word tags Hebrew
/// text) whatever complex-script language they are marked with (Word marks them Arabic where
/// Hebrew isn't set up), unless it is another language written in Hebrew letters (Yiddish).
pub(crate) fn lang_in(doc: &Document, para: Option<&Paragraph>, off: usize, props: &CharProps) -> Option<String> {
    let rc = doc.styles.resolve_char(para.and_then(|p| p.props.style.as_deref()), props);
    let complex = super::format::complex_in(doc, para, off, &rc);
    let lang = super::format::script_lang(&rc, complex);
    let written_in_hebrew = |l: &str| matches!(wordcraft_doc::bidi::primary_subtag(l).as_str(), "he" | "iw" | "yi" | "ji");
    let hebrew_letter = complex && para.and_then(|p| nearest_letter(p, off)).is_some_and(is_hebrew_letter);
    if hebrew_letter && !lang.as_deref().is_some_and(written_in_hebrew) { Some(wordcraft_doc::styles::HEBREW_LANG.into()) } else { lang }
}

/// The letter nearest `off` in `p`: before it, else after it.
fn nearest_letter(p: &Paragraph, off: usize) -> Option<char> {
    let off = off.min(p.text.len());
    let before = p.text.get(..off).and_then(|t| t.chars().rev().find(|c| c.is_alphabetic()));
    before.or_else(|| p.text.get(off..).and_then(|t| t.chars().find(|c| c.is_alphabetic())))
}

fn is_hebrew_letter(c: char) -> bool {
    matches!(c, '\u{0590}'..='\u{05FF}' | '\u{FB1D}'..='\u{FB4F}')
}

/// The language of a new paragraph of `style` before anything is typed in it (a header's or a
/// footer's): of complex-script text when the style reads right to left, as in a Hebrew document.
pub fn style_lang(doc: &Document, style: &str) -> Option<String> {
    let rc = doc.styles.resolve_char(Some(style), &CharProps::default());
    let rtl = doc.styles.resolve_para(&ParaProps { style: Some(style.into()), ..Default::default() }).bidi;
    super::format::script_lang(&rc, rc.rtl || rc.cs || rtl)
}

/// Today's date written out in full in the language `lang`, as a cover page or a letter dates
/// itself: `27 בנובמבר 2026` in Hebrew, `November 27, 2026` otherwise.
pub fn long_date(lang: Option<&str>) -> String {
    if is_hebrew(lang) { super::insert::format_date_in(super::insert::HEBREW_LONG_DATE, lang) } else { super::insert::format_date("MMMM d, yyyy") }
}
