//! Date & Time insertion dialog (Word-style format and language selection).
use egui::{Stroke, Ui};
use serde::Serialize;
use serde_json::{Value, json};

use super::buttons;
use super::field::{DATE_FORMATS, HE_DATE_FORMATS};
use crate::WordApp;
use crate::theme::Tokens;

/// Predefined Portuguese (Brazil) date and time format pictures.
const PT_DATE_FORMATS: &[&str] = &[
    "dd/MM/yyyy",
    "dddd, d 'de' MMMM 'de' yyyy",
    "d 'de' MMMM 'de' yyyy",
    "dd/MM/yy",
    "yyyy-MM-dd",
    "d-MMM-yy",
    "dd.MM.yyyy",
    "d MMMM yyyy",
    "MMM-yy",
    "dd/MM/yyyy HH:mm",
    "dd/MM/yyyy HH:mm:ss",
    "HH:mm",
    "HH:mm:ss",
];

/// The languages the dialog writes dates in (`insert.dateTime`'s): tag and name, in the order of
/// their names.
const LANGUAGES: [(&str, &str); 3] = [("en-US", "English (United States)"), ("he-IL", "Hebrew (Israel)"), ("pt-BR", "Portuguese (Brazil)")];

/// The pictures offered in a language.
fn formats_in(language: &str) -> &'static [&'static str] {
    match language {
        "pt-BR" => PT_DATE_FORMATS,
        "he-IL" => HE_DATE_FORMATS,
        _ => DATE_FORMATS,
    }
}

/// The dialog's language for a language tag: a Portuguese or Hebrew one (any region; Hebrew's old
/// `iw` too) is its Portuguese (Brazil) or Hebrew (Israel), any other English (United States).
fn offered(tag: &str) -> &'static str {
    match wordcraft_doc::bidi::primary_subtag(tag).as_str() {
        "pt" => "pt-BR",
        "he" | "iw" => "he-IL",
        _ => "en-US",
    }
}

/// The date preview list consists of document values (not interface strings).
/// The i18n audit recognizes these dynamic values instead of treating month names as UI labels.
#[cfg(test)]
pub(crate) fn date_time_preview_matches(value: &str) -> bool {
    [PT_DATE_FORMATS, DATE_FORMATS, HE_DATE_FORMATS]
        .iter()
        .flat_map(|list| list.iter())
        .any(|picture| LANGUAGES.iter().any(|(tag, _)| wordcraft_engine::cmd::insert::format_date_in(picture, Some(tag)) == value))
}

/// State of the Word-like Insert > Date & Time dialog, including an optional validation error.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DateTimeForm {
    /// Selected Word field picture, not a Rust strftime format.
    pub format: String,
    /// Language of the resulting text or DATE field.
    pub language: String,
    /// Whether to insert a refreshable field rather than fixed text.
    pub update: bool,
    /// Last insertion failure, as the engine said it (shown through `i18n::message`); retain the
    /// window so the user can change the choice.
    #[serde(skip)]
    pub error: Option<String>,
    /// Brief status confirmation message (e.g. after setting default preferences).
    #[serde(skip)]
    pub status: Option<&'static str>,
}
impl DateTimeForm {
    /// Restore a saved format/language, else start in the language of the text at the caret (a
    /// Hebrew date in Hebrew text, as a new document of the Hebrew interface has), else in the
    /// interface's.
    pub fn new(app: &WordApp) -> Self {
        let language = if !app.ui.date_time_language.is_empty() {
            offered(&app.ui.date_time_language)
        } else {
            match wordcraft_engine::cmd::words::caret_lang(&app.session) {
                Some(tag) => offered(&tag),
                None => offered(crate::i18n::current().code()),
            }
        };
        let list = formats_in(language);
        let format = if list.contains(&app.ui.date_time_format.as_str()) { app.ui.date_time_format.clone() } else { list[0].to_string() };
        Self { format, language: language.to_string(), update: app.ui.date_time_update, error: None, status: None }
    }
    /// Pictures available in the chosen language.
    fn formats(&self) -> &'static [&'static str] {
        formats_in(&self.language)
    }
    /// Engine command parameters corresponding to the current controls.
    pub fn params(&self) -> Value {
        json!({"format": self.format, "language": self.language, "update": self.update})
    }
}
/// Draw the interactive format list, language picker and update checkbox; close on successful
/// insertion or Cancel, keeping errors visible rather than silently dismissing the dialog.
pub(super) fn date_time_ui(app: &mut WordApp, ui: &mut Ui, f: &mut DateTimeForm) -> bool {
    let tokens = Tokens::get(ui.ctx());
    ui.set_width(520.0);
    // Two columns of their own widths, running the interface's way: in a mirrored interface the
    // formats stand at the right and the language at their left, and neither squeezes the other.
    crate::rtl::horizontal_top(ui, |ui| {
        crate::rtl::vertical(ui, |ui| {
            ui.set_width(330.0);
            ui.label(tl!("Date formats:"));
            egui::Frame::new().stroke(Stroke::new(1.0, tokens.border)).inner_margin(2.0).show(ui, |ui| {
                egui::ScrollArea::vertical().id_salt("date_time_formats").max_height(245.0).auto_shrink([false, false]).show(ui, |ui| {
                    for picture in f.formats() {
                        let sample = wordcraft_engine::cmd::insert::format_date_in(picture, Some(&f.language));
                        if ui.selectable_label(f.format == *picture, sample).on_hover_text(*picture).clicked() {
                            f.format = (*picture).to_string();
                        }
                    }
                });
            });
        });
        ui.add_space(12.0);
        crate::rtl::vertical(ui, |ui| {
            ui.set_width(170.0);
            let language_label = ui.label(tl!("Language"));
            let shown = LANGUAGES.iter().find(|(tag, _)| *tag == f.language).map_or("", |(_, name)| name);
            let combo = crate::rtl::ComboBox::from_id_salt("date_time_lang").selected_text(tl!(shown)).width(160.0).show_ui(ui, |ui| {
                for (tag, name) in LANGUAGES {
                    if ui.selectable_label(f.language == tag, tl!(name)).clicked() {
                        f.language = tag.into();
                        f.format = formats_in(tag)[0].into();
                    }
                }
            });
            combo.response.labelled_by(language_label.id);
        });
    });
    ui.add_space(8.0);
    ui.checkbox(&mut f.update, tl!("Update automatically"));
    ui.add_space(6.0);
    if ui.button(tl!("Set as Default")).clicked() {
        app.ui.date_time_format = f.format.clone();
        app.ui.date_time_language = f.language.clone();
        app.ui.date_time_update = f.update;
        f.status = Some(tl!("Default format saved."));
    }
    if let Some(status) = f.status {
        ui.colored_label(tokens.text_dim, status);
    }
    if let Some(error) = &f.error {
        // The engine's English message, in the interface language where the catalog has it.
        ui.colored_label(ui.visuals().error_fg_color, crate::i18n::message(error));
    }
    let (ok, cancel) = buttons(ui, "OK", "Cancel", true);
    if ok {
        match app.run("insert.dateTime", f.params()) {
            Ok(_) => return true,
            Err(error) => {
                f.error = Some(error.to_string());
                return false;
            }
        }
    }
    cancel
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dialogs_insert::InsertDialog;

    /// A new document of the Hebrew interface: right to left, its complex-script text Hebrew.
    fn hebrew_app() -> WordApp {
        let mut session = wordcraft_engine::Session::new(wordcraft_doc::Document::new());
        session.set_new_documents_rtl(true);
        WordApp::new(session, Default::default())
    }

    fn form(app: &mut WordApp) -> DateTimeForm {
        app.run("ui.dialog", json!({"name": "dateTime"})).unwrap();
        match app.dialog.take() {
            Some(crate::dialogs::Dialog::Insert(d)) => match *d {
                InsertDialog::DateTime(f) => f,
                d => panic!("not Date and Time: {d:?}"),
            },
            _ => panic!("no dialog"),
        }
    }

    /// In Hebrew text the dialog starts in Hebrew (Israel), day first, and OK writes a Hebrew
    /// date; English text keeps English (United States), and a saved choice wins over both.
    #[test]
    fn the_dialog_starts_in_the_language_of_the_text_and_offers_hebrew() {
        let mut app = hebrew_app();
        let f = form(&mut app);
        assert_eq!((f.language.as_str(), f.format.as_str()), ("he-IL", HE_DATE_FORMATS[0]));
        let mut long = f.clone();
        long.format = "d 'ב'MMMM yyyy".into();
        app.run("insert.dateTime", long.params()).unwrap();
        let text = app.session.doc.plain_text(wordcraft_doc::StoryRef::Body);
        assert!(text.chars().any(|c| ('\u{05D0}'..='\u{05EA}').contains(&c)) && !text.chars().any(|c| c.is_ascii_alphabetic()), "{text:?}");
        let mut english = WordApp::new(wordcraft_engine::Session::new(wordcraft_doc::Document::from_text("Today")), Default::default());
        english.run("caret.docEnd", json!({})).unwrap();
        assert_eq!(form(&mut english).language, "en-US");
        english.ui.date_time_language = "iw".into();
        assert_eq!(form(&mut english).language, "he-IL", "a saved Hebrew choice, by its old tag too");
        // Every picture offered in Hebrew is a date the engine writes in Hebrew.
        for picture in HE_DATE_FORMATS {
            assert!(!wordcraft_engine::cmd::insert::format_date_in(picture, Some("he-IL")).chars().any(|c| c.is_ascii_alphabetic()), "{picture}");
        }
    }

    /// `app`'s Date and Time dialog drawn in the interface language `lang` (English when `None`).
    fn drawn(lang: Option<crate::i18n::Lang>) -> egui_kittest::Harness<'static, WordApp> {
        let mut app = WordApp::new(wordcraft_engine::Session::new(wordcraft_doc::Document::new()), Default::default());
        app.ui.date_time_language = "en-US".into();
        app.run("ui.dialog", json!({"name": "dateTime"})).unwrap();
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1440.0, 900.0)).build_ui_state(
            move |ui, app: &mut WordApp| {
                let ctx = ui.ctx().clone();
                app.logic(&ctx);
                if let Some(lang) = lang {
                    crate::i18n::set_current(lang);
                }
                crate::rtl::sync(&ctx);
                crate::dialogs::show(app, &ctx);
                crate::i18n::set_current(crate::i18n::Lang::EN);
            },
            app,
        );
        h.run_steps(4);
        h
    }

    /// The dialog's rows start from the reading side: a format in the list, Update
    /// automatically and Set as Default begin at the right edge in a right-to-left interface (Hebrew),
    /// one under the other, and at the left edge left to right; OK and Cancel stand at the far side.
    #[test]
    fn the_rows_start_from_the_reading_side() {
        use egui_kittest::kittest::Queryable;
        let hebrew = crate::i18n::fixture_lang("he", "\tLanguage\tשפה\n");
        let first = wordcraft_engine::cmd::insert::format_date_in(DATE_FORMATS[0], Some("en-US"));
        for (lang, mirrored) in [(None, false), (Some(hebrew), true)] {
            let h = drawn(lang);
            let rows = [h.get_by_label(&first).rect(), h.get_by_label("Update automatically").rect(), h.get_by_label("Set as Default").rect()];
            let ok = h.get_by_label("OK").rect();
            let starts: Vec<f32> = rows.iter().map(|r| if mirrored { r.max.x } else { r.min.x }).collect();
            let spread = starts.iter().fold(f32::MIN, |a, &b| a.max(b)) - starts.iter().fold(f32::MAX, |a, &b| a.min(b));
            assert!(spread <= 6.0, "mirrored {mirrored}: the rows start at one edge: {rows:?}");
            if mirrored {
                assert!(rows.iter().all(|r| r.center().x > ok.center().x), "the rows at the right, OK at the far (left) side: {rows:?} {ok:?}");
            } else {
                assert!(rows.iter().all(|r| r.center().x < ok.center().x), "the rows at the left, OK at the far (right) side: {rows:?} {ok:?}");
            }
        }
    }

    /// An insertion error the engine reports in English shows in the interface language where the
    /// catalog has it. Only the drawn text is translated: the form keeps the engine's English in
    /// `error` (serde-skipped, so not part of the dialog's state that scripts read).
    #[test]
    fn an_insertion_error_is_shown_in_the_interface_language() {
        use egui_kittest::kittest::Queryable;
        let message = "no document is open";
        let hebrew = crate::i18n::fixture_lang("he", "@msg\tno document is open\tאין מסמך פתוח\n");
        let mut h = drawn(Some(hebrew));
        if let Some(crate::dialogs::Dialog::Insert(d)) = &mut h.state_mut().dialog
            && let InsertDialog::DateTime(f) = &mut **d
        {
            f.error = Some(message.into());
        }
        h.run_steps(2);
        assert!(h.query_by_label_contains("אין מסמך פתוח").is_some(), "the message in Hebrew");
        assert!(h.query_by_label_contains(message).is_none(), "not in English");
    }

    /// In a mirrored interface the two columns keep their widths: the language box stands left of
    /// the formats, clear of them, as wide as in a left-to-right interface (it used to be
    /// squeezed to a sliver over the list).
    #[test]
    fn the_columns_keep_their_widths_in_a_mirrored_interface() {
        use egui::accesskit::Role;
        use egui_kittest::kittest::Queryable;
        let mirrored = crate::i18n::fixture_lang("ar", "\tLanguage\tLanguage\n");
        let mut boxes = Vec::new();
        for lang in [None, Some(mirrored)] {
            let mut app = WordApp::new(wordcraft_engine::Session::new(wordcraft_doc::Document::new()), Default::default());
            app.ui.date_time_language = "en-US".into();
            app.run("ui.dialog", json!({"name": "dateTime"})).unwrap();
            let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1440.0, 900.0)).build_ui_state(
                move |ui, app: &mut WordApp| {
                    let ctx = ui.ctx().clone();
                    app.logic(&ctx);
                    if let Some(lang) = lang {
                        crate::i18n::set_current(lang);
                    }
                    crate::rtl::sync(&ctx);
                    crate::dialogs::show(app, &ctx);
                    crate::i18n::set_current(crate::i18n::Lang::EN);
                },
                app,
            );
            h.run_steps(4);
            let combo = h.get_by_role(Role::ComboBox).rect();
            let first = h.get_by_label(&wordcraft_engine::cmd::insert::format_date_in(DATE_FORMATS[0], Some("en-US"))).rect();
            boxes.push((combo, first));
        }
        let [(ltr_combo, ltr_list), (rtl_combo, rtl_list)] = boxes[..] else { panic!("two layouts") };
        assert!(ltr_combo.min.x >= ltr_list.max.x, "left to right: the language after the formats");
        assert!(rtl_combo.max.x <= rtl_list.min.x, "mirrored: the language left of the formats, clear of them: {rtl_combo:?} {rtl_list:?}");
        assert!(rtl_combo.width() >= ltr_combo.width() - 1.0, "as wide as unmirrored: {} vs {}", rtl_combo.width(), ltr_combo.width());
    }
}
