//! Independent complex-script formatting through style inheritance and DOCX round trips.
//! Synthetic packages, written here; no external documents or installed fonts are needed.

use wordcraft_doc::props::CharProps;
use wordcraft_doc::{Document, Paragraph, Run};

fn package(run: &str, defaults: &str, styles: &str, paragraph_style: &str) -> Vec<u8> {
    package_text(run, defaults, styles, paragraph_style, "שָׁלוֹם ABC 123")
}

fn package_text(run: &str, defaults: &str, styles: &str, paragraph_style: &str, text: &str) -> Vec<u8> {
    let ns = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main""#;
    let root = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="r1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="s1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#;
    let document = format!(
        r#"<w:document {ns}><w:body><w:p><w:pPr>{paragraph_style}<w:bidi/></w:pPr><w:r><w:rPr>{run}</w:rPr><w:t xml:space="preserve">{text}</w:t></w:r></w:p></w:body></w:document>"#
    );
    let styles = format!("<w:styles {ns}><w:docDefaults><w:rPrDefault><w:rPr>{defaults}</w:rPr></w:rPrDefault></w:docDefaults>{styles}</w:styles>");
    let mut z = craft_zip::Writer::new();
    for (name, data) in [
        ("_rels/.rels", root),
        ("word/_rels/document.xml.rels", rels),
        ("word/document.xml", document.as_str()),
        ("word/styles.xml", styles.as_str()),
    ] {
        z.add(name, data.as_bytes(), craft_zip::Compression::Deflated).unwrap();
    }
    z.finish().unwrap()
}

fn paragraph(d: &Document) -> &Paragraph {
    d.body.iter().find_map(|b| b.as_para()).unwrap()
}

/// `runs` without run direction and bidi language (in the formatting change too), adjacent equal
/// runs merged.
fn undirected(runs: &[Run]) -> Vec<Run> {
    let strip = |c: &mut CharProps| {
        c.rtl = None;
        c.lang_bidi = None;
    };
    let mut out: Vec<Run> = Vec::new();
    for r in runs {
        let mut props = r.props.clone();
        strip(&mut props);
        if let Some(ch) = props.fmt_change.as_mut() {
            strip(&mut ch.old);
        }
        match out.last_mut() {
            Some(last) if last.props == props => last.len += r.len,
            _ => out.push(Run { len: r.len, props }),
        }
    }
    out
}

/// Each character of the first paragraph with the font, size, bold and italic it is drawn with:
/// the complex-script ones for complex-script letters and every character of a `w:rtl` or `w:cs`
/// run, the others for the rest (ECMA-376 §17.3.2.26, .30), as layout picks them.
fn drawn(d: &Document) -> Vec<(char, String, f32, bool, bool)> {
    let p = paragraph(d);
    let mut out = Vec::new();
    for (range, props) in p.run_ranges() {
        let rc = d.styles.resolve_char(p.props.style.as_deref(), props);
        for ch in p.text.get(range).unwrap_or("").chars() {
            let c = if rc.uses_complex(ch) { rc.complex() } else { rc.clone() };
            out.push((ch, c.font, c.size, c.bold, c.italic));
        }
    }
    out
}

/// `runs` without their complex-script font, size, bold and italic, adjacent equal runs merged:
/// what a save may say differently while every character is drawn as before ([`drawn`]).
fn without_complex_values(runs: &[Run]) -> Vec<Run> {
    let mut out: Vec<Run> = Vec::new();
    for r in runs {
        let mut props = r.props.clone();
        (props.font_cs, props.size_cs, props.bold_cs, props.italic_cs) = (None, None, None, None);
        match out.last_mut() {
            Some(last) if last.props == props => last.len += r.len,
            _ => out.push(Run { len: r.len, props }),
        }
    }
    out
}

fn check_generations(bytes: &[u8], check: impl Fn(&Document)) {
    let mut d = wordcraft_docx::read(bytes).unwrap();
    for generation in 0..3 {
        check(&d);
        let next = wordcraft_docx::read(&wordcraft_docx::write(&d).unwrap()).unwrap();
        assert_eq!(paragraph(&next).text, paragraph(&d).text);
        if generation == 0 {
            // The first save splits the mixed Hebrew/Latin run by direction and names the
            // Hebrew's language (#1543), and the neutral characters it marks right to left say
            // the Latin values they are drawn with as complex-script ones; apart from that
            // nothing changes, and every character is drawn as before.
            assert_eq!(without_complex_values(&undirected(&paragraph(&next).runs)), without_complex_values(&undirected(&paragraph(&d).runs)));
            assert_eq!(drawn(&next), drawn(&d));
        } else {
            assert_eq!(paragraph(&next).runs, paragraph(&d).runs);
        }
        assert_eq!(paragraph(&next).props.bidi, Some(true));
        d = next;
    }
}

#[test]
fn separate_hebrew_font_size_language_and_flags_survive() {
    let run = r#"<w:rFonts w:ascii="Source Serif 4" w:hAnsi="Source Serif 4" w:cs="Arial Hebrew"/><w:b w:val="0"/><w:bCs/><w:i w:val="0"/><w:iCs/><w:sz w:val="24"/><w:szCs w:val="48"/><w:rtl/><w:cs/><w:lang w:val="en-US" w:bidi="he-IL"/>"#;
    check_generations(&package(run, "", "", ""), |d| {
        let c = &paragraph(d).runs[0].props;
        assert_eq!((c.font.as_deref(), c.font_cs.as_deref()), (Some("Source Serif 4"), Some("Arial Hebrew")));
        assert_eq!((c.size, c.size_cs), (Some(12.0), Some(24.0)));
        assert_eq!((c.bold, c.bold_cs, c.italic, c.italic_cs), (Some(false), Some(true), Some(false), Some(true)));
        assert_eq!((c.rtl, c.cs), (Some(true), Some(true)));
        assert_eq!((c.lang.as_deref(), c.lang_bidi.as_deref()), (Some("en-US"), Some("he-IL")));
    });
}

#[test]
fn latin_direct_properties_leave_complex_script_defaults_inherited() {
    let defaults = r#"<w:szCs w:val="36"/><w:bCs/><w:iCs/>"#;
    let run = r#"<w:sz w:val="20"/><w:b w:val="0"/><w:i w:val="0"/>"#;
    check_generations(&package(run, defaults, "", ""), |d| {
        let p = paragraph(d);
        let c = &p.runs[0].props;
        assert_eq!((c.size_cs, c.bold_cs, c.italic_cs), (None, None, None), "Absent direct values must remain absent when saved");
        let resolved = d.styles.resolve_char(p.props.style.as_deref(), c);
        assert_eq!((resolved.size, resolved.bold, resolved.italic), (10.0, false, false));
        let cs = resolved.complex();
        assert_eq!((cs.size, cs.bold, cs.italic), (18.0, true, true));
    });
}

#[test]
fn latin_style_properties_leave_complex_script_style_chain_inherited() {
    let styles = r#"
<w:style w:type="paragraph" w:styleId="HebrewBase"><w:name w:val="HebrewBase"/><w:rPr><w:rFonts w:cs="Arial Hebrew"/><w:szCs w:val="36"/><w:bCs/><w:iCs/><w:lang w:bidi="he-IL"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="MixedChild"><w:name w:val="MixedChild"/><w:basedOn w:val="HebrewBase"/><w:rPr><w:sz w:val="24"/><w:b w:val="0"/><w:i w:val="0"/></w:rPr></w:style>
<w:style w:type="character" w:styleId="LatinOnly"><w:name w:val="LatinOnly"/><w:rPr><w:sz w:val="20"/><w:b w:val="0"/><w:i w:val="0"/></w:rPr></w:style>"#;
    let run = r#"<w:rStyle w:val="LatinOnly"/><w:rFonts w:ascii="Source Serif 4" w:hAnsi="Source Serif 4"/>"#;
    check_generations(&package(run, "", styles, r#"<w:pStyle w:val="MixedChild"/>"#), |d| {
        let p = paragraph(d);
        let c = d.styles.resolve_char(p.props.style.as_deref(), &p.runs[0].props);
        assert_eq!((c.font.as_str(), c.size, c.bold, c.italic), ("Source Serif 4", 10.0, false, false));
        let cs = c.complex();
        assert_eq!((cs.font.as_str(), cs.size, cs.bold, cs.italic), ("Arial Hebrew", 18.0, true, true));
        assert_eq!(d.styles.get("HebrewBase").unwrap().chr.lang_bidi.as_deref(), Some("he-IL"));
    });
}

#[test]
fn equal_explicit_complex_script_values_are_not_discarded() {
    let run = r#"<w:sz w:val="20"/><w:szCs w:val="20"/><w:b w:val="0"/><w:bCs w:val="0"/><w:i w:val="0"/><w:iCs w:val="0"/>"#;
    let defaults = r#"<w:szCs w:val="36"/><w:bCs/><w:iCs/>"#;
    check_generations(&package(run, defaults, "", ""), |d| {
        let p = paragraph(d);
        let c = &p.runs[0].props;
        assert_eq!((c.size_cs, c.bold_cs, c.italic_cs), (Some(10.0), Some(false), Some(false)));
        let cs = d.styles.resolve_char(p.props.style.as_deref(), c).complex();
        assert_eq!((cs.size, cs.bold, cs.italic), (10.0, false, false));
    });
}

#[test]
fn complex_script_direct_properties_leave_latin_defaults_inherited() {
    let run = r#"<w:szCs w:val="20"/><w:bCs w:val="0"/><w:iCs w:val="0"/><w:rtl w:val="0"/><w:cs w:val="0"/>"#;
    let defaults = r#"<w:sz w:val="36"/><w:b/><w:i/>"#;
    check_generations(&package(run, defaults, "", ""), |d| {
        let p = paragraph(d);
        let c = &p.runs[0].props;
        assert_eq!((c.size, c.bold, c.italic), (None, None, None));
        assert_eq!((c.rtl, c.cs), (Some(false), Some(false)));
        let resolved = d.styles.resolve_char(p.props.style.as_deref(), c);
        assert_eq!((resolved.size, resolved.bold, resolved.italic), (18.0, true, true));
        let cs = resolved.complex();
        assert_eq!((cs.size, cs.bold, cs.italic), (10.0, false, false));
    });
}

#[test]
fn tracked_formatting_keeps_independent_complex_script_values() {
    let defaults = r#"<w:szCs w:val="36"/><w:bCs/><w:iCs/>"#;
    let run = r#"<w:sz w:val="24"/><w:b w:val="0"/><w:i w:val="0"/><w:rPrChange w:id="7" w:author="Synthetic Author" w:date="2026-10-11T00:00:00Z"><w:rPr><w:sz w:val="20"/><w:szCs w:val="20"/><w:b w:val="0"/><w:bCs w:val="0"/><w:i w:val="0"/><w:iCs w:val="0"/></w:rPr></w:rPrChange>"#;
    check_generations(&package(run, defaults, "", ""), |d| {
        let p = paragraph(d);
        let c = &p.runs[0].props;
        let current = d.styles.resolve_char(None, c).complex();
        assert_eq!((current.size, current.bold, current.italic), (18.0, true, true));
        let change = c.fmt_change.as_ref().unwrap();
        assert_eq!((change.old.size_cs, change.old.bold_cs, change.old.italic_cs), (Some(10.0), Some(false), Some(false)));
        let old = d.styles.resolve_char(None, &change.old).complex();
        assert_eq!((old.size, old.bold, old.italic), (10.0, false, false));
        let mut overlaid = wordcraft_doc::props::CharProps::default();
        overlaid.overlay(c);
        assert_eq!(overlaid.fmt_change, c.fmt_change);
    });
}

/// `(w:rtl, w:lang)` elements of a run's current `w:rPr` and of its `w:rPrChange` "before" copy.
fn direction_markup(run: &str) -> (String, String) {
    let pick = |rpr: &str| -> String {
        rpr.split('<').filter(|t| t.starts_with("w:rtl") || t.starts_with("w:lang")).map(|t| format!("<{t}")).collect::<Vec<_>>().join("")
    };
    let (now, change) = run.split_once("<w:rPrChange").unwrap_or((run, ""));
    (pick(now), pick(change))
}

#[test]
fn split_runs_add_no_phantom_tracked_formatting() {
    // #1543: direction and language the writer adds to a piece of a run that has a tracked
    // formatting change go into the change's "before" copy too, so Word lists no change the
    // author never made ("Formatted: Right-to-left, Hebrew").
    let run = r#"<w:sz w:val="24"/><w:rPrChange w:id="7" w:author="Synthetic Author" w:date="2026-10-11T00:00:00Z"><w:rPr><w:sz w:val="20"/></w:rPr></w:rPrChange>"#;
    let d = wordcraft_docx::read(&package(run, "", "", "")).unwrap();
    let bytes = wordcraft_docx::write(&d).unwrap();
    let xml = String::from_utf8(
        craft_zip::Archive::open(&bytes, craft_zip::Limits { max_entries: 10_000, max_total_bytes: 64 << 20 })
            .unwrap()
            .read("word/document.xml", 16 << 20)
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    let runs: Vec<&str> = xml.split("<w:r>").skip(1).map(|r| r.split("</w:r>").next().unwrap()).collect();
    assert!(runs.len() >= 2, "the mixed run is split: {xml}");
    assert!(runs.iter().any(|r| r.contains("<w:rtl/>")), "{xml}");
    for r in runs {
        assert!(r.contains("<w:rPrChange"), "every piece keeps the tracked change: {r}");
        let (now, before) = direction_markup(r);
        assert_eq!(now, before, "no direction or language difference to report: {r}");
    }
    // And WordCraft reads no difference back either.
    let r = wordcraft_docx::read(&bytes).unwrap();
    for run in &paragraph(&r).runs {
        let old = &run.props.fmt_change.as_ref().unwrap().old;
        assert_eq!((old.rtl, old.lang_bidi.as_deref()), (run.props.rtl, run.props.lang_bidi.as_deref()));
    }
}

/// RTL-11: a run that sets only the Latin bold, italic and size (`w:b`, `w:i`, `w:sz`, as a
/// Latin-only generator writes it) formats its Latin text alone: its Hebrew letters stay regular
/// and upright at the default size, 10 pt when the document defaults set no `w:szCs` (as no
/// `w:sz` gives 10 pt), as Word shows them. So it stays through saves.
#[test]
fn latin_only_bold_italic_and_size_leave_hebrew_letters_plain() {
    let run = r#"<w:b/><w:i/><w:sz w:val="40"/>"#;
    check_generations(&package(run, "", "", ""), |d| {
        let p = paragraph(d);
        let c = d.styles.resolve_char(p.props.style.as_deref(), &p.runs[0].props);
        assert_eq!((c.bold, c.italic, c.size), (true, true, 20.0), "the Latin text");
        let cs = c.complex();
        assert_eq!((cs.bold, cs.italic, cs.size), (false, false, 10.0), "the Hebrew letters");
    });
    // Sizes in the document defaults: the Hebrew letters take the complex-script one only.
    check_generations(&package(run, r#"<w:sz w:val="24"/><w:szCs w:val="28"/>"#, "", ""), |d| {
        let p = paragraph(d);
        let cs = d.styles.resolve_char(p.props.style.as_deref(), &p.runs[0].props).complex();
        assert_eq!((cs.bold, cs.size), (false, 14.0));
    });
}

/// RTL-11, ECMA-376 §17.3.2.26: in a run with only `w:b` and `w:sz` the spaces and punctuation
/// between Hebrew words are bold at the run's size and the Hebrew letters regular at the
/// complex-script size, as Word shows them; and so after saves, though the writer saves the
/// right-to-left pieces as `w:rtl` runs (#1543): they say the Latin values as complex-script ones.
/// A `w:rtl` run formats every character with the complex-script values.
#[test]
fn neutral_characters_between_hebrew_words_keep_the_latin_bold_and_size_through_saves() {
    let text = "שלום עולם, (טקסט) נוסף.";
    let neutral = |ch: char| " ,().".contains(ch);
    let plain = package_text(r#"<w:b/><w:sz w:val="32"/>"#, r#"<w:sz w:val="24"/>"#, "", "", text);
    check_generations(&plain, |d| {
        for (ch, _, size, bold, _) in drawn(d) {
            let want = if neutral(ch) { (true, 16.0) } else { (false, 10.0) };
            assert_eq!((bold, size), want, "{ch:?}");
        }
    });
    let rtl = package_text(r#"<w:b/><w:sz w:val="32"/><w:rtl/>"#, r#"<w:sz w:val="24"/>"#, "", "", text);
    check_generations(&rtl, |d| {
        for (ch, _, size, bold, _) in drawn(d) {
            assert_eq!((bold, size), (false, 10.0), "{ch:?} of a w:rtl run");
        }
    });
}
