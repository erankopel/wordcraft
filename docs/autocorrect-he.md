# Hebrew AutoCorrect list

> **Last updated:** 2026-11-28 · **Change:** major (first version: the Hebrew edition's built-in Hebrew AutoCorrect list, audit item HE-16) · **Status:** for the operator's review; not reviewed by a native speaker

AutoCorrect replaces a word as you finish typing it. Besides the English typos and the symbols,
WordCraft has a built-in list of 96 Hebrew slips, listed below for review. The rules it follows are
pinned by tests in `crates/engine/src/tests/typing.rs` and described in
[typing parity](typing-parity.md); the list itself is the constant `HEBREW` in
`crates/engine/src/cmd/tools/autocorrect/hebrew.rs`.

## What it does

- **When:** as for every AutoCorrect entry, when a word is finished by a space, one of `, . ; : ! ?`
  or Enter. The word is everything after the last white space before it.
- **Where:** in Hebrew text only. Hebrew letters are Hebrew text as the spelling check reads them:
  in the language of complex-script text (`w:lang/@w:bidi`, Hebrew in the edition's new documents
  and in Word's Hebrew files), else in the text's language or the editing language, and with
  **Detect language automatically** on (the default) whatever language they are marked with. Text
  marked Yiddish or Ladino, which are written in Hebrew letters too, never gets the list.
- **Which words:** a word made only of the Hebrew letters א to ת (final forms included). A word
  with a point (niqqud), a cantillation mark, a geresh ׳, gershayim ״, a quote mark, a maqaf or
  any other character in it is left as typed. So are English words: the English list applies to
  English text only.
- **Prefixes:** an entry marked *yes* below also applies after one to three of the prefix letters
  ו ה ב כ ל מ ש: the rest of the word must be the entry's typed form, and only that part is
  replaced, so ושלהמ becomes ושלהם. An entry marked *no* applies to the whole word only (מכאנ
  stays as typed).
- **Undo:** Undo (Ctrl+Z) right after a replacement brings the word back as typed and keeps the
  space or punctuation that finished it, as for the English entries.

## How the list was made

The list is WordCraft's own. It was written from scratch for WordCraft from general knowledge of
Hebrew spelling: a final letter typed in its ordinary form at the end of a common word (מ for ם, נ
for ן), and common words with two letters swapped, a letter added, doubled, left out or typed as
another letter that sounds the same, or two words run together. Independent reviews then checked
every entry: that its typed form is not a word, that nobody writes it on purpose, and that it is
not as likely a longer word with its end left out, which the replacement would turn into the
wrong word (with and without prefixes); entries that failed were dropped. It was not taken from
Microsoft Word's list or any other product's. **No native speaker has reviewed it**: please read
the tables below and say which entries to drop or change.

## Deleting an entry or turning the list off

- **One entry:** File › Options › Proofing › **AutoCorrect Options** (קובץ › אפשרויות › הגהה ›
  אפשרויות תיקון אוטומטי). On the AutoCorrect tab, type the entry's typed form in **Replace** (or
  pick it in the list) and press **Delete** (מחיקה). A deleted entry stays deleted, after a prefix
  too, and the list then no longer shows it. Adding an entry with the same typed form replaces the
  built-in one: the user's own entry applies to the whole word only, so after a prefix neither
  fires. From the command line or an agent: `tools.autocorrect` with `{"delete": "שלהמ"}`, or with
  `{"add": {"from": "שלהמ", "to": "…"}}`.
- **All of it:** there is no switch for the Hebrew list alone. **Replace text as you type**
  (החלפת טקסט בזמן ההקלדה, `tools.autocorrect` `{"replaceText": false}`) turns off every
  replacement, the English and symbol entries and the user's own too; to keep those, delete the
  Hebrew entries one by one.

## Final letters (61)

A final letter typed in its ordinary form at the end of a word.

| # | Typed | Replaced with | After prefixes |
|---|---|---|---|
| 1 | אותמ | אותם | yes |
| 2 | שלהמ | שלהם | yes |
| 3 | כאנ | כאן | no |
| 4 | כולמ | כולם | yes |
| 5 | אנשימ | אנשים | yes |
| 6 | דברימ | דברים | yes |
| 7 | שנימ | שנים | yes |
| 8 | עדיינ | עדיין | yes |
| 9 | חיימ | חיים | yes |
| 10 | ילדימ | ילדים | yes |
| 11 | עליהמ | עליהם | yes |
| 12 | אליהמ | אליהם | yes |
| 13 | ביניהמ | ביניהם | yes |
| 14 | שלכמ | שלכם | yes |
| 15 | אתכמ | אתכם | yes |
| 16 | פעמימ | פעמים | yes |
| 17 | רוצימ | רוצים | yes |
| 18 | יכולימ | יכולים | yes |
| 19 | צריכימ | צריכים | yes |
| 20 | עושימ | עושים | yes |
| 21 | אומרימ | אומרים | yes |
| 22 | יודעימ | יודעים | yes |
| 23 | הולכימ | הולכים | yes |
| 24 | חושבימ | חושבים | yes |
| 25 | אחרימ | אחרים | yes |
| 26 | רבימ | רבים | yes |
| 27 | חברימ | חברים | yes |
| 28 | הורימ | הורים | yes |
| 29 | מספרימ | מספרים | yes |
| 30 | רואימ | רואים | yes |
| 31 | אמנמ | אמנם | yes |
| 32 | עצממ | עצמם | yes |
| 33 | אצלמ | אצלם | yes |
| 34 | בשבילמ | בשבילם | yes |
| 35 | בגללמ | בגללם | yes |
| 36 | איתכמ | איתכם | yes |
| 37 | עליכמ | עליכם | yes |
| 38 | אליכמ | אליכם | yes |
| 39 | עליהנ | עליהן | yes |
| 40 | אלפימ | אלפים | yes |
| 41 | לאנ | לאן | yes |
| 42 | שונימ | שונים | yes |
| 43 | חדשימ | חדשים | yes |
| 44 | גדולימ | גדולים | yes |
| 45 | טובימ | טובים | yes |
| 46 | ראשונימ | ראשונים | yes |
| 47 | מילימ | מילים | yes |
| 48 | תלמידימ | תלמידים | yes |
| 49 | עובדימ | עובדים | yes |
| 50 | נמצאימ | נמצאים | yes |
| 51 | מדברימ | מדברים | yes |
| 52 | אוהבימ | אוהבים | yes |
| 53 | מבינימ | מבינים | yes |
| 54 | משתמשימ | משתמשים | yes |
| 55 | קיימימ | קיימים | yes |
| 56 | חלקימ | חלקים | yes |
| 57 | יחסימ | יחסים | yes |
| 58 | אחוזימ | אחוזים | yes |
| 59 | עשרימ | עשרים | yes |
| 60 | שניימ | שניים | yes |
| 61 | יומיימ | יומיים | yes |

## Other slips (35)

Letters swapped, added, doubled, left out or typed as a letter that sounds the same, and words run
together. None of these applies after a prefix.

| # | Typed | Replaced with | After prefixes |
|---|---|---|---|
| 1 | עכישו | עכשיו | no |
| 2 | אננחו | אנחנו | no |
| 3 | בשיבל | בשביל | no |
| 4 | בחהלט | בהחלט | no |
| 5 | פאילו | אפילו | no |
| 6 | יאזה | איזה | no |
| 7 | לפמעים | לפעמים | no |
| 8 | בהצלהח | בהצלחה | no |
| 9 | משפהח | משפחה | no |
| 10 | סליהח | סליחה | no |
| 11 | יהיתי | הייתי | no |
| 12 | תאמול | אתמול | no |
| 13 | כמבון | כמובן | no |
| 14 | לנפי | לפני | no |
| 15 | ביעקר | בעיקר | no |
| 16 | מצעטר | מצטער | no |
| 17 | בבשקה | בבקשה | no |
| 18 | יבחד | ביחד | no |
| 19 | לודגמה | לדוגמה | no |
| 20 | באמעצות | באמצעות | no |
| 21 | למלש | למשל | no |
| 22 | בונסף | בנוסף | no |
| 23 | במלהך | במהלך | no |
| 24 | יהייה | יהיה | no |
| 25 | אנחו | אנחנו | no |
| 26 | בבבקשה | בבקשה | no |
| 27 | להיתראות | להתראות | no |
| 28 | פיתאום | פתאום | no |
| 29 | איזשהוא | איזשהו | no |
| 30 | אכשיו | עכשיו | no |
| 31 | כלכך | כל כך | no |
| 32 | בכלזאת | בכל זאת | no |
| 33 | אחרכך | אחר כך | no |
| 34 | בכלאופן | בכל אופן | no |
| 35 | כלהכבוד | כל הכבוד | no |

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-28 | major | First version: the 96 entries of the Hebrew AutoCorrect list, the rules they follow, how the list was made, how to delete an entry or turn replacements off |
