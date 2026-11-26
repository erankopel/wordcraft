use super::{CharProps, Document, Paragraph, ResolvedChar};

/// One run's proofing: its bytes in the paragraph, its language (`w:lang`, resolved through the
/// styles: the document default, paragraph and character styles, direct formatting) and whether
/// it is left unchecked ("Do not check spelling or grammar", hidden text, hyperlinks).
#[derive(Clone, Debug, PartialEq)]
pub struct ProofRun {
    pub range: std::ops::Range<usize>,
    pub lang: wordcraft_proof::ProofLang,
    /// The language of the run's Hebrew letters when `w:lang/@w:bidi` names Hebrew (`he-IL`): a
    /// document whose text language is English and whose complex-script language is Hebrew has its
    /// Hebrew words checked in Hebrew and its Latin words in English, as in Word.
    pub hebrew: Option<wordcraft_proof::ProofLang>,
    pub skip: bool,
}

impl ProofRun {
    pub(super) fn of(range: std::ops::Range<usize>, rc: &ResolvedChar) -> ProofRun {
        ProofRun {
            range,
            lang: wordcraft_proof::ProofLang::from_tag(rc.lang.as_deref()),
            hebrew: hebrew_of(rc.lang_bidi.as_deref()),
            skip: rc.no_proof || rc.hidden || rc.link.is_some(),
        }
    }
}

/// The Hebrew proofing language a `w:lang/@w:bidi` tag names (`he`, `he-IL`, and the older `iw`),
/// or `None` for any other language.
fn hebrew_of(tag: Option<&str>) -> Option<wordcraft_proof::ProofLang> {
    let primary = tag?.trim().split(['-', '_', '.', '@']).next()?;
    (primary.eq_ignore_ascii_case("he") || primary.eq_ignore_ascii_case("iw")).then(|| wordcraft_proof::ProofLang::from_tag(Some("he-IL")))
}

/// Is `c` a letter or point of the Hebrew script, including its presentation forms?
fn is_hebrew_script(c: char) -> bool {
    matches!(c, '\u{0590}'..='\u{05FF}' | '\u{FB1D}'..='\u{FB4F}')
}

/// The language of each stretch of a paragraph: each run in its own language, and, in a run whose
/// complex-script language is Hebrew and whose text language is another, its Hebrew words in Hebrew.
/// Ordered and disjoint, as [`wordcraft_proof::check_text_with`] needs.
fn language_spans(text: &str, runs: &[ProofRun]) -> Vec<(std::ops::Range<usize>, wordcraft_proof::ProofLang)> {
    let mut out = Vec::with_capacity(runs.len());
    for r in runs {
        let Some(hebrew) = r.hebrew.filter(|h| *h != r.lang) else {
            out.push((r.range.clone(), r.lang));
            continue;
        };
        let Some(t) = text.get(r.range.clone()) else { continue };
        let (mut from, mut current) = (r.range.start, r.lang);
        for (i, c) in t.char_indices().filter(|(_, c)| c.is_alphabetic()) {
            let want = if is_hebrew_script(c) { hebrew } else { r.lang };
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
/// formatting ([`CellText::chr`]) for a paragraph in a table, as layout resolves it.
pub fn proof_runs(doc: &Document, p: &Paragraph, table: Option<&CharProps>) -> Vec<ProofRun> {
    let style = p.props.style.as_deref();
    p.run_ranges().map(|(r, c)| ProofRun::of(r, &doc.styles.resolve_char_in(style, table, c))).collect()
}

/// The language of the run holding byte `off` (English past the last run).
pub(super) fn lang_at(runs: &[ProofRun], off: usize) -> wordcraft_proof::ProofLang {
    let k = runs.partition_point(|r| r.range.end <= off);
    runs.get(k).filter(|r| r.range.start <= off).map(|r| r.lang).unwrap_or_default()
}

/// Spelling and grammar issues in a paragraph, in order: each word in its run's language
/// ([`wordcraft_proof::check_text_with`]: what spelling skips, and grammar only with `grammar`),
/// and none touching a run left unchecked.
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
    let issues = wordcraft_proof::check_text_with(&text, &spans, spelling, grammar, list_item);
    if skipped.is_empty() {
        return issues;
    }
    issues.into_iter().filter(|i| !overlaps(&skipped, i.start, i.end)).collect()
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
        ProofRun { range, lang: Default::default(), hebrew: None, skip }
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

    /// A run whose text language is English and whose complex-script language is Hebrew has its
    /// Hebrew words in Hebrew and its Latin words in English; without that language nothing changes.
    #[test]
    fn hebrew_words_take_the_complex_script_language_and_latin_words_keep_theirs() {
        let he = hebrew_of(Some("he-IL")).expect("Hebrew is a language");
        assert_ne!(he, wordcraft_proof::ProofLang::En);
        let text = "hello שלום world";
        let mixed = ProofRun { range: 0..text.len(), lang: wordcraft_proof::ProofLang::En, hebrew: Some(he), skip: false };
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
        let plain = ProofRun { range: 0..text.len(), lang: wordcraft_proof::ProofLang::En, hebrew: None, skip: false };
        assert_eq!(spans_of(text, plain), vec![(0..text.len(), wordcraft_proof::ProofLang::En)], "no complex-script language, no split");
        let all_hebrew = ProofRun { range: 0..text.len(), lang: he, hebrew: Some(he), skip: false };
        assert_eq!(spans_of(text, all_hebrew).len(), 1, "a run already in Hebrew is one stretch");
    }

    #[test]
    fn only_hebrew_complex_script_languages_are_taken_for_hebrew() {
        for tag in ["he", "he-IL", "HE_il", "iw", "iw-IL"] {
            assert!(hebrew_of(Some(tag)).is_some(), "{tag}");
        }
        for tag in ["ar-SA", "fa-IR", "en-US", "", "hebrew"] {
            assert!(hebrew_of(Some(tag)).is_none(), "{tag}");
        }
        assert!(hebrew_of(None).is_none());
    }
}
