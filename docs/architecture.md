# WordCraft architecture

> **Last reviewed:** 2026-10-10 · **Last updated:** 2026-10-13 · **Change:** minor (`apps/wordcraft-android`, the Android app crate, and its narrow unsafe exemption; previously minor: `proof` is per language: English and Polish; previously major: first committed version; `plan/architecture.md` is local-only) · **Target:** Microsoft Word (Microsoft 365) for Mac 16.113.4

How WordCraft is built today (origin/main `40c5d51`). The longer design notes in
`plan/architecture.md` are gitignored and local to the original author's machine; this file is the
committed, current summary. Rules for working in the code are in [`AGENTS.md`](../AGENTS.md).

## Layers

~85,600 lines of Rust, 15 crates, 3 apps and `xtask` (counted on 2026-10-10; the Android app below, added on 2026-10-13, is not in that count). Layering is enforced by `cargo xtask layers`:
a crate may only depend on crates in lower layers, and nothing below L6 depends on egui, eframe,
winit or rfd. Apps and `xtask` are exempt from the layer rules, the Android app included.

| Layer | Crate | Lines | Job |
|---|---|---|---|
| L0 | `geom` | 0.2k | Units (points, twips, EMU, inches, cm), rectangles, measurement parsing |
| L1 | `doc` | 10.0k | Document model: stories, paragraphs, runs, tables, sections, styles, numbering, fields, notes, comments, revisions, equations (OMML-shaped math tree, linear/LaTeX input); edit primitives; bidi |
| L1 | `fonts` | 2.3k | Font database (bundled OFL + system fonts), metrics, outlines, shaping with HarfRust, Word font substitution (`word.rs`: Carlito ↔ Calibri, Caladea ↔ Cambria, Liberation ↔ Arial/Times/Courier), OpenType MATH |
| L1 | `proof` | 3.8k | Proofing per language (`ProofLang` from a run's `w:lang`): English (public-domain word list) and Polish (installed Hunspell dictionary through a Hunspell-style affix engine) spelling and suggestions, rule-based grammar for both, hyphenation dictionary and Liang patterns (English, Polish) |
| L2 | `layout` | 9.6k | Shaping runs, line breaking, tabs, lists, tables (row splitting, floating tables), pagination, headers/footers, notes, floating objects and wrap, math layout, fields, hit testing; produces a display list |
| L2 | `docx` | 8.4k | OOXML WordprocessingML reader and writer (Transitional and Strict namespaces, `.docx/.docm/.dotx/.dotm`), OMML, custom properties, VBA parts preserved |
| L2 | `docbin` | 4.4k | Word 97-2003 `.doc` reader ([MS-DOC]: FIB, piece table, FKPs, SPRMs, stylesheet, lists, tables, notes, pictures) over a CFB container |
| L2 | `formats` | 10.6k | TXT, Markdown, HTML, RTF, ODT and LaTeX import/export through a shared intermediate model |
| L3 | `render` | 0.6k | vello_cpu rasteriser: pages and thumbnails from the display list; PNG/JPEG |
| L3 | `pdf` | 1.6k | krilla PDF export: font subsets, selectable text, links, outline, tagging |
| L4 | `engine` | 15.2k | `Session` (document, selection, undo, layout cache, view state), **command registry** (422 `CommandSpec`s in `cmd/*.rs`), Word feature catalog (`catalog.rs`), samples, file I/O dispatch (`io.rs`), Read Aloud sentence model |
| L4 | `zotero` | 1.7k | Zotero's word-processor integration protocol answered against a `Session` |
| L4 | `control-key` | 0.1k | Per-window control-port key files shared by the app and the MCP bridge |
| L5 | `mcp` | 1.0k | MCP server: every command as a tool; headless or bridged to the running app |
| L6 | `ui-egui` | 13.1k | Word-style front end: ribbon, keytips, mini-toolbar, Backstage, canvas, rulers, panes, dialogs, Equation tab, i18n catalogs, themes, control channel, finger gestures for touch screens (`touch.rs`), headless `ui_shot` example |
| apps | `wordcraft` | 1.6k | Desktop app (eframe + wgpu; logging, window geometry, file associations) |
| apps | `wordcraft-cli` | 0.4k | `convert`, `text`, `inspect`, `run`, `commands`, `parity`, `mcp`, `zotero` |
| apps | `wordcraft-web` | 0.3k | Browser build (eframe web runner, WebGPU with WebGL2 fallback) |
| apps | `wordcraft-android` | 2.1k (with its tests) | Android build: a `cdylib` (`libwordcraft_android.so`) that Android loads through GameActivity, entry point `android_main`; the same `wordcraft-ui-egui` over the engine on eframe with wgpu (Vulkan); settings, custom dictionary, added fonts and a recovery copy of unsaved work in the app's private folder, the log in the app's folder on shared storage; the on-screen keyboard's text is read from GameActivity's buffer and turned into egui input events; the built-in file browser stands in for the system file dialog; on other targets only the plain modules (`ime`, `logging`, `storage`) compile. Unofficial, for personal sideloading |

## Android

`apps/wordcraft-android` is the only crate that does not inherit the workspace's `unsafe_code` forbid. Android starts a native app through the exported symbol `android_main`, and exporting a symbol is an unsafe attribute. The crate sets `unsafe_code` to deny in its own `Cargo.toml` and allows it on that one item in `src/lib.rs`; everything else in the crate, and every other crate, stays safe. It carries the same `#![deny(clippy::unwrap_used, ...)]` line as the production crates. `logging.rs` (logcat plus `wordcraft.log`), `ime.rs` (turning the keyboard's text buffer into key and text events) and `storage.rs` (folders, the recovery copy, when to write it and how to open it without a crash loop) are plain code with tests that run on a desktop; `app.rs`, `host.rs` and `keyboard.rs` need Android. winit allows one event loop per process and never stops it for a destroyed activity, so the Java side (`MainActivity`, which also turns the window insets into padding) ends the process with the activity.

`packaging/android` builds the library for arm64-v8a with cargo-ndk and assembles and signs the APK; `.github/workflows/android.yml` runs that on demand. Targets: Android 17 (API 37), minSdk 35, targetSdk 36, compileSdk 37. Verified so far: the library compiles and links for `aarch64-linux-android` and the APK structure is checked statically. Nothing has run on a physical device or an emulator.

## Data flow

1. **Open:** `engine::io` picks a reader by extension (`docx`, `docbin`, `formats`, JSON) and
   produces a `doc::Document`.
2. **Edit:** every user-visible action is a command id run through `Session::run` (ribbon,
   shortcuts, command search, CLI, control channel and MCP all dispatch by id). `Session::run`
   snapshots for undo (blocks are `Arc`-shared, so snapshots are cheap) and catches an escaped
   panic, restoring the document (never-crash standard).
3. **Layout:** `layout` turns the document into pages of positioned glyphs, boxes and images
   (display list), incrementally: an edit relayouts only what changed (1.4 ms on a 188-page
   document, measured 2026-10-06).
4. **Draw:** the UI paints pages through `render` (CPU raster, cached per page, pixel-aligned) on
   an egui/wgpu canvas; PDF export walks the same display list through `pdf`.
5. **Save:** `engine::io` picks a writer by extension.

## Testing and gates

- 821 `#[test]` functions and 10 property-test blocks: model, layout, DOCX round-trip and hostile
  input, `.doc`, formats, typing parity (`engine/src/tests_typing.rs`), bidi, i18n catalogs, UI
  interaction (egui_kittest), MCP acceptance; `hostile_params_never_panic` runs every command with
  junk params.
- `cargo xtask ci`: fmt, clippy `-D warnings`, tests, asset attribution, layering, wasm build.
- `cargo xtask parity` regenerates [`parity-checklist.md`](parity-checklist.md); a test keeps the
  catalog percentage above a floor that only rises.
- GitHub Actions on PRs: packaging lint, and FreeBSD / Windows ARM64 when build files change;
  release builds on pushes to `release`. The Android workflow (`.github/workflows/android.yml`) runs only on demand and is not part of the releases.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-13 | trivial | Android section: the keyboard bridge (buffer kept filled with the text before the caret, cluster-aware Backspace, text one typing piece at a time), the touch gate in `ui-egui`, the process ending with the activity, and the recovery copy's write timing and restore protocol (after review); the line count is with tests, counted with `wc -l` |
| 2026-10-13 | minor | `wordcraft-android` added to the apps and an Android section: the `android_main` unsafe exemption, what is in the crate, packaging and what is verified (compiles and links for `aarch64-linux-android`; not run on a device); `ui-egui` gains `touch.rs` (the finger scroll and, after review, the press gate) |
| 2026-10-10 | minor | `proof`: proofing languages (English, Polish), affix engine, Polish patterns; layout and engine route each run by its language |
| 2026-10-10 | major | First committed architecture doc, from the crates' sources and `AGENTS.md` |
