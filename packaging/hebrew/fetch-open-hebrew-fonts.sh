#!/usr/bin/env bash
# Fetch the open Hebrew fonts the Hebrew edition stages into its AppImage, from google/fonts at a
# pinned commit, into the folder given as $1 (one folder per family, each with its OFL.txt):
#
#   packaging/hebrew/fetch-open-hebrew-fonts.sh "$HOME/.cache/wordcraft-hebrew-fonts"
#
# Every family is under the SIL Open Font License 1.1. David Libre, Frank Ruhl Libre and Miriam
# Libre are the open look-alikes WordCraft draws for Word's David, Frank Ruehl and Miriam, which
# are proprietary and are never bundled. The files are build inputs: not committed (AGENTS rule
# 25), staged by packaging/linux/package.sh from WORDCRAFT_EXTRA_FONTS_DIR.
set -euo pipefail
OUT="${1:?usage: fetch-open-hebrew-fonts.sh <output folder>}"
COMMIT=5026800714d07231937d92eb4cceabf06e05d74e
FAMILIES=(davidlibre frankruhllibre miriamlibre heebo assistant alef notoserifhebrew rubik secularone suezone varelaround)

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
git -C "$WORK" init -q
git -C "$WORK" remote add origin https://github.com/google/fonts.git
git -C "$WORK" config core.sparseCheckout true
for fam in "${FAMILIES[@]}"; do echo "ofl/$fam" >>"$WORK/.git/info/sparse-checkout"; done
git -C "$WORK" fetch -q --depth 1 --filter=blob:none origin "$COMMIT"
git -C "$WORK" checkout -q FETCH_HEAD

mkdir -p "$OUT"
for fam in "${FAMILIES[@]}"; do
  mkdir -p "$OUT/$fam"
  # Upright faces only: the Hebrew letters have no italic, and each italic file doubles the size.
  find "$WORK/ofl/$fam" -maxdepth 1 -type f \( -name '*.ttf' -o -name 'OFL.txt' \) ! -iname '*Italic*' -exec cp {} "$OUT/$fam/" \;
  [[ -f "$OUT/$fam/OFL.txt" ]] || { echo "error: no OFL.txt for $fam" >&2; exit 1; }
done
echo "fetched ${#FAMILIES[@]} families into $OUT"
