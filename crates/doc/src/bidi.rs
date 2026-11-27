//! Script classification for bidirectional and complex-script text (Persian, Arabic, Hebrew…).
//!
//! The full Unicode Bidirectional Algorithm (UAX #9) runs in layout; this module only answers
//! the cheap questions the model and the engine need: does a character belong to a
//! right-to-left script, and is it formatted with the complex-script properties?

/// A character of a right-to-left script: Hebrew, Arabic (Persian, Urdu…), Syriac, Thaana, N'Ko,
/// Samaritan, Mandaic, and their presentation forms. Includes the Arabic-Indic and Persian
/// (Extended Arabic-Indic) digits and the Arabic combining marks.
pub fn is_rtl_script(c: char) -> bool {
    matches!(u32::from(c),
        0x0590..=0x08FF
        | 0xFB1D..=0xFDFF
        | 0xFE70..=0xFEFF
        | 0x10800..=0x10FFF
        | 0x1E800..=0x1EFFF)
}

/// A character Word formats with the complex-script properties (`w:cs` font, `w:szCs`, `w:bCs`,
/// `w:iCs`): right-to-left scripts, the Indic scripts, Thai and Lao.
pub fn is_complex_script(c: char) -> bool {
    is_rtl_script(c) || matches!(u32::from(c), 0x0900..=0x0EFF)
}

/// A character that belongs to a word for word selection and Ctrl+arrow movement: letters,
/// digits, `_` and `'`, plus what sits inside Persian and Arabic words: the zero-width non-joiner
/// (نیم‌فاصله, as in «می‌خواهم»), the zero-width joiner, tatweel and the Arabic combining marks.
/// In Hebrew words the points (niqqud) are letters already (Unicode counts them as alphabetic),
/// but the cantillation marks of Biblical text (U+0591 to U+05AF) are not, and the geresh and
/// gershayim (U+05F3, U+05F4, as in ג׳ירפה and צה״ל) are punctuation, so they are listed here. The
/// maqaf (U+05BE) and sof pasuq (U+05C3) are not: they separate words.
pub fn is_word_char(c: char) -> bool {
    c.is_alphanumeric()
        || c == '_'
        || c == '\''
        || matches!(u32::from(c), 0x200C | 0x200D | 0x0640 | 0x0610..=0x061A | 0x064B..=0x065F | 0x0670 | 0x06D6..=0x06DC | 0x06DF..=0x06E8 | 0x06EA..=0x06ED | 0x08D3..=0x08FF | 0x0300..=0x036F | 0x0591..=0x05AF | 0x05F3 | 0x05F4)
}

/// A Hebrew point (niqqud: vowel points, dagesh, shin and sin dots, meteg, rafe) or cantillation
/// mark (te'amim): a mark that sits on a letter and doesn't make another letter, so a search may
/// look past it and sorting reads it as a secondary difference. The maqaf (U+05BE), paseq
/// (U+05C0), sof pasuq (U+05C3) and nun hafukha (U+05C6) are punctuation, not marks.
pub fn is_hebrew_point(c: char) -> bool {
    matches!(u32::from(c), 0x0591..=0x05BD | 0x05BF | 0x05C1 | 0x05C2 | 0x05C4 | 0x05C5 | 0x05C7)
}

/// A Hebrew letter: the alphabet, the Yiddish ligatures and the presentation forms (letters with
/// a point among them).
pub fn is_hebrew_letter(c: char) -> bool {
    matches!(u32::from(c), 0x05D0..=0x05EA | 0x05EF..=0x05F2 | 0xFB1D | 0xFB1F..=0xFB28 | 0xFB2A..=0xFB4F)
}

/// A language tag (`he-IL`, `yi`, `ar-EG`, `fa-IR`, `pa-Arab`…) of a language written right to
/// left: text in it is complex-script text, whose language Word keeps in `w:lang/@w:bidi`
/// (ECMA-376 Part 1 §17.3.2.20) rather than in `w:lang/@w:val`.
pub fn is_rtl_language(tag: &str) -> bool {
    let rtl_script =
        tag.split(['-', '_']).skip(1).any(|s| matches!(s.to_ascii_lowercase().as_str(), "arab" | "hebr" | "syrc" | "thaa" | "nkoo" | "adlm"));
    rtl_script || matches!(primary_subtag(tag).as_str(), "he" | "iw" | "yi" | "ji" | "ar" | "fa" | "ur" | "ps" | "sd" | "ug" | "ckb" | "syr" | "dv")
}

/// The language of a tag without its region or script, lowercase: `he` for `he-IL` and `HE_il`.
pub fn primary_subtag(tag: &str) -> String {
    tag.trim().split(['-', '_']).next().unwrap_or("").to_ascii_lowercase()
}

/// Does `text` contain a right-to-left character?
pub fn has_rtl(text: &str) -> bool {
    text.chars().any(is_rtl_script)
}

/// Does the first strong directional character in `text` belong to a right-to-left script?
///
/// Follows UAX #9 rules P2 and P3: characters with strong bidirectional class AL or R
/// (Arabic, Hebrew, Syriac, Thaana…) give `true`; strong L characters (Latin, Greek,
/// Cyrillic, CJK…) give `false`. Neutral, weak and whitespace characters are skipped.
pub fn first_strong_is_rtl(text: &str) -> bool {
    for c in text.chars() {
        if c == '\u{200F}' {
            return true;
        }
        if c == '\u{200E}' {
            return false;
        }
        if c.is_alphabetic() {
            return is_rtl_script(c);
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_persian_and_latin() {
        // Persian letters, the Persian-only letters (پ چ ژ گ ک ی), ZWNJ-free word, digits.
        assert!("سلام".chars().all(is_rtl_script));
        assert!("پچژگکی".chars().all(is_rtl_script));
        assert!("۰۱۲۳۴۵۶۷۸۹".chars().all(is_rtl_script));
        assert!(is_rtl_script('\u{064E}'), "fatha (a combining mark) is Arabic script");
        assert!("שלום".chars().all(is_rtl_script));
        assert!(!"Hello 123".chars().any(is_rtl_script));
        assert!(!is_rtl_script('\u{200C}'), "ZWNJ is a format character, not a letter");
        assert!(is_complex_script('क') && !is_rtl_script('क'));
        assert!(has_rtl("Word ورد 2024") && !has_rtl("Word 2024"));
        assert!("می‌خواهم".chars().all(is_word_char), "ZWNJ is inside the word");
        assert!("کِتاب".chars().all(is_word_char), "so are harakat");
        assert!("\u{05E9}\u{05C1}\u{05B8}\u{05DC}\u{05D5}\u{05B9}\u{05DD}".chars().all(is_word_char), "and Hebrew points");
        assert!("\u{05D1}\u{05B4}\u{0596}\u{05D9}\u{05EA}".chars().all(is_word_char), "and Hebrew cantillation marks");
        assert!("\u{05E6}\u{05D4}\u{05F4}\u{05DC}".chars().all(is_word_char) && is_word_char('\u{05F3}'), "and geresh and gershayim");
        assert!(!is_word_char('\u{05BE}') && !is_word_char('\u{05C3}'), "the maqaf and sof pasuq end a word");
        assert!(!is_word_char(' ') && !is_word_char('،'));
        assert!(first_strong_is_rtl("سلام"));
        assert!(first_strong_is_rtl("من ناوم هێڤاره"));
        assert!(first_strong_is_rtl("  123. سلام"));
        assert!(!first_strong_is_rtl("Hello سلام"));
        assert!(!first_strong_is_rtl("123 Hello"));
        assert!(!first_strong_is_rtl(""));
    }

    /// The languages Word keeps in the complex-script attribute: those written right to left,
    /// whatever the tag's case, region or separator, or a tag that names a right-to-left script.
    #[test]
    fn right_to_left_language_tags() {
        for tag in ["he-IL", "he", "iw", "HE_il", "yi", "ar-SA", "fa-IR", "ur", "pa-Arab-PK", "az-arab"] {
            assert!(is_rtl_language(tag), "{tag}");
        }
        for tag in ["en-US", "", "hu", "pa-IN", "az-Latn", "ru", "zh-Hant"] {
            assert!(!is_rtl_language(tag), "{tag}");
        }
    }
}
