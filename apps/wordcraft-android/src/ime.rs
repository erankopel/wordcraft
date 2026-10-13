//! Typing on the on-screen keyboard.
//!
//! winit 0.30 hands an Android app key events only: the text a soft keyboard produces (letters,
//! autocorrected words, suggestions, composing text) arrives in GameActivity as `TextEvent`s, which
//! winit logs as unknown and drops. The state behind those events is still readable
//! (`AndroidApp::text_input_state`), so this module reads it and turns what changed into the
//! editor's own input: the word being composed (shown at the caret, not yet in the document), the
//! text the keyboard finished, and Backspace and Enter key presses.
//!
//! The keyboard edits a small buffer of its own, which always ends where the editor's caret is. When
//! the caret moves (a tap, a cursor key) the buffer is filled again with the text before the new
//! caret ([`Bridge::seed`]), so what the keyboard sees is what the document holds: Backspace has
//! something to delete (a keyboard with an empty buffer sends a delete that changes nothing),
//! capitalisation follows the real previous sentence, and suggestions see the real previous words.
//! [`Bridge`] remembers the part of the buffer already given to the editor, so only what changed is
//! sent, whether the keyboard appended a word, deleted a character in the buffer (Backspace), or
//! replaced an earlier word (autocorrect).
//!
//! A Backspace is counted the way the editor will perform it: one press removes a whole grapheme
//! cluster (an emoji with its modifiers, a letter with its marks), so a keyboard that deletes one
//! code point at a time leaves the rest of the cluster to be typed again, and one that deletes a
//! whole cluster at once costs a single press.
//!
//! Everything here is plain data in, events out, with tests that run on any host; the Android
//! calls are in `keyboard`.

use wordcraft_doc::cluster;

/// What the editor should be given, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Output {
    /// Press Backspace this many times (the keyboard deleted text that is already in the document).
    Backspace(usize),
    /// Text the keyboard finished: type it at the caret.
    Commit(String),
    /// The Enter key (the keyboard typed a line break).
    Enter,
    /// The word being composed, shown at the caret but not yet part of the document; empty when
    /// there is none.
    Preedit(String),
}

/// The keyboard's text buffer: its text and the part being composed, as character positions.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub text: String,
    /// The composing region (characters), if any.
    pub compose: Option<(usize, usize)>,
    /// The keyboard's cursor is at the end of the text (it is, unless the person moved it inside
    /// the buffer, with a swipe on the space bar for instance).
    pub at_end: bool,
}

impl Snapshot {
    /// A snapshot from what GameActivity reports: the text, the composing region and the selection
    /// (when known), with spans in UTF-16 code units (Java's string indices). Spans that don't fit
    /// the text are clamped; a reversed span is swapped; an empty or undefined one means no
    /// composition.
    pub fn from_utf16_spans(text: String, compose: Option<(usize, usize)>, selection: Option<(usize, usize)>) -> Snapshot {
        let compose = compose.and_then(|(a, b)| {
            let (a, b) = if a <= b { (a, b) } else { (b, a) };
            let (a, b) = (utf16_to_char(&text, a), utf16_to_char(&text, b));
            (a < b).then_some((a, b))
        });
        let units = text.encode_utf16().count();
        let at_end = selection.is_none_or(|(a, b)| a >= units && b >= units);
        Snapshot { text, compose, at_end }
    }
}

/// The character position of a UTF-16 offset in `text` (past the end clamps to the length; an
/// offset inside a surrogate pair counts as before that character).
pub fn utf16_to_char(text: &str, offset: usize) -> usize {
    let mut units = 0;
    for (i, c) in text.chars().enumerate() {
        if units >= offset {
            return i;
        }
        units += c.len_utf16();
    }
    text.chars().count()
}

/// More characters than this deleted from the keyboard's buffer in one change are not a deletion:
/// they are a buffer that was replaced or could not be read (a person deletes a word, not a
/// paragraph, between two reads of 30 ms).
const MAX_DELETE_CHARS: usize = 24;

/// Remembers what has been given to the editor.
#[derive(Debug, Default)]
pub struct Bridge {
    /// The committed part of the keyboard's buffer the editor has been given.
    given: Vec<char>,
    /// The composing text the editor is showing.
    preedit: String,
}

impl Bridge {
    /// The keyboard's buffer is now `now`: what to give the editor so it matches. The committed
    /// text is what comes before the composing region (all of it without one); the composing text
    /// is shown as preedit. A composing region that starts inside text already given (a word the
    /// keyboard reopened for suggestions) leaves that text in the document: only its replacement,
    /// if the keyboard makes one, is sent.
    pub fn observe(&mut self, now: &Snapshot) -> Vec<Output> {
        let chars: Vec<char> = now.text.chars().collect();
        let (committed, composing): (Vec<char>, String) = match now.compose {
            Some((a, b)) if a < b && b <= chars.len() && a >= self.given.len() => {
                (chars.get(..a).unwrap_or_default().to_vec(), chars.get(a..b).unwrap_or_default().iter().collect())
            }
            _ => (chars, String::new()),
        };
        let same = self.given.iter().zip(&committed).take_while(|(a, b)| a == b).count();
        let mut out = Vec::new();
        // What has to be typed again after the deletion: the rest of a cluster the editor deletes
        // whole although the keyboard deleted only part of it, then what the keyboard added.
        let mut typed = String::new();
        if self.given.len() > same {
            let (presses, again) = deletion(&self.given, same);
            if presses > 0 {
                out.push(Output::Backspace(presses));
            }
            typed = again;
        }
        typed.extend(committed.get(same..).unwrap_or_default());
        push_text(&mut out, &typed);
        self.given = committed;
        // Committing text clears the preedit the editor shows, so a word still being composed is
        // sent again after it.
        let cleared = out.iter().any(|o| matches!(o, Output::Commit(_)));
        if composing != self.preedit || (cleared && !composing.is_empty()) {
            out.push(Output::Preedit(composing.clone()));
            self.preedit = composing;
        }
        out
    }

    /// The snapshot is not an edit of what was given: the keyboard's buffer was unreadable (it reads
    /// as empty), or replaced, or a late read of an older buffer. Giving it to the editor would
    /// delete text that is there and type it again, so the buffer is started over instead.
    pub fn out_of_step(&self, now: &Snapshot) -> bool {
        let chars = now.text.chars();
        let same = self.given.iter().zip(chars).take_while(|(a, b)| *a == b).count();
        let removed = self.given.len().saturating_sub(same);
        removed > MAX_DELETE_CHARS || (now.text.is_empty() && self.given.len() > 1)
    }

    /// The keyboard's buffer was emptied (or the caret moved somewhere else): forget it. A word
    /// that was being composed is committed as it stands, where the caret was, before the caret
    /// moves; the keyboard starts over.
    pub fn reset(&mut self) -> Vec<Output> {
        self.given.clear();
        if self.preedit.is_empty() { Vec::new() } else { vec![Output::Commit(std::mem::take(&mut self.preedit))] }
    }

    /// The keyboard's buffer was filled with `context`, the text before the caret: it is in the
    /// document already, so only what changes from here is sent.
    pub fn seed(&mut self, context: &str) {
        self.given = context.chars().collect();
    }

    /// A word is being composed (shown, not yet in the document).
    pub fn composing(&self) -> bool {
        !self.preedit.is_empty()
    }

    /// How many characters of the keyboard's buffer the editor has been given.
    pub fn given_len(&self) -> usize {
        self.given.len()
    }

    /// The part of the buffer given, as text.
    pub fn given_text(&self) -> String {
        self.given.iter().collect()
    }
}

/// How many Backspace presses the editor needs to delete `given[keep..]` (cluster by cluster, as
/// its Backspace does), and the text to type afterwards when those presses took more than that
/// (the rest of a cluster the keyboard only partly deleted).
fn deletion(given: &[char], keep: usize) -> (usize, String) {
    let text: String = given.iter().collect();
    let keep_bytes: usize = given.iter().take(keep).map(|c| c.len_utf8()).sum();
    let mut end = text.len();
    let mut presses = 0;
    while end > keep_bytes && presses < given.len() {
        let next = cluster::backspace_from(&text, end);
        if next >= end {
            break;
        }
        end = next;
        presses += 1;
    }
    let again = if end < keep_bytes { text.get(end..keep_bytes).unwrap_or_default().to_owned() } else { String::new() };
    (presses, again)
}

/// How much of the document's text before the caret the keyboard's buffer is filled with.
pub const CONTEXT_CHARS: usize = 120;

/// The text to fill the keyboard's buffer with when the caret is somewhere new: the end of the
/// caret's paragraph before the caret (`before_caret`), at most [`CONTEXT_CHARS`] characters. When
/// that reaches the paragraph's start a line break comes first, so the keyboard sees a new
/// paragraph (it capitalises, and a Backspace there is the editor's Backspace at a paragraph's
/// start). Pictures and fields (the object character) and other control characters are left out; a
/// line break inside the paragraph is a line break.
pub fn context(before_caret: &str) -> String {
    let mut chars: Vec<char> = before_caret
        .chars()
        .rev()
        .filter(|c| *c != wordcraft_doc::para::OBJ)
        .map(|c| if c == '\u{000B}' || c == '\u{2028}' || c == '\u{2029}' { '\n' } else { c })
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .take(CONTEXT_CHARS)
        .collect();
    let at_start = chars.len() < CONTEXT_CHARS;
    chars.reverse();
    let mut text = String::with_capacity(chars.len() + 1);
    if at_start {
        text.push('\n');
    }
    text.extend(chars);
    text
}

/// A commit longer than this goes to the editor as one piece.
const LONG_COMMIT_CHARS: usize = 64;

/// Add the text the editor should type: a line break is the Enter key, not text, and CR or CRLF
/// (what a pasted clipboard suggestion may hold) is one line break.
fn push_text(out: &mut Vec<Output>, text: &str) {
    if text.is_empty() {
        return;
    }
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines = text.split('\n');
    if let Some(first) = lines.next()
        && !first.is_empty()
    {
        out.push(Output::Commit(first.to_owned()));
    }
    for line in lines {
        out.push(Output::Enter);
        if !line.is_empty() {
            out.push(Output::Commit(line.to_owned()));
        }
    }
}

/// Split text to type into the pieces the editor's typing rules see one at a time: AutoCorrect,
/// smart quotes and list or dash AutoFormat act on a single typed space, punctuation mark or quote
/// (Word's rules, `typing-parity.md`), so those are separate pieces and the words between them are
/// whole.
pub fn typing_pieces(text: &str) -> Vec<String> {
    // A long commit is a paste or a dictated phrase, not a word typed: it goes in whole (and quickly:
    // each piece is an edit with its own bookkeeping).
    if text.chars().count() > LONG_COMMIT_CHARS {
        return vec![text.to_owned()];
    }
    let mut pieces = Vec::new();
    let mut run = String::new();
    for c in text.chars() {
        if c == ' ' || ",.;:!?\"'".contains(c) {
            if !run.is_empty() {
                pieces.push(std::mem::take(&mut run));
            }
            pieces.push(c.to_string());
        } else {
            run.push(c);
        }
    }
    if !run.is_empty() {
        pieces.push(run);
    }
    pieces
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(text: &str, compose: Option<(usize, usize)>) -> Snapshot {
        Snapshot { text: text.to_owned(), compose, at_end: true }
    }

    #[test]
    fn a_word_is_composed_then_committed_with_its_space() {
        let mut b = Bridge::default();
        assert_eq!(b.observe(&snap("h", Some((0, 1)))), vec![Output::Preedit("h".into())]);
        assert_eq!(b.observe(&snap("he", Some((0, 2)))), vec![Output::Preedit("he".into())]);
        assert_eq!(b.observe(&snap("he", Some((0, 2)))), vec![], "nothing changed, nothing sent");
        // The keyboard finishes the word and types the space in one go.
        assert_eq!(b.observe(&snap("he ", None)), vec![Output::Commit("he ".into()), Output::Preedit(String::new())]);
        // The next word composes after it.
        assert_eq!(b.observe(&snap("he w", Some((3, 4)))), vec![Output::Preedit("w".into())]);
        assert_eq!(b.observe(&snap("he wo", Some((3, 5)))), vec![Output::Preedit("wo".into())]);
    }

    #[test]
    fn deleting_in_the_buffer_is_backspace_and_autocorrect_replaces_the_word() {
        let mut b = Bridge::default();
        b.observe(&snap("teh ", None));
        // Backspace in the keyboard's buffer.
        assert_eq!(b.observe(&snap("teh", None)), vec![Output::Backspace(1)]);
        // Autocorrect swaps the word it just typed.
        assert_eq!(b.observe(&snap("the ", None)), vec![Output::Backspace(2), Output::Commit("he ".into())]);
        // Deleting everything.
        assert_eq!(b.observe(&snap("", None)), vec![Output::Backspace(4)]);
    }

    #[test]
    fn a_word_still_being_composed_survives_a_commit_before_it() {
        let mut b = Bridge::default();
        b.observe(&snap("one 😀", Some((4, 5))));
        // The keyboard corrects the committed text and keeps composing: committing clears the
        // editor's preedit, so it is sent again.
        assert_eq!(b.observe(&snap("one! 😀", Some((5, 6)))), vec![Output::Backspace(1), Output::Commit("! ".into()), Output::Preedit("😀".into())]);
    }

    #[test]
    fn a_line_break_is_the_enter_key() {
        let mut b = Bridge::default();
        assert_eq!(b.observe(&snap("one\ntwo", None)), vec![Output::Commit("one".into()), Output::Enter, Output::Commit("two".into())]);
        assert_eq!(b.observe(&snap("one\ntwo\n", None)), vec![Output::Enter]);
        assert_eq!(b.observe(&snap("one\ntwo\n\n", None)), vec![Output::Enter]);
        // A clipboard suggestion with Windows line breaks is one break each, not two.
        let mut b = Bridge::default();
        assert_eq!(
            b.observe(&snap("a\r\nb\rc", None)),
            vec![Output::Commit("a".into()), Output::Enter, Output::Commit("b".into()), Output::Enter, Output::Commit("c".into())]
        );
    }

    #[test]
    fn a_word_the_keyboard_reopens_stays_in_the_document() {
        let mut b = Bridge::default();
        b.seed("hello ");
        // The keyboard marks the word before the caret as composing again, as it does after the
        // caret was put at the end of a word: nothing changes in the document.
        assert_eq!(b.observe(&snap("hello ", Some((0, 5)))), vec![]);
        // Replacing it by a suggestion replaces the word in the document.
        assert_eq!(b.observe(&snap("Hello ", None)), vec![Output::Backspace(6), Output::Commit("Hello ".into())]);
        // Typing on after a reopened word extends the document, with the new part composing.
        let mut b = Bridge::default();
        b.seed("he");
        assert_eq!(b.observe(&snap("hello", Some((0, 5)))), vec![Output::Commit("llo".into())]);
    }

    #[test]
    fn backspace_after_a_seeded_context_is_seen_and_counted() {
        let mut b = Bridge::default();
        b.seed("Dear Sam,\nSee you");
        assert_eq!(b.observe(&snap("Dear Sam,\nSee yo", None)), vec![Output::Backspace(1)]);
        // Deleting back over the line break is one press as well (it joins the paragraphs).
        let mut b = Bridge::default();
        b.seed("\nNew");
        assert_eq!(b.observe(&snap("\nNe", None)), vec![Output::Backspace(1)]);
        assert_eq!(b.observe(&snap("\nN", None)), vec![Output::Backspace(1)]);
        assert_eq!(b.observe(&snap("\n", None)), vec![Output::Backspace(1)]);
        assert_eq!(b.observe(&snap("", None)), vec![Output::Backspace(1)]);
        assert_eq!(b.given_len(), 0);
    }

    #[test]
    fn spans_are_utf16_units_and_junk_is_clamped() {
        // An emoji is two UTF-16 units but one character: composing the letter a and the emoji is units 1..4.
        let s = Snapshot::from_utf16_spans("xa😀".into(), Some((1, 4)), None);
        assert_eq!(s.compose, Some((1, 3)));
        assert_eq!(Snapshot::from_utf16_spans("abc".into(), Some((2, 99)), None).compose, Some((2, 3)));
        assert_eq!(Snapshot::from_utf16_spans("abc".into(), Some((3, 1)), None).compose, Some((1, 3)), "a reversed span is swapped");
        assert_eq!(Snapshot::from_utf16_spans("abc".into(), Some((2, 2)), None).compose, None, "an empty span is no composition");
        assert_eq!(Snapshot::from_utf16_spans("abc".into(), Some((9, 12)), None).compose, None);
        assert_eq!(Snapshot::from_utf16_spans("שלום".into(), Some((0, 4)), None).compose, Some((0, 4)), "Hebrew letters are one unit each");
        assert_eq!(utf16_to_char("a😀b", 3), 2);
        assert_eq!(utf16_to_char("a😀b", 2), 2, "inside a pair counts as before it");
        assert_eq!(utf16_to_char("", 5), 0);
        // A composing span beyond the text is ignored rather than slicing out of range.
        let mut b = Bridge::default();
        assert_eq!(b.observe(&snap("ab", Some((1, 9)))), vec![Output::Commit("ab".into())]);
    }

    #[test]
    fn the_cursor_is_at_the_end_only_when_the_selection_is() {
        let at = |text: &str, sel| Snapshot::from_utf16_spans(text.into(), None, sel).at_end;
        assert!(at("hello", None), "unknown counts as at the end");
        assert!(at("hello", Some((5, 5))));
        assert!(at("", Some((0, 0))));
        assert!(at("a😀", Some((3, 3))), "UTF-16 units: the emoji is two");
        assert!(!at("hello", Some((2, 2))), "a swipe on the space bar");
        assert!(!at("hello", Some((0, 5))), "a selection inside the buffer");
        assert!(!at("a😀", Some((2, 2))));
    }

    #[test]
    fn backspace_counts_clusters_and_types_back_what_it_took_too() {
        let given = |s: &str| s.chars().collect::<Vec<_>>();
        // A keyboard that deletes a whole emoji (a heart and its variation selector) at once: one press.
        assert_eq!(deletion(&given("I like it \u{2764}\u{FE0F}"), 10), (1, String::new()));
        // A family emoji of five code points: one press.
        let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
        assert_eq!(deletion(&given(&format!("a{family}")), 1), (1, String::new()));
        // A flag: two regional indicators, one press.
        assert_eq!(deletion(&given("a\u{1F1EE}\u{1F1F1}"), 1), (1, String::new()));
        // A keyboard that deletes one code point at a time: the editor removes the whole cluster,
        // so what the keyboard kept of it is typed again.
        assert_eq!(deletion(&given("e\u{0301}"), 1), (1, "e".to_owned()));
        assert_eq!(deletion(&given("\u{1F44D}\u{1F3FD}"), 1), (1, "\u{1F44D}".to_owned()));
        // Hebrew: a letter with a vowel mark and a dagesh is one cluster.
        assert_eq!(deletion(&given("\u{05D1}\u{05B0}\u{05BC}"), 2), (1, "\u{05D1}\u{05B0}".to_owned()));
        // Plain text: one press per character.
        assert_eq!(deletion(&given("abc"), 0), (3, String::new()));
        // Thai deletes a character at a time, as the editor does.
        assert_eq!(deletion(&given("\u{0E01}\u{0E34}"), 1), (1, String::new()));
        // Nothing to delete past the start.
        assert_eq!(deletion(&given(""), 0), (0, String::new()));
    }

    #[test]
    fn the_buffer_is_filled_with_the_paragraph_before_the_caret() {
        // A short paragraph reaches its start: a line break first, so the keyboard capitalises and
        // Backspace at the start is the editor's.
        assert_eq!(context("Hello wor"), "\nHello wor");
        assert_eq!(context(""), "\n");
        // A long one is cut to the last characters, with no line break.
        let long = "x".repeat(500);
        assert_eq!(context(&long), "x".repeat(CONTEXT_CHARS));
        // Pictures, fields and control characters are left out; a soft line break is a break.
        assert_eq!(context("a\u{FFFC}b\u{0007}c\u{000B}d"), "\nabc\nd");
        // Characters, not bytes: Hebrew and emoji count once.
        let hebrew = "שלום".repeat(100);
        assert_eq!(context(&hebrew).chars().count(), CONTEXT_CHARS);
        assert_eq!(context("😀"), "\n😀");
    }

    #[test]
    fn typed_text_is_cut_into_what_the_typing_rules_see_one_at_a_time() {
        assert_eq!(typing_pieces("teh "), vec!["teh", " "]);
        assert_eq!(typing_pieces("don't \"go\"."), vec!["don", "'", "t", " ", "\"", "go", "\"", "."]);
        assert_eq!(typing_pieces("1. "), vec!["1", ".", " "]);
        assert_eq!(typing_pieces("שלום, עולם"), vec!["שלום", ",", " ", "עולם"]);
        assert_eq!(typing_pieces(""), Vec::<String>::new());
        assert_eq!(typing_pieces("plain"), vec!["plain"]);
    }

    #[test]
    fn a_snapshot_that_is_not_an_edit_of_the_buffer_is_out_of_step() {
        let full = "Dear Sam,\nSee you soon. Thanks for the long letter and the kind words.";
        let len = full.chars().count();
        let mut b = Bridge::default();
        b.seed(full);
        let cut = |n: usize| Snapshot { text: full.chars().take(len - n).collect(), compose: None, at_end: true };
        // A Backspace, a word: edits.
        assert!(!b.out_of_step(&cut(1)));
        assert!(!b.out_of_step(&cut(10)));
        assert!(!b.out_of_step(&cut(24)));
        // A paragraph at once, or an unreadable buffer (it reads as empty): not a deletion.
        assert!(b.out_of_step(&cut(25)));
        assert!(b.out_of_step(&Snapshot::default()));
        // An older, shorter buffer replacing a newer one is out of step as well.
        let mut b = Bridge::default();
        b.seed(&"x".repeat(60));
        assert!(b.out_of_step(&Snapshot { text: "y".into(), compose: None, at_end: true }));
        // An empty buffer after a single character is the character's deletion.
        let mut b = Bridge::default();
        b.seed("a");
        assert!(!b.out_of_step(&Snapshot { text: String::new(), compose: None, at_end: true }));
        // Nothing given, nothing to be out of step with.
        assert!(!Bridge::default().out_of_step(&Snapshot { text: "typed".into(), compose: Some((0, 5)), at_end: true }));
    }

    #[test]
    fn a_long_commit_goes_in_as_one_piece() {
        let pasted = "word, ".repeat(50);
        assert_eq!(typing_pieces(&pasted), vec![pasted.clone()]);
        assert!(typing_pieces("word, word").len() > 1);
    }

    /// The keyboard's text buffer as Android keeps it, in UTF-16, with the operations a keyboard
    /// performs on it (the `BaseInputConnection` ones), so the bridge is tried against what the
    /// system really does with a buffer rather than against its own idea of it.
    #[derive(Default, Clone)]
    struct Editable {
        text: Vec<u16>,
        sel: usize,
        comp: Option<(usize, usize)>,
    }

    fn u16s(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    impl Editable {
        fn snapshot(&self) -> Snapshot {
            let text = String::from_utf16_lossy(&self.text);
            Snapshot::from_utf16_spans(text, self.comp, Some((self.sel, self.sel)))
        }
        /// What the app sets after a caret move: the context, cursor at its end, nothing composing.
        fn set_state(&mut self, text: &str) {
            self.text = u16s(text);
            self.sel = self.text.len();
            self.comp = None;
        }
        fn replace(&mut self, s: &str, composing: bool) {
            let (a, b) = self.comp.unwrap_or((self.sel, self.sel));
            let new = u16s(s);
            let end = a + new.len();
            self.text.splice(a..b, new);
            self.sel = end;
            self.comp = composing.then_some((a, end)).filter(|(x, y)| x < y);
        }
        fn commit(&mut self, s: &str) {
            self.replace(s, false);
        }
        fn compose(&mut self, s: &str) {
            self.replace(s, true);
        }
        fn reopen(&mut self, a: usize, b: usize) {
            self.comp = Some((a.min(self.text.len()), b.min(self.text.len())));
        }
        /// `deleteSurroundingText(n, 0)` in UTF-16 units: removes the composing text with it.
        fn delete_before(&mut self, units: usize) {
            let start = self.sel.saturating_sub(units);
            self.text.drain(start..self.sel);
            self.sel = start;
            self.comp = None;
        }
    }

    /// The editor, as the bridge's output reaches it: its text, the preedit it shows, and its
    /// Backspace, which is the engine's (one cluster per press).
    #[derive(Default)]
    struct Page {
        doc: String,
        preedit: String,
    }

    impl Page {
        fn apply(&mut self, outputs: &[Output]) {
            for output in outputs {
                match output {
                    Output::Backspace(n) => {
                        for _ in 0..*n {
                            let at = cluster::backspace_from(&self.doc, self.doc.len());
                            self.doc.truncate(at);
                        }
                    }
                    Output::Commit(text) => {
                        self.doc.push_str(text);
                        self.preedit.clear();
                    }
                    Output::Enter => self.doc.push('\n'),
                    Output::Preedit(text) => self.preedit.clone_from(text),
                }
            }
        }
        fn shown(&self) -> String {
            format!("{}{}", self.doc, self.preedit)
        }
    }

    /// A page and its keyboard, as they are after a tap: the keyboard's buffer holds the context.
    fn after_a_tap(doc: &str) -> (Bridge, Page, Editable) {
        let (mut bridge, mut page, mut e) = (Bridge::default(), Page::default(), Editable::default());
        page.doc = doc.to_owned();
        bridge.seed(doc);
        e.set_state(doc);
        (bridge, page, e)
    }

    fn poll(bridge: &mut Bridge, page: &mut Page, e: &Editable) {
        let out = bridge.observe(&e.snapshot());
        page.apply(&out);
    }

    #[test]
    fn backspace_right_after_a_tap_deletes_in_the_document() {
        let (mut bridge, mut page, mut e) = after_a_tap("Dear Sam,\nSee you soon");
        e.delete_before(1);
        poll(&mut bridge, &mut page, &e);
        assert_eq!(page.shown(), "Dear Sam,\nSee you soo");
        // Typing goes on where the caret is, and the keyboard capitalises from the real context.
        e.compose("n");
        poll(&mut bridge, &mut page, &e);
        e.commit("n ");
        poll(&mut bridge, &mut page, &e);
        assert_eq!(page.shown(), "Dear Sam,\nSee you soon ");
    }

    #[test]
    fn deleting_an_emoji_from_the_keyboard_buffer_leaves_the_rest_of_the_text() {
        for (emoji, units) in
            [("\u{2764}\u{FE0F}", 2), ("\u{1F44D}\u{1F3FD}", 4), ("\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}", 8), ("\u{1F1EE}\u{1F1F1}", 4)]
        {
            // A keyboard that deletes the whole emoji with one deleteSurroundingText of its length.
            let (mut bridge, mut page, mut e) = after_a_tap(&format!("I like it {emoji}"));
            e.delete_before(units);
            poll(&mut bridge, &mut page, &e);
            assert_eq!(page.shown(), "I like it ", "{emoji:?}");
            // A keyboard that deletes one code point at a time (the last one first).
            let (mut bridge, mut page, mut e) = after_a_tap(&format!("I like it {emoji}"));
            e.delete_before(emoji.chars().next_back().map_or(1, char::len_utf16));
            poll(&mut bridge, &mut page, &e);
            assert_eq!(page.shown(), String::from_utf16_lossy(&e.text), "{emoji:?}: the page keeps what the keyboard kept");
        }
    }

    #[test]
    fn a_swipe_on_the_space_bar_never_deletes_text() {
        let (mut bridge, mut page, mut e) = after_a_tap("hello world");
        // The keyboard's cursor moves into the buffer and it reopens the word it touches.
        e.sel = 5;
        e.reopen(0, 5);
        let snapshot = e.snapshot();
        assert!(!snapshot.at_end, "the keyboard's cursor is not at the end, so the app starts the buffer again");
        // Nothing was sent for it; the page is as it was.
        assert_eq!(page.shown(), "hello world");
        // The app seeds the buffer again with the context before the real caret.
        assert_eq!(bridge.reset(), vec![]);
        bridge.seed("hello world");
        e.set_state("hello world");
        e.compose("!");
        poll(&mut bridge, &mut page, &e);
        assert_eq!(page.shown(), "hello world!");
    }

    #[test]
    fn typing_after_a_tap_autocorrects_replaces_and_deletes_like_a_keyboard_does() {
        let (mut bridge, mut page, mut e) = after_a_tap("Hi. ");
        for letter in ["t", "te", "teh"] {
            e.compose(letter);
            poll(&mut bridge, &mut page, &e);
        }
        assert_eq!(page.shown(), "Hi. teh");
        // Space: the keyboard replaces the word by its correction and types the space.
        e.commit("the ");
        poll(&mut bridge, &mut page, &e);
        assert_eq!(page.shown(), "Hi. the ");
        // Backspace deletes the space; the keyboard reopens the word, which stays in the document.
        e.delete_before(1);
        e.reopen(4, 7);
        poll(&mut bridge, &mut page, &e);
        assert_eq!(page.shown(), "Hi. the");
        // Typing on extends the word.
        e.compose("them");
        poll(&mut bridge, &mut page, &e);
        assert_eq!(page.shown(), "Hi. them");
    }

    /// Whatever sequence of keyboard operations is run, the page ends up holding the keyboard's
    /// text, and the keyboard's buffer holds what the page does.
    #[test]
    fn the_page_always_ends_up_with_what_the_keyboard_holds() {
        struct Lcg(u64);
        impl Lcg {
            fn next(&mut self, n: usize) -> usize {
                self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                ((self.0 >> 33) as usize) % n.max(1)
            }
        }
        const WORDS: [&str; 9] = ["a", "he", "teh", "שלום", "x y", "😀", "line\nbreak", "e\u{0301}", "\u{2764}\u{FE0F}"];
        let mut checked = 0;
        for seed in 0..400u64 {
            let mut rng = Lcg(seed + 1);
            let start = ["", "Hello world. ", "Line one\nLine two ", "שלום עולם "][rng.next(4)];
            let (mut bridge, mut page, mut e) = after_a_tap(start);
            for step in 0..120 {
                let word = WORDS[rng.next(WORDS.len())];
                match rng.next(8) {
                    0 | 1 => e.compose(word),
                    2 => {
                        e.commit(word);
                        e.commit(" ");
                    }
                    // Backspace: a whole cluster, or one code point (the cluster's last), or a space.
                    3 | 4 => {
                        let text = String::from_utf16_lossy(&e.text[..e.sel.min(e.text.len())]);
                        let at = cluster::backspace_from(&text, text.len());
                        let units = text.get(at..).map_or(1, |t| t.encode_utf16().count());
                        let one = text.chars().next_back().map_or(1, char::len_utf16);
                        e.delete_before(if rng.next(2) == 0 { units } else { one });
                    }
                    // Autocorrect: the last few typed characters are replaced.
                    5 => {
                        let n = rng.next(4);
                        e.delete_before(n);
                        e.commit(word);
                    }
                    // The keyboard reopens a word near the end.
                    6 => {
                        let len = e.text.len();
                        e.reopen(len.saturating_sub(rng.next(6)), len);
                    }
                    // A tap: the app commits the composition where it was, then seeds the buffer
                    // again with the page's text before the caret (here the same caret: the end).
                    _ => {
                        let out = bridge.reset();
                        page.apply(&out);
                        let context = page.doc.clone();
                        bridge.seed(&context);
                        e.set_state(&context);
                    }
                }
                let snapshot = e.snapshot();
                if snapshot.at_end {
                    poll(&mut bridge, &mut page, &e);
                }
                let wanted = String::from_utf16_lossy(&e.text);
                // A lone surrogate half can exist in a buffer after a code point wise delete; the
                // keyboard buffer then differs from the page only in that replacement character.
                if !wanted.contains('\u{FFFD}') {
                    assert_eq!(page.shown(), wanted, "seed {seed} step {step}");
                    checked += 1;
                } else {
                    // Start over, as the app does when a buffer can't be read.
                    let out = bridge.reset();
                    page.apply(&out);
                    let context = page.doc.clone();
                    bridge.seed(&context);
                    e.set_state(&context);
                }
            }
        }
        assert!(checked > 30_000, "{checked} states compared");
    }

    #[test]
    fn hostile_snapshots_never_panic() {
        let mut b = Bridge::default();
        b.seed(&"x".repeat(1000));
        for compose in [None, Some((0, 0)), Some((usize::MAX - 1, usize::MAX)), Some((5, 2)), Some((0, usize::MAX))] {
            let s = Snapshot::from_utf16_spans("a😀\u{0}\n\r".repeat(100), compose, Some((usize::MAX, usize::MAX)));
            let _ = b.observe(&s);
            let _ = b.reset();
        }
        let _ = b.observe(&Snapshot { text: String::new(), compose: Some((3, 1)), at_end: true });
    }
}
