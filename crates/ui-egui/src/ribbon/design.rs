use super::common::{mi, mi_text, stack};
use super::{CONTENT_H, Rect, Sense, Target, Ui, WordApp, big, group, json, menu_button, small, vec2};

/// The Design tab's style set gallery in a box of its own, sized from its thumbnails and clipped
/// to it, with a gap on each side: the Colors and Fonts column can't be drawn over, or cut by,
/// the thumbnails (#638). In a right-to-left interface, the thumbnails lay out from the right
/// (#5750). Returns the box and the space the thumbnails used.
pub(super) fn style_set_box(app: &mut WordApp, ui: &mut Ui) -> (Rect, Rect) {
    ui.add_space(4.0);
    let (gallery, _) = ui.allocate_exact_size(vec2(crate::previews::STYLE_SET_GALLERY_W, CONTENT_H), Sense::hover());
    let mut gui = ui.new_child(egui::UiBuilder::new().max_rect(gallery).layout(crate::rtl::row_layout(ui, egui::Align::Min)));
    gui.set_clip_rect(gallery.intersect(ui.clip_rect()));
    gui.spacing_mut().item_spacing = vec2(1.0, 1.0);
    crate::previews::style_set_gallery(app, &mut gui);
    ui.add_space(8.0);
    (gallery, gui.min_rect())
}

pub(super) fn design(app: &mut WordApp, ui: &mut Ui) {
    group(ui, "Themes", None, app, |ui, app| {
        menu_button(ui, app, crate::widgets::MenuButtonOptions::new("themes", Some("Themes"), "Themes", true, "design.themes"), |ui, app| {
            for (name, ..) in wordcraft_engine::cmd::design::THEMES {
                mi(ui, app, name, "design.theme", json!({"name": name}));
            }
        });
    });
    group(ui, "Document Formatting", None, app, |ui, app| {
        style_set_box(app, ui);
        stack(ui, |ui| {
            menu_button(
                ui,
                app,
                crate::widgets::MenuButtonOptions::new("colors", Some("Colors"), "Theme Colors", false, "design.themeColors"),
                |ui, app| {
                    for (name, ..) in wordcraft_engine::cmd::design::THEMES {
                        mi(ui, app, name, "design.themeColors", json!({"name": name}));
                    }
                },
            );
            menu_button(
                ui,
                app,
                crate::widgets::MenuButtonOptions::new("fonts", Some("Fonts"), "Theme Fonts", false, "design.themeFonts"),
                |ui, app| {
                    for (name, h, b, _) in wordcraft_engine::cmd::design::THEMES {
                        mi_text(ui, app, &format!("{}: {h} / {b}", tl!(name)), "design.themeFonts", json!({"heading": h, "body": b}));
                    }
                },
            );
        });
        stack(ui, |ui| {
            menu_button(
                ui,
                app,
                crate::widgets::MenuButtonOptions::new(
                    "paraSpacing",
                    Some("Paragraph Spacing"),
                    "Paragraph Spacing",
                    false,
                    "design.paragraphSpacing",
                ),
                |ui, app| {
                    for (l, v) in [
                        ("Default", "default"),
                        ("No Paragraph Space", "none"),
                        ("Compact", "compact"),
                        ("Tight", "tight"),
                        ("Open", "open"),
                        ("Relaxed", "relaxed"),
                        ("Double", "double"),
                    ] {
                        // "Open" here is roomy spacing, not the file verb: only a `spacing` row
                        // translates it, else it stays English.
                        if l == "Open" {
                            mi_text(ui, app, crate::i18n::get("spacing", l).unwrap_or(l), "design.paragraphSpacing", json!({"value": v}));
                            continue;
                        }
                        mi(ui, app, l, "design.paragraphSpacing", json!({"value": v}));
                    }
                },
            );
            small(ui, app, "effectsDesign", Some("Effects"), "Effects", crate::widgets::ButtonAction::new("design.effects", json!({}), false));
            small(
                ui,
                app,
                "setDefault",
                Some("Set as Default"),
                "Set as Default",
                crate::widgets::ButtonAction::new("design.setDefault", json!({}), false),
            );
        });
    });
    group(ui, "Page Background", None, app, |ui, app| {
        menu_button(
            ui,
            app,
            crate::widgets::MenuButtonOptions::new("watermark", Some("Watermark"), "Watermark", true, "design.watermark"),
            watermark_menu,
        );
        menu_button(
            ui,
            app,
            crate::widgets::MenuButtonOptions::new("pageColor", Some("Page\nColor"), "Page Color", true, "design.pageColor"),
            |ui, app| {
                mi(ui, app, "No Color", "design.pageColor", json!({"color": null}));
                let theme = app.session.doc.settings.theme_colors.clone();
                crate::colors::menu(ui, app, &theme, Target::cmd("design.pageColor"));
            },
        );
        big(ui, app, "pageBorders", "Page\nBorders", "design.pageBorders", json!({}), false);
    });
}

/// Design › Watermark's menu. A watermark is the document's text, so in Hebrew text (the
/// language of the text at the caret, as the engine decides it for its own words) each item
/// writes the Hebrew word and shows it, whatever the interface language: טיוטה for DRAFT. In other
/// text an item writes the English word under its translated label, as the menu always did.
pub(super) fn watermark_menu(ui: &mut Ui, app: &mut WordApp) {
    use wordcraft_engine::cmd::words::{WATERMARKS, caret_lang, is_hebrew, word};
    let lang = caret_lang(&app.session);
    let hebrew = is_hebrew(lang.as_deref());
    if hebrew {
        // An interface in another language may have no face with Hebrew letters: ask for the
        // installed one (the right-to-left interface font, which has them) from the next frame.
        app.want_system_arabic = true;
    }
    for w in WATERMARKS {
        if hebrew {
            hebrew_item(ui, app, word(lang.as_deref(), w));
        } else {
            mi(ui, app, w, "design.watermark", json!({"text": w}));
        }
    }
    ui.separator();
    mi(ui, app, "Custom Watermark…", "ui.dialog", json!({"name": "watermark"}));
    mi(ui, app, "Remove Watermark", "design.watermark", json!({"remove": true}));
}

/// A watermark menu item that writes, and shows, the Hebrew `text`. Screen readers are told its
/// name is Hebrew, so they read it in Hebrew in an interface in another language.
fn hebrew_item(ui: &mut Ui, app: &mut WordApp, text: &str) {
    let id = "design.watermark";
    let sc = crate::widgets::shortcut_text(app, id);
    let enabled = crate::widgets::enabled(app, id);
    let resp = ui.add_enabled(enabled, crate::rtl::menu_item(ui, text, &sc, 200.0));
    ui.ctx().accesskit_node_builder(resp.id, |node| node.set_language("he"));
    if resp.clicked() {
        crate::widgets::run_from_ribbon(app, id, json!({"text": text}));
        ui.close();
    }
}
