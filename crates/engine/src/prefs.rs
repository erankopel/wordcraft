use serde::{Deserialize, Serialize};

/// Editing preferences that persist between runs (the front end saves and restores them).
/// File › Options sets most of them (`file.options`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Prefs {
    /// Word Count includes text boxes, footnotes and endnotes (Word's default).
    pub count_notes: bool,
    /// On-screen paper for pages without a document background; excluded from saved documents.
    #[serde(deserialize_with = "lenient_paper_color")]
    pub paper_color: crate::cmd::view::PaperColor,
    /// Track Changes Options: what markup shows and how revisions are drawn (per user, as in
    /// Word).
    pub markup: wordcraft_layout::display::MarkupOptions,
    /// File › Print / Export: Print Markup. PDFs, prints and page images show comments in
    /// balloons beside each page and tracked changes as marked up; HTML, Markdown and text
    /// copies end with the comments (`file.printMarkup`).
    pub print_markup: bool,
    /// Language › Detect language automatically (per user, as in Word).
    pub detect_language: bool,
    /// Set Proofing Language › Set As Default: overrides the interface language for future
    /// blank documents and plain text. Older preferences keep following the interface.
    #[serde(deserialize_with = "lenient_default_language")]
    pub default_language: Option<String>,
    /// Typing over a selection replaces it; off, the typed text goes in front of it.
    pub typing_replaces_selection: bool,
    /// The Insert key switches overtype on and off.
    pub insert_key_overtype: bool,
    /// Overtype: typed characters replace the ones after the caret.
    pub overtype: bool,
    /// Ctrl+Right Shift makes the selected paragraphs read right to left and Ctrl+Left Shift left
    /// to right, as in Word (File › Options › Advanced). Off for keyboards whose layout switches
    /// with Ctrl+Shift.
    pub direction_keys: bool,
    /// How Left and Right move the caret through text that mixes right-to-left and left-to-right
    /// writing (File › Options › Advanced › Cursor movement). A damaged saved value reads as the
    /// default.
    #[serde(deserialize_with = "lenient_cursor_movement")]
    pub cursor_movement: CursorMovement,
    /// The unit rulers and dialogs show lengths in, when the user chose one (File › Options ›
    /// Advanced › Measurement units); `None` follows the region ([`Session::units`]). Saved under
    /// a new key; an older `units` value other than inches (the old default) carries over on load.
    #[serde(rename = "measurementUnits")]
    pub units: Option<wordcraft_geom::Unit>,
    /// The region the user chose for new documents, units and the decimal separator (a locale
    /// tag like `pl-PL`, File › Options › Advanced › Region); `None` follows the system.
    pub region: Option<String>,
    /// A folder searched for Hunspell dictionaries before the system locations (File › Options ›
    /// Language, #1063): an app started from Finder or the Dock doesn't see
    /// `WORDCRAFT_DICTIONARIES`. `None` means no configured folder.
    pub dictionaries_folder: Option<String>,
    /// The user templates folder File › New lists personal templates from and Save as Word
    /// Template starts in (File › Options › Advanced › File locations, #2852). `None`: the app's
    /// own `Templates` folder in its settings folder.
    pub user_templates_folder: Option<String>,
    /// Formatting marks shown even while Show/Hide ¶ is off.
    pub marks: wordcraft_layout::display::FormattingMarks,
    /// Mark grammar errors as you type (with Check spelling as you type on).
    pub mark_grammar: bool,
    /// Check spelling as you type (File › Options › Proofing). The live value is
    /// `ViewState::proofing`; this is the copy that is saved and restored (#1792).
    pub check_spelling: bool,
    /// Read Mode's Page Color. The live value is `ViewState::read_page_color`.
    #[serde(deserialize_with = "lenient_read_page_color")]
    pub read_page_color: crate::cmd::view::ReadPageColor,
    pub ignore_uppercase: bool,
    pub ignore_numbers: bool,
    pub ignore_internet: bool,
    /// AutoCorrect Options: replacements, capitalization and AutoFormat As You Type (per user, as
    /// in Word). A damaged saved value reads as the defaults.
    #[serde(deserialize_with = "crate::cmd::tools::lenient_autocorrect")]
    pub autocorrect: crate::cmd::tools::AutoCorrectPrefs,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            paper_color: Default::default(),
            count_notes: true,
            markup: Default::default(),
            print_markup: false,
            typing_replaces_selection: true,
            insert_key_overtype: false,
            overtype: false,
            direction_keys: true,
            cursor_movement: CursorMovement::Visual,
            units: Default::default(),
            region: None,
            dictionaries_folder: None,
            user_templates_folder: None,
            marks: Default::default(),
            mark_grammar: true,
            check_spelling: true,
            read_page_color: Default::default(),
            ignore_uppercase: true,
            ignore_numbers: true,
            ignore_internet: true,
            detect_language: true,
            default_language: None,
            autocorrect: Default::default(),
        }
    }
}

/// How Left and Right move the caret in a line that mixes right-to-left and left-to-right text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CursorMovement {
    /// One place to the left or right on the screen: entering an English word in a Hebrew line,
    /// Left jumps to the word's far end and walks it backwards.
    #[default]
    Visual,
    /// One character back or forward in the order the text was typed: Left goes forward in a
    /// right-to-left paragraph and back in a left-to-right one, Right the other way.
    Logical,
}

impl CursorMovement {
    /// The `file.options` name.
    pub fn name(self) -> &'static str {
        match self {
            CursorMovement::Visual => "visual",
            CursorMovement::Logical => "logical",
        }
    }

    /// The value named `name` (`visual`, `logical`), in any letter case.
    pub fn from_name(name: &str) -> Option<Self> {
        [CursorMovement::Visual, CursorMovement::Logical].into_iter().find(|m| m.name().eq_ignore_ascii_case(name.trim()))
    }
}

impl Prefs {
    /// What the spelling check skips.
    pub fn spelling(&self) -> wordcraft_proof::Options {
        wordcraft_proof::Options {
            ignore_uppercase: self.ignore_uppercase,
            ignore_numbers: self.ignore_numbers,
            ignore_internet: self.ignore_internet,
        }
    }
}

/// Ignore malformed saved language tags without discarding the other preferences.
fn lenient_default_language<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    Ok(value.as_str().and_then(wordcraft_proof::lang::normalize_tag))
}

fn lenient_paper_color<'de, D: serde::Deserializer<'de>>(d: D) -> Result<crate::cmd::view::PaperColor, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    Ok(serde_json::from_value(value).unwrap_or_default())
}

/// Ignore an invalid saved cursor movement without discarding other preferences.
fn lenient_cursor_movement<'de, D: serde::Deserializer<'de>>(d: D) -> Result<CursorMovement, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    Ok(serde_json::from_value(value).unwrap_or_default())
}

/// Ignore an invalid saved Page Color without discarding other preferences.
fn lenient_read_page_color<'de, D: serde::Deserializer<'de>>(d: D) -> Result<crate::cmd::view::ReadPageColor, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    Ok(serde_json::from_value(value).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::Prefs;
    use crate::cmd::view::ReadPageColor;

    #[test]
    fn read_page_color_loads_from_old_and_damaged_prefs() {
        let old: Prefs = serde_json::from_str(r#"{"checkSpelling":false}"#).unwrap();
        assert_eq!(old.read_page_color, ReadPageColor::None);
        assert!(!old.check_spelling);
        for bad in [r#"{"readPageColor":"unknown","checkSpelling":false}"#, r#"{"readPageColor":42,"checkSpelling":false}"#] {
            let prefs: Prefs = serde_json::from_str(bad).unwrap();
            assert_eq!(prefs.read_page_color, ReadPageColor::None);
            assert!(!prefs.check_spelling);
        }
        let saved: Prefs = serde_json::from_str(r#"{"readPageColor":"sepia"}"#).unwrap();
        assert_eq!(saved.read_page_color, ReadPageColor::Sepia);
        assert_eq!(serde_json::to_value(saved).unwrap()["readPageColor"], "sepia");
    }

    /// The cursor movement choice is saved and read back; a damaged one reads as the default and
    /// the preferences saved beside it survive.
    #[test]
    fn cursor_movement_loads_from_old_and_damaged_prefs() {
        use super::CursorMovement;
        let old: Prefs = serde_json::from_str(r#"{"checkSpelling":false}"#).unwrap();
        assert_eq!((old.cursor_movement, old.direction_keys), (CursorMovement::Visual, true));
        let saved = Prefs { cursor_movement: CursorMovement::Logical, direction_keys: false, ..Prefs::default() };
        let back: Prefs = serde_json::from_value(serde_json::to_value(&saved).unwrap()).unwrap();
        assert_eq!(back, saved);
        for bad in [r#"{"cursorMovement":"sideways","checkSpelling":false}"#, r#"{"cursorMovement":7,"checkSpelling":false}"#] {
            let prefs: Prefs = serde_json::from_str(bad).unwrap();
            assert_eq!(prefs.cursor_movement, CursorMovement::Visual);
            assert!(!prefs.check_spelling);
        }
    }
}
