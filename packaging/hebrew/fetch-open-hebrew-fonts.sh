#!/usr/bin/env bash
# Fetch the open Hebrew fonts the Hebrew edition stages into its AppImage, from google/fonts at a
# pinned commit, into the folder given as $1 (one folder per family, each with its OFL.txt), and
# check every file against open-hebrew-fonts.sha256:
#
#   packaging/hebrew/fetch-open-hebrew-fonts.sh "$HOME/.cache/wordcraft-hebrew-fonts"
#   packaging/hebrew/fetch-open-hebrew-fonts.sh --check "$HOME/.cache/wordcraft-hebrew-fonts"
#
# --check only checks a folder fetched before (build-appimage.sh runs it): the same files, byte for
# byte, and nothing else.
#
# Every family is under the SIL Open Font License 1.1. David Libre, Frank Ruhl Libre and Miriam
# Libre are the open look-alikes WordCraft draws for Word's David, Frank Ruehl and Miriam, which
# are proprietary and are never bundled. The files are build inputs: not committed (AGENTS rule
# 25), staged by packaging/linux/package.sh from WORDCRAFT_EXTRA_FONTS_DIR.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SUMS="$HERE/open-hebrew-fonts.sha256"
COMMIT=5026800714d07231937d92eb4cceabf06e05d74e
FAMILIES=(davidlibre frankruhllibre miriamlibre heebo assistant alef notoserifhebrew rubik secularone suezone varelaround)

# The folder $1 holds exactly the files open-hebrew-fonts.sha256 lists, with those contents.
check() {
  local dir="$1"
  [[ -d "$dir" ]] || { echo "error: $dir is not a folder" >&2; return 1; }
  (cd "$dir" && sha256sum --check --quiet --strict "$SUMS") || {
    echo "error: the fonts in $dir are not the pinned ones ($SUMS); fetch them again into an empty folder" >&2
    return 1
  }
  if ! diff <(cd "$dir" && find . -type f | sed 's|^\./||' | LC_ALL=C sort) <(awk '{ print $2 }' "$SUMS" | LC_ALL=C sort) >/dev/null; then
    echo "error: $dir has files that $SUMS doesn't list" >&2
    return 1
  fi
  echo "checked the ${#FAMILIES[@]} font families in $dir against $SUMS"
}

if [[ "${1:-}" = --check ]]; then
  check "${2:?usage: fetch-open-hebrew-fonts.sh --check <folder>}"
  exit
fi
OUT="${1:?usage: fetch-open-hebrew-fonts.sh <output folder>}"

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
check "$OUT"
