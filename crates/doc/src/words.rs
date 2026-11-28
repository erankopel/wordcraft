//! Words WordCraft writes into a document on its own: a table of contents' title, caption labels,
//! "Page X of Y", a mail merge's greeting line, header, content control and decorative-text
//! placeholders, sample items. They are the document's text, so they are written in the language
//! of the text that receives them, not the interface's: Hebrew text gets Hebrew words in an
//! English interface too, and English text gets English ones in a Hebrew interface. A language
//! without words here gets the English ones.

/// Whether `lang` (a BCP 47 tag) is Hebrew: `he`, or its old tag `iw`.
pub fn is_hebrew(lang: Option<&str>) -> bool {
    lang.is_some_and(|l| matches!(crate::bidi::primary_subtag(l).as_str(), "he" | "iw"))
}

/// The Hebrew of the English words WordCraft writes into documents, by their English (for a
/// native speaker's review). Placeholders address the reader in the plural, as the Hebrew
/// interface does.
const HEBREW: &[(&str, &str)] = &[
    ("Bibliography", "ביבליוגרפיה"),
    ("Contents", "תוכן עניינים"),
    // A letter's greeting line (`GREETINGLINE`): the name after לכבוד ("To the honour of"), which
    // greets a man and a woman alike; a recipient with no name gets "שלום רב,".
    ("Dear Sir or Madam,", "שלום רב,"),
    ("Dear {name},", "לכבוד {name},"),
    ("Document subtitle", "כותרת משנה של המסמך"),
    ("Document Title", "כותרת המסמך"),
    ("Equation", "משוואה"),
    ("Figure", "איור"),
    ("Index", "אינדקס"),
    ("Pick a date.", "בחרו תאריך."),
    ("Pick an option.", "בחרו אפשרות."),
    ("References", "מקורות"),
    ("Table", "טבלה"),
    ("Table of Authorities", "רשימת אסמכתאות"),
    ("Table of Contents", "תוכן עניינים"),
    ("Type an equation", "הקלידו משוואה"),
    ("Type here.", "הקלידו כאן."),
    ("Works Cited", "מקורות שצוטטו"),
    ("Your text here", "הטקסט שלכם כאן"),
    ("[Document title]", "[כותרת המסמך]"),
    ("[Type here]", "[הקלידו כאן]"),
    ("DRAFT", "טיוטה"),
    ("CONFIDENTIAL", "חסוי"),
    ("DO NOT COPY", "אין להעתיק"),
    ("SAMPLE", "דוגמה"),
    ("ASAP", "בהקדם האפשרי"),
    ("URGENT", "דחוף"),
];

/// `english` in the language `lang`: its Hebrew in Hebrew text, else the English itself (also
/// for a word the table doesn't have).
pub fn word<'a>(lang: Option<&str>, english: &'a str) -> &'a str {
    if !is_hebrew(lang) {
        return english;
    }
    HEBREW.iter().find(|(en, _)| *en == english).map_or(english, |(_, he)| he)
}

/// The built-in caption labels in the language `lang`, sorted as a list of them is
/// (Equation, Figure, Table; in Hebrew איור, טבלה, משוואה). A label is also its captions' `SEQ`
/// identifier, so a Hebrew caption is numbered by `SEQ איור`, as Hebrew Word documents number
/// theirs.
pub fn caption_labels(lang: Option<&str>) -> [&'static str; 3] {
    let mut labels = ["Equation", "Figure", "Table"].map(|l| word(lang, l));
    labels.sort_unstable();
    labels
}

/// A letter's greeting line to the recipient `name` (first and last), in the language `lang`:
/// `Dear Ada Lovelace,`, or `לכבוד נועה לוי,` in Hebrew; with no name, the greeting for anyone,
/// `Dear Sir or Madam,` (`שלום רב,` in Hebrew).
pub fn greeting(lang: Option<&str>, name: &str) -> String {
    if name.is_empty() { word(lang, "Dear Sir or Madam,").to_string() } else { word(lang, "Dear {name},").replace("{name}", name) }
}

/// The text around the numbers of "Page X of Y": before the page number and between it and the
/// page count (`עמוד X מתוך Y` in Hebrew).
pub fn page_x_of_y(lang: Option<&str>) -> (&'static str, &'static str) {
    if is_hebrew(lang) { ("עמוד ", " מתוך ") } else { ("Page ", " of ") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hebrew_text_gets_hebrew_words_and_other_text_english() {
        for tag in ["he-IL", "he", "HE_il", "iw-IL"] {
            assert!(is_hebrew(Some(tag)), "{tag}");
            assert_ne!(word(Some(tag), "Figure"), "Figure", "{tag}");
        }
        for tag in [None, Some("en-US"), Some("ar-SA"), Some("pt-BR"), Some("")] {
            assert!(!is_hebrew(tag));
            assert_eq!(word(tag, "Contents"), "Contents");
        }
        assert_eq!(word(Some("he-IL"), "Not a document word"), "Not a document word");
        assert_eq!(word(Some("he-IL"), "Contents"), word(Some("he-IL"), "Table of Contents"), "one title for both");
    }

    #[test]
    fn every_hebrew_word_is_hebrew_and_keeps_its_brackets() {
        for (en, he) in HEBREW {
            assert!(he.chars().any(|c| ('\u{05D0}'..='\u{05EA}').contains(&c)), "{en}: {he}");
            assert_eq!(word(Some("he-IL"), en), *he);
            // A placeholder stays a bracketed placeholder.
            assert_eq!((he.starts_with('['), he.ends_with(']')), (en.starts_with('['), en.ends_with(']')), "{en}");
        }
    }

    #[test]
    fn caption_labels_are_one_word_each_in_either_language() {
        for lang in [None, Some("he-IL")] {
            let labels = caption_labels(lang);
            assert!(labels.is_sorted(), "{labels:?}");
            // Each is a SEQ identifier as it is: one word.
            assert!(labels.iter().all(|l| !l.contains(char::is_whitespace)), "{labels:?}");
        }
        assert!(caption_labels(Some("he-IL")).iter().all(|l| !l.is_ascii()));
    }
}
