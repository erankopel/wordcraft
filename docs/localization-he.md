# Hebrew interface and the Hebrew edition

> **Last reviewed:** 2026-11-26 · **Last updated:** 2026-11-28 · **Change:** minor (Word-fidelity review of the complex-script bold, italic and size: neutral characters, typing, paragraph marks, files without styles, saving, HTML export, labels); previously minor (Greeting Line in Hebrew text; the edition's sample without the community board's sentence; review fixes to the Find results, Modify Style, the colour fields, the reference samples' page number box, the Hebrew AutoCorrect list, address lists and text import); previously minor (a built-in Hebrew AutoCorrect list); previously minor (Find and Replace's Match diacritics; dialogs' results and errors in Hebrew; the mini toolbar, Search's results and the reference and list samples mirrored; Modify Style's complex script font; controls for the direction keys and gershayim) · **Target:** WordCraft 0.19.1

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
right. In more detail:

- **Task panes** read from the right: the title, then ▾ and ×, at the left end of the title bar. A
  pane that was never moved opens on the right. Dock Left and Dock Right (in the pane's menu, where
  Dock Right comes first, and in `ui.paneDock` and `view.stylesPaneDock`) still mean the window's
  left and right, and saved places keep that meaning.
- **Comment and revision balloons** sit left of the page, where a right-to-left line ends, with
  the author's stripe, name and time on the card's right.
- **Icons that point along a line or through a sequence** are drawn mirrored: previous and next
  (records, comments, changes, footnotes), undo and redo, bullets, numbering, indents, promote and
  demote. Icons of a fixed side (Align Left, the text directions, Insert Left), the media player's
  and the chevrons are not. Bold, Italic and Underline write מ, נ and ק, the first letters of
  מודגש, נטוי and קו תחתון, as the English buttons write B, I and U; Spelling writes אבג. These are
  catalog rows with the context `icon`, so a language without them keeps the Latin letters.
- **Interface text** is laid out from the right even where a Hebrew row starts with a Latin word,
  a key name or a placeholder (`{language}: WordCraft …`), and file patterns (`(*.png)`), paths,
  web addresses and `code` inside Hebrew text keep their own left-to-right order. This is done
  when a catalog row is looked up, with a right-to-left mark and left-to-right isolates; the
  characters are never reordered by hand. A Hebrew file name keeps its dot between the name and
  the extension (`בדיקה.docx`).
- **Lengths** read number first: `2.5 ס"מ` has the number on the right of its unit.
- Text fields, tooltips, the language-pack bar, Options › Language › Your languages and Add a
  language, File › Recover, the Colors dialog, Customize Keyboard and Customize Ribbon lists, the
  Print page's lists and the style gallery's and status bar's context menus are mirrored too.
- The Home › Styles gallery shows each style's sample from the right in a right-to-left document,
  and Design › Document Formatting draws its samples right to left.
- The mini toolbar over a selection starts its rows at the right (the font box first), and the
  title bar's Search lists each command with its name at the right and its shortcut at the left
  (a screen reader hears the name first). The samples in Table of Contents, Table of Figures and
  Index read from the right with their page numbers at the left margin, as in a right-to-left
  document, so the box under them names no side (מספרי עמודים מיושרים לשוליים, a supplement row
  correcting the published "at the right margin"), and Define New Multilevel List's preview sets
  each level's number from the right, as the list runs in right-to-left text.
  The Find and Replace boxes and Create Source's fields, with their hints, start at the right.

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
craft-i18n checkout; the checkout itself is not changed. `{n} KB` and `{n} MB` keep their Latin
units on purpose. Every placeholder, trailing ellipsis and `\n` matches the English: a test
(`the_embedded_hebrew_catalog_keeps_the_catalog_rules`) runs craft-i18n's own `check_all` on the
Hebrew catalog as the build embeds it, supplement and corrections included.

- **Corrections:** craft-i18n keeps the first of two rows with the same context and English, so a
  row appended to the catalog could only add a string. The build embeds the catalog with a later
  row in place of an earlier one with its key (`crates/ui-egui/build.rs`, `src/i18n/merge.rs`), so a
  supplement row that repeats a published key corrects it. The published catalogs never repeat a
  key, so only the supplement changes anything. The edition corrects the published rows that were
  English or wrong: the OpenDocument and Rich Text file types, the Font dialog's preview (Hebrew
  letters beside `AaBbYyZz`), the About licence line and Options › Language's description of
  Language packs (מיקוף, not היפוך מילים, for hyphenation). The corrections are the supplement's
  last rows, after a `#` line.
- **Messages from the engine and the libraries** (a command's status line, an error in a dialog, why
  a button is off, a download that failed) are made in English, which scripts and agents read, and
  translated where the window shows them (`i18n::message`): a row for the whole message, or an
  `@msg` row whose English has placeholders for the parts that vary (`All done. We made {n}
  replacements.`). A template matches only a whole message, with its other text exactly as written,
  so engine wording that is new or changed shows in English rather than as the translation of
  another message; a placeholder named `error` or `reason` is translated the same way. The `@msg`
  context is craft-i18n's. The dialogs show their messages the same way: Find and Replace's results
  (`Match 2 of 5` reads התאמה 2 מתוך 5, Replace All's count, Reading Highlight's count) and
  refusals (`no matches`), and the errors of Rename, Page Setup, Save Changes, the password
  dialogs, Field, the mail-merge dialogs, Mark Index Entry and Paste Special (its note on what the
  clipboard holds, also when it names only the first formats and says how many more); the dialog's
  state keeps the English, results and counts too, which the control channel reports. The
  supplement has rows for the messages of Replace All, Fit One Page,
  Missing Fonts, Column Selection, content controls, protection and Zotero, for the common
  operating-system errors inside them, and for about 120 errors the ribbon and dialogs can show
  (grouping objects, drawing tables, table formulas, lists, protection, recovery, printing). The
  engine has some 700 messages; most are answers to scripts' parameters, and those stay English.
- **Accessibility pane:** each finding is worded from the checker's rule id, with the numbers the
  engine reports (heading levels, counts, contrast ratios), and what to do about it is a sentence
  that names the place in the ribbon; the engine's fixes, which name commands for scripts
  (`file.properties {title}`), are never shown.
- **Names shown translated, kept in English underneath:** built-in style names in Manage Styles and
  the Organizer (and in Manage Styles' "Based on"), Design › Document Formatting's style sets, Insert › Symbol's
  Unicode subsets (context `unicode`), Insert › Icons' names and categories (context `icons`;
  searching finds an icon by its Hebrew name or category as well as by the library's English) and
  About › Language data's titles.
- **Names the interface gives new things** are in the interface language and are kept as given:
  New Style offers `סגנון 1`, `סגנון 2`… (the first name no style has yet; it was always `Style1`,
  which a second new style couldn't take), New Table Style `סגנון טבלה 1`…, and Customize Ribbon's
  New Tab and New Group make `כרטיסייה חדשה` and `קבוצה חדשה` (the engine names a new tab's first
  group in English; the dialog renames it). File › Recover and its Discard question name the copy
  of a document that was never saved `טיוטה ללא שם` (the copy itself keeps the engine's English
  name, which `file.recover` and the control channel report). Type a New List… names a new
  mail-merge address list's columns in Hebrew (שם פרטי, שם משפחה, כתובת, עיר, מיקוד), and Address
  Block and Greeting Line find the name and the address in them: the engine knows each address
  part by its Hebrew name as by its English one (`address_map` in
  `crates/engine/src/cmd/mailings/merge.rs`: also תואר, חברה, רחוב, יישוב, מחוז, מדינה and a few
  alternatives). Country is מדינה, so a state or province is מחוז, in the list and in Match Fields
  (a context row, `address`, for State; the plain row gives State and Country the same word); a
  list that names its country ארץ has its state in its מדינה column, and the two stay apart.
- **Pictures described in the interface language:** an icon inserted from Insert › Icons is
  described (its alternative text) by the icon's name as the interface shows it, Hebrew in the
  Hebrew interface (`insert.icon` takes `alt`; without it the library's English name).

- **Review:** no native Hebrew speaker has read the catalog or the supplement. The rows were
  written from the plain meaning of the English, like the rest of the catalog. The 106 rows least
  likely to be right, each with the reason, are in `packaging/hebrew/he-uncertain.tsv`: start there.
  The proper home for the rows is craft-i18n (the 49 rows that name shared things, such as colours,
  belong in its `common.tsv`); this repository keeps them only so that the edition can be built.
- **Not turned on:** one supplement row describes downloading LibreOffice dictionaries (GPL, LGPL
  and MPL data). That feature is off in WordCraft and the edition must not fetch or ship such
  dictionaries; the row is flagged `LICENCE` in the review list.

## Fonts

**Interface.** Inter and Source Sans 3 have no Hebrew letters. The interface draws Hebrew with
craft-fonts' Noto Sans Hebrew (SIL OFL 1.1, one Regular face), embedded when the build has
`CRAFT_FONTS_DIR`, in the regular, medium, semibold and monospace families (so a semibold ribbon
label is drawn at Regular weight, as for Arabic and CJK, and the update window's buttons and Markdown
Source, which are monospace, draw Hebrew rather than boxes). A release build with `CRAFT_FONTS_REQUIRED` fails
without it. Without `CRAFT_FONTS_DIR` the build looks for an installed Hebrew font when the
interface is right to left (Noto Sans Hebrew, FreeSans, Liberation Sans, DejaVu Sans, Segoe UI,
Arial). A character no font has is drawn as a box and never panics in the app: `main.rs` sets
egui's `MissingGlyphPolicy::Tofu` explicitly, and only the tests use the strict policy.
`theme::tests::the_hebrew_interface_has_a_face_for_every_hebrew_letter_it_shows` lays out every
Hebrew letter of the catalog and of the language list in all three weights and in monospace under
that strict policy; it fails when the Hebrew face is not in a family's chain.

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
left after the last one is closed. The File view's templates (on Home and, through More templates,
on New) then open in their Hebrew versions, right to left (`file.new` with `{"template": …,
"direction": "rtl"}`); their thumbnails show those versions. It does not change files that are
opened, documents that already have text or history, a script's `file.new` with a template and no
direction (the English template), or any headless session (CLI, MCP and the tests stay left to
right): `file.new` takes `{"direction": "rtl" | "ltr"}` for a blank document whatever the setting.
One paragraph can be switched with Home › Paragraph. A page number asked for on the right stays on
the right in a right-to-left document.

## Spelling

Hebrew proofing is the `he` Language pack of craft-i18n, which WordCraft already knows how to
install: the bar above the document offers `Install (821 KB)` for a Hebrew document, one click
turns downloading on, and the pack then works offline. It is built from Hspell and is licensed
AGPL-3.0-or-later, so craft-i18n's ADR 0004 makes it Download-only: it is never bundled in a build,
which is why the edition has no spell check on its first launch and needs one online click.

Once the list of packs is known, the bar names the pack's licence and links its source next to
Install (the upstream source the pack index names, else the pack's archive); after the person's
Install it says that Hebrew proofing is installed, with the licence and source, until it is closed. Before the list is known (a fresh profile) the bar's Install fetches the
list and then the pack, and Options › Language says the language is available to download, with
the same Install, instead of "not available yet". A download that a network stops is explained in
Hebrew with what to try: a security certificate the network puts in place of the server's (a filter
at work or school, or an antivirus program, whose certificate WordCraft's downloads don't trust),
no connection, no answer in time, or a refusal; any other failure keeps the font store's words.
Before the list is known the status bar also marks Hebrew as having no dictionary (`עברית (אין
מילון)`) while the bar offers the pack, as it does once the list is known.

Hebrew words are proofed in the language of complex-script text (`w:lang/@w:bidi`), as Word does:
in a document whose text language is English and whose complex-script language is Hebrew (a Word
file, or a new document of the edition) the Hebrew words are checked in Hebrew and the Latin words
in English. A run already in Hebrew is unchanged. The old code `iw` finds the `he` pack, and such a
document makes the bar offer the pack. With no pack installed Hebrew shows no marks, as before. The
status bar names Hebrew while the caret is in Hebrew text (or in an empty right-to-left paragraph) and
the text language, English, while it is in Latin text.

The Spelling and Grammar pane's Ignore Once (התעלמות חד-פעמית) runs `review.ignoreOnce`: the word it
shows is left unmarked, that occurrence only, until its paragraph changes, and checking again doesn't
bring it back. It adds nothing to a dictionary.

## The AI assistant in the edition

The edition includes Review › AI › **AI Assistant** (see [the AI assistant](assistant.md)), whose text is
all in Hebrew: the pane, its agreement step, its errors, the ribbon group and button, the command names and
what a screen reader is told. Only the abbreviations AI, API and KB, the name of the environment variable
`ANTHROPIC_API_KEY`, the file names `assistant-key` and `assistant.json` and the model names stay in Latin
letters. The rows are in `packaging/hebrew/he-supplement.tsv` (17 more for the one-time agreement, Settings
(הגדרות, the catalog's word), the withdrawal, the cache's token use, a failed or stopped request's token use
(סבב for a round, as in the progress line) and an agreement or a withdrawal that could not be saved); the least
certain are in `he-uncertain.tsv`. The pane is mirrored like the rest of the interface, its buttons starting at the right, but
it docks at the window's right, not at the end edge (the left) where Comments docks; Dock Left moves it like
any pane. After the one-time setup the pane shows only the conversation and a
Settings button. The instruction box is right to left in every paragraph, whatever letter the paragraph starts
with (as Word's text boxes are in its Hebrew interface): a Hebrew line, a short one included, ends at the box's
right edge, a sentence that starts with an English word (Word היא תוכנה) keeps that word at the right, and the
hint and the caret of the empty box are at the right edge, inside the box. The reply the model writes is in whatever language it
writes in, each paragraph laid out in its own direction: a Hebrew one from the right, an English one from the
left.

## Hebrew whatever the system language

The edition's text is Hebrew on a computer whose language is English or anything else: the setting `auto`
means the edition's language (see Choosing Hebrew), so nothing depends on the system locale. The desktop's
file dialogs follow the desktop's language (on Linux they come from its portal), so the edition opens,
saves, exports and inserts pictures through WordCraft's own file browser, which is in Hebrew and
mirrored. **File › Options › Advanced › File dialogs** (`ui.fileDialogs`: `auto`, `wordcraft` or `system`)
chooses the system's dialogs instead; Automatic is WordCraft's own browser in an edition and the
system's in the stock build. In WordCraft's browser, as in the system's dialog, Add Font… takes
several files at once: Ctrl and a click (or Ctrl+Space) adds a file or takes it out, Shift and a click
(or Shift+Space) takes the files from the highlighted one, the file name box names each in quotes,
and a status line beside the buttons, which screen readers announce, says how many there are. The
system's own menus do not follow WordCraft's language. Page size and units are Israel's (A4,
centimetres) unless `WORDCRAFT_REGION` is set, and Mailings › Envelopes and
Labels start on the region's paper (a DL envelope and A4 label sheets). Starting with
`WORDCRAFT_LANGUAGE=en` gives English.

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
- **Find and Replace:** Search Options has **Match diacritics** (התאמת ניקוד וטעמים), the searches'
  `matchDiacritics`: off, as in Word, שלום finds שָׁלוֹם; ticked, points and cantillation marks must
  match too. The dialog opens with it as the last search left it (a script's search can turn it on,
  and then Search Options open by themselves on the ticked box), and its own searches send it.
- **Typing options:** `"` and `'` typed right after a Hebrew letter are gershayim ״ and geresh ׳
  while AutoCorrect Options › AutoFormat As You Type › **Type gershayim and geresh after Hebrew
  letters** (הקלדת גרשיים וגרש אחרי אותיות עבריות, `tools.autocorrect` `hebrewMarks`) is on; until
  someone sets it, it follows the interface, and the box shows it ticked in the Hebrew one. **File ›
  Options › Advanced › Use Ctrl+Right Shift and Ctrl+Left Shift to set the paragraph direction**
  (`file.options` `directionKeys`) turns those keys on and off, with a note to turn them off where
  Ctrl+Shift switches the keyboard layout (they start off on Linux and the BSDs). Both are described
  in [typing parity](typing-parity.md).
- **AutoCorrect (HE-16, done):** Hebrew text has a built-in AutoCorrect list of 96 entries of our
  own, listed for review in [the Hebrew AutoCorrect list](autocorrect-he.md): a final letter typed
  in its ordinary form (שלהמ → שלהם), most of them also after one to three of the prefix letters
  ו ה ב כ ל מ ש, which stay as typed (ושלהמ → ושלהם), and other common slips (עכישו → עכשיו). It
  applies to a word of Hebrew letters alone (no points, geresh, gershayim or quote mark) in text
  that proofing reads as Hebrew, never in Yiddish or Ladino text. AutoCorrect Options (אפשרויות
  תיקון אוטומטי) list the entries; each can be deleted or replaced with one's own, and Replace text
  as you type turns all replacements off. The list was written from scratch and checked entry by
  entry by independent reviews; no native speaker has reviewed it yet. See also
  [typing parity](typing-parity.md).
- **Styles:** Modify Style shows a style's complex script font and size (David in a Hebrew
  document) on rows of their own, beside the Latin text font and size, in the Hebrew interface or
  for a style right-to-left text uses (`para::style_used_for_rtl`). OK sets each script's font and
  size as shown, so a new Latin text font no longer reaches the Hebrew text, and the complex script
  rows change the Hebrew text alone. A complex script font or size is sent only when it was changed,
  or to keep it where a new Latin one would otherwise reach it (a font in a style right-to-left
  text uses; a size there or in a style without a complex script size of its own), so with nothing
  changed none is sent, and a style of an English document doesn't take on the complex script
  values it inherits. Its colour is a swatch with a palette, named by its row's label for a screen
  reader (the Font dialog's too), and the automatic colour reads אוטומטי (Automatic), not `auto`.
  New Style's Style based on lists the paragraph styles by the names the interface shows
  (כותרת 1) and bases the new style on the style picked, by its id.
- **Mail merge's Greeting Line** writes its words in the language of the text it stands in, as the
  engine's other words do (`wordcraft_doc::words`): in Hebrew text the recipient's name after
  לכבוד, which greets a man and a woman alike (לכבוד נועה לוי,), and, for a record with no name,
  שלום רב, (for anyone). English text keeps `Dear Ada Lovelace,` and `Dear Sir or Madam,`. The
  words are the document's, not the interface's: a Hebrew document gets them in the English
  interface too. No native speaker has checked them.
- **Text and Markdown files:** these formats carry no direction, so on opening (and when a
  Markdown source is applied) a paragraph whose first letter is Hebrew or another right-to-left
  letter becomes a right-to-left paragraph, as typing such a letter into a new paragraph does:
  headings, list items and table cells too. Lines that start in English, numbers alone, blank
  lines and a Markdown code block's lines keep the left-to-right direction, and an English file
  opens as before (see "Not done"). Text that comes inside another file (a `.docx` file's text
  chunk, a web archive's text) takes its direction from where it lands, as before.
- **Drop caps:** a Hebrew first letter is dropped at the size of a Latin one (the complex script
  size grows with the Latin size), not drawn at the size of the text beside it.
- **Checked and already right:** the caret steps over a letter with all its points as one stop;
  Ctrl+arrow crosses a pointed word in one step in reading order; the word count is the same with
  or without points; typed Hebrew is never underlined as misspelled English.
- **Watermarks:** Design › Watermark writes its words in the language of the text at the caret, as
  the engine writes its own words (`cmd::words::caret_lang`): in Hebrew text טיוטה, חסוי, אין
  להעתיק, דוגמה, בהקדם האפשרי and דחוף for DRAFT, CONFIDENTIAL, DO NOT COPY, SAMPLE, ASAP and
  URGENT (the catalog's words, kept with the document's other words in `wordcraft_doc::words`), in
  other text the English ones (`cmd::words::WATERMARKS`). In Hebrew text each item shows the Hebrew
  word it writes, whatever the interface language: an English interface shows טיוטה, asks for the
  installed Hebrew interface font when it has no embedded one, and tells screen readers the item's
  name is Hebrew (AccessKit language `he`). In other text the items show their translated labels
  as before, so a Hebrew interface over English text shows טיוטה and writes DRAFT. Custom Watermark
  starts from חסוי in Hebrew text, CONFIDENTIAL elsewhere; `design.watermark` without a text, in a
  document with no watermark, writes DRAFT in the language of the text at the caret (טיוטה in
  Hebrew text). Scripts and agents pass the word they want as before.
  - **Drawing:** the watermark's text is laid out in layout (`display::watermark_run` in
    `crates/layout/src/display/watermark.rs`), by the paragraph layout body text goes through, once
    for each watermark and document formatting: the layout is kept (by the text, the font, the
    document's style sheet, its font table and the installed fonts), so every page, the PNG and
    PDF exports and printing draw the same one, and strict export checks the same glyphs. Its Latin
    letters are in the face its font resolves to, as they always were (a document's font table
    doesn't send them to another face as it sends body text; no font is the fallback face). Its
    Hebrew letters are drawn in that face when it has every one of them, else in the document's
    complex-script font, as body text's are (David's stand-in in the edition's documents), and a
    letter neither has comes from the fallback faces: the default watermark font, Aptos (drawn with
    Carlito, which has no Hebrew), no longer draws boxes. Chinese, Japanese and Korean letters take
    the watermark's face the same way, else the document's East Asian font. Nothing else of the
    document's formatting reaches it (no bold, italic, capitals, spacing, raise, emphasis marks,
    right-to-left run or automatic space between East Asian and Latin text; the shaper's ligatures
    and kerning). The line is ordered by the Unicode Bidirectional Algorithm, reading the way its
    first strong letter does: סודי reads right to left (it was drawn ידוס), and Latin words and
    numbers beside Hebrew stand where they read (in סודי DRAFT 2026, DRAFT 2026 is drawn to the
    left of the Hebrew word). Points and other marks sit where the font puts them.
  - **Saved:** the text, Hebrew included, round-trips through DOCX (Word's `v:textpath`), with the
    watermark's font, which is kept and drawn as before.

## Installing on Ubuntu

The edition is one file, `wordcraft-hebrew-<version>-linux-x86_64.AppImage`, with its SHA-256 in
the `.sha256` file beside it. It runs on **Ubuntu 22.04 or newer** (or another x86_64 Linux with
glibc 2.35 or newer), and needs nothing installed: not libfuse2, not the fonts.

```bash
sha256sum -c wordcraft-hebrew-0.19.1-linux-x86_64.AppImage.sha256   # optional: the file is intact
mkdir -p ~/Applications && mv wordcraft-hebrew-0.19.1-linux-x86_64.AppImage ~/Applications/
chmod +x ~/Applications/wordcraft-hebrew-0.19.1-linux-x86_64.AppImage
~/Applications/wordcraft-hebrew-0.19.1-linux-x86_64.AppImage --install
```

- **`--install`** adds WordCraft (עברית) to the applications menu and search (in Hebrew, on an
  English desktop too: its name, description and search words, Hebrew and English, are the
  entry's defaults), and to Files' Open With for Word, OpenDocument, RTF, Markdown, LaTeX and HTML
  files. It is for the current user only and needs no password: it writes
  `~/.local/share/applications/ai.storyteller.wordcraft.hebrew.desktop` (or under
  `$XDG_DATA_HOME`), whose command is this AppImage at its current path, and the icons under
  `~/.local/share/icons/hicolor`, then refreshes the Open With list (`update-desktop-database`).
  The AppImage stays where it is, so put it somewhere lasting first (`~/Applications` above); after
  moving or renaming it, run `--install` again. Running it twice changes nothing. An entry of that
  name that `--install` didn't write (your own, or one copied out of the AppImage by hand) is left
  as it is, with its icons: `--install` says so and fails, and replaces it once you remove it.
- **Default app:** once it is in the menu, **File › Options › General › Make WordCraft the default
  for Word documents…** (הגדרת WordCraft כברירת המחדל למסמכי Word…) makes it open .docx, .doc, .rtf
  and .odt files and Word templates (`xdg-mime`, from xdg-utils). Before `--install` the button
  says, in the status bar, that WordCraft isn't in the menu yet and how to add it.
- **`--uninstall`** removes the menu entry and icons that `--install` wrote, and nothing else: not
  the AppImage, not the settings, and not an entry of that name that it didn't write or the icons
  beside it (it says so and fails). A default-app choice in `~/.config/mimeapps.list` that named the
  entry is left; desktops skip an entry that is gone.
- **`--cli ARGS…`** runs the command-line tool: `… .AppImage --cli convert letter.docx letter.pdf`,
  `--cli --help` for the rest. It gets the edition's region too.
- **`--help`** and **`--version`** say the same about Ubuntu 22.04 and glibc 2.35. `--version` also
  names the edition's language and desktop id (`edition: language he, desktop id
  ai.storyteller.wordcraft.hebrew`), as the program was built.
- Without FUSE (some containers and minimal systems) the AppImage's runtime unpacks itself to a
  temporary folder each time it starts, which only makes the start slower; `--appimage-extract`
  unpacks it once into `squashfs-root/`, whose `AppRun` takes the same options. Its `--install`
  makes the entry start that `AppRun`, even in a terminal that another AppImage program opened, whose
  `APPIMAGE` names that program.
- AppImageLauncher and Gear Lever can add it to the menu too, under a name of their own; the
  default-app button then still needs `--install`'s entry. Use one or the other.

The edition has its own desktop id, `ai.storyteller.wordcraft.hebrew` (the menu entry, the icons,
the window's app id and the AppStream file), so it and a stock WordCraft install keep separate menu
entries, taskbar buttons and default-app choices. They share the settings folder
`~/.config/wordcraft`: a language, a region or a saved AI key chosen in one applies in the other.

**Updates:** the edition never updates itself. The releases WordCraft's updater reads are the stock
English ones, and installing one would replace the edition with them, so it doesn't check at start
and its Help tab has no Check for Updates… button (the updater's window would also name the stock
program's maker and point at its releases). Asked for anyway (`ui.checkForUpdates`, from the
command search or a script), the check answers with the reason, which the status bar shows in
Hebrew, and sends no request. The AppImage carries no update information, so AppImageUpdate and
AppImageLauncher can't fetch the stock release either; README's line that the AppImage updates
itself with AppImageUpdate does not apply to the edition. To update, replace the file with a newer
build of the edition and run `--install` again if its name changed.

**What is in the AppImage besides the programs** (`usr/share/doc/wordcraft`): `NOTICE-hebrew-edition.txt`
(what the edition is and the licence of every font it bundles or embeds), the licence files it names
(`OFL-<family>.txt` for the bundled Hebrew fonts and craft-fonts' faces, `OFL-Inter.txt` and the other
assets/fonts licences, `epaint_default_fonts-*.txt` for egui's Ubuntu Light, Hack, Noto Emoji and icon
fonts), `open-hebrew-fonts.sha256`, `BUILD-INFO.txt`, WordCraft's `NOTICE`, `LICENSE-*` and
`ATTRIBUTION.md`, this page and [the AI assistant](assistant.md)'s. The ArtCraft name and marks are
the ArtCraft Team's (`docs/brand/LICENSE-brand.txt`), which keeps them and the links to its community
for WordCraft's own builds. The edition, a modified version, says in plain text that it is based on
WordCraft by the ArtCraft team, and has no Discord button, Help › Community Discord or Backstage
community link (`links_to_artcraft` in `crates/ui-egui/src/lib.rs`); its sample document (`--sample`
and File › New's sample, in English and in Hebrew) leaves out the sentence that sends readers to
the community board and its link, since the edition has no board of its own (`COMMUNITY_LINK` in
`crates/engine/src/sample.rs`, from the same build variable, `WORDCRAFT_DEFAULT_LANGUAGE`; WordCraft's
own builds keep the sentence and its link to getartcraft.com); see "Not done" for About. For
the same reason the stock `README.md`, with the ArtCraft logo and those links, is not in the AppImage
(`packaging/linux/package.sh` leaves it out of an edition with its own desktop id).

## Building the edition

```bash
packaging/hebrew/fetch-open-hebrew-fonts.sh "$HOME/.cache/wordcraft-hebrew-fonts"
CRAFT_I18N_SRC=/path/to/craft-i18n CRAFT_FONTS_DIR=/path/to/craft-fonts \
HEBREW_FONTS_DIR="$HOME/.cache/wordcraft-hebrew-fonts" packaging/hebrew/build-appimage.sh
```

It writes `dist/release/wordcraft-hebrew-<version>-linux-x86_64.AppImage` and its `.sha256`, then
runs `packaging/hebrew/test-appimage.sh` on it (below). With `--skip-build` it packages the programs
built last, and refuses unless their `--version` names Hebrew and the edition's desktop id. Every
input is pinned, and the script stops when one isn't the pinned one:

| Input | Pin | Checked by |
|---|---|---|
| craft-i18n (`CRAFT_I18N_SRC`) | the commit in `craft-i18n.rev`, with `packaging/hebrew/he-supplement.tsv` appended to a copy | `git rev-parse HEAD` |
| craft-fonts (`CRAFT_FONTS_DIR`), whose Noto Sans Hebrew the interface embeds | `CRAFT_FONTS_REV` in `build-appimage.sh`, `baf0f2c` | `git rev-parse HEAD` |
| the bundled Hebrew fonts (`HEBREW_FONTS_DIR`) | google/fonts commit `5026800` in `fetch-open-hebrew-fonts.sh`, and the SHA-256 of all 25 files (14 fonts, 11 licences) in `open-hebrew-fonts.sha256` | `fetch-open-hebrew-fonts.sh --check`: the same files byte for byte and nothing else; a fetch checks itself too |
| Rust | `RUST_VERSION` in `build-appimage.sh`, 1.97.0 (`WORDCRAFT_ANY_RUST=1` for a try-out with another) | `rustc --version` |
| Rust crates | `Cargo.lock` | `cargo build --locked` |
| appimagetool 1.9.1 and the type2-runtime 20251108 | SHA-256 in `packaging/linux/package.sh` | `sha256sum -c` on every run |

It also sets `WORDCRAFT_DEFAULT_LANGUAGE=he`, `CRAFT_I18N_REQUIRED` and `CRAFT_FONTS_REQUIRED`, the
edition's id (`WORDCRAFT_APP_ID`, built into the program for its window and default-app button, and
the name of its desktop entry, AppStream file and icons), its desktop entry, AppStream file and
AppRun (`packaging/hebrew/`), `WORDCRAFT_EDITION_REGION=he-IL` (the AppImage starts with Israel's A4
paper and metric units unless `WORDCRAFT_REGION` is set) and `WORDCRAFT_NO_UPDATE_INFO`. The
release date in the AppStream file and the times in the AppImage's file system are the commit's
(`SOURCE_DATE_EPOCH`), not the clock's. `desktop-file-validate` and `appstreamcli` must be installed
(`sudo apt install desktop-file-utils appstream`): `package.sh` validates the desktop entry and the
AppStream file with them, and the script refuses to build without them.

**Build system and glibc.** CI builds the stock packages on Ubuntu 22.04 so that they run on 22.04
and later. The edition can be built on a newer Ubuntu: Rust's standard library refers to two
functions of glibc 2.39 (`pidfd_spawnp`, `pidfd_getpid`) as weak symbols, which it does without when
they are missing, but linked on a glibc 2.39 system the programs record glibc 2.39 as a requirement
and a 22.04 system refuses to start them. `packaging/linux/glibc-floor.py`, run by `package.sh` with
`WORDCRAFT_GLIBC_FLOOR=2.35`, drops every requirement newer than 2.35 that only weak symbols have (the
symbols lose their version, as when linking against glibc 2.35, which has no such functions to
version) and fails the build when any other requirement newer than 2.35 remains. Checked on Ubuntu
24.04.5 (glibc 2.39): after it, both programs need glibc 2.35 (`libm`'s `GLIBC_2.35` is the newest),
`--check-only 2.35` passes, and with a copy of the system's libc that lacks the `GLIBC_2.39` version
both start (`--version`) and the CLI writes and converts a Hebrew document, where the unchanged
programs stop with ``version `GLIBC_2.39' not found``. The two weak functions still bind to
glibc 2.39's on 24.04. Not checked: a run on a real Ubuntu 22.04 desktop.

**`test-appimage.sh`** checks an AppImage of the edition (it needs no FUSE and writes only to a
temporary folder): the program is the edition's build (its `--version` names Hebrew and the
edition's desktop id, so programs left from a stock `cargo build` and packaged with `--skip-build`
fail); both programs pass `glibc-floor.py --check-only 2.35`; the desktop id, the menu entry and the
AppStream file validate; the icons are under the edition's id; the bundled fonts match
`open-hebrew-fonts.sha256`; every bundled and embedded font has its licence file, and the notices
are there, without the stock `README.md` or any link to the ArtCraft team's site or community; the
AppImage has no update information; `--version`, `--help` and `--cli` (which writes a Hebrew
document and converts it to PDF) work; and, in a throwaway HOME, `--install` run from a path with
spaces, quotes, `$` and `%` writes an entry that validates, whose command (read as the Desktop Entry
spec says) is that path and starts the AppImage, in Hebrew, with the Word types in Open With's cache
and the icons; a second `--install` changes nothing; `--uninstall` removes all of it and succeeds
when nothing is installed; an entry of that name that `--install` didn't write, and its icons, are
left by both options, which fail; `XDG_DATA_HOME` is followed; and the extracted folder's `AppRun`,
given another program's `APPIMAGE` and `APPDIR`, makes the entry start itself.

**`BUILD-INFO.txt`**, in the AppImage, records the source commit and its date, the craft-i18n,
craft-fonts and google/fonts revisions, the supplement's SHA-256, `rustc` and `cargo`, and the build
system (`/etc/os-release`, the machine, `ldd --version`). Packaging the same programs again gives
the same AppImage byte for byte (two runs of `build-appimage.sh --skip-build` gave one SHA-256).
The programs themselves come out the same from the same commit, inputs and toolchain only when the
source and Cargo home folders are the same too: the binaries carry some of their paths.

## Not done, or not checked

- A native-speaker review of the catalog, the supplement, the Hebrew sample text and the Hebrew
  AutoCorrect list.
- Hebrew spelling on the very first launch with no network (see Spelling), and the quality of the
  `he` pack inside WordCraft's affix engine, which was not examined.
- How Microsoft Word reads `w:bidi` in the document defaults was not observed. HTML export marks
  a right-to-left document's whole page `dir="rtl"` but no paragraph's own direction (`craft-html`
  writes none), so an English paragraph in a Hebrew document reads right to left in a browser;
  the HTML clipboard fragment carries no direction at all. ODT export says each paragraph's
  direction and the default style's, but reading an ODT file takes no direction from its default
  style (`craft-odf`). HTML has one bold, italic and size for every script and `craft-html`
  writes only the Latin ones, so HTML export and the HTML clipboard put Hebrew letters (and every
  character of a `w:rtl` run) in spans of their own that say their complex-script font, bold,
  italic and size as those (`html::by_script`); MHT is only read.
- Complex-script bold, italic and size (`w:bCs`, `w:iCs`, `w:szCs`) are WordCraft's own, as in
  Word, but not everywhere yet: the Word 97-2003 reader (`craft-doc-binary`) reads only the
  complex-script font, so a `.doc` file's Hebrew text takes the Latin bold, italic and size, as
  before (so a `.doc` file whose Hebrew is formatted otherwise than its Latin text shows it
  wrongly), until it reads sprmCFBoldBi, sprmCFItalicBi, sprmCHpsBi and the run direction
  sprmCFBiDi; a draft of that reader change was written for storytold/craft-libs but is not
  merged, so no test of differing `.doc` values exists yet; the ODT
  writer gives Hebrew text the Latin bold and italic where the document sets no complex-script
  ones (`craft-odf`); and the Font dialog's and the bullet dialog's samples (`crates/ui-egui`)
  draw Hebrew letters without the bold, italic and size they show. A complex-script font that
  nothing sets still falls back to the Latin font (Word's own choice there was not observed).
- Spaces, digits and punctuation of a run that says neither `w:rtl` nor `w:cs` take its Latin
  font, size, bold and italic whatever their bidi level, as Word picks them there by Unicode
  range; WordCraft's typed Hebrew says `w:rtl`, as Word's does. Such a file's runs are saved by
  direction (`craft-docx` marks their right-to-left pieces `w:rtl`), so the neutral characters of
  those pieces are saved in runs of their own that say their Latin values as complex-script ones
  (`crates/docx/src/save.rs`); that step resolves them without the table style's conditional
  formatting. `craft-docx` itself could keep them out of the `w:rtl` pieces only if Word ordered
  them the same, which was not observed. How Word orders the characters of a run without `w:rtl`
  in Hebrew text was not compared with WordCraft's Unicode bidi order.
- A `.docx` file without a styles part: WordCraft sets all its text at 10 pt, Latin and Hebrew
  alike, as Word does, but keeps its built-in styles for paragraphs naming them (Word formats them
  as Normal), now with the same complex-script bold, italic and size as their Latin ones.
- Keyboard switching of paragraph direction with the two Shift keys of Windows Hebrew keyboards,
  the Hebrew IME and keyboard layouts on each platform, Hebrew calendar dates and gematria date
  formats, and Hebrew hyphenation (no pattern file exists for it).
- Windows and macOS builds of the edition.
- The About window (Help › Help) still says the program is part of the Crafting Apps from the
  ArtCraft team and links to the ArtCraft team's site and Discord. The brand licence keeps those
  for WordCraft's own builds, so a copy of the edition that is passed on must say instead, in plain
  text, that it is based on WordCraft by the ArtCraft team, without the links. That page is ready as
  `crate::credits::about_ui` (its Hebrew row is in the supplement), but the window, drawn in
  `crates/ui-egui/src/dialogs/dispatch.rs`, doesn't call it yet: its About tab's own text has to give
  way to `about_ui` before the AppImage is passed on.
- The AppImage on a real Ubuntu 22.04 desktop: its glibc needs were checked as above, and its menu
  entry with `desktop-file-validate` and a throwaway HOME, not in GNOME's menu, dock and search.
- Not mirrored yet: the Date and Time dialog; most drop-down lists still start their rows at the
  left. Menu rows with a shortcut drawn by `rtl::menu_item` (the ribbon's menus, the page's context
  menu) are mirrored but named for a screen reader shortcut first (`Ctrl+B Bold`); Search's results
  set their own name, the command first.
- Still English: the Date and Time dialog's messages; a built-in style's name in Modify Style's
  Name box (a free text field that renames the style with what it holds: it needs to show the
  translated name and send a name only when it was changed); and engine messages no row covers.
- File › Options › Advanced has no control for the cursor movement setting (`file.options`
  `cursorMovement`, visual or logical, see [typing parity](typing-parity.md)); the supplement has
  its rows (תנועת הסמן: לוגית, חזותית).
- Installing a Language pack from a file downloaded in a browser (for a network whose filter breaks
  the download): craft-font-store at the pinned craft-libs has no public way to install a local
  archive against the pinned index's size and sha256 (its local path is the testing override, which
  skips the pin), and the downloads trust only the certificates compiled into rustls, not the
  system's, so an inspecting network's certificate is refused. Both need craft-libs changes.
- A Hebrew text or Markdown file opens with right-to-left paragraphs but not as a right-to-left
  document: its tables are not mirrored, its blank lines stay left to right, and the document
  defaults are the stock ones (only new blank documents take the edition's right-to-left default).
  An edit in Markdown Source reads the whole file again, so a paragraph's direction set by hand
  goes back to what its first letter says, as other formatting Markdown can't hold is lost.
- Watermarks: Custom Watermark offers no font, so a watermark's font comes only from a file that has
  one (`design.watermark` takes none either). A watermark reads the way its first letter does
  (UAX #9's rules P2 and P3), not the way its document does, so DRAFT סודי reads left to right in a
  Hebrew document. The gallery's words follow the text at the caret, not the document as a whole.
  In a build without craft-fonts on a system with no Hebrew font, an interface in another language
  draws the gallery's Hebrew items as boxes (they still write the Hebrew words; screen readers read
  their names). How Word orders and draws a Hebrew watermark (its direction, the face of its letters, its points)
  was not observed.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-26 | major | First version: Hebrew interface language and numbering, Hebrew edition (default language, catalog supplement, fonts, right-to-left new documents, Hebrew proofing through the complex-script language) |
| 2026-11-27 | minor | Mirrored task panes, balloons, directional icons and lists; Hebrew letters on Bold, Italic, Underline and Spelling; mixed Hebrew and Latin text in order; lengths number first; Hebrew in monospace text |
| 2026-11-27 | minor | Engine messages through the catalog (rows and `@msg` templates); supplement rows correct published ones; Accessibility pane from rule ids; translated style, style set, Unicode subset, icon and Language data names; Language pack licence, confirmation and network failures; WordCraft's own file browser by default in the edition; Mailings on the region's paper |
| 2026-11-27 | minor | Installing on Ubuntu (`--install`, `--uninstall`, `--cli`), the edition's own desktop id, Hebrew menu entry and AppStream text; no self-update and no Check for Updates button; the licence of every font in the AppImage; pinned and checked build inputs, `BUILD-INFO.txt`, `test-appimage.sh`; programs held to glibc 2.35 (Ubuntu 22.04) |
| 2026-11-27 | minor | `--install` and `--uninstall` leave an entry they didn't write and its icons; the extracted folder's `--install` ignores another program's `APPIMAGE`; `--version` names the edition, which `test-appimage.sh` and `build-appimage.sh --skip-build` check; no stock README in the AppImage; Add Font… takes several files in WordCraft's own browser; the edition's About page (`credits::about_ui`), not yet in the window |
| 2026-11-27 | minor | The File view's templates open in Hebrew, right to left, where new documents do; New Style, New Table Style and Customize Ribbon name what they make in the interface language (New Style no longer offers a taken name); untitled recovery copies named in it; the pane's Ignore Once leaves the word until its paragraph changes; why watermarks stay English in Hebrew text; the Find dialog keeps Match diacritics |
| 2026-11-27 | minor | File › Recover's Discard question names an untitled copy as the Recover page does; corrects the row above: Find and Replace has no Match diacritics, and its searches keep the setting the last search sent |
| 2026-11-27 | minor | Hebrew watermarks: drawn like body text on the page, in PNG, PDF and print (fallback faces, the document's complex-script font for Hebrew, the bidirectional order) and checked so by strict export; Design › Watermark and Custom Watermark write Hebrew words in Hebrew text and show the word they write; DOCX keeps the text (removed from "Not done", what is left noted there) |
| 2026-11-27 | minor | Find and Replace's Match diacritics box, sent with the dialog's searches; the dialogs' results and errors in Hebrew; the mini toolbar, Search's results and the reference and list samples mirrored, the Find and Create Source fields from the right; Modify Style's complex script font and size and its Automatic colour, New Style's Based on by translated names; check boxes for the direction keys (Options › Advanced) and gershayim (AutoCorrect Options) |
| 2026-11-28 | minor | Watermark review fixes: Latin letters in the face the watermark's font resolves to (as before the Hebrew change), East Asian letters in the watermark's face when it has them, no right-to-left run or automatic East Asian spacing from the document; the layout kept for every page, the PDF and strict export; the menu's labels translated as before except in Hebrew text, where it asks for the Hebrew interface font and marks the items Hebrew for screen readers; `design.watermark` starts a new watermark from DRAFT in the caret's language |
| 2026-11-28 | minor | A built-in Hebrew AutoCorrect list for Hebrew text (HE-16): 96 entries of our own, most final-letter ones also after prefix letters, listed in [autocorrect-he.md](autocorrect-he.md) for review |
| 2026-11-28 | minor | Hebrew drop caps at full size; Hebrew text and Markdown files open with right-to-left paragraphs; a new address list's columns in Hebrew, which Address Block and Greeting Line read (State as מחוז); inserted icons described in the interface language; the Language pack bar links the index's source; the edition's sample links no ArtCraft site; Greeting Line's English words noted as not done |
| 2026-11-28 | minor | Greeting Line greets in Hebrew in Hebrew text (לכבוד …, and שלום רב, with no name), English text unchanged; the edition's sample leaves out the community board's sentence instead of linking `example.org`; review fixes: Find's counts kept in English for scripts, Modify Style sends a complex script font or size only when it was changed or would be overwritten, the colour fields named by their rows, Match diacritics greyed with regular expressions, the page number box under the mirrored reference samples names no side, Paste Special's "and N more" in Hebrew; the Hebrew AutoCorrect list keeps a deleted or replaced entry's word as typed; a list with a ארץ column keeps its state in its מדינה column; a `.docx` file's text chunks take no direction from their letters; Markdown code lines stay left to right; Markdown Source's reset of a hand-set direction noted |
| 2026-11-28 | minor | Hebrew letters take only the complex-script bold, italic and size (`w:bCs`, `w:iCs`, `w:szCs`, RTL-11), as in Word: a Word file's run with only `w:b`, `w:i` or `w:sz` leaves them plain, and with no `w:szCs` in its defaults they are 10 pt; HTML, Markdown, RTF, LaTeX and Word 97-2003 files give them the Latin values when read, ODT its sizes; Convert Text to Table, Quick Tables, Draw Table, the "+--+" AutoFormat table and mail-merge labels are right to left in right-to-left text; ODT and HTML export write a right-to-left document's default direction |
| 2026-11-28 | minor | Word-fidelity review of RTL-11: spaces and punctuation of a run without `w:rtl` keep its Latin font, size, bold and italic (ECMA-376 §17.3.2.26), also after a save; typed Hebrew, and what is typed after it, says `w:rtl` as in Word; a `w:rtl` paragraph mark takes the complex-script size; a file without styles at 10 pt in both scripts; an unset complex-script default size kept through a save; HTML export and the HTML clipboard format Hebrew letters with their own values; labels take the direction of new documents; complex-script pairing reaches note separators, the glossary and tracked formatting |
