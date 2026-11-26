# Hebrew interface

> **Last reviewed:** 2026-11-26 · **Last updated:** 2026-11-26 · **Change:** major (first version) · **Target:** WordCraft 0.18.1

Choose **File › Options › Interface language › עברית**, or follow a Hebrew system display language
(`he`, `he-IL`, `he_IL.UTF-8`, and the older `iw`, `iw-IL`, `iw_IL.UTF-8` that some Java, Android and
glibc locales still report). The catalog code is `he`; `ui.language` with `{"value":"he"}` selects it
explicitly. Document text, command ids, CLI/MCP/control responses and file paths do not change.

Hebrew is written right to left, so the interface is mirrored the way the Arabic one is: the tabs
and ribbon groups run from the right, side panes swap sides, the status bar starts at the right and
dialogs read from the right. The document canvas keeps its own direction: a paragraph is right to
left only when it is set so (Home › Paragraph › Right-to-Left Text Direction, `para.rtl`).

## Catalog

The catalog is craft-i18n's `locales/he/wordcraft.tsv` (storytold/craft-i18n, 2,579 entries at the
commit pinned in `craft-i18n.rev`) over the shared `locales/he/common.tsv`. Its header says every
entry was written from the plain meaning of the English text in ordinary computing vocabulary and
that no other product's localisation was consulted. This change adds no translation text: it only
registers the language. Nothing in this repository was written from another product's Hebrew
interface, and the Hebrew sample text in the tests is plain everyday vocabulary.

- **Coverage:** 284 English strings that the Arabic catalog translates have no Hebrew row yet (for
  example `Align Left`, `All Caps`, `Add to Dictionary`, `Accept All Changes and Stop Tracking`).
  They show in English, as a missing translation always does. In the screenshots taken for this
  change `Paragraph Tools` and the status bar's `font not installed` were still English. Adding the
  rows is craft-i18n's work, not this repository's.
- **Review:** no native Hebrew speaker has reviewed the catalog or this change. The terminology has
  not been compared with how Hebrew-speaking Word users name things; do that review in craft-i18n.
- `@plural` rows exist only in the shared catalog, with CLDR's three forms, and are read by the
  `craft-i18n` crate. WordCraft's own catalog has no plural rows in any language.

## Fonts

The interface faces Inter and Source Sans 3 have no Hebrew letters. Hebrew text draws with
craft-fonts' Noto Sans Hebrew (SIL OFL 1.1, one Regular face, embedded when the build has
`CRAFT_FONTS_DIR`). With it the headless screenshots of the Home tab, the ribbon groups, the status
bar and a Hebrew document drew without a missing glyph, including where the interface asks for its
semibold face.
Without `CRAFT_FONTS_DIR` the build has no embedded Hebrew face and relies on an installed Hebrew
font loaded on demand (Noto Sans Hebrew, Segoe UI, Arial Hebrew or SF Hebrew, DejaVu Sans,
FreeSans, Culmus…).

- **Screenshots:** the headless `ui_shot` window panics on the first Hebrew glyph when it is built
  without `CRAFT_FONTS_DIR` (egui reports no glyph for the letter in the semibold family). Arabic
  does the same. Take Hebrew and Arabic screenshots with `CRAFT_FONTS_DIR` set. Whether the
  desktop app, which loads installed fonts on demand, behaves differently without it was not
  checked.
- **Documents:** Hebrew document fonts (David, Miriam, Arial Hebrew, Calibri Hebrew) are
  substituted with installed or open faces by `crates/fonts/src/word.rs`, as before. No font file
  is added to this repository.

## In the document

- **List numbering:** `hebrew1` counts in Hebrew numerals (א ב ג … י יא … כ … ק … ת), with 15 and 16
  as טו and טז, never יה or יו, 500 to 900 as תק to תתק, and thousands before a geresh
  (1000 is א׳). From 1,000,000 the numerals run out and the label is the decimal number. No
  geresh or gershayim is drawn inside the number; the level text (`%1.`) supplies the punctuation.
  `hebrew2` counts the 22 letters א to ת, then doubles them (אא, בב …), as the Arabic alphabetic
  formats do. Both were decimal before. The `w:numFmt` values `hebrew1` and `hebrew2` were already
  read and written by name (`doc::section::tests::number_formats` round-trips every format); a
  saved and reopened `.docx` with a Hebrew list was not tried.
- **Word selection and Ctrl+arrow:** a Hebrew word keeps its cantillation marks (U+0591 to U+05AF),
  geresh (U+05F3) and gershayim (U+05F4): a double-click on צה״ל picks the whole acronym, not צה.
  The vowel points (niqqud) were already part of the word, because Unicode counts them as
  alphabetic. The maqaf (U+05BE) and sof pasuq (U+05C3) still end a word.
- **Checked and already right:** the caret steps over a letter with all its points as one stop;
  Ctrl+arrow crosses a pointed word in one step in reading order; the word count is the same with
  or without points; typed Hebrew is never underlined as misspelled English, while an English typo
  beside it still is.

## Verification and remaining scope

Tests: `crates/ui-egui/src/i18n/tests.rs` (Hebrew locale tags and the saved preference, the old
`iw` tag, that only Arabic and Hebrew are right to left, Hebrew lookups in typing order),
`crates/ui-egui/src/i18n/audit.rs` (the mirrored-interface audit now runs for Arabic and Hebrew:
tab, group and pane order, the Print page, dialogs, context menus and every ribbon control at
several widths without a panic), `crates/doc/src/section.rs` (Hebrew numerals: the letters add up
to the number for 1 to 999, no two numbers share a label, no label ends in יה or יו; the
alphabet), `crates/doc/src/bidi.rs` and `crates/engine/src/tests/hebrew.rs` (word selection, caret
steps, word count, spelling marks). The shared checks (catalog validation, the missing-translation
report, `cargo xtask i18n-report`) include Hebrew automatically.

Not done, or not checked:

- Hebrew spelling and grammar: the app has no Hebrew proofing, so Hebrew text shows no marks.
- Keyboard switching of paragraph direction with the two Shift keys (Windows Hebrew keyboards) and
  the Hebrew IME and keyboard layouts on each platform.
- Hebrew calendar dates and gematria date formats.
- Right-to-left tables, headers and footers, footnotes and PDF export of pointed or accented text
  with Hebrew specifically (they are covered for Arabic and, in places, Hebrew by earlier work:
  #362, #1383, #1670).
- A native-speaker review of the catalog and of the sample text (see Catalog).

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-26 | major | First version: Hebrew (`he`) interface language, mirrored shell, Hebrew list numbering (`hebrew1`, `hebrew2`), cantillation, geresh and gershayim inside words |
