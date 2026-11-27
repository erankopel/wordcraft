#!/usr/bin/env bash
# Build the Hebrew edition of WordCraft as a Linux AppImage.
#
# The edition is WordCraft with its interface in Hebrew from the first launch (a language chosen in
# File > Options still wins), new documents that start right to left, Hebrew fonts that need
# nothing installed, A4 paper, its own desktop id (it never takes a stock install's menu entry,
# icon or default-app association), and no updates from the upstream English releases.
#
#   packaging/hebrew/fetch-open-hebrew-fonts.sh "$HOME/.cache/wordcraft-hebrew-fonts"
#   CRAFT_I18N_SRC=/path/to/craft-i18n CRAFT_FONTS_DIR=/path/to/craft-fonts \
#   HEBREW_FONTS_DIR="$HOME/.cache/wordcraft-hebrew-fonts" \
#     packaging/hebrew/build-appimage.sh [--skip-build]
#
# Writes $DIST/wordcraft-hebrew-<version>-linux-<arch>.AppImage and its .sha256 (DIST defaults to
# dist/release), then checks the result with test-appimage.sh. --skip-build packages the programs
# built last, which must be the edition's.
#
# Every input is pinned, and the build stops when one isn't the pinned one:
# CRAFT_I18N_SRC: a checkout of storytold/craft-i18n at the commit in craft-i18n.rev. It is copied,
#   the rows of he-supplement.tsv are added to the copy's Hebrew catalog (a row with a key the
#   catalog has replaces that row), and the copy is the build's CRAFT_I18N_DIR. Your checkout is not
#   changed.
# CRAFT_FONTS_DIR: a checkout of storytold/craft-fonts at CRAFT_FONTS_REV below: the build embeds
#   its Noto Sans Hebrew for the interface (and fails without it).
# HEBREW_FONTS_DIR: the folder fetch-open-hebrew-fonts.sh fills, checked against
#   open-hebrew-fonts.sha256.
# Rust: RUST_VERSION below (WORDCRAFT_ANY_RUST=1 builds with another one, for a try-out).
# The tools: desktop-file-validate (desktop-file-utils) and appstreamcli (appstream) check the menu
#   entry and the AppStream file; python3 runs glibc-floor.py.
#
# The build system can be newer than the systems the AppImage is for: the programs are held to
# glibc 2.35 (Ubuntu 22.04) by packaging/linux/glibc-floor.py, and the build fails when they need
# more. BUILD-INFO.txt in the AppImage (usr/share/doc/wordcraft) records the revisions, the
# toolchain, the build system and the font hashes; docs/localization-he.md says how to rebuild.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
CRAFT_FONTS_REV=baf0f2ccd1745893aa6337a35141ec12a23d4fc1
RUST_VERSION=1.97.0
GLIBC_FLOOR=2.35
APP_ID=ai.storyteller.wordcraft.hebrew
: "${CRAFT_I18N_SRC:?set CRAFT_I18N_SRC to a craft-i18n checkout}"
: "${CRAFT_FONTS_DIR:?set CRAFT_FONTS_DIR to a craft-fonts checkout}"
: "${HEBREW_FONTS_DIR:?set HEBREW_FONTS_DIR to the folder packaging/hebrew/fetch-open-hebrew-fonts.sh fills}"

fail() {
  echo "error: $*" >&2
  exit 1
}
# The commit a checkout is at.
rev() { git -C "$1" rev-parse HEAD 2>/dev/null || echo "(not a git checkout)"; }

I18N_REV="$(tr -d '[:space:]' <"$ROOT/craft-i18n.rev")"
[[ "$(rev "$CRAFT_I18N_SRC")" = "$I18N_REV" ]] || fail "CRAFT_I18N_SRC=$CRAFT_I18N_SRC is at $(rev "$CRAFT_I18N_SRC"), not at craft-i18n.rev ($I18N_REV)"
[[ "$(rev "$CRAFT_FONTS_DIR")" = "$CRAFT_FONTS_REV" ]] || fail "CRAFT_FONTS_DIR=$CRAFT_FONTS_DIR is at $(rev "$CRAFT_FONTS_DIR"), not at $CRAFT_FONTS_REV"
"$HERE/fetch-open-hebrew-fonts.sh" --check "$HEBREW_FONTS_DIR"
RUSTC="$(rustc --version)"
if [[ "$RUSTC" != "rustc $RUST_VERSION "* ]]; then
  [[ -n "${WORDCRAFT_ANY_RUST:-}" ]] || fail "$RUSTC is not Rust $RUST_VERSION (rustup toolchain install $RUST_VERSION; or WORDCRAFT_ANY_RUST=1 for a try-out)"
  echo "warning: building with $RUSTC, not Rust $RUST_VERSION" >&2
fi
for tool in desktop-file-validate appstreamcli python3; do
  command -v "$tool" >/dev/null || fail "$tool not found: install desktop-file-utils, appstream and python3"
done
if [[ -n "$(git -C "$ROOT" status --porcelain --untracked-files=no)" ]]; then
  echo "warning: the working tree has uncommitted changes; BUILD-INFO.txt names the commit, not them" >&2
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
cp -a "$CRAFT_I18N_SRC" "$WORK/craft-i18n"
CATALOG="$WORK/craft-i18n/locales/he/wordcraft.tsv"
[[ -f "$CATALOG" ]] || fail "$CATALOG not found: is CRAFT_I18N_SRC a craft-i18n checkout?"
# The file ends with a newline. Most of the supplement's rows are new keys; a row that repeats a
# key of the catalog corrects it: the build embeds the later row (crates/ui-egui/src/i18n/merge.rs).
cat "$HERE/he-supplement.tsv" >>"$CATALOG"

# Dates from the commit, not the clock, so a rebuild of the same commit names the same day and the
# AppImage's file system carries the same times (mksquashfs reads SOURCE_DATE_EPOCH).
SOURCE_DATE_EPOCH="$(git -C "$ROOT" log -1 --format=%ct)"
WORDCRAFT_BUILD_DATE="$(date -u -d "@$SOURCE_DATE_EPOCH" +%Y-%m-%d)"
export SOURCE_DATE_EPOCH WORDCRAFT_BUILD_DATE
# VERSION and DIST, as package.sh sees them.
# shellcheck source=../env.sh
. "$ROOT/packaging/env.sh"

# What went into this build, for usr/share/doc/wordcraft.
{
  echo "WordCraft Hebrew edition $VERSION, built from"
  echo "  source:        $(rev "$ROOT") (commit date $WORDCRAFT_BUILD_DATE)"
  echo "  craft-i18n:    $I18N_REV, with packaging/hebrew/he-supplement.tsv (sha256 $(sha256sum "$HERE/he-supplement.tsv" | cut -d' ' -f1))"
  echo "  craft-fonts:   $CRAFT_FONTS_REV"
  echo "  google/fonts:  $(sed -n 's/^COMMIT=//p' "$HERE/fetch-open-hebrew-fonts.sh"), the files in open-hebrew-fonts.sha256"
  echo "  AppImage tools: appimagetool and type2-runtime at the versions and SHA-256 sums pinned in packaging/linux/package.sh"
  echo "with"
  echo "  $RUSTC; $(cargo --version)"
  echo "  build system:  $(sed -n 's/^PRETTY_NAME=//p' /etc/os-release 2>/dev/null | tr -d '"'), $(uname -m), $(ldd --version 2>/dev/null | head -n 1)"
  echo "and held to glibc $GLIBC_FLOOR (Ubuntu 22.04) by packaging/linux/glibc-floor.py."
} >"$WORK/BUILD-INFO.txt"

export CRAFT_I18N_DIR="$WORK/craft-i18n" CRAFT_I18N_REQUIRED=1 CRAFT_FONTS_REQUIRED=1
export WORDCRAFT_DEFAULT_LANGUAGE=he
export WORDCRAFT_APP_ID="$APP_ID"
export WORDCRAFT_PACKAGE_NAME=wordcraft-hebrew
export WORDCRAFT_DESKTOP_ENTRY="$HERE/$APP_ID.desktop"
export WORDCRAFT_METAINFO="$HERE/$APP_ID.metainfo.xml.in"
export WORDCRAFT_APPRUN="$HERE/AppRun.in"
export WORDCRAFT_EXTRA_FONTS_DIR="$HEBREW_FONTS_DIR"
export WORDCRAFT_EXTRA_DOCS="$HERE/NOTICE-hebrew-edition.txt:$HERE/open-hebrew-fonts.sha256:$WORK/BUILD-INFO.txt:$ROOT/docs/localization-he.md:$ROOT/docs/assistant.md"
export WORDCRAFT_EDITION_REGION=he-IL
export WORDCRAFT_GLIBC_FLOOR="$GLIBC_FLOOR"
export WORDCRAFT_NO_UPDATE_INFO=1
# --skip-build packages the programs built last, so they have to be the edition's: built in Hebrew
# with the edition's desktop id, which their --version names (test-appimage.sh checks the AppImage's).
if [[ " $* " = *" --skip-build "* && -z "${CROSS_TARGET:-}" ]]; then
  BUILT="$("$CARGO_TARGET_DIR/release/wordcraft" --version 2>/dev/null)" || BUILT=""
  [[ "$BUILT" = *"edition: language he, desktop id $APP_ID"* ]] ||
    fail "$CARGO_TARGET_DIR/release/wordcraft is not the edition's build (its --version doesn't name Hebrew and $APP_ID); build without --skip-build"
fi
cd "$ROOT"
packaging/linux/package.sh --formats appimage "$@"

ARCH="${CROSS_ARCH:-$(uname -m)}"
[[ "$ARCH" != arm64 ]] || ARCH=aarch64
OUT="$DIST/$WORDCRAFT_PACKAGE_NAME-$VERSION-linux-$ARCH.AppImage"
(cd "$DIST" && sha256sum "$(basename "$OUT")" >"$(basename "$OUT").sha256")
echo "wrote $OUT.sha256"
"$HERE/test-appimage.sh" "$OUT"
