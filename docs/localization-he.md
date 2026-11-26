# Hebrew interface and the Hebrew edition

> **Last reviewed:** 2026-11-26 · **Last updated:** 2026-11-26 · **Change:** major (first version) · **Target:** WordCraft 0.19.1

WordCraft has a Hebrew (`he`) interface language, mirrored right to left like the Arabic one, and a
Hebrew edition: a build of the same program that starts in Hebrew, starts new documents right to
left, carries Hebrew fonts of its own and needs no setup. This page says what each part does, what
was checked and what was not.

## Choosing Hebrew

The stock build follows the system language: a Hebrew system display language (`he`, `he-IL`,
`he_IL.UTF-8`, and the older `iw`, `iw-IL`, `iw_IL.UTF-8` that some Java, Android and glibc
locales report) gives the Hebrew interface, and **File › Options › Language › Interface language**
picks it by hand (`ui.language` with `{"value":"he"}`). Document text, command ids, CLI, MCP and
control responses and file paths never change with the interface language.

The Hebrew edition is built with `WORDCRAFT_DEFAULT_LANGUAGE=he`. In it the setting `auto`, which
is what every profile has until someone picks a language, means Hebrew instead of the system
language. Order of precedence: `WORDCRAFT_LANGUAGE` in the environment (`en` or `C` asks for
English), then a language picked in Options, then the edition's language, then the system's. A
person who wants English in the edition picks English once in Options. A stock build never reads
`WORDCRAFT_DEFAULT_LANGUAGE`, so `auto` is unchanged there and no existing test depends on it.

Hebrew is written right to left, so the interface is mirrored: the tabs and ribbon groups run from
the right, side panes swap sides, the status bar starts at the right and dialogs read from the
right.

## Catalog

The catalog is craft-i18n's `locales/he/wordcraft.tsv` over the shared `locales/he/common.tsv`
(storytold/craft-i18n, at the commit pinned in `craft-i18n.rev`). Its header says every entry was
written from the plain meaning of the English text in ordinary computing vocabulary, without
consulting another product's localisation. This repository does not copy another product's Hebrew
interface either.

That catalog has no Hebrew row for 171 strings that WordCraft 0.19.1 looks up (a static scan of the
labels, dialog and pane text, command names and plurals; for example `Align Left`, `All Caps`,
`Add to Dictionary`, `Accept All Changes and Stop Tracking`). They show in English. The edition
fills them from `packaging/hebrew/he-supplement.tsv`, 320 original rows (276 used by 0.19.1 and 44 for
features of later releases), which `packaging/hebrew/build-appimage.sh` appends to a copy of the
craft-i18n checkout; the checkout itself is not changed. After them three looked-up strings stay
English on purpose (`pi`, `{n} KB`, `{n} MB`). Every placeholder, trailing ellipsis and `\n` matches the English, and craft-i18n's own
`check_all` finds no problem in the result.

- **Review:** no native Hebrew speaker has read the catalog or the supplement. The rows were
  written from the plain meaning of the English, like the rest of the catalog. The 45 rows least
  likely to be right, each with the reason, are in `packaging/hebrew/he-uncertain.tsv`: start there.
  The proper home for the rows is craft-i18n (the 49 rows that name shared things, such as colours,
  belong in its `common.tsv`); this repository keeps them only so that the edition can be built.
- **Not turned on:** one supplement row describes downloading LibreOffice dictionaries (GPL, LGPL
  and MPL data). That feature is off in WordCraft and the edition must not fetch or ship such
  dictionaries; the row is flagged `LICENCE` in the review list.

## Fonts

**Interface.** Inter and Source Sans 3 have no Hebrew letters. The interface draws Hebrew with
craft-fonts' Noto Sans Hebrew (SIL OFL 1.1, one Regular face), embedded when the build has
`CRAFT_FONTS_DIR`, in the regular, medium and semibold families (so a semibold ribbon label is
drawn at Regular weight, as for Arabic and CJK). A release build with `CRAFT_FONTS_REQUIRED` fails
without it. Without `CRAFT_FONTS_DIR` the build looks for an installed Hebrew font when the
interface is right to left (Noto Sans Hebrew, FreeSans, Liberation Sans, DejaVu Sans, Segoe UI,
Arial). A character no font has is drawn as a box and never panics in the app: `main.rs` sets
egui's `MissingGlyphPolicy::Tofu` explicitly, and only the tests use the strict policy.
`theme::tests::the_hebrew_interface_has_a_face_for_every_hebrew_letter_it_shows` lays out every
Hebrew letter of the catalog and of the language list in all three weights under that strict policy;
it fails when the Hebrew face is not in the chain.

**Documents.** Word's Hebrew families David, Frank Ruehl (also FrankRuehl and Narkisim), Miriam,
Miriam Fixed, Rod, Aharoni, Gisha and Levenim MT are proprietary and are never bundled. A document
that asks for one is drawn, when it is not installed, in an open look-alike with Hebrew letters
(`crates/fonts/src/word.rs`): David in David Libre, Frank Ruehl and Narkisim in Frank Ruhl Libre,
Miriam in Miriam Libre, Aharoni, Gisha and Levenim MT in Heebo, Miriam Fixed and Rod in Liberation
Mono, each then falling back to Noto Serif or Noto Sans Hebrew. The font name in the file stays
David, so the file is unchanged for Word. The look-alikes do not measure like the originals, so the
status bar still says the text is shown in another font. The Home font list names these families
(`font.list` with `all` reports them as `lookAlikes`) and previews them in their stand-in. If the person has the real
David, a copy in the user font folder (`~/.config/wordcraft/fonts` on Linux) or the system folders
is used as is.

**Bundled fonts.** The edition ships open fonts, all SIL OFL 1.1, from google/fonts at a pinned
commit (`packaging/hebrew/fetch-open-hebrew-fonts.sh`): David Libre, Frank Ruhl Libre, Miriam Libre,
Heebo, Assistant, Alef, Noto Serif Hebrew, Rubik, Secular One, Suez One and Varela Round (about
2 MB). The files are build inputs, not committed. `packaging/linux/package.sh` stages the files in
`WORDCRAFT_EXTRA_FONTS_DIR` to `usr/share/wordcraft/fonts` and each licence to
`usr/share/doc/wordcraft/OFL-<family>.txt`. The app scans, besides the system and user folders,
the folders in `WORDCRAFT_FONT_DIR` (a list separated like `PATH`) and `share/wordcraft/fonts` and
`fonts` next to the program (`bundled_font_dirs`), which is how the fonts inside an AppImage are
found. Fonts in the font list appear under their legacy names too (for example `Heebo Light`), as
installed fonts do.

## New documents

A new blank document starts right to left when the interface is Hebrew, and only then (Arabic's does
not): the document defaults carry a right-to-left paragraph direction (`w:bidi` in `w:pPrDefault`),
Hebrew as the language of complex-script text (`w:lang/@w:bidi="he-IL"`) and `David` as the
complex-script font (`w:rFonts/@w:cs`). The caret starts at the right margin, a paragraph that
begins with Latin letters does not flip, a table inserted from Insert › Table is mirrored, and the
direction survives a save and a reopen. The text language is left as it was, so Latin words keep
English. This applies to the first blank document, File › New, an empty file and the blank document
left after the last one is closed. It does not change templates, files that are opened, documents
that already have text or history, or any headless session (CLI, MCP and the tests stay left to
right): `file.new` takes `{"direction": "rtl" | "ltr"}` for a blank document whatever the setting.
One paragraph can be switched with Home › Paragraph. A page number asked for on the right stays on
the right in a right-to-left document.

## Spelling

Hebrew proofing is the `he` Language pack of craft-i18n, which WordCraft already knows how to
install: the bar above the document offers `Install (821 KB)` for a Hebrew document, one click
turns downloading on, and the pack then works offline. It is built from Hspell and is licensed
AGPL-3.0-or-later, so craft-i18n's ADR 0004 makes it Download-only: it is never bundled in a build,
which is why the edition has no spell check on its first launch and needs one online click.

Hebrew words are proofed in the language of complex-script text (`w:lang/@w:bidi`), as Word does:
in a document whose text language is English and whose complex-script language is Hebrew (a Word
file, or a new document of the edition) the Hebrew words are checked in Hebrew and the Latin words
in English. A run already in Hebrew is unchanged. The old code `iw` finds the `he` pack, and such a
document makes the bar offer the pack. With no pack installed Hebrew shows no marks, as before. The
status bar names Hebrew while the caret is in Hebrew text (or in an empty right-to-left paragraph) and
the text language, English, while it is in Latin text.

## In the document

- **List numbering:** `hebrew1` counts in Hebrew numerals (א ב ג … י יא … כ … ק … ת), with 15 and 16
  as טו and טז, never יה or יו, 500 to 900 as תק to תתק, and thousands before a geresh
  (1000 is א׳). From 1,000,000 the numerals run out and the label is the decimal number. No
  geresh or gershayim is drawn inside the number; the level text (`%1.`) supplies the punctuation.
  `hebrew2` counts the 22 letters א to ת, then doubles them (אא, בב …).
- **Word selection and Ctrl+arrow:** a Hebrew word keeps its cantillation marks (U+0591 to U+05AF),
  geresh (U+05F3) and gershayim (U+05F4): a double-click on צה״ל picks the whole acronym, not צה.
  The vowel points were already part of the word. The maqaf (U+05BE) and sof pasuq (U+05C3) still
  end a word.
- **Checked and already right:** the caret steps over a letter with all its points as one stop;
  Ctrl+arrow crosses a pointed word in one step in reading order; the word count is the same with
  or without points; typed Hebrew is never underlined as misspelled English.

## Building the edition

```bash
packaging/hebrew/fetch-open-hebrew-fonts.sh "$HOME/.cache/wordcraft-hebrew-fonts"
CRAFT_I18N_SRC=/path/to/craft-i18n CRAFT_FONTS_DIR=/path/to/craft-fonts \
HEBREW_FONTS_DIR="$HOME/.cache/wordcraft-hebrew-fonts" packaging/hebrew/build-appimage.sh
```

The script sets `WORDCRAFT_DEFAULT_LANGUAGE=he`, `CRAFT_I18N_REQUIRED` and `CRAFT_FONTS_REQUIRED`,
the extra fonts, `WORDCRAFT_EDITION_REGION=he-IL` (the AppImage starts with Israel's A4 paper and
metric units unless `WORDCRAFT_REGION` is set) and `WORDCRAFT_NO_UPDATE_INFO` (no AppImage update
information, because the upstream releases are in English). The edition does not check for updates
when it starts, for the same reason; Help › Check for Updates is still a manual action and would
offer the upstream release, which would replace the edition.

## Not done, or not checked

- A native-speaker review of the catalog, the supplement and the Hebrew sample text.
- Hebrew spelling on the very first launch with no network (see Spelling), and the quality of the
  `he` pack inside WordCraft's affix engine, which was not examined.
- The tables created by Convert to Table, Draw Table, autoformat and mail-merge labels are not
  mirrored in a Hebrew-first document, and ODT and HTML export do not write the default direction
  (DOCX and RTF do): both are follow-ups. How Microsoft Word reads `w:bidi` in the document
  defaults was not observed.
- Keyboard switching of paragraph direction with the two Shift keys of Windows Hebrew keyboards,
  the Hebrew IME and keyboard layouts on each platform, Hebrew calendar dates and gematria date
  formats, and Hebrew hyphenation (no pattern file exists for it).
- Windows and macOS builds of the edition.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-26 | major | First version: Hebrew interface language and numbering, Hebrew edition (default language, catalog supplement, fonts, right-to-left new documents, Hebrew proofing through the complex-script language) |
