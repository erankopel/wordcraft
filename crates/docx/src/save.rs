//! What a document says before `craft-docx` writes it, so Word shows it as WordCraft does.
//!
//! - Spaces, digits and punctuation at a right-to-left level of a run that says neither `w:rtl`
//!   nor `w:cs` are drawn with the run's Latin font, size, bold and italic, as Word draws them in
//!   such a run (ECMA-376 §17.3.2.26: by the character's Unicode range). The writer saves those
//!   pieces as `w:rtl` runs (#1543), whose every character Word formats with the complex-script
//!   values (§17.3.2.30). So the pieces of them it will mark say, as complex-script values, the
//!   Latin values they are drawn with; the letters keep theirs.
//! - Document defaults without a complex-script size (`w:szCs`) are drawn at WordCraft's default
//!   size, which the writer writes as `w:sz` only; the reader then gives `w:szCs` 10 pt. So they
//!   say it.

use std::borrow::Cow;
use std::sync::Arc;

use wordcraft_doc::para::OBJ;
use wordcraft_doc::props::{CharProps, DEFAULT_FONT_SIZE};
use wordcraft_doc::resolve::ResolvedChar;
use wordcraft_doc::{Document, Paragraph, Run, StyleSheet};

/// `doc` as it is to be written: unchanged (borrowed) when nothing needs saying.
pub(crate) fn prepare(doc: &Document) -> Cow<'_, Document> {
    let defaults = doc.styles.default_chr.size_cs.is_none();
    let mut neutral = false;
    doc.for_each_para(&mut |p| neutral = neutral || neutral_pieces(&doc.styles, p).is_some());
    if !defaults && !neutral {
        return Cow::Borrowed(doc);
    }
    let mut doc = doc.clone();
    if defaults {
        Arc::make_mut(&mut doc.styles).default_chr.size_cs = Some(DEFAULT_FONT_SIZE);
    }
    if neutral {
        let styles = doc.styles.clone();
        doc.for_each_para_mut(&mut |p| {
            if let Some(runs) = neutral_pieces(&styles, p) {
                p.runs = runs;
                p.touch();
            }
        });
    }
    Cow::Owned(doc)
}

/// The complex-script values that make `c`'s complex-script text look like its other text (the
/// font, size, bold and italic it is drawn with), where they differ; `None` where none do.
fn latin_as_complex(styles: &StyleSheet, para_style: Option<&str>, c: &CharProps) -> Option<CharProps> {
    let rc: ResolvedChar = styles.resolve_char(para_style, c);
    let mut patch = CharProps::default();
    let mut any = false;
    if rc.font_cs != rc.font {
        patch.font_cs = Some(rc.font.clone());
        any = true;
    }
    if rc.size_cs != rc.size {
        patch.size_cs = Some(rc.size);
        any = true;
    }
    if rc.bold_cs != rc.bold {
        patch.bold_cs = Some(rc.bold);
        any = true;
    }
    if rc.italic_cs != rc.italic {
        patch.italic_cs = Some(rc.italic);
        any = true;
    }
    any.then_some(patch)
}

/// Set the values `patch` sets on `c`.
fn apply(c: &mut CharProps, patch: &CharProps) {
    if patch.font_cs.is_some() {
        c.font_cs.clone_from(&patch.font_cs);
    }
    c.size_cs = patch.size_cs.or(c.size_cs);
    c.bold_cs = patch.bold_cs.or(c.bold_cs);
    c.italic_cs = patch.italic_cs.or(c.italic_cs);
}

/// Append `len` bytes in `props` to `out`, in its last run when that has the same.
fn push(out: &mut Vec<Run>, len: usize, props: Cow<'_, CharProps>) {
    match out.last_mut() {
        Some(last) if last.props == *props => last.len += len,
        _ => out.push(Run { len, props: props.into_owned() }),
    }
}

/// `p`'s runs with the neutral characters the writer will mark `w:rtl` in pieces of their own that
/// say their Latin values as complex-script ones; `None` when none need it. The writer marks the
/// pieces of a run without a direction of its own (`rtl` unset) at an odd level of the Unicode
/// bidi algorithm run on the paragraph's text in its direction, as here.
fn neutral_pieces(styles: &StyleSheet, p: &Paragraph) -> Option<Vec<Run>> {
    if !wordcraft_doc::bidi::has_rtl(&p.text) {
        return None;
    }
    let style = p.props.style.as_deref();
    // The runs whose neutral characters would change, with what they need, before any bidi work.
    let patches: Vec<Option<(CharProps, Option<CharProps>)>> = p
        .runs
        .iter()
        .map(|r| {
            let c = &r.props;
            if c.rtl.is_some() || c.cs == Some(true) {
                return None;
            }
            let rc = styles.resolve_char(style, c);
            if rc.rtl || rc.cs {
                return None;
            }
            let patch = latin_as_complex(styles, style, c)?;
            let old = c.fmt_change.as_ref().and_then(|ch| latin_as_complex(styles, style, &ch.old));
            Some((patch, old))
        })
        .collect();
    if patches.iter().all(Option::is_none) {
        return None;
    }
    let rtl_para = styles.resolve_para(&p.props).bidi;
    let level = if rtl_para { unicode_bidi::Level::rtl() } else { unicode_bidi::Level::ltr() };
    let info = unicode_bidi::ParagraphBidiInfo::new(&p.text, Some(level));
    let odd = |at: usize| info.levels.get(at).is_some_and(|l| l.is_rtl());
    let mut out: Vec<Run> = Vec::with_capacity(p.runs.len());
    let mut changed = false;
    let mut start = 0usize;
    for (run, patch) in p.runs.iter().zip(&patches) {
        let end = start.saturating_add(run.len);
        let (Some((patch, old)), Some(text)) = (patch, p.text.get(start..end)) else {
            push(&mut out, run.len, Cow::Borrowed(&run.props));
            start = end;
            continue;
        };
        let mut neutral_props = run.props.clone();
        apply(&mut neutral_props, patch);
        if let (Some(old), Some(ch)) = (old, neutral_props.fmt_change.as_mut()) {
            apply(&mut ch.old, old);
        }
        // Each character: a neutral one at a right-to-left level, or one drawn as before.
        let mut piece: Option<(usize, bool)> = None;
        for (i, ch) in text.char_indices() {
            let neutral = odd(start + i) && ch != OBJ && !wordcraft_doc::bidi::is_complex_script(ch);
            match piece {
                Some((_, n)) if n == neutral => {}
                Some((from, n)) => {
                    push(&mut out, i - from, if n { Cow::Borrowed(&neutral_props) } else { Cow::Borrowed(&run.props) });
                    piece = Some((i, neutral));
                }
                None => piece = Some((i, neutral)),
            }
            changed |= neutral;
        }
        if let Some((from, n)) = piece {
            push(&mut out, text.len() - from, if n { Cow::Borrowed(&neutral_props) } else { Cow::Borrowed(&run.props) });
        }
        start = end;
    }
    changed.then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs that don't cover the paragraph's text exactly (a hostile file) never panic, and keep
    /// their lengths.
    #[test]
    fn runs_off_the_text_pass_through() {
        let mut p = Paragraph::with_text("שלום, עולם.", CharProps { bold: Some(true), ..Default::default() });
        p.runs.push(Run { len: 1000, props: CharProps { bold: Some(true), italic: Some(true), ..Default::default() } });
        let styles = StyleSheet::builtin();
        let total: usize = p.runs.iter().map(|r| r.len).sum();
        let runs = neutral_pieces(&styles, &p).expect("the neutral characters are split out");
        assert_eq!(runs.iter().map(|r| r.len).sum::<usize>(), total);
    }
}
