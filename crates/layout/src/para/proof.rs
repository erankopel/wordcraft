use super::{CharProps, Document, Paragraph, ResolvedChar};

mod hebrew;

pub use hebrew::{dictionary_form, spelling_suggestions};

/// One run's proofing: its bytes in the paragraph, its language (`w:lang`, resolved through the
/// styles: the document default, paragraph and character styles, direct formatting) and whether
/// it is left unchecked ("Do not check spelling or grammar", hidden text, hyperlinks).
#[derive(Clone, Debug, PartialEq)]
pub struct ProofRun {
    pub range: std::ops::Range<usize>,
    pub lang: wordcraft_proof::ProofLang,
    /// The language of the run's Hebrew letters ([`hebrew_letters_lang`]): a document whose text
    /// language is English and whose complex-script language is Hebrew (or Arabic, as Word marks
    /// it where Hebrew isn't set up) has its Hebrew words checked in Hebrew and its Latin words in
    /// English, as in Word.
    pub hebrew: wordcraft_proof::ProofLang,
    pub skip: bool,
}

impl ProofRun {
    /// `detect`: Detect language automatically ([`crate::ProofOptions::detect_language`]).
    pub(super) fn of(range: std::ops::Range<usize>, rc: &ResolvedChar, detect: bool) -> ProofRun {
        let lang = wordcraft_proof::ProofLang::from_tag(rc.lang.as_deref());
        ProofRun { range, lang, hebrew: hebrew_letters_lang(rc, lang, detect), skip: rc.no_proof || rc.hidden || rc.link.is_some() }
    }
}

/// The language a run's Hebrew letters are checked in. A complex-script language
/// (`w:lang/@w:bidi`) written in Hebrew letters is theirs: Hebrew, whatever the tag's region (and
/// the older `iw`), as `he-IL`; or Yiddish. Any other, or none, makes them Hebrew with `detect`
/// (Detect language automatically, on by default), as Hebrew letters are never Arabic: Word marks
/// Hebrew text `ar-SA` where Hebrew isn't set up, and a file may name no complex-script language
/// (then a text language written in Hebrew letters is kept). Without `detect` they are in the
/// language they are marked with: the complex-script language, else the text language (`lang`).
/// The document's words and a PDF's language tags read Hebrew letters the same way.
fn hebrew_letters_lang(rc: &ResolvedChar, lang: wordcraft_proof::ProofLang, detect: bool) -> wordcraft_proof::ProofLang {
    let hebrew = || wordcraft_proof::ProofLang::from_tag(Some(wordcraft_doc::styles::HEBREW_LANG));
    let primary = |tag: &str| wordcraft_doc::bidi::primary_subtag(tag);
    let written_in_hebrew = |tag: &str| matches!(primary(tag).as_str(), "he" | "iw" | "yi" | "ji");
    match rc.lang_bidi.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        Some(tag) if matches!(primary(tag).as_str(), "he" | "iw") => hebrew(),
        Some(tag) if written_in_hebrew(tag) || !detect => wordcraft_proof::ProofLang::from_tag(Some(tag)),
        Some(_) => hebrew(),
        None if !detect || rc.lang.as_deref().is_some_and(written_in_hebrew) => lang,
        None => hebrew(),
    }
}

/// Is `c` a letter or point of the Hebrew script, including its presentation forms?
fn is_hebrew_script(c: char) -> bool {
    matches!(c, '\u{0590}'..='\u{05FF}' | '\u{FB1D}'..='\u{FB4F}')
}

/// The language of each stretch of a paragraph: each run in its own language, and, in a run whose
/// Hebrew letters are in another language than its text ([`ProofRun::hebrew`]), its Hebrew words
/// in theirs. Ordered and disjoint, as [`wordcraft_proof::check_text_with`] needs.
fn language_spans(text: &str, runs: &[ProofRun]) -> Vec<(std::ops::Range<usize>, wordcraft_proof::ProofLang)> {
    let mut out = Vec::with_capacity(runs.len());
    for r in runs {
        if r.hebrew == r.lang {
            out.push((r.range.clone(), r.lang));
            continue;
        }
        let Some(t) = text.get(r.range.clone()) else { continue };
        let (mut from, mut current) = (r.range.start, r.lang);
        for (i, c) in t.char_indices().filter(|(_, c)| c.is_alphabetic()) {
            let want = if is_hebrew_script(c) { r.hebrew } else { r.lang };
            if want != current {
                let at = r.range.start + i;
                if at > from {
                    out.push((from..at, current));
                }
                (from, current) = (at, want);
            }
        }
        out.push((from..r.range.end, current));
    }
    out
}

/// The proofing runs of a paragraph, in order. `table` is the cell's table-style character
/// formatting ([`CellText::chr`]) for a paragraph in a table, as layout resolves it; `detect` is
/// Detect language automatically ([`crate::ProofOptions::detect_language`]).
pub fn proof_runs(doc: &Document, p: &Paragraph, table: Option<&CharProps>, detect: bool) -> Vec<ProofRun> {
    let style = p.props.style.as_deref();
    p.run_ranges().map(|(r, c)| ProofRun::of(r, &doc.styles.resolve_char_in(style, table, c), detect)).collect()
}

/// The language of the run holding byte `off` (English past the last run).
pub(super) fn lang_at(runs: &[ProofRun], off: usize) -> wordcraft_proof::ProofLang {
    let k = runs.partition_point(|r| r.range.end <= off);
    runs.get(k).filter(|r| r.range.start <= off).map(|r| r.lang).unwrap_or_default()
}

/// Spelling and grammar issues in a paragraph, in order: each word in its run's language
/// ([`wordcraft_proof::check_text_with`]: what spelling skips, and grammar only with `grammar`),
/// and none touching a run left unchecked. Hebrew text is checked as the Hebrew dictionary spells
/// its words: without points, with the keyboard's marks in acronyms and borrowed words,
/// a prefix before a hyphen and a Latin word apart, and the user dictionary's words after prefix
/// letters.
///
/// `list_item` (see [`is_list_item`]) turns off the capital-first-word grammar rule.
pub fn paragraph_issues(
    p: &Paragraph,
    runs: &[ProofRun],
    spelling: &wordcraft_proof::Options,
    grammar: bool,
    list_item: bool,
) -> Vec<wordcraft_proof::Issue> {
    if p.text.trim().is_empty() || p.text.len() > 100_000 {
        return Vec::new();
    }
    let text = proof_text(p);
    let spans = language_spans(&text, runs);
    let skipped = skipped_ranges(runs);
    let mut issues = match hebrew::View::of(&text, &spans) {
        Some(view) => view.check(&text, &spans, spelling, grammar, list_item),
        None => wordcraft_proof::check_text_with(&text, &spans, spelling, grammar, list_item),
    };
    ignored_once(p.rev, &mut issues);
    if skipped.is_empty() {
        return issues;
    }
    issues.into_iter().filter(|i| !overlaps(&skipped, i.start, i.end)).collect()
}

/// An issue Ignore Once left out: its paragraph's revision ([`Paragraph::rev`]), its byte range,
/// and whether it is a grammar issue.
type IgnoredOnce = (u64, usize, usize, bool);

/// Most issues Ignore Once keeps; past that, those kept so far are all forgotten.
const MAX_IGNORED_ONCE: usize = 10_000;

/// The issues ignored once in this run of the program. A paragraph's revision changes whenever
/// it is edited, and is new to every paragraph made (copies of it, as undo keeps, share it), so
/// an issue comes back once its paragraph changes and never turns up in another one.
static IGNORED_ONCE: std::sync::Mutex<Option<std::collections::HashSet<IgnoredOnce>>> = std::sync::Mutex::new(None);

/// Ignore Once: leave the spelling or grammar (`grammar`) issue at bytes `start..end` of the
/// paragraph at revision `rev` out of [`paragraph_issues`] and the squiggles, until that paragraph
/// changes. The caller lays the document out again.
pub fn ignore_once(rev: u64, start: usize, end: usize, grammar: bool) {
    let mut ignored = IGNORED_ONCE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let set = ignored.get_or_insert_default();
    if set.len() >= MAX_IGNORED_ONCE {
        set.clear();
    }
    set.insert((rev, start, end, grammar));
}

/// `issues` (of the paragraph at revision `rev`) without those ignored once.
fn ignored_once(rev: u64, issues: &mut Vec<wordcraft_proof::Issue>) {
    let ignored = IGNORED_ONCE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(set) = ignored.as_ref().filter(|s| !s.is_empty()) {
        issues.retain(|i| !set.contains(&(rev, i.start, i.end, i.kind == wordcraft_proof::IssueKind::Grammar)));
    }
}

/// The byte ranges of the runs left unchecked, in order, those that overlap merged: their
/// starts and ends both rise, so [`overlaps`] finds an issue's by binary search instead of
/// looking at every run for every issue.
fn skipped_ranges(runs: &[ProofRun]) -> Vec<(usize, usize)> {
    let mut ranges: Vec<(usize, usize)> = runs.iter().filter(|r| r.skip).map(|r| (r.range.start, r.range.end)).collect();
    ranges.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::with_capacity(ranges.len());
    for (s, e) in ranges {
        crate::work::steps(1);
        match merged.last_mut() {
            Some(last) if s < last.1 => last.1 = last.1.max(e),
            _ => merged.push((s, e)),
        }
    }
    merged
}

/// Whether bytes `a..b` overlap one of `skipped` ([`skipped_ranges`]): the first range ending
/// after `a` is the only one that can, as every later one starts no earlier.
fn overlaps(skipped: &[(usize, usize)], a: usize, b: usize) -> bool {
    crate::work::steps(1);
    let k = skipped.partition_point(|&(_, e)| e <= a);
    skipped.get(k).is_some_and(|&(s, _)| s < b)
}

/// Whether the paragraph is a list item: its own numbering, or its style's, names a list (not 0).
pub fn is_list_item(doc: &Document, p: &Paragraph) -> bool {
    p.props.numbering.or_else(|| doc.styles.resolve_para(&p.props).numbering).is_some_and(|n| n.num != 0)
}

/// Spelling and grammar issues as squiggles (start, end, is grammar), less spelling issues overlapping `typing`.
pub(super) fn proof_issues(
    p: &Paragraph,
    runs: &[ProofRun],
    opts: &crate::ProofOptions,
    typing: Option<(usize, usize)>,
    list_item: bool,
) -> Vec<(usize, usize, bool)> {
    paragraph_issues(p, runs, &opts.spelling, opts.grammar, list_item)
        .into_iter()
        .map(|i| (i.start, i.end, i.kind == wordcraft_proof::IssueKind::Grammar))
        .filter(|&(a, b, grammar)| grammar || !typing.is_some_and(|(s, e)| a < e && s < b))
        .collect()
}

/// Text for proofing with the same byte offsets: inline objects and tracked deletions become
/// U+0001 bytes (neither words nor spaces).
pub fn proof_text(p: &wordcraft_doc::Paragraph) -> String {
    let mut out = String::with_capacity(p.text.len());
    // The run holding each character ([`Paragraph::props_of_char`]), found by walking the runs
    // along with the text rather than from the first run for every character.
    let mut runs = p.runs.iter();
    let mut run = runs.next();
    let mut end = run.map_or(0, |r| r.len);
    // Each run this walk looks at is a step ([`crate::work`]), so the linear-time test counts
    // them instead of timing the walk: a caller that went back to the first run for every
    // character would count the runs once per character and show up as a quadratic count (#5185).
    crate::work::steps(usize::from(run.is_some()));
    for (i, c) in p.text.char_indices() {
        while run.is_some() && i >= end {
            run = runs.next();
            end = end.saturating_add(run.map_or(0, |r| r.len));
            crate::work::steps(usize::from(run.is_some()));
        }
        let props = run.map_or(&p.mark, |r| &r.props);
        let deleted = c == wordcraft_doc::para::OBJ || props.del.is_some();
        if deleted {
            for _ in 0..c.len_utf8() {
                out.push('\u{1}');
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(range: std::ops::Range<usize>, skip: bool) -> ProofRun {
        ProofRun { range, lang: Default::default(), hebrew: Default::default(), skip }
    }

    /// The lookup finds what a look at every run finds, overlapping and empty runs included.
    #[test]
    fn skipped_lookup_matches_a_scan_of_every_run() {
        let runs = [run(0..4, false), run(4..9, true), run(5..6, true), run(9..9, true), run(9..12, false), run(12..20, true), run(14..30, true)];
        let skipped = skipped_ranges(&runs);
        for a in 0..32 {
            for b in a..32 {
                let scan = runs.iter().any(|r| r.skip && r.range.start < b && a < r.range.end);
                assert_eq!(overlaps(&skipped, a, b), scan, "{a}..{b}");
            }
        }
        assert!(skipped_ranges(&[run(0..5, false)]).is_empty());
    }

    fn spans_of(text: &str, run: ProofRun) -> Vec<(std::ops::Range<usize>, wordcraft_proof::ProofLang)> {
        language_spans(text, &[run])
    }

    fn hebrew() -> wordcraft_proof::ProofLang {
        wordcraft_proof::ProofLang::from_tag(Some("he-IL"))
    }

    /// A run whose text language is English and whose Hebrew letters are Hebrew has its Hebrew
    /// words in Hebrew and its Latin words in English; Hebrew letters in the run's own language
    /// change nothing.
    #[test]
    fn hebrew_words_take_the_complex_script_language_and_latin_words_keep_theirs() {
        let he = hebrew();
        assert_ne!(he, wordcraft_proof::ProofLang::En);
        let text = "hello שלום world";
        let mixed = ProofRun { range: 0..text.len(), lang: wordcraft_proof::ProofLang::En, hebrew: he, skip: false };
        let spans = spans_of(text, mixed);
        let lang_of = |needle: &str| {
            let at = text.find(needle).unwrap_or(usize::MAX);
            spans.iter().find(|(r, _)| r.contains(&at)).map(|(_, l)| *l)
        };
        assert_eq!(lang_of("שלום"), Some(he), "the Hebrew word is checked as Hebrew");
        assert_eq!(lang_of("hello"), Some(wordcraft_proof::ProofLang::En));
        assert_eq!(lang_of("world"), Some(wordcraft_proof::ProofLang::En), "English comes back after the Hebrew word");
        assert!(spans.windows(2).all(|w| w[0].0.end <= w[1].0.start), "ordered and disjoint: {spans:?}");
        assert_eq!(spans.last().map(|(r, _)| r.end), Some(text.len()), "up to the end of the run");
        let plain = ProofRun { range: 0..text.len(), lang: wordcraft_proof::ProofLang::En, hebrew: wordcraft_proof::ProofLang::En, skip: false };
        assert_eq!(spans_of(text, plain), vec![(0..text.len(), wordcraft_proof::ProofLang::En)], "Hebrew letters in the run's language: no split");
        let all_hebrew = ProofRun { range: 0..text.len(), lang: he, hebrew: he, skip: false };
        assert_eq!(spans_of(text, all_hebrew).len(), 1, "a run already in Hebrew is one stretch");
    }

    /// The language a run of `lang` and complex-script language `bidi` checks its Hebrew letters
    /// in, with or without Detect language automatically.
    fn hebrew_letters(lang: Option<&str>, bidi: Option<&str>, detect: bool) -> wordcraft_proof::ProofLang {
        let props = CharProps { lang: lang.map(str::to_string), lang_bidi: bidi.map(str::to_string), ..Default::default() };
        let rc = wordcraft_doc::StyleSheet::default().resolve_char(None, &props);
        ProofRun::of(0..1, &rc, detect).hebrew
    }

    /// HE-09: Hebrew letters are checked as Hebrew when the complex-script language names Hebrew
    /// (any region, or the older `iw`), and, while Detect language automatically is on, also when
    /// it names a language not written in Hebrew letters (Word's `ar-SA` outside Hebrew setups) or
    /// none; Yiddish keeps them, and so does a text language written in Hebrew letters. With it
    /// off they are in the language they are marked with.
    #[test]
    fn hebrew_letters_are_hebrew_whatever_language_they_are_marked_with() {
        for tag in ["he", "he-IL", "HE_il", "iw", "iw-IL"] {
            for detect in [true, false] {
                assert_eq!(hebrew_letters(Some("en-US"), Some(tag), detect), hebrew(), "{tag}, detect {detect}");
            }
        }
        for tag in [Some("ar-SA"), Some("fa-IR"), Some("en-US"), Some(""), Some("hebrew"), None] {
            assert_eq!(hebrew_letters(Some("en-US"), tag, true), hebrew(), "{tag:?}");
        }
        let marked = |lang: Option<&str>, bidi: Option<&str>| hebrew_letters(lang, bidi, false);
        assert_eq!(marked(Some("en-US"), Some("ar-SA")), wordcraft_proof::ProofLang::from_tag(Some("ar-SA")), "as marked: Arabic");
        assert_eq!(marked(Some("en-US"), None), wordcraft_proof::ProofLang::En, "as marked: the text language");
        let yiddish = wordcraft_proof::ProofLang::from_tag(Some("yi"));
        assert_eq!(hebrew_letters(Some("en-US"), Some("yi"), true), yiddish);
        let own = wordcraft_proof::ProofLang::from_tag(Some("he-ZZ"));
        assert_eq!(hebrew_letters(Some("he-ZZ"), None, true), own, "the text's own Hebrew");
    }

    /// HE-09: a misspelled Hebrew word in a run whose complex-script language is Arabic, as in a
    /// file from Word outside a Hebrew setup, goes to the Hebrew spelling check, its English words
    /// to the English one.
    #[test]
    fn hebrew_words_marked_arabic_are_checked_in_hebrew() {
        let text = "Hello \u{05E9}\u{05DC}\u{05D5}\u{05DD}\u{05DD} world";
        let props = CharProps { lang: Some("en-US".into()), lang_bidi: Some("ar-SA".into()), ..Default::default() };
        let rc = wordcraft_doc::StyleSheet::default().resolve_char(None, &props);
        let spans = spans_of(text, ProofRun::of(0..text.len(), &rc, true));
        let at = text.find('\u{05E9}').unwrap_or(usize::MAX);
        assert_eq!(spans.iter().find(|(r, _)| r.contains(&at)).map(|(_, l)| *l), Some(hebrew()), "{spans:?}");
        assert_eq!(spans.first().map(|(_, l)| *l), Some(wordcraft_proof::ProofLang::En));
        let off = spans_of(text, ProofRun::of(0..text.len(), &rc, false));
        assert!(off.iter().all(|(_, l)| *l != hebrew()), "detection off: as marked: {off:?}");
    }
}
