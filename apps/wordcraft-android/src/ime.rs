//! Typing on the on-screen keyboard.
//!
//! winit 0.30 hands an Android app key events only: the text a soft keyboard produces (letters,
//! autocorrected words, suggestions, composing text) arrives in GameActivity as `TextEvent`s, which
//! winit logs as unknown and drops. The state behind those events is still readable
//! (`AndroidApp::text_input_state`), so this module reads it and turns what changed into the
//! editor's own input: [`egui::ImeEvent`] preedit and commit events (the page already handles them
//! for desktop input methods), and Backspace and Enter key presses.
//!
//! The keyboard edits a small buffer of its own. [`Bridge`] remembers the part of it already given
//! to the editor, so only what changed is sent, whether the keyboard appended a word, deleted a
//! character in the buffer (Backspace), or replaced an earlier word (autocorrect). The buffer is
//! just a log of what was typed: where it goes is wherever the editor's caret is, so tapping
//! elsewhere never needs the buffer to know. [`Bridge::should_reset`] says when to empty it, and a
//! touch or a cursor key resets it: the word being composed is committed where it was typed, and
//! the keyboard starts a fresh composition.
//!
//! Everything here is plain data in, events out, with tests that run on any host; the Android
//! calls are in `keyboard`.

/// What the editor should be given, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Output {
    /// Press Backspace this many times (the keyboard deleted text it had typed earlier).
    Backspace(usize),
    /// Text the keyboard finished: insert it at the caret.
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
}

impl Snapshot {
    /// A snapshot from what GameActivity reports: the text and spans in UTF-16 code units (Java's
    /// string indices). Spans that don't fit the text are clamped; a reversed span is swapped; an
    /// empty or undefined one (a negative end) means no composition.
    pub fn from_utf16_spans(text: String, compose: Option<(usize, usize)>) -> Snapshot {
        let compose = compose.and_then(|(a, b)| {
            let (a, b) = if a <= b { (a, b) } else { (b, a) };
            let (a, b) = (utf16_to_char(&text, a), utf16_to_char(&text, b));
            (a < b).then_some((a, b))
        });
        Snapshot { text, compose }
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

/// Remembers what has been given to the editor.
#[derive(Debug, Default)]
pub struct Bridge {
    /// The committed part of the keyboard's buffer the editor has been given.
    given: Vec<char>,
    /// The composing text the editor is showing.
    preedit: String,
}

/// The buffer is emptied when it grows past this many characters (and nothing is being composed).
const MAX_BUFFER_CHARS: usize = 256;

impl Bridge {
    /// The keyboard's buffer is now `now`: what to give the editor so it matches. The committed
    /// text is what comes before the composing region (all of it without one); the composing text
    /// is shown as preedit.
    pub fn observe(&mut self, now: &Snapshot) -> Vec<Output> {
        let chars: Vec<char> = now.text.chars().collect();
        let (committed, composing): (Vec<char>, String) = match now.compose {
            // Text after the composing region (a word reopened for composition, with what followed
            // it) isn't given yet: it comes back as committed text when the composition ends.
            Some((a, b)) if a < b && b <= chars.len() => (chars[..a].to_vec(), chars[a..b].iter().collect()),
            _ => (chars, String::new()),
        };
        let mut out = Vec::new();
        let same = self.given.iter().zip(&committed).take_while(|(a, b)| a == b).count();
        if self.given.len() > same {
            out.push(Output::Backspace(self.given.len() - same));
        }
        if committed.len() > same {
            let added: String = committed[same..].iter().collect();
            // A line break is the Enter key, not text.
            let mut lines = added.split('\n');
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

    /// Whether the keyboard's buffer should be emptied now: it has grown long and nothing is being
    /// composed (emptying it while a word is being composed would cut the word off).
    pub fn should_reset(&self) -> bool {
        self.given.len() > MAX_BUFFER_CHARS && self.preedit.is_empty()
    }

    /// The keyboard's buffer was emptied (or the caret moved somewhere else): forget it. A word
    /// that was being composed is committed as it stands, where the caret was, before the caret
    /// moves; the keyboard starts over.
    pub fn reset(&mut self) -> Vec<Output> {
        self.given.clear();
        if self.preedit.is_empty() { Vec::new() } else { vec![Output::Commit(std::mem::take(&mut self.preedit))] }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(text: &str, compose: Option<(usize, usize)>) -> Snapshot {
        Snapshot { text: text.to_owned(), compose }
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
    }

    #[test]
    fn a_word_reopened_for_composition_is_taken_back_from_the_page() {
        let mut b = Bridge::default();
        b.observe(&snap("hello ", None));
        // The keyboard marks "hello" as composing again (tapping a suggestion for it): the page
        // takes the committed characters back, including the space after the word, and shows the
        // word as preedit.
        assert_eq!(b.observe(&snap("hello ", Some((0, 5)))), vec![Output::Backspace(6), Output::Preedit("hello".into())]);
        // When the composition ends the whole text comes back as committed.
        assert_eq!(b.observe(&snap("hello ", None)), vec![Output::Commit("hello ".into()), Output::Preedit(String::new())]);
    }

    #[test]
    fn spans_are_utf16_units_and_junk_is_clamped() {
        // An emoji is two UTF-16 units but one character: composing "a😀" is units 1..4.
        let s = Snapshot::from_utf16_spans("xa😀".into(), Some((1, 4)));
        assert_eq!(s.compose, Some((1, 3)));
        assert_eq!(Snapshot::from_utf16_spans("abc".into(), Some((2, 99))).compose, Some((2, 3)));
        assert_eq!(Snapshot::from_utf16_spans("abc".into(), Some((3, 1))).compose, Some((1, 3)), "a reversed span is swapped");
        assert_eq!(Snapshot::from_utf16_spans("abc".into(), Some((2, 2))).compose, None, "an empty span is no composition");
        assert_eq!(Snapshot::from_utf16_spans("abc".into(), Some((9, 12))).compose, None);
        assert_eq!(Snapshot::from_utf16_spans("שלום".into(), Some((0, 4))).compose, Some((0, 4)), "Hebrew letters are one unit each");
        assert_eq!(utf16_to_char("a😀b", 3), 2);
        assert_eq!(utf16_to_char("a😀b", 2), 2, "inside a pair counts as before it");
        assert_eq!(utf16_to_char("", 5), 0);
        // A composing span beyond the text is ignored rather than slicing out of range.
        let mut b = Bridge::default();
        assert_eq!(b.observe(&snap("ab", Some((1, 9)))), vec![Output::Commit("ab".into())]);
    }

    /// A keyboard simulated against the bridge: whatever the keyboard does to its buffer (type,
    /// compose, delete, autocorrect, be reset by a touch), the editor ends up with the text the
    /// buffer holds on top of what was there at the last reset.
    #[test]
    fn the_editor_always_ends_up_with_what_the_keyboard_typed() {
        struct Lcg(u64);
        impl Lcg {
            fn next(&mut self, n: usize) -> usize {
                self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                ((self.0 >> 33) as usize) % n.max(1)
            }
        }
        const WORDS: [&str; 7] = ["a", "he", "teh", "שלום", "x y", "😀", "line\nbreak"];
        for seed in 0..300u64 {
            let mut rng = Lcg(seed + 1);
            let mut bridge = Bridge::default();
            // The keyboard's buffer: committed text, then the word being composed (if any).
            let (mut committed, mut composing): (Vec<char>, Vec<char>) = (Vec::new(), Vec::new());
            // The editor: its text, the preedit it shows, and what was there at the last reset.
            let (mut doc, mut preedit, mut base): (Vec<char>, String, Vec<char>) = (Vec::new(), String::new(), Vec::new());
            for step in 0..200 {
                let word: Vec<char> = WORDS[rng.next(WORDS.len())].chars().collect();
                match rng.next(6) {
                    // Compose: the word grows or is replaced.
                    0 | 1 => composing = word,
                    // Commit the composition, plus what the keyboard adds with it.
                    2 => {
                        committed.append(&mut composing);
                        committed.extend(word);
                        committed.push(' ');
                    }
                    // Backspace: shortens the composition, or the committed text.
                    3 => {
                        if composing.pop().is_none() {
                            committed.pop();
                        }
                    }
                    // Autocorrect: the last few committed characters are replaced.
                    4 => {
                        let cut = rng.next(committed.len().min(6) + 1);
                        committed.truncate(committed.len() - cut);
                        committed.extend(word);
                    }
                    // A touch: the composition is committed where it was typed and the keyboard starts over.
                    _ => {
                        apply(&bridge.reset(), &mut doc, &mut preedit);
                        committed.clear();
                        composing.clear();
                        base = doc.clone();
                    }
                }
                let mut text = committed.clone();
                text.extend(&composing);
                let compose = (!composing.is_empty()).then(|| (committed.len(), text.len()));
                let outputs = bridge.observe(&Snapshot { text: text.iter().collect(), compose });
                apply(&outputs, &mut doc, &mut preedit);
                if bridge.should_reset() {
                    apply(&bridge.reset(), &mut doc, &mut preedit);
                    committed.clear();
                    composing.clear();
                    base = doc.clone();
                }
                let shown: String = doc.iter().collect::<String>() + &preedit;
                let wanted: String = base.iter().chain(&committed).chain(&composing).collect();
                assert_eq!(shown, wanted, "seed {seed} step {step}");
            }
        }

        fn apply(outputs: &[Output], doc: &mut Vec<char>, preedit: &mut String) {
            for output in outputs {
                match output {
                    Output::Backspace(n) => doc.truncate(doc.len().saturating_sub(*n)),
                    Output::Commit(text) => {
                        doc.extend(text.chars());
                        preedit.clear();
                    }
                    Output::Enter => doc.push('\n'),
                    Output::Preedit(text) => preedit.clone_from(text),
                }
            }
        }
    }

    #[test]
    fn the_buffer_is_emptied_when_it_is_long_and_idle_and_when_the_caret_moves() {
        let mut b = Bridge::default();
        b.observe(&snap(&"x".repeat(MAX_BUFFER_CHARS), None));
        assert!(!b.should_reset());
        b.observe(&snap(&"x".repeat(MAX_BUFFER_CHARS + 1), None));
        assert!(b.should_reset());
        assert_eq!(b.reset(), vec![]);
        assert!(!b.should_reset());
        // The keyboard was emptied: what it types next is all new.
        assert_eq!(b.observe(&snap("a", None)), vec![Output::Commit("a".into())]);
        // Not while a word is being composed.
        let long = format!("{}z", "x".repeat(MAX_BUFFER_CHARS + 5));
        b.observe(&snap(&long, Some((MAX_BUFFER_CHARS + 5, MAX_BUFFER_CHARS + 6))));
        assert!(!b.should_reset());
        // Moving the caret commits the word being composed, where the caret was.
        assert_eq!(b.reset(), vec![Output::Commit("z".into())]);
        assert_eq!(b.reset(), vec![]);
    }
}
