//! The watermark's text as the page raster and the PDF export draw it (printing goes through
//! them) and strict export checks it ([`crate::missing_watermark_glyphs`]): laid out here, on one
//! line, by the paragraph layout body text goes through, once for each watermark and document
//! formatting, and kept, so every page, the PDF and the check draw and read that one layout
//! ([`watermark_run`]). Its Latin text is in the face its font resolves to, as it always was. A
//! character that face lacks is drawn where body text would draw it (Hebrew, Arabic or Thai in the
//! document's complex-script font, Chinese, Japanese and Korean in its East Asian font, anything
//! else from the fallback faces), and the line is put in visual order by the Unicode
//! Bidirectional Algorithm (UAX #9), so a Hebrew watermark reads right to left and Hebrew beside
//! Latin letters or digits stands where it reads. The back ends stretch and turn what is placed
//! here; they don't shape or order it again.

use std::ops::Range;
use std::sync::{Arc, Mutex, PoisonError, Weak};

use wordcraft_doc::para::OBJ;
use wordcraft_doc::props::{Align, CharProps, EmphasisMark, Ligatures, ParaProps, VertAlign};
use wordcraft_doc::{Document, Paragraph, StyleSheet, Watermark};
use wordcraft_fonts::FaceRef;

use crate::FieldCtx;
use crate::para::{ClKind, Grid, ParaEnv, ParaLayout, draws_something, is_east_asian, layout_para};

/// The size the text is laid out at, points; what [`watermark_run`] gives is per em of it.
const LAYOUT_SIZE: f32 = 100.0;
/// The line's room for each character, ems: far more than any glyph takes, so the text stays on
/// one line.
const ROOM_PER_CHAR: f32 = 64.0;

/// The watermark's text laid out on one line ([`watermark_run`]). Lengths are in ems of the size
/// it is drawn at, which the watermark's box sets.
#[derive(Clone, Debug)]
pub struct WatermarkText {
    /// The glyphs, a run per face, in the order of the text.
    pub runs: Vec<WatermarkRun>,
    /// The line's advance, from its left edge to its right edge.
    pub width: f32,
    /// How far below the baseline the foot of the em is: the deepest descent of the faces the text
    /// is drawn in. The watermark's box stands the text on it.
    pub descent: f32,
}

/// Glyphs of the watermark in one face.
#[derive(Clone, Debug)]
pub struct WatermarkRun {
    pub face: FaceRef,
    /// Glyph id, x from the line's left edge and y down from the baseline, ems, in the order of
    /// the text (where it runs right to left, x falls).
    pub glyphs: Vec<(u32, f32, f32)>,
    /// The text the glyphs show, and the byte range of it each one shows (a ligature's covers all
    /// its letters), for the PDF's text and for strict export's report.
    pub text: String,
    pub ranges: Vec<Range<usize>>,
}

/// The first `max_chars` characters of the watermark's text laid out as body text is, in the
/// watermark's font with nothing else of the document's formatting: no bold, italic, capitals,
/// spacing, raise, emphasis marks or right-to-left run, the shaper's ligatures, kerned, one size
/// for every script and no automatic space between East Asian and Latin text. Its letters are in
/// the face [`wordcraft_fonts::word::resolve`] gives its font, as they always were (not the
/// alternate or the roman stand-in a document's font table gives body text; an empty font is the
/// fallback face). Complex-script characters (Hebrew, Arabic, Thai…) take that face when it has
/// every one of them, else the document's complex-script font, as body text takes it; Chinese,
/// Japanese and Korean characters likewise take that face or the document's East Asian font;
/// characters none of these has come from the fallback faces. The paragraph reads the way its
/// first strong character does (UAX #9, rules P2 and P3). A tab or a break is a space: the text is
/// one line. `None` for a blank watermark, or one with nothing to draw.
///
/// The layout is kept: asked again for the same text and font, with the document's styles
/// unchanged (the same style sheet), the same font table and no font added or removed, it gives
/// the same layout, so a document's pages, its PDF and strict export lay it out once.
pub fn watermark_run(doc: &Document, wm: &Watermark, max_chars: usize) -> Option<Arc<WatermarkText>> {
    #[cfg(test)]
    crate::test_fonts();
    if wm.text.trim().is_empty() {
        return None;
    }
    let text: String =
        wm.text.chars().take(max_chars).map(|c| if c.is_control() || matches!(c, OBJ | '\u{2028}' | '\u{2029}') { ' ' } else { c }).collect();
    let key = Key::of(doc, &wm.font, &text);
    let mut kept = KEPT.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(i) = kept.iter().position(|(k, _)| k.same(&key)) {
        let entry = kept.remove(i);
        let laid = entry.1.clone();
        kept.insert(0, entry);
        return laid;
    }
    // Laid out without the lock: another thread may lay the same text out meanwhile, and the
    // first to finish is kept.
    drop(kept);
    let laid = lay_out(doc, &wm.font, &text).map(Arc::new);
    let mut kept = KEPT.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some((_, first)) = kept.iter().find(|(k, _)| k.same(&key)) {
        return first.clone();
    }
    kept.insert(0, (key, laid.clone()));
    kept.truncate(KEEP);
    laid
}

/// How many watermark layouts are kept ([`watermark_run`]): one per open document, and the PDF's,
/// which is cut shorter for a long text.
const KEEP: usize = 16;

/// The watermark layouts kept, the latest first.
static KEPT: Mutex<Vec<(Key, Option<Arc<WatermarkText>>)>> = Mutex::new(Vec::new());

/// What a watermark's layout depends on besides the fixed settings of [`lay_out`].
struct Key {
    text: String,
    font: String,
    /// The document's styles, by identity. The key holds a weak reference, so the style sheet's
    /// allocation stays while the key is kept and no other style sheet can come to have its
    /// address; and a changed style sheet is a new one (`Arc::make_mut` clones a shared style sheet
    /// and moves an unshared one that has weak references, `Arc::get_mut` refuses it).
    styles: Weak<StyleSheet>,
    /// The font table's classes and alternates, which choose the complex-script and East Asian
    /// faces.
    roman: Vec<String>,
    modern: Vec<String>,
    alternates: Vec<(String, String)>,
    /// The font database's generation: fonts added or removed.
    fonts: u64,
}

impl Key {
    fn of(doc: &Document, font: &str, text: &str) -> Key {
        let s = &doc.settings;
        Key {
            text: text.to_string(),
            font: font.to_string(),
            styles: Arc::downgrade(&doc.styles),
            roman: s.roman_fonts.clone(),
            modern: s.modern_fonts.clone(),
            alternates: s.font_alt_names.clone(),
            fonts: wordcraft_fonts::FontDb::global().generation(),
        }
    }

    fn same(&self, other: &Key) -> bool {
        Weak::ptr_eq(&self.styles, &other.styles)
            && self.fonts == other.fonts
            && self.text == other.text
            && self.font == other.font
            && self.roman == other.roman
            && self.modern == other.modern
            && self.alternates == other.alternates
    }
}

/// [`watermark_run`]'s layout of `text` (already cut and one line) in `font`.
fn lay_out(doc: &Document, font: &str, text: &str) -> Option<WatermarkText> {
    let p = paragraph(font, text);
    let fields = FieldCtx::default();
    let env = ParaEnv {
        doc,
        width: (text.chars().count() as f32 + 1.0) * ROOM_PER_CHAR * LAYOUT_SIZE,
        label: None,
        fields: &fields,
        show_hidden: false,
        field_codes: false,
        hide_deleted: Default::default(),
        table: None,
        proofing: None,
        typing_word: None,
        numeral: Default::default(),
        exclusions: &[],
        eq_number: 0,
        read_line_spacing: None,
        grid: Grid::default(),
        fit_objects: false,
        fitted: &[],
    };
    placed(&layout_para(&p, &env), &p.text)
}

/// The font name the watermark's Latin text is set in, and the face it draws: the face
/// [`wordcraft_fonts::word::resolve`] gives `font`. An installed font is set by its own name,
/// which body text's lookup leaves alone; a font that isn't installed (or none) is set by the name
/// of the face drawn in its place, so a document's font table can't send it to another face, as it
/// sends body text in that font (#2702), and nothing else stands in for the face.
fn latin_font(font: &str) -> (String, FaceRef) {
    let r = wordcraft_fonts::word::resolve(font, false, false);
    if !r.substituted {
        return (font.to_string(), r.face);
    }
    let names = [r.face.family.as_str(), r.face.legacy_family.as_str()];
    let own = names.into_iter().find(|n| {
        let by_name = wordcraft_fonts::word::resolve(n, false, false);
        !n.trim().is_empty() && !by_name.substituted && by_name.face.id() == r.face.id()
    });
    (own.unwrap_or(font).to_string(), r.face)
}

/// The watermark's text as a paragraph of the document: one run in its font (see
/// [`watermark_run`]), starting at the left edge.
fn paragraph(font: &str, text: &str) -> Paragraph {
    let (font, face) = latin_font(font);
    // Does the text have characters of this kind, and the face every one of them?
    let covers = |kind: fn(char) -> bool| {
        let mut chars = text.chars().filter(|c| kind(*c) && draws_something(*c)).peekable();
        chars.peek().is_some() && chars.all(|c| face.covers(c))
    };
    let own_complex = covers(wordcraft_doc::bidi::is_complex_script);
    let own_east_asian = covers(is_east_asian);
    let chr = CharProps {
        font_cs: own_complex.then(|| font.clone()),
        font_ea: own_east_asian.then(|| font.clone()),
        font: Some(font),
        size: Some(LAYOUT_SIZE),
        size_cs: Some(LAYOUT_SIZE),
        bold: Some(false),
        bold_cs: Some(false),
        italic: Some(false),
        italic_cs: Some(false),
        caps: Some(false),
        small_caps: Some(false),
        hidden: Some(false),
        rtl: Some(false),
        cs: Some(false),
        emphasis: Some(EmphasisMark::None),
        vert_align: Some(VertAlign::Baseline),
        position: Some(0.0),
        spacing: Some(0.0),
        scale: Some(100.0),
        // Kerned, with standard and contextual ligatures, as the shaper shapes by default.
        kern: Some(1.0),
        ligatures: Some(Ligatures { standard: true, contextual: true, ..Default::default() }),
        ..Default::default()
    };
    let mut p = Paragraph::with_text(text, chr);
    p.props = ParaProps {
        bidi: Some(wordcraft_doc::bidi::first_strong_is_rtl(text)),
        align: Some(Align::Left),
        indent_left: Some(0.0),
        indent_right: Some(0.0),
        indent_first: Some(0.0),
        drop_cap: Some(0),
        auto_space_de: Some(false),
        auto_space_dn: Some(false),
        ..Default::default()
    };
    p
}

/// The glyphs of the first line of `pl` (the paragraph of `text`) as [`WatermarkText`]: each
/// cluster's at its visual place, measured from the line's left edge.
fn placed(pl: &ParaLayout, text: &str) -> Option<WatermarkText> {
    let line = pl.lines.first()?;
    let drawn = |k: usize| pl.clusters.get(k).filter(|c| matches!(c.kind, ClKind::Text | ClKind::Space));
    let (mut left, mut right) = (f32::INFINITY, f32::NEG_INFINITY);
    for k in (line.c0..line.c1).filter(|k| drawn(*k).is_some()) {
        if let (Some(a), Some(b)) = (line.cl_left(k), line.cl_right(k)) {
            left = left.min(a.min(b));
            right = right.max(a.max(b));
        }
    }
    if !(left.is_finite() && right.is_finite() && right > left) {
        return None;
    }
    let em = |v: f32| v / LAYOUT_SIZE;
    let mut runs: Vec<WatermarkRun> = Vec::new();
    let mut style = None;
    for k in line.c0..line.c1 {
        let Some(c) = drawn(k) else { continue };
        let (Some(st), Some(x)) = (pl.styles.get(c.style as usize), line.cl_left(k)) else { continue };
        if style != Some(c.style) {
            runs.push(WatermarkRun { face: st.face, glyphs: Vec::new(), text: String::new(), ranges: Vec::new() });
            style = Some(c.style);
        }
        let Some(run) = runs.last_mut() else { continue };
        let a = run.text.len();
        run.text.push_str(text.get(c.start..c.end).unwrap_or(""));
        let b = run.text.len();
        let glyphs = pl.glyphs.get(c.g0 as usize..c.g1 as usize).unwrap_or_default();
        if glyphs.is_empty() {
            // No glyph of its own (a ligature's second letter): its text is the glyphs' before it.
            let from = run.ranges.last().map(|r| r.start);
            for r in run.ranges.iter_mut().rev().take_while(|r| Some(r.start) == from) {
                r.end = b;
            }
        }
        for g in glyphs {
            run.glyphs.push((g.gid, em(x - left + g.dx), em(-st.shift - g.dy)));
            run.ranges.push(a..b);
        }
    }
    runs.retain(|r| !r.glyphs.is_empty());
    let descent = runs
        .iter()
        .map(|r| (wordcraft_fonts::word::line_metrics(&r.face).1 / r.face.upem.max(1.0)) as f32)
        .filter(|d| d.is_finite())
        .fold(0.0f32, f32::max)
        .clamp(0.0, 1.0);
    (!runs.is_empty()).then(|| WatermarkText { runs, width: em(right - left), descent })
}

#[cfg(test)]
mod tests;
