//! The watermark's text is laid out as body text is: in visual order (UAX #9), with fallback faces
//! for what its font lacks. The fixed test fonts have no Hebrew letters, so Hebrew is placed but
//! drawn as empty boxes here; the raster's, the PDF's and strict export's tests, which run with the
//! installed fonts, check that a face that has the letters draws them.

use std::sync::Arc;

use wordcraft_doc::{Document, Watermark};

use super::{WatermarkText, watermark_run};

/// `doc`'s watermark of `text`, horizontal, laid out; with each glyph's character and x, in the
/// order of the text.
fn drawn(doc: &Document, text: &str) -> (WatermarkText, Vec<(char, f32)>) {
    let wm = Watermark { text: text.into(), diagonal: false, ..Default::default() };
    let t = watermark_run(doc, &wm, 200).expect("the text is laid out");
    let glyphs =
        t.runs.iter().flat_map(|r| r.glyphs.iter().zip(&r.ranges).map(|(g, range)| (r.text[range.clone()].chars().next().unwrap(), g.1))).collect();
    (t, glyphs)
}

/// The x of the glyphs whose character is `kind`, in the order of the text.
fn xs(glyphs: &[(char, f32)], kind: impl Fn(char) -> bool) -> Vec<f32> {
    glyphs.iter().filter(|(c, _)| kind(*c)).map(|(_, x)| *x).collect()
}

fn hebrew(c: char) -> bool {
    wordcraft_doc::bidi::is_hebrew_letter(c)
}

/// A Hebrew watermark reads right to left: its first letter is the rightmost (סודי, not ידוס as
/// its letters were drawn in typing order before), every glyph on the line.
#[test]
fn a_hebrew_watermark_reads_right_to_left() {
    let (t, glyphs) = drawn(&Document::new(), "סודי");
    assert_eq!(glyphs.iter().map(|g| g.0).collect::<String>(), "סודי", "a glyph a letter, in the order of the text");
    let x = xs(&glyphs, hebrew);
    assert!(x.windows(2).all(|w| w[0] > w[1]), "ס rightmost, י leftmost: {glyphs:?}");
    assert!(x.iter().all(|x| *x >= 0.0 && *x < t.width), "on the line: {x:?} of {}", t.width);
}

/// Hebrew beside Latin letters and digits stands where UAX #9 puts it, whichever script comes
/// first: DRAFT and 2026 read left to right, to the left of the Hebrew word, which reads right to
/// left (typing order put 2026 after the Hebrew word in the second text).
#[test]
fn mixed_directions_are_in_the_order_of_the_bidi_algorithm() {
    for text in ["סודי DRAFT 2026", "DRAFT סודי 2026"] {
        let (_, glyphs) = drawn(&Document::new(), text);
        let (latin, digits, he) = (xs(&glyphs, |c| c.is_ascii_alphabetic()), xs(&glyphs, |c| c.is_ascii_digit()), xs(&glyphs, hebrew));
        assert!(latin.windows(2).all(|w| w[0] < w[1]) && digits.windows(2).all(|w| w[0] < w[1]), "{text}: {glyphs:?}");
        assert!(he.windows(2).all(|w| w[0] > w[1]), "{text}: {glyphs:?}");
        let (max, min) = (|v: &[f32]| v.iter().copied().fold(f32::MIN, f32::max), |v: &[f32]| v.iter().copied().fold(f32::MAX, f32::min));
        assert!(max(&latin) < min(&digits) && max(&digits) < min(&he), "{text}: DRAFT, 2026, then the Hebrew word: {glyphs:?}");
    }
}

/// A character the watermark's font lacks is drawn from a face that has it, as body text's is,
/// and strict export's check finds nothing missing (the star was an empty box in the watermark's
/// font, and reported).
#[test]
fn a_character_the_font_lacks_comes_from_a_face_that_has_it() {
    let doc = Document::new();
    let (t, glyphs) = drawn(&doc, "★ DRAFT ★");
    let font = super::substitute_face(&doc, &Watermark::default().font)
        .unwrap_or_else(|| wordcraft_fonts::word::resolve(&Watermark::default().font, false, false).face);
    assert!(!font.covers('★') && font.covers('D'), "the watermark's font lacks the star: {}", font.family);
    for r in &t.runs {
        for ((gid, ..), range) in r.glyphs.iter().zip(&r.ranges) {
            let c = r.text[range.clone()].chars().next().unwrap();
            assert!(c == ' ' || (*gid != 0 && r.face.covers(c)), "{c:?} drawn in {}: {glyphs:?}", r.face.family);
        }
    }
    assert!(t.runs.iter().any(|r| r.text.contains('★') && r.face.id() != font.id()), "a fallback face draws the stars");
    let wm = Watermark { text: "★ DRAFT ★".into(), ..Default::default() };
    assert!(crate::missing_watermark_glyphs(&doc, &wm, 200).is_empty(), "the check sees what is drawn");
}

/// Hebrew letters the watermark's font lacks take the document's complex-script font, as body
/// text's do, while its Latin letters keep the watermark's font. (No fixed test font has Hebrew,
/// so the complex-script font draws them as empty boxes, unless a face that has them is found.)
#[test]
fn hebrew_takes_the_documents_complex_script_font() {
    let mut doc = Document::new();
    Arc::make_mut(&mut doc.styles).default_chr.font_cs = Some("JetBrains Mono".into());
    let (t, _) = drawn(&doc, "DRAFT סודי");
    let face_of = |c: char| t.runs.iter().find(|r| r.text.contains(c)).map(|r| r.face).unwrap();
    let (latin, he) = (face_of('D'), face_of('ס'));
    assert!(!latin.covers('ס') && latin.family != "JetBrains Mono", "the watermark's font: {}", latin.family);
    assert!(he.family == "JetBrains Mono" || he.covers('ס'), "Hebrew in the complex-script font: {}", he.family);
    assert_ne!(he.id(), latin.id(), "not in the watermark's font, which lacks it");
}

/// A tab or a line break is a space, and a watermark longer than the room the PDF gives it keeps
/// its first characters: one line either way.
#[test]
fn the_text_is_one_line_of_its_first_characters() {
    let doc = Document::new();
    let (_, glyphs) = drawn(&doc, "A\tB\nC");
    let x = xs(&glyphs, |c| c.is_ascii_alphabetic());
    assert!(x.len() == 3 && x.windows(2).all(|w| w[0] < w[1]), "{glyphs:?}");
    let wm = Watermark { text: "ABCDEF".into(), ..Default::default() };
    let t = watermark_run(&doc, &wm, 3).unwrap();
    assert_eq!(t.runs.iter().map(|r| r.text.as_str()).collect::<String>(), "ABC");
    assert!(watermark_run(&doc, &Watermark { text: " \t ".into(), ..Default::default() }, 200).is_none(), "a blank watermark");
}
