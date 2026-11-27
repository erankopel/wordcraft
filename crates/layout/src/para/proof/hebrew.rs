//! Hebrew words as the Hebrew dictionary spells them, on WordCraft's side of the spelling check.
//!
//! The Hebrew Language pack (a Hunspell list of Hebrew words) spells its words without points,
//! and its acronyms and borrowed words with the keyboard's marks: `עו"ד`, `צ'יפס`. Text writes
//! them with points (`שָׁלוֹם`), with the Hebrew marks gershayim and geresh (`עו״ד`, `צ׳יפס`), or
//! with the typographic quotes AutoCorrect put there (`עו”ד`, `צ’יפס`). craft-proof looks a word
//! up as it is written and splits it at `"` and `׳`, so all of those were marked.
//!
//! So a paragraph with Hebrew text is checked as the dictionary reads it ([`View`]): in its
//! Hebrew stretches the points and cantillation marks are left out and a gershayim or geresh
//! between two Hebrew letters is read as `"` or `'` ([`dictionary_form`]); an acronym is checked
//! here whole, as craft-proof would split it; a hyphen between Hebrew and another script parts
//! the two (`ב-Python`), a prefix before it (`ב`, `וב`) being no word of its own. The issues come
//! back at the paragraph's own offsets. Then a word with points is never marked (pointed text is
//! written in the defective spelling the list lacks), and a word of the user dictionary is taken
//! after the prefix letters Hebrew writes in front of a word (ו ה ב ל מ ש כ).

use std::borrow::Cow;
use std::collections::HashSet;
use std::ops::Range;
use std::sync::{Arc, Mutex, PoisonError};

use wordcraft_doc::bidi::{is_hebrew_letter as is_letter, is_hebrew_point as is_point};
use wordcraft_proof::{Issue, IssueKind, Options, ProofLang};

/// What the check reads in place of text that is neither a word to check nor white space (an
/// acronym checked here, a prefix before a hyphen): the byte [`super::proof_text`] writes for
/// objects, one per byte replaced so the offsets after it stay in step.
const FILLER: char = '\u{1}';

/// The message of a spelling mark in a language craft-proof has no message of its own for, as
/// Hebrew: an acronym checked here gets it too, so every Hebrew mark reads the same.
const SPELLING_MESSAGE: &str = "Possible spelling mistake";

/// Is `lang` Hebrew (`he`, or the old code `iw`)?
pub(super) fn is_hebrew(lang: ProofLang) -> bool {
    lang.pack_locale().is_some_and(|l| matches!(l.primary(), "he" | "iw"))
}

/// Does `word` carry points: a point or cantillation mark, or a presentation form holding one
/// (shin with its dot, a letter with dagesh)?
fn pointed(word: &str) -> bool {
    word.chars().any(|c| is_point(c) || matches!(c, '\u{FB1D}' | '\u{FB1F}' | '\u{FB2A}'..='\u{FB4E}'))
}

/// The dictionary's mark for `c` where `c` stands between two Hebrew letters: `"` for a
/// gershayim (`"`, `”`, `״`), `'` for a geresh (`'`, `’`, `׳`).
fn ascii_mark(c: char) -> Option<char> {
    match c {
        '"' | '\u{201D}' | '\u{05F4}' => Some('"'),
        '\'' | '\u{2019}' | '\u{05F3}' => Some('\''),
        _ => None,
    }
}

/// A character craft-proof keeps inside a word: a letter or digit, an apostrophe, a hyphen, a
/// Hebrew point or another combining mark of the Hebrew and Latin scripts.
fn word_char(c: char) -> bool {
    c.is_alphanumeric()
        || is_point(c)
        || matches!(c, '\'' | '\u{2019}' | '-' | '\u{0300}'..='\u{036F}' | '\u{FB1E}' | '\u{1AB0}'..='\u{1AFF}' | '\u{1DC0}'..='\u{1DFF}' | '\u{20D0}'..='\u{20FF}' | '\u{FE20}'..='\u{FE2F}')
}

/// The prefixes Hebrew writes in front of a word, in their order: "and" (ו); "that", "when" or
/// "since" (ש, כש, מש, לכש); "from", "to", "in" or "as" (מ, ל, ב, כ); "the" (ה). Any of them may
/// be missing.
const PREFIX_STEPS: [&[&str]; 4] = [&["ו"], &["ש", "כש", "מש", "לכש"], &["מ", "ל", "ב", "כ"], &["ה"]];

/// Where each prefix `word` may start with ends ([`PREFIX_STEPS`]), in bytes, nearest first;
/// empty when `word` starts with no prefix letter.
fn prefix_ends(word: &str) -> Vec<usize> {
    let mut ends = vec![0];
    for step in PREFIX_STEPS {
        let mut next = ends.clone();
        for &at in &ends {
            next.extend(step.iter().filter(|p| word.get(at..).is_some_and(|w| w.starts_with(**p))).map(|p| at + p.len()));
        }
        ends = next;
    }
    ends.sort_unstable();
    ends.dedup();
    ends.retain(|&e| e > 0);
    ends
}

/// Is `word` prefix letters only (`ב`, `וב`, `וכשה`), with no word after them?
fn is_prefix(word: &str) -> bool {
    !word.is_empty() && prefix_ends(word).contains(&word.len())
}

/// `word` as the Hebrew dictionary spells it: without points and cantillation marks, and with a
/// gershayim or geresh between two Hebrew letters written `"` or `'` (`צה״ל` → `צה"ל`). Other words
/// come back as they are. Add to Dictionary, Ignore All and the suggestions use it to meet the
/// words the check looks up.
pub fn dictionary_form(word: &str) -> Cow<'_, str> {
    if !word.chars().any(|c| is_point(c) || matches!(c, '\u{201D}' | '\u{05F4}' | '\u{2019}' | '\u{05F3}')) {
        return Cow::Borrowed(word);
    }
    let chars: Vec<char> = word.chars().collect();
    let mut out = String::with_capacity(word.len());
    let mut after_letter = false;
    for (k, &c) in chars.iter().enumerate() {
        if is_point(c) {
            continue;
        }
        let joins = after_letter && chars.get(k + 1).is_some_and(|n| is_letter(*n));
        out.push(ascii_mark(c).filter(|_| joins).unwrap_or(c));
        after_letter = is_letter(c);
    }
    Cow::Owned(out)
}

/// The mark `word` writes between two Hebrew letters for the dictionary's `ascii` (`"` or `'`).
fn typed_mark(word: &str, ascii: char) -> Option<char> {
    let chars: Vec<char> = word.chars().collect();
    let mut after_letter = false;
    for (k, &c) in chars.iter().enumerate() {
        if after_letter && ascii_mark(c) == Some(ascii) && chars.get(k + 1).is_some_and(|n| is_letter(*n)) {
            return Some(c);
        }
        if !is_point(c) {
            after_letter = is_letter(c);
        }
    }
    None
}

/// `word` (a suggestion in the dictionary's spelling) with `gershayim` and `geresh` for the
/// dictionary's `"` and `'` between two Hebrew letters.
fn with_marks(word: &str, gershayim: char, geresh: char) -> String {
    let chars: Vec<char> = word.chars().collect();
    let mut out = String::with_capacity(word.len() + 2);
    let mut after_letter = false;
    for (k, &c) in chars.iter().enumerate() {
        let joins = after_letter && chars.get(k + 1).is_some_and(|n| is_letter(*n));
        out.push(match c {
            '"' if joins => gershayim,
            '\'' if joins => geresh,
            _ => c,
        });
        if !is_point(c) {
            after_letter = is_letter(c);
        }
    }
    out
}

/// Spelling suggestions for `word`, misspelled in `lang` (best first, at most `max`). A Hebrew
/// word's come from its dictionary form ([`dictionary_form`]) and are written with the marks the
/// word was typed with, or the Hebrew gershayim (`״`) and geresh (`׳`) when it has none.
pub fn spelling_suggestions(word: &str, max: usize, lang: ProofLang) -> Vec<String> {
    if !is_hebrew(lang) {
        return wordcraft_proof::suggest_in(word, max, lang);
    }
    let gershayim = typed_mark(word, '"').unwrap_or('\u{05F4}');
    let geresh = typed_mark(word, '\'').unwrap_or('\u{05F3}');
    let mut out: Vec<String> = Vec::new();
    for s in wordcraft_proof::suggest_in(&dictionary_form(word), max, lang) {
        let s = with_marks(&s, gershayim, geresh);
        if !out.contains(&s) {
            out.push(s);
        }
    }
    out
}

/// The user dictionary's words with a Hebrew letter, as the dictionary spells them
/// ([`dictionary_form`]), read again whenever the dictionary generation changes.
fn user_words() -> Arc<HashSet<String>> {
    static WORDS: Mutex<Option<(u64, Arc<HashSet<String>>)>> = Mutex::new(None);
    let generation = wordcraft_proof::dictionary_generation();
    let mut cached = WORDS.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some((g, words)) = cached.as_ref()
        && *g == generation
    {
        return words.clone();
    }
    let words: HashSet<String> =
        wordcraft_proof::user_dictionary().iter().filter(|w| w.chars().any(is_letter)).map(|w| dictionary_form(w).into_owned()).collect();
    let words = Arc::new(words);
    *cached = Some((generation, words.clone()));
    words
}

/// Is `word` (a Hebrew word in the dictionary's spelling) a word of the user dictionary, as it is
/// or after prefix letters ([`prefix_ends`]): `ובוורדקראפט` once `וורדקראפט` was added? A word
/// that starts with a vav writes it twice after a prefix (`ויקי`, `בוויקי`).
fn user_word_after_prefix(word: &str) -> bool {
    let words = user_words();
    if words.is_empty() {
        return false;
    }
    let word = word.to_lowercase();
    std::iter::once(0).chain(prefix_ends(&word)).any(|e| {
        let Some(rest) = word.get(e..) else { return false };
        let single_vav = rest.strip_prefix('ו').filter(|r| e > 0 && r.starts_with('ו'));
        rest.chars().count() >= 2 && (words.contains(rest) || single_vav.is_some_and(|r| words.contains(r)))
    })
}

/// `o` (at most the end of `text`) moved back to the start of the character it falls in.
fn char_start(text: &str, o: usize) -> usize {
    let mut o = o.min(text.len());
    while o > 0 && !text.is_char_boundary(o) {
        o -= 1;
    }
    o
}

/// `o` (at most the end of `text`) moved on to the end of the character it falls in.
fn char_end(text: &str, o: usize) -> usize {
    let mut o = o.min(text.len());
    while o < text.len() && !text.is_char_boundary(o) {
        o += 1;
    }
    o
}

/// An acronym checked here rather than by craft-proof, which would split it at its gershayim.
#[derive(Debug, PartialEq)]
struct OwnWord {
    /// Its bytes in the paragraph text, without the apostrophes and hyphens a word is trimmed of.
    range: Range<usize>,
    /// As the dictionary spells it.
    word: String,
    pointed: bool,
    lang: ProofLang,
}

/// The view's text as it is written ([`View::text`]) and where it parts from the paragraph text.
struct Builder {
    out: String,
    anchors: Vec<(usize, usize)>,
}

impl Builder {
    /// Filler for `c`, a byte for each of its bytes, so the offsets after it stay in step.
    fn fill(&mut self, c: char) {
        self.out.extend(std::iter::repeat_n(FILLER, c.len_utf8()));
    }

    /// Leave out the character ending at text offset `end`.
    fn skip(&mut self, end: usize) {
        self.anchors.push((self.out.len(), end));
    }

    /// Write `to` for `c`, which ends at text offset `end`.
    fn swap(&mut self, c: char, to: char, end: usize) {
        self.out.push(to);
        if c.len_utf8() != to.len_utf8() {
            self.anchors.push((self.out.len(), end));
        }
    }
}

/// A paragraph's proofing text as the Hebrew dictionary reads it, and the way back to the text's
/// own offsets. Text outside the Hebrew stretches is as it was.
#[derive(Debug)]
pub(super) struct View {
    text: String,
    /// `(view offset, text offset)` from which the two run in step again after text the view
    /// leaves out or writes shorter, in order: an offset between two anchors lies as far past the
    /// earlier one in both.
    anchors: Vec<(usize, usize)>,
    own: Vec<OwnWord>,
}

impl View {
    /// `text` (the paragraph's proofing text) as the dictionary reads it, where `spans` (each
    /// stretch's language, ordered and disjoint) has Hebrew; `None` without Hebrew.
    pub(super) fn of(text: &str, spans: &[(Range<usize>, ProofLang)]) -> Option<View> {
        if !spans.iter().any(|(_, l)| is_hebrew(*l)) {
            return None;
        }
        let chars: Vec<(usize, char)> = text.char_indices().collect();
        // Each character's language where it is in a Hebrew stretch.
        let mut k = 0;
        let hebrew: Vec<Option<ProofLang>> = chars
            .iter()
            .map(|&(i, _)| {
                while spans.get(k).is_some_and(|(r, _)| r.end <= i) {
                    k += 1;
                }
                spans.get(k).filter(|(r, l)| r.start <= i && is_hebrew(*l)).map(|(_, l)| *l)
            })
            .collect();
        // Whether the nearest character before each one that isn't a point is a Hebrew letter.
        let mut after_letter = Vec::with_capacity(chars.len());
        let mut last = false;
        for &(_, c) in &chars {
            after_letter.push(last);
            if !is_point(c) {
                last = is_letter(c);
            }
        }
        let at = |j: usize| chars.get(j).map(|&(_, c)| c);
        let offset = |j: usize| chars.get(j).map_or(text.len(), |&(i, _)| i);
        let in_hebrew = |j: usize| hebrew.get(j).copied().flatten();
        let follows_letter = |j: usize| after_letter.get(j).copied().unwrap_or(false);
        // A gershayim or geresh between two Hebrew letters in Hebrew text belongs to the word.
        let joins =
            |j: usize| in_hebrew(j).is_some() && at(j).and_then(ascii_mark).is_some() && follows_letter(j) && at(j + 1).is_some_and(is_letter);
        // A hyphen in Hebrew text between a Hebrew letter and a letter or digit of another script
        // (`ב-Python`, `PDF-ים`) parts the two.
        let other = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() && !is_letter(c) && !is_point(c));
        let parts_scripts = |j: usize| {
            in_hebrew(j).is_some()
                && at(j) == Some('-')
                && ((follows_letter(j) && other(at(j + 1))) || (other(j.checked_sub(1).and_then(at)) && at(j + 1).is_some_and(is_letter)))
        };
        let wordy = |j: usize| at(j).is_some_and(word_char) || joins(j);
        let mut b = Builder { out: String::with_capacity(text.len()), anchors: Vec::new() };
        let mut own = Vec::new();
        let mut j = 0;
        while j < chars.len() {
            if !wordy(j) {
                if let Some(c) = at(j) {
                    b.out.push(c);
                }
                j += 1;
                continue;
            }
            let start = j;
            while j < chars.len() && wordy(j) {
                j += 1;
            }
            // The word's parts between the hyphens that part two scripts.
            let mut from = start;
            for h in start..=j {
                let cut = h < j && parts_scripts(h);
                if h < j && !cut {
                    continue;
                }
                let part = from..h;
                let prefix = cut
                    && follows_letter(h)
                    && part.clone().all(|k| in_hebrew(k).is_some() && at(k).is_some_and(|c| is_letter(c) || is_point(c)))
                    && is_prefix(&part.clone().filter_map(at).filter(|c| !is_point(*c)).collect::<String>());
                if prefix {
                    // Prefix letters before a word of another script (`וב-Python`): no word of their own.
                    part.filter_map(at).for_each(|c| b.fill(c));
                } else if part.clone().any(|k| joins(k) && at(k).and_then(ascii_mark) == Some('"')) {
                    // An acronym (`עו"ד`, `צה״ל`): checked whole here, craft-proof reads none of it.
                    part.clone().filter_map(at).for_each(|c| b.fill(c));
                    let edge = |k: usize| at(k).is_some_and(|c| matches!(c, '\'' | '\u{2019}' | '-'));
                    let (mut a, mut z) = (part.start, part.end);
                    while a < z && edge(a) {
                        a += 1;
                    }
                    while z > a && edge(z - 1) {
                        z -= 1;
                    }
                    let range = offset(a)..offset(z);
                    let written = text.get(range.clone()).unwrap_or_default();
                    if let Some(lang) = (a..z).find_map(in_hebrew) {
                        own.push(OwnWord { word: dictionary_form(written).into_owned(), pointed: pointed(written), range, lang });
                    }
                } else {
                    for k in part {
                        let Some(c) = at(k) else { continue };
                        if in_hebrew(k).is_some() && is_point(c) {
                            b.skip(offset(k + 1));
                        } else if let Some(m) = ascii_mark(c).filter(|_| joins(k)) {
                            b.swap(c, m, offset(k + 1));
                        } else {
                            b.out.push(c);
                        }
                    }
                }
                if cut {
                    b.fill('-');
                }
                from = h + 1;
            }
        }
        Some(View { text: b.out, anchors: b.anchors, own })
    }

    /// The paragraph text offset of view offset `v`. Past text left out, it is where the text
    /// goes on, so a word's end takes its last points along.
    fn to_text(&self, v: usize) -> usize {
        let k = self.anchors.partition_point(|&(av, _)| av <= v);
        match k.checked_sub(1).and_then(|k| self.anchors.get(k)) {
            Some(&(av, ao)) => ao + (v - av),
            None => v,
        }
    }

    /// The view offset of paragraph text offset `o`.
    fn to_view(&self, o: usize) -> usize {
        let k = self.anchors.partition_point(|&(_, ao)| ao <= o);
        match k.checked_sub(1).and_then(|k| self.anchors.get(k)) {
            Some(&(av, ao)) => av + (o - ao),
            None => o,
        }
    }

    /// Spelling and grammar issues of `text`, the paragraph's proofing text this view was made of,
    /// at its own offsets and in order ([`wordcraft_proof::check_text_with`] on the view), less a
    /// Hebrew word with points and a word of the user dictionary after Hebrew prefix letters; with
    /// the acronyms checked here.
    pub(super) fn check(&self, text: &str, spans: &[(Range<usize>, ProofLang)], opts: &Options, grammar: bool, list_item: bool) -> Vec<Issue> {
        let spans: Vec<(Range<usize>, ProofLang)> = spans.iter().map(|(r, l)| (self.to_view(r.start)..self.to_view(r.end), *l)).collect();
        let found = wordcraft_proof::check_text_with(&self.text, &spans, opts, grammar, list_item);
        let mut out = Vec::with_capacity(found.len());
        for i in found {
            // An issue never starts or ends inside filler, which stands for whole words; were a
            // grammar rule's to, it still covers whole characters.
            let (start, end) = (char_start(text, self.to_text(i.start)), char_end(text, self.to_text(i.end)));
            if i.kind == IssueKind::Spelling && is_hebrew(i.lang) {
                let word = self.text.get(i.start..i.end).unwrap_or_default();
                if text.get(start..end).is_some_and(pointed) || user_word_after_prefix(word) {
                    continue;
                }
            }
            out.push(Issue { start, end, ..i });
        }
        for w in &self.own {
            if w.pointed || wordcraft_proof::is_correct_in_with(&w.word, w.lang, opts) || user_word_after_prefix(&w.word) {
                continue;
            }
            out.push(Issue {
                start: w.range.start,
                end: w.range.end,
                kind: IssueKind::Spelling,
                message: SPELLING_MESSAGE.into(),
                suggestions: Vec::new(),
                lang: w.lang,
            });
        }
        out.sort_by_key(|i| (i.start, i.end));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn he() -> ProofLang {
        ProofLang::from_tag(Some("he-IL"))
    }

    /// A paragraph all in Hebrew.
    fn hebrew_view(text: &str) -> View {
        View::of(text, &[(0..text.len(), he())]).expect("Hebrew text has a view")
    }

    /// The view's words (filler and spaces apart), as craft-proof's word rule would read them.
    fn view_words(v: &View) -> Vec<&str> {
        v.text.split(|c: char| c.is_whitespace() || c == FILLER || c == ',' || c == '.').filter(|w| !w.is_empty()).collect()
    }

    /// Each view offset of a character goes back to that character's offset in the text, and the
    /// end of a word to the end of its last points, so a mark covers the word as written. (Filler
    /// stands byte for byte for what it hides, so it maps back in step too.)
    fn assert_round_trip(text: &str, v: &View) {
        for (i, c) in v.text.char_indices().filter(|(_, c)| *c != FILLER) {
            let o = v.to_text(i);
            assert!(text.is_char_boundary(o), "{i} → {o}");
            if ascii_mark(c).is_none() {
                assert_eq!(text[o..].chars().next(), Some(c), "view {i} → text {o}");
            }
            assert_eq!(v.to_view(o), i, "text {o} back to view {i}");
        }
        assert_eq!(v.to_text(v.text.len()), text.len(), "the ends meet");
    }

    #[test]
    fn only_hebrew_text_gets_a_view() {
        let text = "hello שלום";
        assert!(View::of(text, &[(0..text.len(), ProofLang::En)]).is_none());
        assert!(View::of(text, &[(0..6, ProofLang::En), (6..text.len(), he())]).is_some());
        assert!(is_hebrew(he()) && is_hebrew(ProofLang::from_tag(Some("iw"))));
        assert!(!is_hebrew(ProofLang::from_tag(Some("ar-SA"))) && !is_hebrew(ProofLang::En));
    }

    /// SE-1: the points and cantillation marks are left out of the words the dictionary reads,
    /// and every offset after them maps back exactly.
    #[test]
    fn points_are_left_out_and_offsets_map_back() {
        // שָׁלוֹם לְךָ, יֶלֶד טוֹב. and Genesis 1:1 with its cantillation.
        let text = "\u{05E9}\u{05C1}\u{05B8}\u{05DC}\u{05D5}\u{05B9}\u{05DD} \u{05DC}\u{05B0}\u{05DA}\u{05B8}, \u{05D9}\u{05B6}\u{05DC}\u{05B6}\u{05D3} \u{05D8}\u{05D5}\u{05B9}\u{05D1}. \u{05D1}\u{05BC}\u{05B0}\u{05E8}\u{05B5}\u{05D0}\u{05E9}\u{05C1}\u{05B4}\u{05D9}\u{05EA}\u{0596} \u{05D4}\u{05B7}\u{05E9}\u{05BC}\u{05B8}\u{05DE}\u{05B7}\u{0591}\u{05D9}\u{05B4}\u{05DD} הילדדים";
        let v = hebrew_view(text);
        assert_eq!(view_words(&v), ["שלום", "לך", "ילד", "טוב", "בראשית", "השמים", "הילדדים"]);
        assert_round_trip(text, &v);
        // The unpointed word after them is found where it is written.
        let at = v.text.find("הילדדים").unwrap();
        assert_eq!(&text[v.to_text(at)..v.to_text(at + "הילדדים".len())], "הילדדים");
        // A pointed word's end takes its last point along: לְךָ ends after its qamats.
        let lekha = v.text.find("לך").unwrap();
        let written = &text[v.to_text(lekha)..v.to_text(lekha + "לך".len())];
        assert!(written.ends_with('\u{05B8}') && pointed(written), "{written:?}");
    }

    /// SE-2: a geresh between Hebrew letters is the dictionary's `'` whatever it was typed as; a
    /// gershayim makes an acronym, checked whole in its dictionary form; the offsets around them
    /// still map back.
    #[test]
    fn gershayim_and_geresh_between_letters_are_the_dictionarys_marks() {
        for (typed, gershayim, geresh) in [("ASCII", '"', '\''), ("typographic", '\u{201D}', '\u{2019}'), ("Hebrew", '\u{05F4}', '\u{05F3}')] {
            let text = format!("עו{gershayim}ד ביה{gershayim}ס, צ{geresh}יפס וג{geresh}ירפה טעות");
            let v = hebrew_view(&text);
            assert_round_trip(&text, &v);
            assert_eq!(view_words(&v), ["צ'יפס", "וג'ירפה", "טעות"], "{typed}: the acronyms are not craft-proof's to read");
            let own: Vec<(&str, &str)> = v.own.iter().map(|w| (&text[w.range.clone()], w.word.as_str())).collect();
            let want_ad = format!("עו{gershayim}ד");
            let want_school = format!("ביה{gershayim}ס");
            assert_eq!(own, [(want_ad.as_str(), "עו\"ד"), (want_school.as_str(), "ביה\"ס")], "{typed}");
            assert!(v.own.iter().all(|w| !w.pointed && w.lang == he()));
        }
        // A mark that isn't between two Hebrew letters is left as it is: quotes around a word, an
        // apostrophe in English, a geresh after the last letter.
        let text = "“שלום” \"כן\" don’t וכו׳";
        let v = hebrew_view(text);
        assert_eq!(v.text, text);
        assert!(v.own.is_empty());
    }

    /// A pointed acronym is still one word, and known to carry points.
    #[test]
    fn a_pointed_acronym_is_one_pointed_word() {
        let text = "\u{05E6}\u{05B8}\u{05D4}\u{05B7}\u{05F4}\u{05DC} אמר";
        let v = hebrew_view(text);
        assert_eq!(v.own.len(), 1);
        assert_eq!(v.own[0].word, "צה\"ל");
        assert!(v.own[0].pointed);
        assert_eq!(&text[v.own[0].range.clone()], "\u{05E6}\u{05B8}\u{05D4}\u{05B7}\u{05F4}\u{05DC}");
        assert_round_trip(text, &v);
    }

    /// SE-8: a prefix joined by a hyphen to a word of another script is no word of its own, and
    /// the hyphen parts the two; a Hebrew word before the hyphen is still read, and a hyphen
    /// between Hebrew words still joins them.
    #[test]
    fn a_hyphen_parts_hebrew_from_another_script() {
        let text = "כתבתי ב-Python וגם וב-JavaScript, שלוםם-Excel ב-2020 PDF-ים בית-ספר";
        let v = hebrew_view(text);
        assert_round_trip(text, &v);
        assert_eq!(view_words(&v), ["כתבתי", "Python", "וגם", "JavaScript", "שלוםם", "Excel", "2020", "PDF", "ים", "בית-ספר"]);
        // In a paragraph whose Latin words are English (an English run with Hebrew as its
        // complex-script language), the Latin word is left where its English stretch starts.
        let text = "קוד ב-Python";
        let latin = text.find('P').unwrap();
        let v = View::of(text, &[(0..latin, he()), (latin..text.len(), ProofLang::En)]).unwrap();
        assert_eq!(view_words(&v), ["קוד", "Python"]);
        assert_eq!(v.to_view(latin), latin, "the English stretch starts at the same offset");
        // Only the prefix letters go: a word ending in those letters before a hyphen stays.
        for (prefix, gone) in [("ו", true), ("וכשה", true), ("שב", true), ("מה", true), ("לכש", true), ("בב", false), ("בית", false)]
        {
            let text = format!("{prefix}-Word");
            let v = hebrew_view(&text);
            assert_eq!(!v.text.contains(prefix), gone, "{prefix}");
        }
    }

    #[test]
    fn hebrew_prefixes_in_their_order() {
        fn ends(w: &str) -> Vec<&str> {
            prefix_ends(w).into_iter().map(|e| &w[..e]).collect()
        }
        assert_eq!(ends("ובוורדקראפט"), ["ו", "וב"]);
        assert_eq!(ends("וכשהמחשב"), ["ו", "וכ", "וכש", "וכשה"]);
        assert_eq!(ends("משהגיע"), ["מ", "מש", "משה"]);
        assert!(ends("קראפט").is_empty());
        // ה comes last: "the" after "from", never before it.
        assert_eq!(ends("המבית"), ["ה"]);
        assert!(is_prefix("וב") && is_prefix("כשה") && !is_prefix("בב") && !is_prefix("") && !is_prefix("ספר"));
    }

    #[test]
    fn dictionary_forms_and_suggestions_in_the_typed_marks() {
        assert_eq!(dictionary_form("צה״ל"), "צה\"ל");
        assert_eq!(dictionary_form("צ’יפס"), "צ'יפס");
        assert_eq!(dictionary_form("\u{05E9}\u{05C1}\u{05B8}\u{05DC}\u{05D5}\u{05B9}\u{05DD}"), "שלום");
        assert!(matches!(dictionary_form("don’t"), Cow::Owned(_)), "read, but an English apostrophe stays");
        assert_eq!(dictionary_form("don’t"), "don’t");
        assert!(matches!(dictionary_form("שלום"), Cow::Borrowed(_)));
        // The marks a word was typed with, else the Hebrew ones.
        assert_eq!(typed_mark("מנכ”לל", '"'), Some('\u{201D}'));
        assert_eq!(typed_mark("מנכלל", '"'), None);
        assert_eq!(with_marks("מנכ\"ל", '\u{201D}', '\u{05F3}'), "מנכ”ל");
        assert_eq!(with_marks("צ'יפס \"כן\"", '\u{05F4}', '\u{05F3}'), "צ׳יפס \"כן\"", "only between Hebrew letters");
    }
}
