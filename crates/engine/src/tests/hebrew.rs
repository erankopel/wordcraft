//! Hebrew text: pointed words (niqqud), geresh and gershayim, and how words are selected, moved
//! over and counted. The marks are written as escapes so the offsets below are exact.

use serde_json::json;
use wordcraft_doc::{Pos, StoryRef};

use crate::Session;

/// שָׁלוֹם: shin, shin dot, qamats, lamed, vav, holam, final mem. Seven characters, five letters.
const SHALOM: &str = "\u{05E9}\u{05C1}\u{05B8}\u{05DC}\u{05D5}\u{05B9}\u{05DD}";

/// צה״ל: tsadi, he, gershayim, lamed.
const TSAHAL: &str = "\u{05E6}\u{05D4}\u{05F4}\u{05DC}";

/// ג׳ירפה: gimel, geresh, yod, resh, pe, he.
const GIRAFFE: &str = "\u{05D2}\u{05F3}\u{05D9}\u{05E8}\u{05E4}\u{05D4}";

fn run(s: &mut Session, id: &str, v: serde_json::Value) -> serde_json::Value {
    s.run(id, &v).unwrap_or_else(|e| panic!("{id}: {e}"))
}

/// A right-to-left session holding `text` in its first paragraph.
fn session(text: &str) -> Session {
    let mut s = Session::new(wordcraft_doc::Document::new());
    run(&mut s, "document.setText", json!({"text": text}));
    run(&mut s, "select.all", json!({}));
    run(&mut s, "para.rtl", json!({}));
    run(&mut s, "caret.docStart", json!({}));
    s
}

/// What Select Word picks with the caret `offset` bytes into the first paragraph, without the
/// trailing space it also takes (as a double-click does).
fn word_at(text: &str, offset: usize) -> String {
    let mut s = session(text);
    s.sel = crate::Selection::caret(Pos::new(StoryRef::Body, wordcraft_doc::Path::top(0), offset));
    run(&mut s, "select.word", json!({}));
    s.selected_text().trim_end().to_owned()
}

/// A pointed word is one word: the points sit inside it, so a double-click anywhere in it picks
/// the whole word, not the part up to the next vowel mark.
#[test]
fn a_word_with_niqqud_is_selected_whole() {
    let text = format!("{SHALOM} עולם");
    // At each place a caret can sit inside the word: after the first letter with its points, after
    // the second letter, and after the third.
    for offset in
        ["\u{05E9}\u{05C1}\u{05B8}".len(), "\u{05E9}\u{05C1}\u{05B8}\u{05DC}".len(), "\u{05E9}\u{05C1}\u{05B8}\u{05DC}\u{05D5}\u{05B9}".len()]
    {
        assert_eq!(word_at(&text, offset), SHALOM, "caret at byte {offset}");
    }
}

/// Biblical text carries cantillation marks (te'amim) between the letters of a word, after the
/// points: they are part of the word as the points are. Here bet, sheva, dagesh, resh, tsere, alef,
/// shin, shin dot, hiriq, tipha, yod, tav.
#[test]
fn a_word_with_cantillation_marks_is_selected_whole() {
    let word = "\u{05D1}\u{05B0}\u{05BC}\u{05E8}\u{05B5}\u{05D0}\u{05E9}\u{05C1}\u{05B4}\u{0596}\u{05D9}\u{05EA}";
    let text = format!("{word} ברא");
    // Before the mark, and after it.
    for offset in ["\u{05D1}\u{05B0}\u{05BC}".len(), "\u{05D1}\u{05B0}\u{05BC}\u{05E8}\u{05B5}\u{05D0}\u{05E9}\u{05C1}\u{05B4}\u{0596}".len()] {
        assert_eq!(word_at(&text, offset), word, "caret at byte {offset}");
    }
}

/// Gershayim inside an acronym is part of the word, wherever the caret is in it.
#[test]
fn gershayim_stays_inside_an_acronym() {
    let text = format!("{TSAHAL} אמר");
    assert_eq!(word_at(&text, "\u{05E6}".len()), TSAHAL, "before the gershayim");
    assert_eq!(word_at(&text, "\u{05E6}\u{05D4}\u{05F4}".len()), TSAHAL, "just after it");
}

/// Geresh inside a borrowed word is part of the word.
#[test]
fn geresh_stays_inside_a_borrowed_word() {
    let text = format!("{GIRAFFE} גבוהה");
    assert_eq!(word_at(&text, "\u{05D2}".len()), GIRAFFE, "before the geresh");
    assert_eq!(word_at(&text, "\u{05D2}\u{05F3}".len()), GIRAFFE, "just after it");
}

/// The maqaf joins two words into one phrase but they stay two words: a double-click picks one.
#[test]
fn a_maqaf_separates_words() {
    let text = "בית\u{05BE}ספר";
    assert_eq!(word_at(text, "בי".len()), "בית");
    assert_eq!(word_at(text, "בית\u{05BE}ס".len()), "ספר");
}

/// Ctrl+arrow moves over a pointed word in one step, reading order, never stopping inside it.
#[test]
fn word_moves_cross_a_pointed_word_in_one_step() {
    let text = format!("{SHALOM} עולם");
    let mut s = session(&text);
    run(&mut s, "caret.wordLeft", json!({}));
    // In a right-to-left paragraph Ctrl+Left goes forward in reading order: to the start of the second word.
    assert_eq!(s.sel.focus.off, SHALOM.len() + " ".len(), "past the pointed word and its space");
    run(&mut s, "caret.wordRight", json!({}));
    assert_eq!(s.sel.focus.off, 0, "and back to the start of the first");
}

/// A single arrow press steps over a letter with all its points, never between them.
#[test]
fn arrow_keys_step_over_a_letter_with_its_points() {
    let mut s = session(SHALOM);
    let mut stops = vec![s.sel.focus.off];
    for _ in 0..12 {
        let before = s.sel.focus.off;
        run(&mut s, "caret.left", json!({}));
        if s.sel.focus.off == before {
            break;
        }
        stops.push(s.sel.focus.off);
    }
    // Four clusters: shin with its two points, lamed, vav with its point, final mem.
    let clusters = ["\u{05E9}\u{05C1}\u{05B8}", "\u{05DC}", "\u{05D5}\u{05B9}", "\u{05DD}"];
    let mut expected = vec![0];
    let mut at = 0;
    for c in clusters {
        at += c.len();
        expected.push(at);
    }
    assert_eq!(stops, expected, "the caret rests only between clusters");
}

/// Hebrew has no spelling tools here, so typed Hebrew is never underlined as misspelled English,
/// pointed or not; an English typo beside it still is, which shows the checker is running.
#[test]
fn hebrew_text_is_not_flagged_as_misspelled_english() {
    let flagged = |text: &str| -> Vec<String> {
        let mut s = session(text);
        let issues = run(&mut s, "review.issues", json!({}));
        issues.as_array().map(|a| a.iter().filter_map(|i| i["text"].as_str().map(str::to_owned)).collect()).unwrap_or_default()
    };
    assert!(flagged(&format!("{SHALOM} עולם {TSAHAL} {GIRAFFE}")).is_empty(), "Hebrew words are left unmarked");
    let mixed = flagged("שלום helllo עולם");
    assert!(mixed.iter().any(|w| w == "helllo"), "the English typo is still marked: {mixed:?}");
    assert!(mixed.iter().all(|w| w.is_ascii()), "and only the English word: {mixed:?}");
}

/// Points and punctuation do not change how many words there are: whitespace separates words.
#[test]
fn words_are_counted_the_same_with_or_without_points() {
    let count = |text: &str| {
        let mut s = Session::new(wordcraft_doc::Document::new());
        run(&mut s, "document.setText", json!({"text": text}));
        s.word_count()
    };
    assert_eq!(count(&format!("{SHALOM} עולם")), 2);
    assert_eq!(count("שלום עולם"), 2);
    assert_eq!(count(&format!("{TSAHAL} {GIRAFFE}")), 2, "gershayim and geresh do not split a word");
    assert_eq!(count("\u{05BE}"), 0, "a lone maqaf is punctuation, not a word");
}
