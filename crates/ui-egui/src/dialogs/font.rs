use super::{Dialog, Ui, Value, WordApp, json, vec2};

/// The Font dialog preview's size: the Size box's points between 6 and 72, else 12. `"NaN"` and
/// `"inf"` parse as floats too, and a NaN font size trips epaint's glyph assertion (#3663).
pub(super) fn preview_size(size: &str) -> f32 {
    size.trim().parse::<f32>().ok().filter(|pt| pt.is_finite()).unwrap_or(12.0).clamp(6.0, 72.0)
}

/// The page colour the sample sits on.
pub(super) fn sample_paper(app: &WordApp) -> wordcraft_doc::Rgb {
    app.session.doc.settings.page_color.unwrap_or(wordcraft_doc::Rgb(255, 255, 255))
}

const SAMPLE_WIDTH: f32 = 300.0;

/// The sample's character properties: the dialog's fields as the document reads them.
#[allow(clippy::too_many_arguments)]
pub(super) fn font_sample_props(
    app: &mut WordApp,
    paper: wordcraft_doc::Rgb,
    font: &str,
    size: &str,
    color: &str,
    (bold, italic, underline, strike): (bool, bool, bool, bool),
    (small_caps, caps, hidden): (bool, bool, bool),
    spacing: f32,
) -> wordcraft_doc::props::CharProps {
    use wordcraft_doc::props::{CharProps, TextColor, Underline};
    // Automatic text is black, or white on a dark page.
    let dark = u32::from(paper.0) * 299 + u32::from(paper.1) * 587 + u32::from(paper.2) * 114 < 128_000;
    let auto = if dark { 255 } else { 0 };
    let ink = wordcraft_doc::Rgb::parse(color).unwrap_or(wordcraft_doc::Rgb(auto, auto, auto));
    // A hidden run is dotted, as Word draws hidden text when it is shown.
    let line = if underline {
        Underline::Single
    } else if hidden {
        Underline::Dotted
    } else {
        Underline::None
    };
    CharProps {
        font: Some(app.previews.font_stored(font)),
        size: Some(preview_size(size).min(40.0)),
        bold: Some(bold),
        italic: Some(italic),
        underline: Some(line),
        strike: Some(strike),
        color: Some(TextColor::Rgb(ink)),
        caps: Some(caps),
        small_caps: Some(small_caps),
        spacing: Some(spacing),
        ..Default::default()
    }
}

/// Draws the Font dialog's sample. A raised or lowered "2" follows the text
/// when Superscript or Subscript is on.
pub(super) fn font_sample_box(
    app: &mut WordApp,
    ui: &mut Ui,
    paper: wordcraft_doc::Rgb,
    props: &wordcraft_doc::props::CharProps,
    (sup, sub): (bool, bool),
    hidden: bool,
) {
    use wordcraft_doc::props::VertAlign;
    let dark = app.session.view.dark_mode;
    let mut render_options = crate::canvas::screen_render_options();
    render_options.dark = dark;
    render_options.dark_paper = wordcraft_render::DARK_PAPER;
    let display_paper = render_options.paper_of(&app.session.doc);
    let mark = if sup {
        Some(VertAlign::Superscript)
    } else if sub {
        Some(VertAlign::Subscript)
    } else {
        None
    };
    // A fixed width: sizing it from the dialog's width would grow the dialog frame by frame.
    let (w, h) = (SAMPLE_WIDTH, 56.0);
    let ppp = ui.ctx().pixels_per_point();
    let text = tl!("AaBbYyZz");
    let key = format!("fontsample:{text}:{props:?}:{mark:?}:{paper:?}:{display_paper:?}:{dark}:{w}:{ppp}");
    let tex = app.previews.get_or(ui.ctx(), &key, || crate::previews::font_sample(text, props, mark, (w, h, ppp), paper, dark));
    let (r, _) = ui.allocate_exact_size(vec2(w + 4.0, h + 2.0), egui::Sense::hover());
    let t = crate::theme::Tokens::get(ui.ctx());
    let page = crate::theme::c32(display_paper);
    ui.painter().rect(r, 6.0, page, egui::Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
    if let Some(tex) = tex {
        let full = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        ui.painter().image(tex.id(), egui::Rect::from_min_size(r.min, tex.size_vec2() / ppp), full, egui::Color32::WHITE);
    }
    if hidden {
        ui.label(egui::RichText::new(tl!("Hidden text is not shown unless you display it.")).small().weak());
    }
}

/// One of the Font dialog's font boxes: type to find a font, with the missing-font warning
/// `shown_as` (the families drawn instead of `current`, see [`WordApp::font_dialog_warning`]).
pub(super) fn font_box(
    ui: &mut egui::Ui,
    (id, current): (&str, &str),
    items: &[String],
    preview: &dyn Fn(&mut egui::Ui, &str) -> egui::Response,
    shown_as: Option<&str>,
) -> Option<String> {
    let options =
        crate::widgets::FontComboOptions { id, width: 220.0, current, items, recent: &[], preview: Some(preview), tip: tl!("Font"), shown_as };
    crate::widgets::font_combo_with_recent_hint(ui, options)
}

/// The character properties the Font dialog's fields stand for.
pub(super) fn font_props(d: &Dialog) -> serde_json::Map<String, Value> {
    let Dialog::Font {
        font,
        font_ea,
        font_cs,
        size_cs,
        complex,
        size,
        bold,
        italic,
        underline,
        strike,
        sup,
        sub,
        small_caps,
        caps,
        hidden,
        color,
        spacing,
        kern,
        kern_at,
        ..
    } = d
    else {
        return serde_json::Map::new();
    };
    let mut props = serde_json::Map::new();
    props.insert("bold".into(), json!(*bold));
    props.insert("italic".into(), json!(*italic));
    props.insert("strike".into(), json!(*strike));
    props.insert("underline".into(), json!(if *underline { "single" } else { "none" }));
    let vert = if *sup {
        "superscript"
    } else if *sub {
        "subscript"
    } else {
        "baseline"
    };
    props.insert("vertAlign".into(), json!(vert));
    props.insert("smallCaps".into(), json!(*small_caps));
    props.insert("caps".into(), json!(*caps));
    props.insert("hidden".into(), json!(*hidden));
    props.insert("spacing".into(), json!(*spacing));
    match kern {
        Some(true) => {
            props.insert("kern".into(), json!(kern_at.clamp(1.0, 1638.0)));
        }
        Some(false) => {
            props.insert("kern".into(), json!(0.0));
        }
        None => {}
    }
    if !font.trim().is_empty() {
        props.insert("font".into(), json!(font.trim()));
    }
    if !font_ea.trim().is_empty() {
        props.insert("fontEa".into(), json!(font_ea.trim()));
    }
    if let Ok(s) = size.trim().parse::<f64>() {
        props.insert("size".into(), json!(s));
    }
    // The complex script row: Hebrew, Arabic… text's own font and size.
    if *complex {
        if !font_cs.trim().is_empty() {
            props.insert("fontCs".into(), json!(font_cs.trim()));
        }
        if let Ok(s) = size_cs.trim().parse::<f64>() {
            props.insert("sizeCs".into(), json!(s));
        }
    }
    if let Some(c) = wordcraft_doc::Rgb::parse(color) {
        props.insert("color".into(), json!({"Rgb": [c.0, c.1, c.2]}));
    } else if color.eq_ignore_ascii_case("auto") {
        // Automatic, from the colour field's drop-down (#606).
        props.insert("color".into(), json!("Auto"));
    }
    props
}

/// Is `c` Chinese, Japanese or Korean text (ideographs, kana, Hangul, CJK punctuation, fullwidth
/// forms)?
pub(super) fn east_asian_char(c: char) -> bool {
    matches!(u32::from(c), 0x1100..=0x11FF | 0x2E80..=0x9FFF | 0xA960..=0xA97F | 0xAC00..=0xD7FF | 0xF900..=0xFAFF | 0xFE30..=0xFE4F | 0xFF00..=0xFFEF | 0x20000..=0x3FFFF)
}

/// The Font dialog formatting Define New Bullet's bullet (#2786): it opens on the bullet's font,
/// size, bold, italic and colour, and carries the form back (see `Dialog::Font::bullet`).
pub fn font_for_bullet(app: &mut WordApp, form: crate::dialogs_bullet::DefineBulletForm) -> Option<Dialog> {
    let mut d = Dialog::open("font", app)?;
    if let Dialog::Font { font, font_was, size, bold, italic, color, complex, .. } = &mut d {
        // A bullet has one font and size.
        *complex = false;
        if !form.font.is_empty() {
            *font = app.previews.font_shown(&form.font);
            *font_was = font.clone();
        }
        *size = form.size.clone();
        *bold = form.bold;
        *italic = form.italic;
        *color = form.color.clone();
    }
    let props = font_props(&d);
    if let Dialog::Font { start, bullet, .. } = &mut d {
        *start = props;
        *bullet = Some(Box::new(form));
    }
    Some(d)
}

/// The Font dialog's OK or Cancel while it formats Define New Bullet's bullet (#2786): back to
/// that dialog, with the font (its stored English name, and only when the user changed it, so a
/// bullet in the text's font keeps following the text), size, bold, italic and colour on OK.
/// False when the dialog isn't formatting a bullet.
pub(super) fn hand_back_bullet(app: &mut WordApp, d: &mut Dialog, ok: bool) -> bool {
    let Dialog::Font { bullet, font, font_was, size, bold, italic, color, .. } = d else { return false };
    let Some(mut form) = bullet.take() else { return false };
    if ok {
        if font.trim() != font_was.trim() {
            form.font = app.previews.font_stored(font.trim());
        }
        form.size = size.trim().to_string();
        form.bold = *bold;
        form.italic = *italic;
        form.color = color.clone();
    }
    app.dialog = Some(Dialog::Insert(Box::new(crate::dialogs_insert::InsertDialog::DefineBullet(*form))));
    true
}

/// The Font dialog's OK: one formatting step that sets only the properties the user changed, so
/// formatting the dialog can't show or left alone stays as it is. Fonts are set by their stored
/// (English) names. With the complex script row shown, each script's font and size are set on
/// their own (Bold and Italic still format both, as the buttons do), so a new Latin text font
/// leaves Hebrew text's font as it is; without it the font is the Persian/Arabic text's font too
/// (as `format.font`), and a size sizes both.
pub(super) fn apply_font(app: &mut WordApp, d: &Dialog) {
    let Dialog::Font { start, complex, .. } = d else { return };
    let mut changed: serde_json::Map<String, Value> = font_props(d).into_iter().filter(|(k, v)| start.get(k) != Some(v)).collect();
    for key in ["font", "fontCs", "fontEa"] {
        if let Some(f) = changed.get(key).and_then(Value::as_str).map(str::to_string) {
            changed.insert(key.into(), json!(app.previews.font_stored(&f)));
        }
    }
    if !*complex && let Some(f) = changed.get("font").cloned() {
        changed.insert("fontCs".into(), f);
    }
    if changed.is_empty() {
        return;
    }
    if *complex {
        for (latin, cs) in [("bold", "boldCs"), ("italic", "italicCs")] {
            if let Some(v) = changed.get(latin).cloned() {
                changed.entry(cs).or_insert(v);
            }
        }
        let _ = app.run("format.set", json!({"props": changed, "pair": false}));
    } else {
        let _ = app.run("format.set", json!({"props": changed}));
    }
}

#[cfg(test)]
mod tests {
    use super::{Dialog, WordApp, apply_font, json};
    use crate::Services;
    use wordcraft_engine::Session;

    /// The Latin and complex-script font and size of the first paragraph's first letter.
    fn fonts(a: &WordApp) -> (String, String, f32, f32) {
        let s = &a.session;
        let p = s.doc.para_at(&wordcraft_doc::Pos::body(0, 0)).expect("a first paragraph");
        let rc = s.doc.styles.resolve_char(p.props.style.as_deref(), p.props_of_char(0));
        (rc.font, rc.font_cs, rc.size, rc.size_cs)
    }

    /// UWB-09, HE-02: in a Hebrew document the Font dialog has a complex script row showing
    /// Hebrew text's own font. A new Latin text font leaves that font as it is, and the row sets
    /// Hebrew text's font and size without touching the Latin ones.
    #[test]
    fn the_font_dialog_sets_the_hebrew_font_apart_from_the_latin_one() {
        let mut a = WordApp::new(Session::new(wordcraft_doc::Document::new()), Services::default());
        a.session.set_new_documents_rtl(true);
        a.run("text.insert", json!({"text": "\u{05E9}\u{05DC}\u{05D5}\u{05DD}"})).unwrap();
        a.run("select.all", json!({})).unwrap();
        let before = fonts(&a);
        let Some(mut d) = Dialog::open("font", &mut a) else { panic!("no Font dialog") };
        let Dialog::Font { complex, font, font_cs, .. } = &mut d else { panic!("not the Font dialog") };
        assert!(*complex, "Hebrew text shows the complex script row");
        assert_eq!((font.as_str(), font_cs.as_str()), (before.0.as_str(), before.1.as_str()), "each row shows its own font");
        *font = "Georgia".into();
        apply_font(&mut a, &d);
        let latin = fonts(&a);
        assert_eq!((latin.0.as_str(), latin.1.as_str()), ("Georgia", before.1.as_str()), "the Hebrew font stays");
        let Some(mut d) = Dialog::open("font", &mut a) else { panic!("no Font dialog") };
        let Dialog::Font { font_cs, size_cs, .. } = &mut d else { panic!("not the Font dialog") };
        (*font_cs, *size_cs) = ("Arial".into(), "20".into());
        apply_font(&mut a, &d);
        let complex = fonts(&a);
        assert_eq!((complex.1.as_str(), complex.3), ("Arial", 20.0), "the complex script row reaches Hebrew text");
        assert_eq!((complex.0.as_str(), complex.2), ("Georgia", latin.2), "and leaves the Latin font and size alone");
    }
}
