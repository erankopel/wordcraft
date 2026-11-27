//! Messages the engine, the libraries and the system send (a command's status line or error, why
//! a button is off, a download that failed) in the interface language. They are made in English,
//! which scripts and agents read; the window translates what it shows, and only what the catalog
//! knows:
//!
//! - a row for the whole message: an `@msg` row, else a plain one;
//! - an `@msg` row whose English has `{name}` placeholders: a template for every message that
//!   differs from it only there (`All done. We made {n} replacements.`). A template matches a whole
//!   message, with its other text exactly as written, so engine wording that is new or changed stays
//!   English instead of turning into the translation of something else. Where several match, the one
//!   with the most fixed text wins. A placeholder named `error` or `reason` holds a message of its
//!   own and is translated the same way; any other value goes in as it came.
//!
//! The `@msg` context and its templates are craft-i18n's (its `docs/catalog-format.md`).

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex, PoisonError};

use super::{Lang, current, ready, source_texts, tr};

/// How deep a message inside a message is translated (`{error}` within `{error}`).
const MAX_DEPTH: usize = 3;
/// Longest message the templates are tried on; a longer one shows as it is.
const MAX_MESSAGE: usize = 4000;
/// Most templates one catalog contributes.
const MAX_TEMPLATES: usize = 2000;

/// `text`, a message from the engine or a library, in the interface language where the catalog
/// knows it (see the module), else as it came.
pub fn message(text: &str) -> String {
    message_in(current(), text)
}

/// [`message`] in `lang`.
pub(crate) fn message_in(lang: Lang, text: &str) -> String {
    #[cfg(test)]
    if super::pseudo::on() {
        return super::t(text).to_string();
    }
    known(lang, text, 0).unwrap_or_else(|| text.to_string())
}

/// The values of `template`'s `{name}` placeholders, by name, when `text` is the template with
/// each filled in (none empty, the rest of the text exactly as written); `None` when it isn't.
pub(crate) fn captures(template: &str, text: &str) -> Option<Vec<(String, String)>> {
    let t = Template::parse(template)?;
    t.captures(text).map(|got| got.into_iter().map(|(name, value)| (name.to_string(), value.to_string())).collect())
}

/// `text` translated, when the catalog knows it.
fn known(lang: Lang, text: &str, depth: usize) -> Option<String> {
    if text.trim().is_empty() || lang == Lang::EN {
        return None;
    }
    if let Some(row) = lang.i18n().get(lang.lang, "@msg", text) {
        return Some(ready(lang, text, row).to_string());
    }
    let plain = tr(lang, text);
    if plain != text {
        return Some(plain.to_string());
    }
    if depth > MAX_DEPTH || text.len() > MAX_MESSAGE {
        return None;
    }
    let templates = templates(lang);
    // Most fixed text first: the first that matches is the best.
    let (template, got) = templates.iter().find_map(|t| t.captures(text).map(|got| (t, got)))?;
    let row = lang.i18n().get(lang.lang, "@msg", &template.source)?;
    let row = ready(lang, &template.source, row);
    let values: Vec<(&str, String)> = got
        .into_iter()
        .map(|(name, value)| {
            let translated = if matches!(name, "error" | "reason") { known(lang, value, depth + 1) } else { None };
            (name, translated.unwrap_or_else(|| value.to_string()))
        })
        .collect();
    let args: Vec<(&str, &str)> = values.iter().map(|(name, value)| (*name, value.as_str())).collect();
    Some(super::fmt(row, &args))
}

/// One part of a template: text that must be there as written, or a placeholder.
#[derive(Debug, PartialEq)]
enum Part {
    Text(String),
    Hole(String),
}

/// An `@msg` row's English with placeholders.
#[derive(Debug)]
struct Template {
    source: String,
    parts: Vec<Part>,
    /// Characters of fixed text: of two templates that match, the one with more is more specific.
    fixed: usize,
}

impl Template {
    /// `source` split into its text and placeholders; `None` for what can never match a message
    /// on its own terms: no placeholder, a `{` without its `}` (or a `}` without a `{`), an empty
    /// name, two placeholders in a row or no fixed text at all (craft-i18n's CI refuses these too).
    fn parse(source: &str) -> Option<Template> {
        let mut parts = Vec::new();
        let mut rest = source;
        while let Some((before, after)) = rest.split_once('{') {
            if before.contains('}') {
                return None;
            }
            if !before.is_empty() {
                parts.push(Part::Text(before.to_string()));
            } else if matches!(parts.last(), Some(Part::Hole(_))) {
                return None;
            }
            let (name, after) = after.split_once('}')?;
            if name.is_empty() || name.contains('{') {
                return None;
            }
            parts.push(Part::Hole(name.to_string()));
            rest = after;
        }
        if rest.contains('}') {
            return None;
        }
        if !rest.is_empty() {
            parts.push(Part::Text(rest.to_string()));
        }
        let fixed = parts
            .iter()
            .map(|p| match p {
                Part::Text(t) => t.chars().count(),
                Part::Hole(_) => 0,
            })
            .sum();
        let holes = parts.iter().filter(|p| matches!(p, Part::Hole(_))).count();
        (holes > 0 && fixed > 0).then(|| Template { source: source.to_string(), parts, fixed })
    }

    /// The placeholders' values when `text` is this template filled in.
    fn captures<'t>(&self, text: &'t str) -> Option<Vec<(&str, &'t str)>> {
        let mut got = Vec::new();
        fill(&self.parts, text, &mut got).then_some(got)
    }
}

/// Match `parts` against all of `text`, pushing each placeholder's value onto `got`. A placeholder
/// is followed by fixed text (or ends the template); every place that text occurs is tried, nearest
/// first, so a value may contain the text that follows it.
fn fill<'p, 't>(parts: &'p [Part], text: &'t str, got: &mut Vec<(&'p str, &'t str)>) -> bool {
    match parts.split_first() {
        None => text.is_empty(),
        Some((Part::Text(fixed), rest)) => text.strip_prefix(fixed.as_str()).is_some_and(|after| fill(rest, after, got)),
        Some((Part::Hole(name), rest)) => match rest.first() {
            None => {
                if text.is_empty() {
                    return false;
                }
                got.push((name, text));
                true
            }
            Some(Part::Text(next)) => {
                for (at, _) in text.match_indices(next.as_str()) {
                    // A value is never empty.
                    if at == 0 {
                        continue;
                    }
                    let (Some(value), Some(after)) = (text.get(..at), text.get(at..)) else { continue };
                    let mark = got.len();
                    got.push((name, value));
                    if fill(rest, after, got) {
                        return true;
                    }
                    got.truncate(mark);
                }
                false
            }
            // Never parsed: two placeholders in a row can't be told apart.
            Some(Part::Hole(_)) => false,
        },
    }
}

/// `lang`'s templates, most fixed text first, read from its catalogs once.
fn templates(lang: Lang) -> Arc<Vec<Template>> {
    type Key = (&'static str, usize);
    static CACHE: LazyLock<Mutex<HashMap<Key, Arc<Vec<Template>>>>> = LazyLock::new(Default::default);
    let key = (lang.code(), lang.catalogs.map_or(0, |c| std::ptr::from_ref(c) as usize));
    let mut cache = CACHE.lock().unwrap_or_else(PoisonError::into_inner);
    cache.entry(key).or_insert_with(|| Arc::new(read_templates(&source_texts(lang)))).clone()
}

/// The `@msg` rows with placeholders in `catalogs` (catalog text), most fixed text first and, among
/// equals, in catalog order.
fn read_templates(catalogs: &[&str]) -> Vec<Template> {
    let mut out: Vec<Template> = catalogs
        .iter()
        .flat_map(|text| text.lines())
        .filter_map(|line| {
            let mut columns = line.trim_end_matches('\r').split('\t');
            if columns.next()? != "@msg" {
                return None;
            }
            let source = unescape(columns.next()?)?;
            if !source.contains('{') {
                return None;
            }
            Template::parse(&source)
        })
        .take(MAX_TEMPLATES)
        .collect();
    out.sort_by_key(|t| std::cmp::Reverse(t.fixed));
    out
}

/// A catalog column with its escapes (`\n`, `\t`, `\\`) read; `None` for an unknown one.
fn unescape(text: &str) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next()? {
            'n' => out.push('\n'),
            't' => out.push('\t'),
            '\\' => out.push('\\'),
            _ => return None,
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{fixture_lang, set_current};

    fn hebrew() -> Lang {
        fixture_lang(
            "he",
            concat!(
                "\tNothing to undo\tאין מה לבטל\n",
                "@msg\tno matches\tלא נמצאו התאמות\n",
                "@msg\tAll done. We made {n} replacements.\tהכול הושלם. מספר ההחלפות: {n}.\n",
                "@msg\tNot installed: {fonts}\tלא מותקנים: {fonts}\n",
                "@msg\tNot installed: {fonts} and {n} more\tלא מותקנים: {fonts} ועוד {n}\n",
                "@msg\tZotero isn't running (start Zotero and try again): {error}\tZotero אינו פועל (יש להפעיל את Zotero ולנסות שוב): {error}\n",
                "@msg\tConnection refused (os error {code})\tהחיבור נדחה (שגיאת מערכת {code})\n",
                "@msg\tnot available: {reason}\tלא זמין: {reason}\n",
                "\tno other WordCraft window is open\tאין חלון WordCraft אחר פתוח\n",
                "@msg\tCouldn't open {file}\tלא ניתן לפתוח את {file}\n",
                "@msg\t{a}{b} glued\t{a}{b} שגוי\n",
                "@msg\t{alone}\t{alone} לבד\n",
            ),
        )
    }

    fn unmarked(text: &str) -> String {
        text.chars().filter(|c| !matches!(c, '\u{2066}'..='\u{2069}' | '\u{200E}' | '\u{200F}')).collect()
    }

    /// A message the catalog knows whole, or as a template with the parts that vary, is shown
    /// translated with those parts kept; a message it doesn't know, or one that only starts like a
    /// template, stays English rather than becoming the translation of another.
    #[test]
    fn engine_messages_are_translated_only_where_the_catalog_knows_them() {
        let he = hebrew();
        assert_eq!(message_in(he, "Nothing to undo"), "אין מה לבטל", "a plain row");
        assert_eq!(message_in(he, "no matches"), "לא נמצאו התאמות", "an @msg row");
        assert_eq!(unmarked(&message_in(he, "All done. We made 12 replacements.")), "הכול הושלם. מספר ההחלפות: 12.");
        // The longer template wins over the shorter one that also matches.
        assert_eq!(unmarked(&message_in(he, "Not installed: Aptos, David and 3 more")), "לא מותקנים: Aptos, David ועוד 3");
        assert_eq!(unmarked(&message_in(he, "Not installed: Aptos, David")), "לא מותקנים: Aptos, David");
        // Unknown wording, or text around a template's, stays as it came.
        for english in ["All done. We made 12 replacements. Undo?", "All done: 12 replacements.", "A brand-new engine message"] {
            assert_eq!(message_in(he, english), english);
        }
        // Text already in the interface language is left alone.
        assert_eq!(message_in(he, "לא נמצאו התאמות"), "לא נמצאו התאמות");
        // English is the source: nothing changes.
        assert_eq!(message_in(Lang::EN, "no matches"), "no matches");
    }

    /// An error or a reason inside a message is a message of its own: translated where known, and
    /// kept as it came where not. Other values (a file name) are never looked up.
    #[test]
    fn an_error_inside_a_message_is_translated_too() {
        let he = hebrew();
        let zotero = message_in(he, "Zotero isn't running (start Zotero and try again): Connection refused (os error 111)");
        assert_eq!(unmarked(&zotero), "Zotero אינו פועל (יש להפעיל את Zotero ולנסות שוב): החיבור נדחה (שגיאת מערכת 111)");
        let reason = message_in(he, "not available: no other WordCraft window is open");
        assert_eq!(unmarked(&reason), "לא זמין: אין חלון WordCraft אחר פתוח");
        let unknown = message_in(he, "Zotero isn't running (start Zotero and try again): something new");
        assert!(unknown.contains("something new") && unknown.contains("Zotero אינו פועל"), "{unknown}");
        // A file named like a catalog row keeps its name.
        assert_eq!(unmarked(&message_in(he, "Couldn't open no matches")), "לא ניתן לפתוח את no matches");
        set_current(Lang::EN);
    }

    /// Templates that could match anything are never used: one placeholder alone, or two in a row.
    #[test]
    fn templates_that_could_match_anything_are_refused() {
        for bad in ["{alone}", "{a}{b} glued", "no placeholder", "open {", "close }", "{} empty", "a } b {c}"] {
            assert!(Template::parse(bad).is_none(), "{bad}");
        }
        let he = hebrew();
        assert_eq!(message_in(he, "xy glued"), "xy glued");
        assert_eq!(message_in(he, "anything at all"), "anything at all");
    }

    /// The pane's own templates read their values the same way: whole text only, a value may hold
    /// the text that follows it.
    #[test]
    fn captures_read_the_values_of_a_whole_match() {
        let got = captures("Heading level skipped (Heading {to} after Heading {from})", "Heading level skipped (Heading 4 after Heading 2)");
        assert_eq!(got, Some(vec![("to".to_string(), "4".to_string()), ("from".to_string(), "2".to_string())]));
        assert_eq!(captures("{n} empty paragraphs in a row", "3 empty paragraphs in a row, then more"), None);
        let got = captures("{a}: {b}", "x: y: z");
        assert_eq!(got, Some(vec![("a".to_string(), "x".to_string()), ("b".to_string(), "y: z".to_string())]));
    }
}
