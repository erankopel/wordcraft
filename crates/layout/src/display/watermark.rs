//! The watermark's text as the page raster and the PDF export draw it (printing goes through
//! them) and strict export checks it ([`crate::missing_watermark_glyphs`]): laid out once, here, on
//! one line, by the paragraph layout body text goes through. A character the watermark's font
//! lacks is drawn where body text would draw it (Hebrew, Arabic or Thai in the document's
//! complex-script font, anything else from the fallback faces), and the line is put in visual
//! order by the Unicode Bidirectional Algorithm (UAX #9), so a Hebrew watermark reads right to
//! left and Hebrew beside Latin letters or digits stands where it reads. The back ends stretch
//! and turn what is placed here; they don't shape or order it again.

use std::ops::Range;

use wordcraft_doc::para::OBJ;
use wordcraft_doc::props::{Align, CharProps, ParaProps, VertAlign};
use wordcraft_doc::{Document, Paragraph, Watermark};
use wordcraft_fonts::FaceRef;

use crate::FieldCtx;
use crate::para::{ClKind, Grid, ParaEnv, ParaLayout, draws_something, layout_para, substitute_face};

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
/// spacing or raise, and one size for every script. Complex-script characters (Hebrew, Arabic,
/// Thai…) take the watermark's font when it has every one of them, else the document's
/// complex-script font, as body text takes it; characters neither has come from the fallback
/// faces. The paragraph reads the way its first strong character does (UAX #9, rules P2 and P3).
/// A tab or a break is a space: the text is one line. `None` for a blank watermark, or one with
/// nothing to draw.
pub fn watermark_run(doc: &Document, wm: &Watermark, max_chars: usize) -> Option<WatermarkText> {
    #[cfg(test)]
    crate::test_fonts();
    if wm.text.trim().is_empty() {
        return None;
    }
    let text: String =
        wm.text.chars().take(max_chars).map(|c| if c.is_control() || matches!(c, OBJ | '\u{2028}' | '\u{2029}') { ' ' } else { c }).collect();
    let p = paragraph(doc, wm, &text);
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

/// The watermark's text as a paragraph of the document: one run in its font (see
/// [`watermark_run`]), starting at the left edge.
fn paragraph(doc: &Document, wm: &Watermark, text: &str) -> Paragraph {
    let font = if wm.font.trim().is_empty() { Watermark::default().font } else { wm.font.clone() };
    let face = substitute_face(doc, &font).unwrap_or_else(|| wordcraft_fonts::word::resolve(&font, false, false).face);
    let mut complex = text.chars().filter(|c| wordcraft_doc::bidi::is_complex_script(*c) && draws_something(*c)).peekable();
    let own_complex = complex.peek().is_some() && complex.all(|c| face.covers(c));
    let chr = CharProps {
        font_cs: own_complex.then(|| font.clone()),
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
        cs: Some(false),
        vert_align: Some(VertAlign::Baseline),
        position: Some(0.0),
        spacing: Some(0.0),
        scale: Some(100.0),
        // Kerned, as the shaper kerns by default.
        kern: Some(1.0),
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
