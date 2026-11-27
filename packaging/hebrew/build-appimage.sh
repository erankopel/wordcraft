#!/usr/bin/env bash
# Build the Hebrew edition of WordCraft as a Linux AppImage.
#
# The edition is WordCraft with its interface in Hebrew from the first launch (a language chosen in
# File > Options still wins), new documents that start right to left, Hebrew fonts that need
# nothing installed, A4 paper, and no updates from the upstream English releases.
#
#   CRAFT_I18N_SRC=/path/to/craft-i18n CRAFT_FONTS_DIR=/path/to/craft-fonts \
#   HEBREW_FONTS_DIR="$HOME/.cache/wordcraft-hebrew-fonts" \
#     packaging/hebrew/build-appimage.sh
#
# CRAFT_I18N_SRC: a checkout of storytold/craft-i18n at the rev in craft-i18n.rev. It is copied, the
#   rows of he-supplement.tsv are added to the copy's Hebrew catalog (a row with a key the catalog
#   has replaces that row), and the copy is the build's CRAFT_I18N_DIR. Your checkout is not changed.
# CRAFT_FONTS_DIR: a checkout of storytold/craft-fonts: the build needs its Noto Sans Hebrew for the
#   interface (the build fails without it).
# HEBREW_FONTS_DIR: the folder fetch-open-hebrew-fonts.sh fills.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
: "${CRAFT_I18N_SRC:?set CRAFT_I18N_SRC to a craft-i18n checkout}"
: "${CRAFT_FONTS_DIR:?set CRAFT_FONTS_DIR to a craft-fonts checkout}"
: "${HEBREW_FONTS_DIR:?set HEBREW_FONTS_DIR to the folder packaging/hebrew/fetch-open-hebrew-fonts.sh fills}"
[[ -d "$HEBREW_FONTS_DIR" ]] || { echo "error: $HEBREW_FONTS_DIR is not a folder; run fetch-open-hebrew-fonts.sh first" >&2; exit 1; }

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
cp -a "$CRAFT_I18N_SRC" "$WORK/craft-i18n"
CATALOG="$WORK/craft-i18n/locales/he/wordcraft.tsv"
[[ -f "$CATALOG" ]] || { echo "error: $CATALOG not found: is CRAFT_I18N_SRC a craft-i18n checkout?" >&2; exit 1; }
# The file ends with a newline. Most of the supplement's rows are new keys; a row that repeats a
# key of the catalog corrects it: the build embeds the later row (crates/ui-egui/src/i18n/merge.rs).
cat "$HERE/he-supplement.tsv" >>"$CATALOG"

export CRAFT_I18N_DIR="$WORK/craft-i18n" CRAFT_I18N_REQUIRED=1 CRAFT_FONTS_REQUIRED=1
export WORDCRAFT_DEFAULT_LANGUAGE=he
export WORDCRAFT_EXTRA_FONTS_DIR="$HEBREW_FONTS_DIR"
export WORDCRAFT_EDITION_REGION=he-IL
export WORDCRAFT_NO_UPDATE_INFO=1
cd "$ROOT"
packaging/linux/package.sh --formats appimage "$@"
