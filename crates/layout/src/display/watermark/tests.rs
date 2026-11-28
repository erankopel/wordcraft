//! The watermark's text is laid out as body text is: in visual order (UAX #9), with fallback faces
//! for what its font lacks. The fixed test fonts have no Hebrew letters, so Hebrew is placed but
//! drawn as empty boxes here; the raster's, the PDF's and strict export's tests, which run with the
//! installed fonts, check that a face that has the letters draws them.

use std::sync::Arc;

use wordcraft_doc::{Document, Watermark};
use wordcraft_fonts::FaceRef;
use wordcraft_fonts::word::resolve;

use super::{WatermarkText, watermark_run};

/// `doc`'s watermark of `text`, horizontal, laid out; with each glyph's character and x, in the
/// order of the text.
fn drawn(doc: &Document, text: &str) -> (Arc<WatermarkText>, Vec<(char, f32)>) {
    drawn_in(doc, text, &Watermark::default().font)
}

/// [`drawn`] in `font`.
fn drawn_in(doc: &Document, text: &str, font: &str) -> (Arc<WatermarkText>, Vec<(char, f32)>) {
    let wm = Watermark { text: text.into(), font: font.into(), diagonal: false, ..Default::default() };
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
    let font = resolve(&Watermark::default().font, false, false).face;
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

/// The face each character of `t` is drawn in.
fn face_of(t: &WatermarkText, c: char) -> FaceRef {
    t.runs.iter().find(|r| r.text.contains(c)).map(|r| r.face).expect("the character is drawn")
}

/// An English watermark is drawn in the face its font resolves to, as it always was, not in the
/// face body text in that font takes from the document's font table (#2702): a missing font the
/// table classes as roman (body text: Cambria's stand-in), one it gives an alternate, and no font
/// at all (the fallback face; it was Aptos's stand-in, or what the table makes of Aptos).
#[test]
fn an_english_watermark_keeps_the_face_its_font_resolves_to() {
    crate::test_fonts();
    let mut doc = Document::new();
    doc.settings.roman_fonts = vec!["Qwfpgj".into(), Watermark::default().font];
    doc.settings.font_alt_names = vec![("Zxcvbn".into(), "JetBrains Mono".into())];
    for font in ["Qwfpgj", "Zxcvbn", ""] {
        let own = resolve(font, false, false).face;
        // Body text in that font (an empty one is the default watermark font's) is drawn elsewhere.
        let named = if font.is_empty() { Watermark::default().font } else { font.to_string() };
        let body = crate::para::substitute_face(&doc, &named).unwrap_or_else(|| resolve(&named, false, false).face);
        assert_ne!(body.id(), own.id(), "{font:?}: body text in it takes {}, not {}", body.family, own.family);
        let (t, glyphs) = drawn_in(&doc, "DRAFT 2026", font);
        assert!(t.runs.iter().all(|r| r.face.id() == own.id()), "{font:?}: drawn in {}: {glyphs:?}", own.family);
    }
}

/// A document whose default text runs right to left (`w:rtl`, as a right-to-left template's Normal
/// style may have) and has a complex-script font of its own leaves an English watermark in the
/// watermark's font, reading left to right: the watermark's run is not a right-to-left one.
#[test]
fn a_right_to_left_document_leaves_an_english_watermark_in_its_font() {
    let mut doc = Document::new();
    let defaults = &mut Arc::make_mut(&mut doc.styles).default_chr;
    defaults.rtl = Some(true);
    defaults.font_cs = Some("JetBrains Mono".into());
    let (t, glyphs) = drawn(&doc, "DRAFT");
    let own = resolve(&Watermark::default().font, false, false).face;
    assert_ne!(own.family, "JetBrains Mono");
    assert!(t.runs.iter().all(|r| r.face.id() == own.id()), "in {}, not the complex-script font: {glyphs:?}", own.family);
    let x = xs(&glyphs, |c| c.is_ascii_alphabetic());
    assert!(x.len() == 5 && x.windows(2).all(|w| w[0] < w[1]), "DRAFT reads left to right: {glyphs:?}");
}

/// Chinese, Japanese and Korean characters of a watermark whose font has them are set in that font,
/// not in the document's East Asian font, as Word draws a watermark in its one font; a watermark
/// whose font lacks them leaves them to the document's East Asian font, as body text does. (No two
/// fixed test fonts share an East Asian character, so the font the text asks for is checked.)
#[test]
fn east_asian_text_takes_the_watermark_font_when_it_has_it() {
    crate::test_fonts();
    let mut doc = Document::new();
    Arc::make_mut(&mut doc.styles).default_chr.font_ea = Some("Source Serif 4".into());
    // JetBrains Mono has the full-width braces; Source Sans 3 hasn't.
    let text = "\u{FF5B}DRAFT\u{FF5D}";
    let asks = |font: &str| doc.styles.resolve_char(None, &super::paragraph(font, text).mark).font_ea;
    assert_eq!(asks("JetBrains Mono").as_deref(), Some("JetBrains Mono"), "the watermark's font has them");
    assert_eq!(asks("Source Sans 3").as_deref(), Some("Source Serif 4"), "the document's East Asian font");
    let (t, glyphs) = drawn_in(&doc, text, "JetBrains Mono");
    let brace = face_of(&t, '\u{FF5B}');
    assert!(brace.family == "JetBrains Mono" && brace.covers('\u{FF5B}'), "in {}: {glyphs:?}", brace.family);
}

/// A watermark has no automatic space between East Asian text and Latin letters or digits, which
/// the document's paragraphs have by default: its glyphs stand where they do in a document with
/// that space turned off.
#[test]
fn east_asian_and_latin_text_touch_in_a_watermark() {
    let spaced = Document::new();
    let mut unspaced = Document::new();
    let para = &mut Arc::make_mut(&mut unspaced.styles).default_para;
    para.auto_space_de = Some(false);
    para.auto_space_dn = Some(false);
    for text in ["A\u{65E5}1", "1\u{65E5}A"] {
        let x = |doc: &Document| drawn(doc, text).1.iter().map(|g| g.1).collect::<Vec<f32>>();
        let (with, without) = (x(&spaced), x(&unspaced));
        assert!(with.len() == 3 && with.iter().zip(&without).all(|(a, b)| (a - b).abs() < 1e-4), "{text}: {with:?} {without:?}");
    }
}

/// The layout is kept: a second page (the same document, or a copy sharing its styles) gets the
/// first one's layout, not a new one, until the document's styles change.
#[test]
fn the_layout_is_kept_until_the_styles_change() {
    let mut doc = Document::new();
    let wm = Watermark { text: "DRAFT סודי".into(), ..Default::default() };
    let first = watermark_run(&doc, &wm, 200).expect("laid out");
    assert!(Arc::ptr_eq(&first, &watermark_run(&doc.clone(), &wm, 200).expect("laid out")), "the same layout");
    assert_ne!(face_of(&first, 'ס').family, "JetBrains Mono");
    Arc::make_mut(&mut doc.styles).default_chr.font_cs = Some("JetBrains Mono".into());
    let changed = watermark_run(&doc, &wm, 200).expect("laid out");
    assert_eq!(face_of(&changed, 'ס').family, "JetBrains Mono", "laid out again in the new complex-script font");
}
