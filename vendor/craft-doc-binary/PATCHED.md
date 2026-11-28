# Patched copy of craft-doc-binary

- **Source:** https://github.com/storytold/craft-libs, folder `crates/doc-binary`, at rev
  `93dac78c93a7f1fd66e2022edeb089233b1dc45a` (the rev the root `Cargo.toml` pins for every
  craft-libs crate). Taken with `git archive` of that rev; byte for byte the same as Cargo's
  checkout of it before the changes below.
- **Licence:** MIT OR Apache-2.0, as craft-libs and WordCraft (`LICENSE-MIT`, `LICENSE-APACHE` at
  the root of this repository). Copyright stays with the craft-libs contributors.
- **Used through:** `[patch."https://github.com/storytold/craft-libs"] craft-doc-binary = { path =
  "vendor/craft-doc-binary" }` in the root `Cargo.toml`. The workspace excludes `vendor/`, so the
  copy is a dependency, not a member.

## What changed

1. `src/fmt.rs`, `apply_char`: the reader now applies four character sprms it skipped, the ones
   the crate's own writer writes (`src/write/grpprl.rs`), as [MS-DOC] 2.6.1 gives them:
   - sprmCFBoldBi (0x085C) and sprmCFItalicBi (0x085D) set `bold_cs` and `italic_cs`, with the
     toggle rule sprmCFBold uses (0x80 keeps, 0x81 inverts the style's complex-script value);
   - sprmCHpsBi (0x4A61) sets `size_cs` from half-points, clamped as sprmCHps is;
   - sprmCFBiDi (0x085A) sets `rtl` (0 and 1 as written; 0x80 and 0x81 by the toggle rule).
   Styles and list levels go through the same function, so their complex-script values are read
   wherever their Latin ones are.
2. `src/fmt.rs`, `stylesheet`: the base complex-script size is 10 pt when the file gives none, as
   the Latin one is.
3. `Cargo.toml`: the values the crate took from craft-libs' workspace are written out (version,
   edition, licence, rust-version, repository, lints), and craft-cfb, craft-codepage and
   craft-wordmodel are git dependencies at the same rev, so they unify with the workspace's copies.

Nothing else differs from the source.

## Why

Without these sprms, a Word 97-2003 file's Hebrew (and Arabic) text took the Latin bold, italic
and size, which is wrong where a Word file formats them differently, and lost its run direction.
WordCraft cannot change storytold/craft-libs from the Hebrew edition's fork, so the edition carries
this copy; `docs/localization-he.md` (Patched craft-libs code) states the departure from AGENTS.md
rules 9 and 10.

## When to drop it

Once craft-libs reads these sprms: remove this folder, the `[patch]` entry and the `exclude`, and
move the craft-libs rev in a change of its own. WordCraft's tests in `crates/docbin/tests/it`
check the behaviour either way.
