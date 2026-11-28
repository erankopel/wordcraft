//! The built-in Hebrew AutoCorrect list and the rules it is applied by: in Hebrew text, to a word
//! of Hebrew letters alone, and for most entries also after the prefix letters ו ה ב כ ל מ ש.

/// Built-in Hebrew replacements: the typed form, its replacement, and whether the entry also
/// applies after one to three prefix letters ([`PREFIX_LETTERS`]), the rest of the word being the
/// typed form.
///
/// This is our own list, written for WordCraft from general knowledge of Hebrew spelling and
/// checked entry by entry by independent reviews: that the typed form is not a word, is not a
/// spelling anyone writes on purpose, and is not a longer word with its end dropped. It is not
/// taken from any other product's list. The rows were written once from the reviewed list, in its
/// order; nothing is read at run time. No native speaker has reviewed it yet;
/// `docs/autocorrect-he.md` lists every entry for review.
pub(super) const HEBREW: &[(&str, &str, bool)] = &[
    // A final letter typed in its ordinary form at the end of a word (מ for ם, נ for ן).
    ("אותמ", "אותם", true),
    ("שלהמ", "שלהם", true),
    ("כאנ", "כאן", false),
    ("כולמ", "כולם", true),
    ("אנשימ", "אנשים", true),
    ("דברימ", "דברים", true),
    ("שנימ", "שנים", true),
    ("עדיינ", "עדיין", true),
    ("חיימ", "חיים", true),
    ("ילדימ", "ילדים", true),
    ("עליהמ", "עליהם", true),
    ("אליהמ", "אליהם", true),
    ("ביניהמ", "ביניהם", true),
    ("שלכמ", "שלכם", true),
    ("אתכמ", "אתכם", true),
    ("פעמימ", "פעמים", true),
    ("רוצימ", "רוצים", true),
    ("יכולימ", "יכולים", true),
    ("צריכימ", "צריכים", true),
    ("עושימ", "עושים", true),
    ("אומרימ", "אומרים", true),
    ("יודעימ", "יודעים", true),
    ("הולכימ", "הולכים", true),
    ("חושבימ", "חושבים", true),
    ("אחרימ", "אחרים", true),
    ("רבימ", "רבים", true),
    ("חברימ", "חברים", true),
    ("הורימ", "הורים", true),
    ("מספרימ", "מספרים", true),
    ("רואימ", "רואים", true),
    ("אמנמ", "אמנם", true),
    ("עצממ", "עצמם", true),
    ("אצלמ", "אצלם", true),
    ("בשבילמ", "בשבילם", true),
    ("בגללמ", "בגללם", true),
    ("איתכמ", "איתכם", true),
    ("עליכמ", "עליכם", true),
    ("אליכמ", "אליכם", true),
    ("עליהנ", "עליהן", true),
    ("אלפימ", "אלפים", true),
    ("לאנ", "לאן", true),
    ("שונימ", "שונים", true),
    ("חדשימ", "חדשים", true),
    ("גדולימ", "גדולים", true),
    ("טובימ", "טובים", true),
    ("ראשונימ", "ראשונים", true),
    ("מילימ", "מילים", true),
    ("תלמידימ", "תלמידים", true),
    ("עובדימ", "עובדים", true),
    ("נמצאימ", "נמצאים", true),
    ("מדברימ", "מדברים", true),
    ("אוהבימ", "אוהבים", true),
    ("מבינימ", "מבינים", true),
    ("משתמשימ", "משתמשים", true),
    ("קיימימ", "קיימים", true),
    ("חלקימ", "חלקים", true),
    ("יחסימ", "יחסים", true),
    ("אחוזימ", "אחוזים", true),
    ("עשרימ", "עשרים", true),
    ("שניימ", "שניים", true),
    ("יומיימ", "יומיים", true),
    // Other slips: two letters swapped; a letter added, doubled, left out or typed as another
    // letter that sounds the same; two words run together.
    ("עכישו", "עכשיו", false),
    ("אננחו", "אנחנו", false),
    ("בשיבל", "בשביל", false),
    ("בחהלט", "בהחלט", false),
    ("פאילו", "אפילו", false),
    ("יאזה", "איזה", false),
    ("לפמעים", "לפעמים", false),
    ("בהצלהח", "בהצלחה", false),
    ("משפהח", "משפחה", false),
    ("סליהח", "סליחה", false),
    ("יהיתי", "הייתי", false),
    ("תאמול", "אתמול", false),
    ("כמבון", "כמובן", false),
    ("לנפי", "לפני", false),
    ("ביעקר", "בעיקר", false),
    ("מצעטר", "מצטער", false),
    ("בבשקה", "בבקשה", false),
    ("יבחד", "ביחד", false),
    ("לודגמה", "לדוגמה", false),
    ("באמעצות", "באמצעות", false),
    ("למלש", "למשל", false),
    ("בונסף", "בנוסף", false),
    ("במלהך", "במהלך", false),
    ("יהייה", "יהיה", false),
    ("אנחו", "אנחנו", false),
    ("בבבקשה", "בבקשה", false),
    ("להיתראות", "להתראות", false),
    ("פיתאום", "פתאום", false),
    ("איזשהוא", "איזשהו", false),
    ("אכשיו", "עכשיו", false),
    ("כלכך", "כל כך", false),
    ("בכלזאת", "בכל זאת", false),
    ("אחרכך", "אחר כך", false),
    ("בכלאופן", "בכל אופן", false),
    ("כלהכבוד", "כל הכבוד", false),
];

/// The letters a Hebrew word can start with as prefixes: ו (and), ה (the), ב (in), כ (as), ל (to),
/// מ (from) and ש (that).
const PREFIX_LETTERS: [char; 7] = ['ו', 'ה', 'ב', 'כ', 'ל', 'מ', 'ש'];

/// Most prefix letters before an entry's typed form.
const MAX_PREFIXES: usize = 3;

/// Is `word` made of Hebrew letters alone (א to ת, the final forms included), as the Hebrew list
/// needs? A point, cantillation mark, geresh, gershayim, quote mark, maqaf or any other character
/// in it leaves the word as typed.
pub(super) fn is_hebrew_word(word: &str) -> bool {
    !word.is_empty() && word.chars().all(|c| ('\u{05D0}'..='\u{05EA}').contains(&c))
}

/// Is text whose language is `lang` (`w:lang/@w:val`, or the editing language when the text has
/// none) and whose complex-script language is `bidi` (`w:lang/@w:bidi`) Hebrew text, where the
/// Hebrew list applies? Hebrew letters are read as proofing reads them
/// (`wordcraft_layout::para::ProofRun::hebrew`): in the complex-script language, else in the
/// text language, and with Detect language automatically on (`detect`) as Hebrew whatever
/// language they are marked with, as Word marks Hebrew text Arabic where Hebrew isn't set up.
/// Yiddish and Ladino are written in Hebrew letters too: text whose language or complex-script
/// language names either is never Hebrew text here.
pub(super) fn is_hebrew_text(lang: Option<&str>, bidi: Option<&str>, detect: bool) -> bool {
    let lang = lang.map(str::trim).filter(|t| !t.is_empty());
    let bidi = bidi.map(str::trim).filter(|t| !t.is_empty());
    let yiddish_or_ladino = |t: &str| matches!(wordcraft_doc::bidi::primary_subtag(t).as_str(), "yi" | "ji" | "yid" | "ydd" | "yih" | "lad");
    if lang.into_iter().chain(bidi).any(yiddish_or_ladino) {
        return false;
    }
    detect || wordcraft_doc::words::is_hebrew(bidi.or(lang))
}

/// The ways an entry's typed form can sit in `word`: the whole word (at byte 0), then the rest
/// after each of the word's first one to [`MAX_PREFIXES`] letters, while those are all prefix
/// letters, with the byte offset the rest starts at. The rest is never empty.
pub(super) fn prefix_splits(word: &str) -> impl Iterator<Item = (usize, &str)> {
    let prefixes = word.chars().take(MAX_PREFIXES).take_while(|c| PREFIX_LETTERS.contains(c)).count();
    word.char_indices().take(prefixes + 1).map(move |(at, _)| (at, word.get(at..).unwrap_or_default()))
}
