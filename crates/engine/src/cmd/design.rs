//! Design tab: themes, style sets, colours, fonts, paragraph spacing, watermark, page colour, borders.

use serde_json::{Value, json};
use std::sync::Arc;
use wordcraft_doc::props::{Border, BorderStyle, Borders, CharProps, LineSpacing, Rgb, TextColor};
use wordcraft_doc::{Watermark, styles};

use super::sel_result;
use crate::{CmdError, CmdResult, CommandSpec, Session, p};

/// Our own themes: (name, heading font, body font, accent colours).
pub const THEMES: &[(&str, &str, &str, [Rgb; 6])] = &[
    (
        "Craft",
        styles::HEADING_FONT,
        styles::BODY_FONT,
        [Rgb(0x15, 0x60, 0x82), Rgb(0xE9, 0x71, 0x32), Rgb(0x19, 0x6B, 0x24), Rgb(0x0F, 0x9E, 0xD5), Rgb(0xA0, 0x2B, 0x93), Rgb(0x4E, 0xA7, 0x2E)],
    ),
    (
        "Studio",
        "Georgia",
        "Georgia",
        [Rgb(0x8C, 0x2F, 0x39), Rgb(0xC9, 0x8B, 0x2E), Rgb(0x3E, 0x5C, 0x76), Rgb(0x6B, 0x8F, 0x71), Rgb(0x9A, 0x6F, 0xB0), Rgb(0x4A, 0x4A, 0x4A)],
    ),
    (
        "Gallery",
        "Helvetica Neue",
        "Helvetica Neue",
        [Rgb(0x22, 0x22, 0x22), Rgb(0xE6, 0x39, 0x46), Rgb(0x45, 0x7B, 0x9D), Rgb(0x1D, 0x35, 0x57), Rgb(0xA8, 0xDA, 0xDC), Rgb(0xF4, 0xA2, 0x61)],
    ),
    (
        "Atelier",
        "Palatino",
        "Palatino",
        [Rgb(0x6D, 0x59, 0x7A), Rgb(0xB5, 0x65, 0x76), Rgb(0xE5, 0x6B, 0x6F), Rgb(0xEA, 0xAC, 0x8B), Rgb(0x35, 0x50, 0x70), Rgb(0x8E, 0x9A, 0xAF)],
    ),
    (
        "Darkroom",
        "Futura",
        "Avenir Next",
        [Rgb(0x26, 0x46, 0x53), Rgb(0x2A, 0x9D, 0x8F), Rgb(0xE9, 0xC4, 0x6A), Rgb(0xF4, 0xA2, 0x61), Rgb(0xE7, 0x6F, 0x51), Rgb(0x5E, 0x54, 0x8E)],
    ),
    (
        "Sketchbook",
        "Gill Sans",
        "Gill Sans",
        [Rgb(0x58, 0x81, 0x57), Rgb(0x3A, 0x5A, 0x40), Rgb(0xA3, 0xB1, 0x8A), Rgb(0xDA, 0xD7, 0xCD), Rgb(0x34, 0x4E, 0x41), Rgb(0xBC, 0x6C, 0x25)],
    ),
];

/// The Hebrew fonts of each theme's font set, (theme, heading, body): Word's Hebrew families,
/// which Hebrew Windows has and WordCraft draws in open look-alikes when they are missing, each
/// of the kind of the set's Latin fonts. A document whose complex-script text is Hebrew gets them
/// with the set (Design › Themes and Design › Fonts); the default set keeps David, the Hebrew
/// body font of a new right-to-left document, for both.
pub const HEBREW_FONTS: &[(&str, &str, &str)] = &[
    ("Craft", styles::HEBREW_BODY_FONT, styles::HEBREW_BODY_FONT),
    ("Studio", "FrankRuehl", "FrankRuehl"),
    ("Gallery", "Arial", "Arial"),
    ("Atelier", "Narkisim", "Narkisim"),
    ("Darkroom", "Aharoni", "Gisha"),
    ("Sketchbook", "Miriam", "Miriam"),
];

/// The Hebrew (heading, body) fonts of the theme named `name` (any letter case).
pub fn hebrew_fonts(name: &str) -> Option<(&'static str, &'static str)> {
    HEBREW_FONTS.iter().find(|t| t.0.eq_ignore_ascii_case(name)).map(|t| (t.1, t.2))
}

/// Whether the complex-script text of `doc` is Hebrew: the language (`w:lang/@w:bidi`) its Normal
/// style, else its document defaults, give that text.
pub fn hebrew_document(doc: &wordcraft_doc::Document) -> bool {
    let sheet = &doc.styles;
    let normal = sheet.styles.iter().find(|st| st.id == "Normal").and_then(|st| st.chr.lang_bidi.as_deref());
    wordcraft_doc::words::is_hebrew(normal.or(sheet.default_chr.lang_bidi.as_deref()))
}

/// Longest font name taken, in characters.
const MAX_FONT_NAME_CHARS: usize = 128;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("design.theme", "Themes", "Design › Document Formatting", theme)
            .params(r#"{"name": string} — a document whose complex-script text is Hebrew also gets the theme's Hebrew fonts"#),
        CommandSpec::new("design.themeFonts", "Fonts", "Design › Document Formatting", theme_fonts).params(
            r#"{"heading": string, "body"?: string, "headingCs"?: string, "bodyCs"?: string (the fonts of Hebrew and other complex-script text; without them, in a document whose complex-script text is Hebrew, a heading and body pair that is a theme's set brings that theme's Hebrew fonts)} — returns "complexScriptFonts": {"heading", "body"} when those changed, else null"#,
        ),
        CommandSpec::new("design.themeColors", "Colors", "Design › Document Formatting", |s, v| {
            let name = p::req_str(v, "name")?;
            let t = THEMES.iter().find(|t| t.0.eq_ignore_ascii_case(name)).ok_or_else(|| CmdError::Params(format!("unknown colours `{name}`")))?;
            set_accent(s, t.3[0]);
            sel_result(s)
        })
        .params(r#"{"name": string (see design.themes)}"#),
        CommandSpec::new("design.styleSet", "Style Set", "Design › Document Formatting", style_set)
            .params(r#"{"name": "default|basic|lines|shaded|casual|centered|minimalist|title"}"#),
        CommandSpec::new("design.paragraphSpacing", "Paragraph Spacing", "Design › Document Formatting", para_spacing)
            .params(r#"{"value": "default|none|compact|tight|open|relaxed|double"}"#),
        CommandSpec::new("design.scaleFontSizes", "Scale Font Sizes", "Design › Document Formatting", scale_font_sizes)
            .params(r#"{"delta": pt (-1638..1638, not 0) | "percent": number (-99..1000, not 0; 10 = +10%)} (exactly one)"#),
        CommandSpec::new("design.watermark", "Watermark", "Design › Page Background", watermark)
            .params(r#"{"text"?: string (default: the watermark's own, or for a new one DRAFT in the language of the text at the caret), "remove"?: bool, "diagonal"?: bool, "color"?: "RRGGBB"}"#),
        CommandSpec::new("design.pageColor", "Page Color", "Design › Page Background", |s, v| {
            s.doc.settings.page_color = p::str(v, "color").and_then(Rgb::parse);
            s.doc.settings.page_picture = None;
            sel_result(s)
        })
        .params(r#"{"color": "RRGGBB" | null}"#),
        CommandSpec::new("design.pageBorders", "Page Borders", "Design › Page Background", page_borders).params(
            r#"{"kind"?: "box|none", "width"?: pt, "color"?: "RRGGBB", "style"?: "single|double|dotted|dashed|thick|triple|dotDash|wave", "sides"?: {"top"|"left"|"bottom"|"right": {"style"?, "width"?: pt, "color"?: "RRGGBB"} | null} (exactly these sides, instead of `kind`), "applyTo"?: "section|document"}"#,
        ),
        CommandSpec::new("design.setDefault", "Set as Default", "Design › Document Formatting", |s, _| {
            s.status = "These settings will be used for new documents.".into();
            sel_result(s)
        })
        .pure(),
        CommandSpec::new("design.themes", "Theme List", "Design › Document Formatting", |_, _| {
            Ok(json!(THEMES.iter().map(|t| t.0).collect::<Vec<_>>()))
        })
        .pure(),
    ]
}

/// Design › Fonts: the heading and body fonts, and those of complex-script text when given or,
/// in a Hebrew document, when the Latin pair is one of the themes' sets.
fn theme_fonts(s: &mut Session, v: &Value) -> CmdResult {
    let heading = p::req_str(v, "heading")?.to_string();
    let body = p::str(v, "body").unwrap_or(&heading).to_string();
    let cs_name = |k: &str| -> Result<Option<String>, CmdError> {
        match v.get(k) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(n)) if !n.trim().is_empty() && n.chars().count() <= MAX_FONT_NAME_CHARS && !n.chars().any(char::is_control) => {
                Ok(Some(n.trim().to_string()))
            }
            Some(_) => Err(CmdError::Params(format!("`{k}` must be a font name of 1 to {MAX_FONT_NAME_CHARS} characters"))),
        }
    };
    let cs = match (cs_name("headingCs")?, cs_name("bodyCs")?) {
        // A theme's set in a Hebrew document brings its Hebrew fonts.
        (None, None) => hebrew_document(&s.doc)
            .then(|| THEMES.iter().find(|t| t.1.eq_ignore_ascii_case(&heading) && t.2.eq_ignore_ascii_case(&body)))
            .flatten()
            .and_then(|t| hebrew_fonts(t.0))
            .map(|(h, b)| (h.to_string(), b.to_string())),
        // One given stands for both, as `body` does for the Latin pair.
        (h, b) => h.clone().or(b.clone()).zip(b.or(h)),
    };
    let cs = cs.as_ref().map(|(h, b)| (h.as_str(), b.as_str()));
    set_fonts(s, &heading, &body, cs);
    fonts_result(s, cs)
}

/// [`sel_result`] with the complex-script fonts the command set, or null when it left them.
fn fonts_result(s: &Session, cs: Option<(&str, &str)>) -> CmdResult {
    let mut r = sel_result(s)?;
    if let Some(o) = r.as_object_mut() {
        o.insert("complexScriptFonts".into(), cs.map_or(Value::Null, |(h, b)| json!({"heading": h, "body": b})));
    }
    Ok(r)
}

/// Whether a style takes the heading fonts: the headings (and their linked character styles) and
/// the title.
fn heading_style(st: &styles::Style) -> bool {
    st.id.starts_with("Heading") || st.id == "Title"
}

/// The theme's fonts: `heading` and `body` for Latin text and, when given, `cs` (heading, body)
/// for Hebrew and other complex-script text. The document defaults take the body fonts. A style
/// with a font of its own takes the heading font when it is a heading and the body font
/// otherwise; the headings take the complex-script heading font even without one of their own,
/// since they would otherwise show the body's.
fn set_fonts(s: &mut Session, heading: &str, body: &str, cs: Option<(&str, &str)>) {
    s.doc.settings.major_font = heading.into();
    s.doc.settings.minor_font = body.into();
    let sheet = Arc::make_mut(&mut s.doc.styles);
    sheet.default_chr.font = Some(body.into());
    if let Some((_, cs_body)) = cs {
        sheet.default_chr.font_cs = Some(cs_body.into());
    }
    for st in &mut sheet.styles {
        let is_heading = heading_style(st);
        if let Some(f) = &st.chr.font
            && (f == styles::HEADING_FONT || is_heading)
        {
            st.chr.font = Some(heading.into());
        } else if st.chr.font.is_some() && st.chr.font.as_deref() != Some(heading) {
            st.chr.font = Some(body.into());
        }
        if let Some((cs_heading, cs_body)) = cs {
            if is_heading {
                st.chr.font_cs = Some(cs_heading.into());
            } else if st.chr.font_cs.is_some() && st.chr.font_cs.as_deref() != Some(cs_heading) {
                st.chr.font_cs = Some(cs_body.into());
            }
        }
    }
}

fn set_accent(s: &mut Session, accent: Rgb) {
    for st in &mut Arc::make_mut(&mut s.doc.styles).styles {
        if st.id.starts_with("Heading") && st.id.len() == 8 && st.id.as_bytes().get(7).is_some_and(|d| *d <= b'5')
            || st.id == "IntenseQuote"
            || st.id == "IntenseEmphasis"
            || st.id == "IntenseReference"
        {
            st.chr.color = Some(TextColor::Rgb(accent));
            if let Some(b) = st.para.borders.as_mut() {
                for e in [&mut b.top, &mut b.bottom].into_iter().flatten() {
                    e.color = Some(accent);
                }
            }
        }
    }
    if let Some(c) = s.doc.settings.theme_colors.get_mut(4) {
        *c = accent;
    }
}

fn theme(s: &mut Session, v: &Value) -> CmdResult {
    let name = p::req_str(v, "name")?;
    let t = THEMES.iter().find(|t| t.0.eq_ignore_ascii_case(name)).ok_or_else(|| CmdError::Params(format!("unknown theme `{name}`")))?;
    let cs = hebrew_document(&s.doc).then(|| hebrew_fonts(t.0)).flatten();
    set_fonts(s, t.1, t.2, cs);
    set_accent(s, t.3[0]);
    s.doc.settings.theme_name = t.0.into();
    let mut colors = s.doc.settings.theme_colors.clone();
    for (i, c) in t.3.iter().enumerate() {
        if let Some(x) = colors.get_mut(4 + i) {
            *x = *c;
        }
    }
    s.doc.settings.theme_colors = colors;
    fonts_result(s, cs)
}

fn style_set(s: &mut Session, v: &Value) -> CmdResult {
    let name = p::req_str(v, "name")?;
    let accent = s.doc.settings.theme_colors.get(4).copied().unwrap_or(styles::HEADING_BLUE);
    // The styles' complex-script fonts (a theme's Hebrew fonts among them): the set's fresh styles
    // have none, so each gets its own back below, as the Latin ones get the theme's.
    let fonts_cs: Vec<(String, String)> = s.doc.styles.styles.iter().filter_map(|st| Some((st.id.clone(), st.chr.font_cs.clone()?))).collect();
    let mut fresh = wordcraft_doc::StyleSheet::builtin();
    let line = Border { style: BorderStyle::Single, width: 0.75, color: Some(accent), space: 1.0 };
    for st in &mut fresh.styles {
        let heading = st.id.starts_with("Heading") || st.id == "Title";
        match name {
            "basic" if heading => {
                st.chr.color = Some(TextColor::Rgb(Rgb(0x26, 0x26, 0x26)));
                st.chr.bold = Some(true);
                st.chr.bold_cs = Some(true);
            }
            "lines" if st.id == "Heading1" || st.id == "Title" => st.para.borders = Some(Borders { bottom: Some(line), ..Default::default() }),
            "shaded" if st.id == "Heading1" => {
                st.para.shading = Some(accent);
                st.chr.color = Some(TextColor::Rgb(Rgb::WHITE));
            }
            "casual" if heading => {
                st.chr.italic = Some(true);
                st.chr.italic_cs = Some(true);
            }
            "centered" if heading => st.para.align = Some(wordcraft_doc::Align::Center),
            "minimalist" if heading => {
                st.chr.caps = Some(true);
                st.chr.spacing = Some(1.0);
                st.chr.color = Some(TextColor::Rgb(Rgb(0x40, 0x40, 0x40)));
            }
            "title" if st.id == "Title" => {
                st.chr.size = Some(36.0);
                st.chr.size_cs = Some(36.0);
            }
            _ => {}
        }
    }
    // Overlay the set onto the document's sheet: every other style, the latent styles and the
    // document defaults stay as they are (no set defines its own defaults).
    let sheet = Arc::make_mut(&mut s.doc.styles);
    for mut st in fresh.styles {
        match sheet.styles.iter_mut().find(|x| x.id == st.id) {
            Some(old) => {
                st.default = old.default;
                st.locked = old.locked;
                st.builtin = old.builtin;
                *old = st;
            }
            None => sheet.styles.push(st),
        }
    }
    let (h, b) = (s.doc.settings.major_font.clone(), s.doc.settings.minor_font.clone());
    set_fonts(s, &h, &b, None);
    let sheet = Arc::make_mut(&mut s.doc.styles);
    for (id, f) in fonts_cs {
        if let Some(st) = sheet.styles.iter_mut().find(|st| st.id == id) {
            st.chr.font_cs = Some(f);
        }
    }
    set_accent(s, accent);
    sel_result(s)
}

fn para_spacing(s: &mut Session, v: &Value) -> CmdResult {
    let (before, after, line) = match p::req_str(v, "value")? {
        "none" => (0.0, 0.0, 1.0),
        "compact" => (0.0, 4.0, 1.0),
        "tight" => (0.0, 6.0, 1.15),
        "open" => (0.0, 10.0, 1.15),
        "relaxed" => (0.0, 6.0, 1.5),
        "double" => (0.0, 8.0, 2.0),
        _ => (0.0, 8.0, 1.15),
    };
    Arc::make_mut(&mut s.doc.styles).default_para.space_before = Some(before);
    Arc::make_mut(&mut s.doc.styles).default_para.space_after = Some(after);
    Arc::make_mut(&mut s.doc.styles).default_para.line_spacing = Some(LineSpacing::Multiple(line));
    sel_result(s)
}

/// Grow or shrink every style's font size at once. Only explicit style sizes change (including
/// table style parts); styles that inherit one (via `based_on` or the defaults) follow, and runs
/// keep their own direct formatting.
fn scale_font_sizes(s: &mut Session, v: &Value) -> CmdResult {
    let (delta, percent) = (p::f32(v, "delta"), p::f32(v, "percent"));
    // A field that is present but not a number is an error, not "absent".
    if (v.get("delta").is_some() && delta.is_none()) || (v.get("percent").is_some() && percent.is_none()) {
        return Err(CmdError::Params("`delta` and `percent` must be numbers".into()));
    }
    if delta.is_some() == percent.is_some() {
        return Err(CmdError::Params("give exactly one of `delta` (points) or `percent`".into()));
    }
    if delta.is_some_and(|d| !(-1638.0..=1638.0).contains(&d) || d == 0.0) || percent.is_some_and(|x| !(-99.0..=1000.0).contains(&x) || x == 0.0) {
        return Err(CmdError::Params("`delta` must be non-zero within ±1638 pt and `percent` non-zero within -99..1000".into()));
    }
    let factor = 1.0 + percent.unwrap_or(0.0) / 100.0;
    let scale = |size: f32| ((size * factor + delta.unwrap_or(0.0)).clamp(1.0, 1638.0) * 2.0).round() / 2.0;
    let scale_chr = |c: &mut CharProps| {
        c.size = c.size.map(scale);
        c.size_cs = c.size_cs.map(scale);
    };
    let st = Arc::make_mut(&mut s.doc.styles);
    // An unset document default resolves to the fallback size; make it explicit so inheriting styles follow.
    st.default_chr.size = Some(scale(st.default_chr.size.unwrap_or(wordcraft_doc::props::DEFAULT_FONT_SIZE)));
    st.default_chr.size_cs = st.default_chr.size_cs.map(scale);
    for style in &mut st.styles {
        scale_chr(&mut style.chr);
        if let Some(t) = style.table.as_mut() {
            for c in [&mut t.header_chr, &mut t.band_chr, &mut t.first_col_chr, &mut t.last_col_chr, &mut t.total_chr, &mut t.col_band_chr] {
                scale_chr(c);
            }
            for part in t.regions.values_mut() {
                scale_chr(&mut part.chr);
            }
        }
    }
    sel_result(s)
}

fn watermark(s: &mut Session, v: &Value) -> CmdResult {
    if p::bool(v, "remove").unwrap_or(false) {
        s.doc.settings.watermark = None;
        return sel_result(s);
    }
    // A new watermark without text of its own says DRAFT in the language of the text at the caret
    // (טיוטה in Hebrew text), as Design › Watermark's first word does.
    let mut wm = match s.doc.settings.watermark.clone() {
        Some(wm) => wm,
        None => Watermark { text: super::words::word(super::words::caret_lang(s).as_deref(), "DRAFT").into(), ..Default::default() },
    };
    if let Some(t) = p::str(v, "text") {
        wm.text = t.chars().take(200).collect();
    }
    if let Some(d) = p::bool(v, "diagonal") {
        wm.diagonal = d;
    }
    if let Some(c) = p::str(v, "color").and_then(Rgb::parse) {
        wm.color = c;
    }
    s.doc.settings.watermark = Some(Watermark { ..wm });
    sel_result(s)
}

/// Page borders of the caret's section (or every section with `applyTo: "document"`).
fn page_borders(s: &mut Session, v: &Value) -> CmdResult {
    let borders = match v.get("sides") {
        Some(sides) => Some(super::para::border_sides(sides, 24.0, 24.0)?).filter(Borders::any_visible),
        None => {
            let kind = p::str(v, "kind").unwrap_or("box");
            let width = p::f32(v, "width").unwrap_or(1.0).clamp(0.25, 6.0);
            let color = p::str(v, "color").and_then(Rgb::parse);
            let style = p::str(v, "style").map(BorderStyle::from_ooxml).unwrap_or(BorderStyle::Single);
            (kind != "none").then(|| Borders::box_(Border { style, width, color, space: 24.0 }))
        }
    };
    let blocks: Vec<usize> = match p::str(v, "applyTo") {
        Some("document") => s.doc.sections().iter().map(|(end, _)| *end).collect(),
        Some("section") | None => vec![s.sel.focus.path.0.first().copied().unwrap_or(0) as usize],
        Some(x) => return Err(CmdError::Params(format!("unknown `applyTo` `{x}`"))),
    };
    for block in blocks {
        s.doc.section_mut(block).page_borders = borders;
    }
    sel_result(s)
}
