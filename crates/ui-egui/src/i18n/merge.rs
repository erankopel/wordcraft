//! A catalog with rows added after the published ones that correct them. craft-i18n keeps the
//! first of two rows with the same context and English, so a row appended to a catalog can only
//! add a string. An edition appends its supplement to the published catalog
//! (`packaging/hebrew/he-supplement.tsv`), and a row there that repeats a published row's key is a
//! correction: before the catalog is embedded, the later row takes the earlier one's place. The
//! published catalogs have no repeated keys (craft-i18n's CI refuses them), so only a supplement
//! changes anything.
//!
//! `build.rs` uses this file too (`#[path]`): plain `std`, nothing from the crate.

use std::collections::HashMap;

/// `catalog` (catalog text) with every row whose context and English come again further down
/// left out (its line kept, blank, so the problems craft-i18n reports keep their line numbers).
pub fn last_rows_win(catalog: &str) -> String {
    let lines: Vec<&str> = catalog.split('\n').collect();
    let mut last: HashMap<(&str, &str), usize> = HashMap::new();
    for (i, line) in lines.iter().enumerate() {
        if let Some(key) = key(line) {
            last.insert(key, i);
        }
    }
    let mut out = String::with_capacity(catalog.len());
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let replaced = key(line).and_then(|k| last.get(&k)).is_some_and(|&at| at != i);
        if !replaced {
            out.push_str(line);
        }
    }
    out
}

/// A row's context and English (as written, escapes and all); `None` for a comment, a blank line
/// or a line that isn't a row.
fn key(line: &str) -> Option<(&str, &str)> {
    let line = line.strip_suffix('\r').unwrap_or(line);
    let line = line.strip_prefix('\u{feff}').unwrap_or(line);
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let mut columns = line.split('\t');
    let (context, source) = (columns.next()?, columns.next()?);
    columns.next()?;
    Some((context, source))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{fixture_lang, tr};

    /// The edition's supplement corrects a published row by repeating its key further down: the
    /// corrected row is the one looked up, the other rows and the line numbers stay as they were.
    #[test]
    fn a_supplement_row_replaces_the_published_row_with_its_key() {
        let published = "# catalog\n\thyphenation\tהיפוך מילים\n\tOffice\tOffice\nicons\tOffice\tמשרד ישן\n\tHome\tבית\n";
        let supplement = "\thyphenation\tמיקוף\nicons\tOffice\tמשרד\n\tNew row\tשורה חדשה\n";
        let merged = last_rows_win(&format!("{published}{supplement}"));
        assert_eq!(merged.lines().count(), format!("{published}{supplement}").lines().count(), "line numbers stay");
        // Leaked: a fixture language reads its catalog for the life of the program.
        let he = fixture_lang("he", Box::leak(merged.into_boxed_str()));
        assert_eq!(tr(he, "hyphenation"), "מיקוף", "the supplement's correction wins");
        assert_eq!(crate::i18n::trc(he, "icons", "Office"), "משרד", "keys are context and English");
        assert_eq!(tr(he, "Office"), "Office", "the plain row with that English is another key");
        assert_eq!(tr(he, "Home"), "בית");
        assert_eq!(tr(he, "New row"), "שורה חדשה");
        // A catalog without repeats is unchanged.
        assert_eq!(last_rows_win(published), published);
    }

    /// The Hebrew catalog as it is embedded (in the edition, craft-i18n's rows with the
    /// supplement's added and corrected) breaks none of craft-i18n's rules: the same placeholders
    /// and trailing ellipsis on both sides, the language's plural forms, one row per key.
    #[test]
    fn the_embedded_hebrew_catalog_keeps_the_catalog_rules() {
        if !crate::i18n::catalogs_embedded() {
            return;
        }
        let problems: Vec<String> = crate::i18n::I18N.check_all().into_iter().filter(|p| p.starts_with("he/")).collect();
        assert!(problems.is_empty(), "{problems:#?}");
    }
}
