use super::typing::{INITIAL_CAPS_EXCEPTIONS, NOT_SENTENCE_END};
use super::{CmdError, CmdResult, Deserialize, Pos, Serialize, Session, Value, json, p};

mod hebrew;

/// Smileys replaced even when typed straight after a word ("ok:)"), as Word does.
pub(super) const SMILEYS: &[&str] = &[":-)", ":-(", ":)", ":(", ":|"];

/// Built-in AutoCorrect replacements (our own list of common typos and symbols).
pub const AUTOCORRECT: &[(&str, &str)] = &[
    ("teh", "the"),
    ("adn", "and"),
    ("recieve", "receive"),
    ("recieved", "received"),
    ("seperate", "separate"),
    ("definately", "definitely"),
    ("occured", "occurred"),
    ("untill", "until"),
    ("wich", "which"),
    ("becuase", "because"),
    ("thier", "their"),
    ("alot", "a lot"),
    ("accomodate", "accommodate"),
    ("acheive", "achieve"),
    ("beleive", "believe"),
    ("calender", "calendar"),
    ("goverment", "government"),
    ("tommorow", "tomorrow"),
    ("wierd", "weird"),
    ("i", "I"),
    ("dont", "don't"),
    ("cant", "can't"),
    ("wont", "won't"),
    ("didnt", "didn't"),
    ("doesnt", "doesn't"),
    ("isnt", "isn't"),
    ("(c)", "©"),
    ("(r)", "®"),
    ("(tm)", "™"),
    ("...", "…"),
    ("->", "→"),
    ("-->", "→"),
    ("<-", "←"),
    ("=>", "⇒"),
    (":)", "☺"),
    (":-)", "☺"),
    (":(", "☹"),
    (":-(", "☹"),
    (":|", "\u{1F610}"),
    ("1/2", "½"),
    ("1/4", "¼"),
    ("3/4", "¾"),
];

/// A built-in entry that is an English word (letters and apostrophes only), not a symbol.
fn is_english_entry(from: &str) -> bool {
    from.chars().any(char::is_alphabetic) && from.chars().all(|c| c.is_alphabetic() || c == '\'')
}

/// The text a built-in replacement applies to: the built-in lists are kept by language.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BuiltInLang {
    /// Text in any language: symbols, arrows, smileys and fractions.
    Any,
    /// English text (#835): the English words.
    English,
    /// Hebrew text, to a word of Hebrew letters alone; with `prefixes`, also after one to three
    /// prefix letters ([`hebrew::HEBREW`]).
    Hebrew { prefixes: bool },
}

/// Every built-in replacement, `(typed, replacement, the text it applies to)`: [`AUTOCORRECT`]'s
/// English words and symbols, then the Hebrew list. An entry of [`AUTOCORRECT`] is English when it
/// is a word ([`is_english_entry`]), a test that would take Hebrew letters for English ones too,
/// so the Hebrew entries are kept apart from it.
fn built_ins() -> impl Iterator<Item = (&'static str, &'static str, BuiltInLang)> {
    let common = AUTOCORRECT.iter().map(|&(a, b)| (a, b, if is_english_entry(a) { BuiltInLang::English } else { BuiltInLang::Any }));
    common.chain(hebrew::HEBREW.iter().map(|&(a, b, prefixes)| (a, b, BuiltInLang::Hebrew { prefixes })))
}

/// The language of the text a finished word is in, as the built-in lists read it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextLang {
    /// English text: the built-in English words apply (#835).
    pub english: bool,
    /// Hebrew text: the built-in Hebrew words apply, to a word of Hebrew letters alone.
    pub hebrew: bool,
}

impl TextLang {
    /// The language of text whose language is `lang` (the run's, its style's or the document's
    /// `w:lang`; the editing language for text with none) and whose complex-script language is
    /// `bidi` (`w:lang/@w:bidi`), with Detect language automatically `detect`. English is decided
    /// as proofing decides it (#835), every English (en-GB, en-CA…) counting (#1281); Hebrew much
    /// as proofing reads Hebrew letters ([`hebrew::is_hebrew_text`]), except that text with no
    /// language has the editing language here, and a Yiddish or Ladino tag in either attribute
    /// keeps the Hebrew list off.
    pub(super) fn of(lang: Option<&str>, bidi: Option<&str>, detect: bool) -> TextLang {
        TextLang { english: wordcraft_proof::ProofLang::from_tag(lang).is_english(), hebrew: hebrew::is_hebrew_text(lang, bidi, detect) }
    }
}

/// AutoCorrect Options (Word keeps them per user; the front end saves them with its preferences).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AutoCorrectPrefs {
    /// All of AutoCorrect and AutoFormat As You Type.
    pub enabled: bool,
    /// Replace text as you type (the replacement list).
    pub replace_text: bool,
    /// Correct TWo INitial CApitals: "WOrd" → "Word" (not the exceptions, e.g. "IDs").
    pub two_initial_caps: bool,
    /// Capitalize the first letter of sentences.
    pub cap_sentences: bool,
    /// Capitalize the first letter of table cells.
    pub cap_cells: bool,
    /// Capitalize names of days.
    pub cap_days: bool,
    /// Correct accidental usage of cAPS LOCK: "wORD" → "Word".
    pub caps_lock: bool,
    /// AutoFormat As You Type: "straight quotes" with “smart quotes”.
    pub smart_quotes: bool,
    /// With smart quotes, `"` and `'` typed right after a Hebrew letter, with no quote of their
    /// kind open, are gershayim ״ (U+05F4) and geresh ׳ (U+05F3), the marks Hebrew writes in
    /// acronyms and borrowed sounds, where Word writes ” and ’. `None` (the default) follows the
    /// interface: on with the Hebrew one ([`hebrew_marks`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hebrew_marks: Option<bool>,
    /// Fractions (1/2) with fraction characters (½).
    pub fractions: bool,
    /// Ordinals (1st) with superscript.
    pub ordinals: bool,
    /// *Bold* and _italic_ with real formatting: the marks are removed and the text between them
    /// is made bold or italic. Off unless the user turns it on.
    pub bold_italic: bool,
    /// Hyphens (--) with dashes.
    pub dashes: bool,
    /// Internet addresses with hyperlinks.
    pub links: bool,
    /// Automatic bulleted lists.
    pub bullets: bool,
    /// Automatic numbered lists.
    pub numbering: bool,
    /// Border lines.
    pub border_lines: bool,
    /// Set left- and first-indent with tabs and backspaces: Tab at the start of a line of a
    /// paragraph with text indents it instead of typing a tab, and Backspace at its start takes
    /// the indent back. Off by default.
    pub tab_indent: bool,
    /// Built-in Heading styles: a second Enter after a line of five or fewer words that ends in a
    /// letter or digit makes it Heading 1 (Heading 2 or 3 after one or two leading tabs).
    /// Off by default.
    pub heading_styles: bool,
    /// A second Space after a word, number or closing bracket or quote turns the first into a
    /// period (macOS's "Add period with double-space"); off by default.
    pub double_space_period: bool,
    /// The user's entries: added, or replacing a built-in one.
    pub entries: Vec<(String, String)>,
    /// Built-in entries the user deleted.
    pub removed: Vec<String>,
    /// Abbreviations the user added after which a sentence doesn't end ("approx.").
    pub exceptions: Vec<String>,
    /// Built-in abbreviations the user deleted from the exceptions.
    pub exceptions_removed: Vec<String>,
    /// The user's words that keep two initial capitals (besides the built-in ones).
    pub initial_caps_exceptions: Vec<String>,
    /// Undoing (or backspacing and retyping) an automatic sentence capital adds the word before
    /// it to the exceptions (First Letter's "Automatically add words to list").
    pub auto_add_first_letter: bool,
    /// The same for undoing a TWo INitial CApitals fix (INitial CAps's switch).
    pub auto_add_initial_caps: bool,
}

impl Default for AutoCorrectPrefs {
    fn default() -> Self {
        AutoCorrectPrefs {
            enabled: true,
            replace_text: true,
            two_initial_caps: true,
            cap_sentences: true,
            cap_cells: true,
            cap_days: true,
            caps_lock: true,
            smart_quotes: true,
            hebrew_marks: None,
            fractions: true,
            ordinals: true,
            bold_italic: false,
            dashes: true,
            links: true,
            bullets: true,
            numbering: true,
            border_lines: true,
            tab_indent: false,
            heading_styles: false,
            double_space_period: false,
            entries: Vec::new(),
            removed: Vec::new(),
            exceptions: Vec::new(),
            exceptions_removed: Vec::new(),
            initial_caps_exceptions: Vec::new(),
            auto_add_first_letter: true,
            auto_add_initial_caps: true,
        }
    }
}

/// Longest AutoCorrect "replace" text, in characters.
const MAX_FROM: usize = 255;

/// Longest "with" text, in characters.
const MAX_TO: usize = 2000;

/// Most entries (and exceptions) kept.
const MAX_ENTRIES: usize = 10_000;

/// Built-in fractions: AutoFormat's (Fractions), not the replacement list's.
const FRACTIONS: [&str; 3] = ["1/2", "1/4", "3/4"];

impl AutoCorrectPrefs {
    /// Drop what a damaged or hostile saved value could hold: empty or overlong entries,
    /// duplicates, too many of them.
    pub fn cleaned(mut self) -> Self {
        let ok = |t: &str, max: usize| !t.trim().is_empty() && t.chars().count() <= max && !t.contains(['\n', '\r']);
        let mut seen = std::collections::HashSet::new();
        self.entries.retain(|(a, b)| ok(a, MAX_FROM) && b.chars().count() <= MAX_TO && !b.contains(['\n', '\r']) && seen.insert(a.clone()));
        self.entries.truncate(MAX_ENTRIES);
        for list in [&mut self.removed, &mut self.exceptions, &mut self.exceptions_removed, &mut self.initial_caps_exceptions] {
            let mut seen = std::collections::HashSet::new();
            list.retain(|a| ok(a, MAX_FROM) && seen.insert(a.clone()));
            list.truncate(MAX_ENTRIES);
        }
        self
    }

    /// The replacement for `word` in English text, if any: the user's entry, else a built-in
    /// one not deleted. Built-in lowercase entries also match a capitalised word ("Teh" → "The").
    pub fn replacement(&self, word: &str) -> Option<String> {
        self.replacement_in(word, true)
    }

    /// The replacement for `word` in text that is (`english`) or isn't English: the built-in
    /// English words ("teh", "i", "dont") apply only to English text, as Word keeps a list per
    /// language (#835: Polish "i" means "and"); symbols, fractions and the user's entries apply
    /// whatever the language. The Hebrew words are left out ([`AutoCorrectPrefs::replacement_at`]
    /// has them).
    pub fn replacement_in(&self, word: &str, english: bool) -> Option<String> {
        self.replacement_at(word, TextLang { english, hebrew: false }).map(|(_, r)| r)
    }

    /// What AutoCorrect does to a finished `word` in text of language `lang`: the byte offset in
    /// `word` where the part it replaces starts, and the text that replaces that part (to the end
    /// of the word). The user's entry for the whole word comes first; then a built-in entry the
    /// user has neither deleted nor replaced with one of their own: the English words in English
    /// text (a lowercase one also matches the word capitalised, "Teh" → "The"), the symbols in any
    /// text, and in Hebrew text the Hebrew words, for a word of Hebrew letters alone. A Hebrew entry
    /// matches the whole word, or, when it allows prefixes, the word after one to three prefix
    /// letters, which stay as typed ("ושלהמ" → "ו" and "שלהם"). The longest typed form that matches
    /// decides: when the user has deleted or replaced that entry, the word stays as typed, and a
    /// shorter entry inside it ("דברימ" in "מדברימ", after the prefix letter מ) doesn't fire instead.
    pub fn replacement_at(&self, word: &str, lang: TextLang) -> Option<(usize, String)> {
        if let Some((_, b)) = self.entries.iter().find(|(a, _)| a == word) {
            return Some((0, b.clone()));
        }
        let live = |a: &str| !self.removed.iter().any(|r| r == a) && !self.entries.iter().any(|(e, _)| e == a);
        let common = built_ins()
            .filter(|(_, _, l)| match l {
                BuiltInLang::Any => true,
                BuiltInLang::English => lang.english,
                BuiltInLang::Hebrew { .. } => false,
            })
            .filter(|(a, _, _)| live(a))
            .filter(|(a, _, _)| self.fractions || !FRACTIONS.contains(a))
            .find(|(a, _, _)| {
                *a == word || (a.chars().all(char::is_lowercase) && word.to_lowercase() == *a && word.chars().next().is_some_and(char::is_uppercase))
            })
            .map(|(a, b, _)| {
                if a != word && b.chars().next().is_some_and(char::is_lowercase) {
                    let mut c = b.chars();
                    c.next().map(|x| x.to_uppercase().chain(c).collect::<String>()).unwrap_or_default()
                } else {
                    b.to_string()
                }
            });
        if let Some(r) = common {
            return Some((0, r));
        }
        if !lang.hebrew || !hebrew::is_hebrew_word(word) {
            return None;
        }
        // The splits run from the whole word to the shortest rest: the first entry that matches
        // decides, live or not.
        hebrew::prefix_splits(word)
            .find_map(|(at, rest)| {
                built_ins()
                    .find(|&(a, _, l)| a == rest && matches!(l, BuiltInLang::Hebrew { prefixes } if at == 0 || prefixes))
                    .map(|(a, b, _)| live(a).then(|| (at, b.to_string())))
            })
            .flatten()
    }

    /// The replacement list as the dialog shows it, sorted: `(from, to, built in)`. The built-in
    /// entries are those of every language, the Hebrew words too.
    pub fn list(&self) -> Vec<(String, String, bool)> {
        let mut v: Vec<(String, String, bool)> = built_ins()
            .filter(|(a, _, _)| !self.removed.iter().any(|r| r == a) && !self.entries.iter().any(|(e, _)| e == a))
            .map(|(a, b, _)| (a.to_string(), b.to_string(), true))
            .chain(self.entries.iter().map(|(a, b)| (a.clone(), b.clone(), false)))
            .collect();
        v.sort_by(|x, y| x.0.to_lowercase().cmp(&y.0.to_lowercase()).then(x.0.cmp(&y.0)));
        v
    }

    /// Add a First Letter exception (lowercase; one the built-in list has just stops being deleted).
    pub(super) fn add_exception(&mut self, e: String) {
        if !plain_word(&e) {
            return;
        }
        self.exceptions_removed.retain(|r| *r != e);
        if !NOT_SENTENCE_END.contains(&e.as_str()) && !self.exceptions.contains(&e) && self.exceptions.len() < MAX_ENTRIES {
            self.exceptions.push(e);
        }
    }

    /// Add a word that keeps its two initial capitals.
    fn add_initial_caps_exception(&mut self, w: &str) {
        if !plain_word(w) {
            return;
        }
        if !INITIAL_CAPS_EXCEPTIONS.contains(&w)
            && !self.initial_caps_exceptions.iter().any(|e| e == w)
            && self.initial_caps_exceptions.len() < MAX_ENTRIES
        {
            self.initial_caps_exceptions.push(w.to_string());
        }
    }

    /// Abbreviations after which a sentence doesn't end, lowercase.
    pub fn exception_list(&self) -> Vec<String> {
        let mut v: Vec<String> = NOT_SENTENCE_END
            .iter()
            .filter(|a| !self.exceptions_removed.iter().any(|r| r.eq_ignore_ascii_case(a)))
            .map(ToString::to_string)
            .chain(self.exceptions.iter().map(|a| a.to_lowercase()))
            .collect();
        v.sort();
        v.dedup();
        v
    }
}

/// Is `w` a word an exception list may hold: one short line, no spaces or object characters?
fn plain_word(w: &str) -> bool {
    !w.trim().is_empty() && w.chars().count() <= MAX_FROM && !w.contains(|c: char| c.is_whitespace() || c == '\u{FFFC}')
}

/// The newest automatic capitalisation (a sentence capital, a TWo INitial CApitals fix), kept so
/// that undoing it, or backspacing it and typing the word again, makes an exception (#1857).
#[derive(Clone, Debug)]
pub struct AutoCap {
    /// Undo steps pushed (ever) right after the change (`u64::MAX` until the caller has pushed its step).
    pub(crate) depth: u64,
    pub(super) story: wordcraft_doc::StoryRef,
    pub(super) path: wordcraft_doc::Path,
    /// Byte offset of the word in its paragraph.
    pub(super) start: usize,
    /// The token before a sentence capital's word, lowercase ("there."); empty when none or when
    /// the capital wasn't a sentence one.
    pub(super) prev: String,
    /// The TWo INitial CApitals word as typed ("TWo").
    pub(super) two_caps: Option<String>,
    /// The user backspaced into the word after the change.
    pub(super) backspaced: bool,
}

impl AutoCap {
    pub(crate) fn add_exceptions(self, ac: &mut AutoCorrectPrefs) {
        if ac.auto_add_first_letter && !self.prev.is_empty() {
            ac.add_exception(self.prev);
        }
        if ac.auto_add_initial_caps
            && let Some(w) = self.two_caps
        {
            ac.add_initial_caps_exception(&w);
        }
    }

    /// A Backspace deleted `a..f`; it reached back over the capitalised word's first letter.
    pub(crate) fn note_backspace(&mut self, a: &Pos, f: &Pos) {
        if f.story == self.story && f.path == self.path && a.off <= self.start && f.off > self.start {
            self.backspaced = true;
        }
    }
}

/// Saved AutoCorrect options: anything that doesn't read as them reads as the defaults (a
/// damaged `ui.json` never loses the other preferences).
pub fn lenient_autocorrect<'de, D: serde::Deserializer<'de>>(d: D) -> Result<AutoCorrectPrefs, D::Error> {
    let v = Value::deserialize(d)?;
    Ok(serde_json::from_value::<AutoCorrectPrefs>(v).map(AutoCorrectPrefs::cleaned).unwrap_or_default())
}

/// Are `"` and `'` after a Hebrew letter typed as gershayim and geresh
/// ([`AutoCorrectPrefs::hebrew_marks`])? Unless the user chose, with the Hebrew interface: the one
/// that starts new documents right to left ([`Session::new_documents_rtl`]), as the Hebrew
/// edition starts. Elsewhere smart quotes write ” and ’ there, as Word does.
pub fn hebrew_marks(s: &Session) -> bool {
    s.prefs.autocorrect.hebrew_marks.unwrap_or(s.new_documents_rtl)
}

/// `tools.autocorrect`: change AutoCorrect Options and report them.
pub(super) fn autocorrect_options(s: &mut Session, v: &Value) -> CmdResult {
    let hebrew = match v.get("hebrewMarks") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(Value::Bool(b)) => Some(Some(*b)),
        Some(_) => return Err(CmdError::Params("`hebrewMarks` is true, false or null (follow the interface)".into())),
    };
    let ac = &mut s.prefs.autocorrect;
    if let Some(h) = hebrew {
        ac.hebrew_marks = h;
    }
    for (k, f) in [
        ("enabled", &mut ac.enabled as &mut bool),
        ("replaceText", &mut ac.replace_text),
        ("twoInitialCaps", &mut ac.two_initial_caps),
        ("capSentences", &mut ac.cap_sentences),
        ("capCells", &mut ac.cap_cells),
        ("capDays", &mut ac.cap_days),
        ("capsLock", &mut ac.caps_lock),
        ("smartQuotes", &mut ac.smart_quotes),
        ("fractions", &mut ac.fractions),
        ("ordinals", &mut ac.ordinals),
        ("boldItalic", &mut ac.bold_italic),
        ("dashes", &mut ac.dashes),
        ("links", &mut ac.links),
        ("bullets", &mut ac.bullets),
        ("numbering", &mut ac.numbering),
        ("borderLines", &mut ac.border_lines),
        ("tabIndent", &mut ac.tab_indent),
        ("headingStyles", &mut ac.heading_styles),
        ("doubleSpacePeriod", &mut ac.double_space_period),
        ("autoAddFirstLetter", &mut ac.auto_add_first_letter),
        ("autoAddInitialCaps", &mut ac.auto_add_initial_caps),
    ] {
        if let Some(b) = p::bool(v, k) {
            *f = b;
        }
    }
    let bad = |t: &str, max: usize| t.trim().is_empty() || t.chars().count() > max || t.contains(['\n', '\r']);
    if let Some(add) = v.get("add") {
        let (Some(f), Some(t)) = (add.get("from").and_then(Value::as_str), add.get("to").and_then(Value::as_str)) else {
            return Err(CmdError::Params("`add` is {\"from\": string, \"to\": string}".into()));
        };
        if bad(f, MAX_FROM) || t.chars().count() > MAX_TO || t.contains(['\n', '\r']) {
            return Err(CmdError::Params(format!("an entry replaces 1–{MAX_FROM} characters (one line) with at most {MAX_TO}")));
        }
        ac.entries.retain(|(a, _)| a != f);
        // A Hebrew built-in added back as it was is the built-in again, after prefix letters too;
        // a user's entry would match the whole word alone.
        let hebrew_built_in = built_ins().any(|(a, b, l)| a == f && b == t && matches!(l, BuiltInLang::Hebrew { .. }));
        if !hebrew_built_in {
            if ac.entries.len() >= MAX_ENTRIES {
                return Err(CmdError::Params("too many AutoCorrect entries".into()));
            }
            ac.entries.push((f.to_string(), t.to_string()));
        }
        ac.removed.retain(|r| r != f);
    }
    if let Some(f) = p::str(v, "delete") {
        let n = ac.entries.len();
        ac.entries.retain(|(a, _)| a != f);
        if built_ins().any(|(a, _, _)| a == f) && !ac.removed.iter().any(|r| r == f) {
            ac.removed.push(f.to_string());
        } else if n == ac.entries.len() {
            return Err(CmdError::Params(format!("no AutoCorrect entry {f:?}")));
        }
    }
    if let Some(e) = p::str(v, "addException") {
        let e = e.trim().to_lowercase();
        if bad(&e, MAX_FROM) || e.contains(char::is_whitespace) {
            return Err(CmdError::Params("an exception is one abbreviation, like \"approx.\"".into()));
        }
        ac.add_exception(e);
    }
    if let Some(e) = p::str(v, "deleteException") {
        let e = e.trim().to_lowercase();
        ac.exceptions.retain(|r| *r != e);
        if NOT_SENTENCE_END.contains(&e.as_str()) && !ac.exceptions_removed.contains(&e) {
            ac.exceptions_removed.push(e);
        }
    }
    if let Some(w) = p::str(v, "addInitialCapsException") {
        let w = w.trim();
        if bad(w, MAX_FROM) || w.contains(char::is_whitespace) {
            return Err(CmdError::Params("an INitial CAps exception is one word, like \"IDs\"".into()));
        }
        ac.add_initial_caps_exception(w);
    }
    if let Some(w) = p::str(v, "deleteInitialCapsException") {
        let w = w.trim();
        ac.initial_caps_exceptions.retain(|e| e != w);
    }
    let hebrew = hebrew_marks(s);
    let ac = &s.prefs.autocorrect;
    Ok(json!({
        "enabled": ac.enabled,
        "replaceText": ac.replace_text,
        "twoInitialCaps": ac.two_initial_caps,
        "capSentences": ac.cap_sentences,
        "capCells": ac.cap_cells,
        "capDays": ac.cap_days,
        "capsLock": ac.caps_lock,
        "smartQuotes": ac.smart_quotes,
        "hebrewMarks": hebrew,
        "fractions": ac.fractions,
        "ordinals": ac.ordinals,
        "boldItalic": ac.bold_italic,
        "dashes": ac.dashes,
        "links": ac.links,
        "bullets": ac.bullets,
        "numbering": ac.numbering,
        "borderLines": ac.border_lines,
        "tabIndent": ac.tab_indent,
        "headingStyles": ac.heading_styles,
        "doubleSpacePeriod": ac.double_space_period,
        "autoAddFirstLetter": ac.auto_add_first_letter,
        "autoAddInitialCaps": ac.auto_add_initial_caps,
        "entries": ac.list().into_iter().map(|(a, b, builtin)| json!({"from": a, "to": b, "builtIn": builtin})).collect::<Vec<_>>(),
        "user": ac.entries,
        "exceptions": ac.exception_list(),
        "initialCapsExceptions": INITIAL_CAPS_EXCEPTIONS.iter().map(|e| e.to_string()).chain(ac.initial_caps_exceptions.iter().cloned()).collect::<Vec<_>>(),
    }))
}
