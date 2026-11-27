use super::chrome::close_button;
use super::{Dialog, Tokens, Ui, Value, WordApp, json, vec2};

/// Manage Styles' body. Reads the list from `styles.manage`'s data (never re-running the command,
/// which would ask to open this dialog again) and acts through commands.
pub(super) fn manage_styles(app: &mut WordApp, ui: &mut Ui, alphabetical: &mut bool, selected: &mut String) -> bool {
    let list = wordcraft_engine::cmd::para::manage_list(&app.session, *alphabetical);
    let mut list = list.as_array().cloned().unwrap_or_default();
    let str_of = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    // Alphabetical by the names shown (a built-in style's in the interface language).
    if *alphabetical {
        list.sort_by_cached_key(|st| crate::i18n::lowercase(&shown_name(st)));
    }
    if !list.iter().any(|x| str_of(x, "id") == *selected) {
        *selected = list.first().map(|x| str_of(x, "id")).unwrap_or_default();
    }
    ui.horizontal(|ui| {
        ui.label(tl!("Sort order:"));
        craft_ui::ComboBox::from_id_salt("manage_styles_sort")
            .selected_text(if *alphabetical { tl!("Alphabetical") } else { tl!("As Recommended") })
            .show_ui(ui, |ui| {
                ui.selectable_value(alphabetical, false, tl!("As Recommended"));
                ui.selectable_value(alphabetical, true, tl!("Alphabetical"));
            });
    });
    let t = Tokens::get(ui.ctx());
    let shown = egui::Id::unique("manage_styles_shown");
    egui::Frame::NONE.stroke(egui::Stroke::new(1.0, t.border)).inner_margin(4).show(ui, |ui| {
        egui::ScrollArea::vertical().id_salt("manage_styles_list").max_height(200.0).auto_shrink([false, false]).show(ui, |ui| {
            ui.set_width(380.0);
            for st in &list {
                let id = str_of(st, "id");
                let glyph = match st.get("type").and_then(Value::as_str) {
                    Some("character") => "a",
                    Some("linked") => "¶a",
                    Some("table") => "⊞",
                    _ => "¶",
                };
                let hidden = st.get("hidden").and_then(Value::as_bool).unwrap_or(false);
                let mut name = egui::RichText::new(format!("{glyph}  {}", shown_name(st)));
                if hidden {
                    name = name.weak();
                }
                let r = ui.selectable_label(*selected == id, name);
                // Bring the selection into view once, when it changes (opening, New Style…).
                if *selected == id && ui.data(|d| d.get_temp::<String>(shown)).as_deref() != Some(id.as_str()) {
                    r.scroll_to_me(Some(egui::Align::Center));
                    ui.data_mut(|d| d.insert_temp(shown, id.clone()));
                }
                if r.clicked() {
                    ui.data_mut(|d| d.insert_temp(shown, id.clone()));
                    *selected = id.clone();
                }
                if r.double_clicked() {
                    *selected = id;
                    if let Some(Dialog::ModifyStyle { id, name, font, size, bold, italic, color, before, after, .. }) =
                        Dialog::modify_style(app, selected)
                    {
                        app.dialog = Some(Dialog::ModifyStyle { back: true, resume: None, id, name, font, size, bold, italic, color, before, after });
                    }
                }
            }
        });
    });
    let leave = |ui: &mut Ui| {
        ui.data_mut(|d| d.remove::<String>(shown));
        true
    };
    if app.dialog.is_some() {
        return leave(ui);
    }
    let Some(cur) = list.iter().find(|x| str_of(x, "id") == *selected).cloned() else { return close_button(ui) && leave(ui) };
    let ty = str_of(&cur, "type");
    ui.add_space(6.0);
    crate::previews::style_preview(app, ui, selected, &ty, vec2(388.0, if ty == "table" { 64.0 } else { 40.0 }));
    ui.add_space(4.0);
    ui.add(egui::Label::new(egui::RichText::new(describe(app, &cur)).small()).wrap());
    ui.add_space(6.0);
    let builtin = cur.get("builtIn").and_then(Value::as_bool).unwrap_or(true);
    if ty != "table" {
        let mut gallery = cur.get("inGallery").and_then(Value::as_bool).unwrap_or(false);
        if ui.checkbox(&mut gallery, tl!("Show in the Styles gallery")).changed() {
            let _ = app.run("styles.setVisibility", json!({"style": selected, "gallery": gallery}));
        }
    }
    let mut hidden = cur.get("hidden").and_then(Value::as_bool).unwrap_or(false);
    if ui.checkbox(&mut hidden, tl!("Hide from style lists")).changed() {
        let _ = app.run("styles.setVisibility", json!({"style": selected, "hidden": hidden}));
    }
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        // Table styles are edited from Table Design.
        if ui.add_enabled(ty != "table", egui::Button::new(tl!("Modify…"))).clicked()
            && let Some(Dialog::ModifyStyle { id, name, font, size, bold, italic, color, before, after, .. }) = Dialog::modify_style(app, selected)
        {
            app.dialog = Some(Dialog::ModifyStyle { back: true, resume: None, id, name, font, size, bold, italic, color, before, after });
        }
        if ui.button(tl!("New Style…")).clicked() {
            app.dialog = Some(Dialog::NewStyle { name: "Style1".into(), based_on: str_of(&cur, "name"), back: true });
        }
        let del = ui.add_enabled(!builtin, egui::Button::new(tl!("Delete")));
        let del = if builtin { del.on_disabled_hover_text(tl!("Built-in styles can't be deleted.")) } else { del };
        if del.clicked() {
            let _ = app.run("styles.delete", json!({"style": selected}));
            selected.clear();
        }
    });
    (app.dialog.is_some() || close_button(ui)) && leave(ui)
}

/// A `styles.manage` entry's name as the list shows it: a built-in style's in the interface
/// language ([`crate::i18n::builtin_style_name`]); the id and the stored name stay for commands.
fn shown_name(st: &Value) -> String {
    let name = st.get("name").and_then(Value::as_str).unwrap_or("");
    crate::dialogs_templates::shown_style_name(name, st.get("builtIn").and_then(Value::as_bool).unwrap_or(false))
}

/// A style's description: its type, the formatting it sets and what it is based on.
fn describe(app: &WordApp, st: &Value) -> String {
    let f = st.get("format").cloned().unwrap_or_default();
    let fs = |k: &str| f.get(k).and_then(Value::as_str);
    let fnum = |k: &str| f.get(k).and_then(Value::as_f64);
    let pt = |x: f64| format!("{}", (x * 10.0).round() / 10.0);
    let mut parts: Vec<String> = vec![
        match st.get("type").and_then(Value::as_str) {
            Some("character") => tl!("Character style"),
            Some("linked") => tl!("Linked style (paragraph and character)"),
            Some("table") => tl!("Table style"),
            _ => tl!("Paragraph style"),
        }
        .to_string(),
    ];
    if st.get("builtIn").and_then(Value::as_bool).unwrap_or(false) {
        parts.push(tl!("Built-in style").to_string());
    }
    if let Some(x) = fs("font") {
        parts.push(crate::i18n::fmt(tl!("Font: {font}"), &[("font", x)]));
    }
    if let Some(x) = fnum("size") {
        parts.push(crate::i18n::fmt(tl!("{size} pt"), &[("size", &pt(x))]));
    }
    if f.get("bold").and_then(Value::as_bool) == Some(true) {
        parts.push(tl!("Bold").to_string());
    }
    if f.get("italic").and_then(Value::as_bool) == Some(true) {
        parts.push(tl!("Italic").to_string());
    }
    if let Some(x) = fs("color") {
        parts.push(crate::i18n::fmt(tl!("Color: {color}"), &[("color", &format!("#{x}"))]));
    }
    if let Some(x) = fnum("spaceBefore") {
        parts.push(crate::i18n::fmt(tl!("Space before: {pt} pt"), &[("pt", &pt(x))]));
    }
    if let Some(x) = fnum("spaceAfter") {
        parts.push(crate::i18n::fmt(tl!("Space after: {pt} pt"), &[("pt", &pt(x))]));
    }
    if let Some(b) = st.get("basedOn").and_then(Value::as_str) {
        // The name of the style it is based on (`styles.manage` gives the name), as the list names it.
        let based = app.session.doc.styles.find(b).map_or_else(|| b.to_string(), crate::i18n::style_name);
        parts.push(crate::i18n::fmt(tl!("Based on: {style}"), &[("style", &based)]));
    }
    parts.join(", ")
}

#[cfg(test)]
mod tests {
    use crate::i18n::{Lang, fixture_lang, set_current};
    use crate::{Services, WordApp};
    use serde_json::json;

    /// The texts Manage Styles draws with `selected` chosen, in the recommended order, once the
    /// list has scrolled to it (isolate marks taken out). Time moves on a second a frame, so the
    /// list's scrolling ends.
    fn drawn(app: &mut WordApp, selected: &str) -> Vec<String> {
        let ctx = egui::Context::default();
        crate::theme::install_fonts_for(&ctx, false);
        let mut texts = Vec::new();
        for frame in 0..4 {
            let (mut alphabetical, mut selected) = (false, selected.to_string());
            let input = egui::RawInput { time: Some(f64::from(frame)), ..Default::default() };
            let mut out = ctx.run_ui(input, |ui| {
                super::manage_styles(app, ui, &mut alphabetical, &mut selected);
            });
            out.textures_delta.clear();
            texts = out
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    egui::Shape::Text(t) => Some(t.galley.text().chars().filter(|c| !matches!(c, '\u{2066}'..='\u{2069}')).collect::<String>()),
                    _ => None,
                })
                .collect();
        }
        texts
    }

    /// UWB-04: Manage Styles names a built-in style in the interface language (as the gallery
    /// does) and a style someone made as they named it; the style one is based on is named the
    /// same way.
    #[test]
    fn manage_styles_names_built_in_styles_in_the_interface_language() {
        let mut app = WordApp::new(wordcraft_engine::Session::new(wordcraft_doc::Document::new()), Services::default());
        app.run("styles.create", json!({"name": "Normal notes", "basedOn": "Heading 1", "fromSelection": false})).unwrap();
        let mine = app.session.doc.styles.find("Normal notes").map(|st| st.id.clone()).unwrap_or_default();
        set_current(fixture_lang("he", "\tNormal\tרגיל\n\tHeading {n}\tכותרת {n}\n\tBased on: {style}\tמבוסס על: {style}\n"));
        let top = drawn(&mut app, "Normal");
        let own = drawn(&mut app, &mine);
        set_current(Lang::EN);
        assert!(top.iter().any(|t| t.ends_with("  רגיל")) && top.iter().any(|t| t.ends_with("  כותרת 1")), "{top:?}");
        assert!(!top.iter().any(|t| t.ends_with("  Normal") || t.ends_with("  Heading 1")), "{top:?}");
        assert!(own.iter().any(|t| t.ends_with("  Normal notes")), "a style someone made keeps its name: {own:?}");
        assert!(own.iter().any(|t| t.contains("מבוסס על: כותרת 1")), "{own:?}");
    }
}
