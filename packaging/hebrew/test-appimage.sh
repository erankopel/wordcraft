#!/usr/bin/env bash
# Check an AppImage of WordCraft's Hebrew edition, as build-appimage.sh makes it:
#
#   packaging/hebrew/test-appimage.sh dist/release/wordcraft-hebrew-<version>-linux-x86_64.AppImage
#
# - What it carries: programs that start on glibc 2.35 (Ubuntu 22.04); the edition's own id, menu
#   entry and AppStream file, both validated; its icons; the pinned fonts and the licence of every
#   font; the notices; no update information.
# - Its AppRun: --version, --help and --cli; and --install and --uninstall in a throwaway HOME, run
#   from a path with spaces, quotes, $ and %: the menu entry validates, starts this AppImage, is in
#   Hebrew and lists the Word types; the icons and the Open With cache are there; --install again
#   changes nothing; --uninstall removes all of it and nothing it didn't write; XDG_DATA_HOME is
#   followed.
#
# Needs python3, desktop-file-validate and appstreamcli (desktop-file-utils, appstream). Runs the
# AppImage without FUSE (APPIMAGE_EXTRACT_AND_RUN=1) and writes only to a temporary folder.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
IMAGE="$(readlink -f "${1:?usage: test-appimage.sh <AppImage>}")"
[[ -f "$IMAGE" && -x "$IMAGE" ]] || { echo "error: $IMAGE is not an executable file" >&2; exit 2; }
T="$(mktemp -d)"
trap 'rm -rf "$T"' EXIT
mkdir -p "$T/tmp" "$T/home"
FAILS=0
ok() { echo "ok:   $*"; }
bad() {
  echo "FAIL: $*" >&2
  FAILS=$((FAILS + 1))
}
# check DESCRIPTION COMMAND...: passes when the command succeeds; shows its output when it doesn't.
check() {
  local what="$1"
  shift
  if "$@" >"$T/out" 2>&1; then ok "$what"; else
    bad "$what"
    sed 's/^/      /' "$T/out" >&2
  fi
}
# The AppImage (or a copy, $1) as a person's shell starts it: a clean environment with the
# throwaway HOME, and no FUSE.
run() { env -i HOME="$T/home" PATH="$PATH" LANG=C.UTF-8 TMPDIR="$T/tmp" APPIMAGE_EXTRACT_AND_RUN=1 "$@"; }

# ---- what it carries ----------------------------------------------------------------------------
(cd "$T" && env -u APPIMAGE_EXTRACT_AND_RUN "$IMAGE" --appimage-extract >/dev/null)
R="$T/squashfs-root"
APP_ID="$(basename "$(find "$R" -maxdepth 1 -name '*.desktop' | head -n 1)" .desktop)"
DOC="$R/usr/share/doc/wordcraft"
VERSION="$("$R/usr/bin/wordcraft" --version | awk '{ print $2 }')"
SUMS="$HERE/open-hebrew-fonts.sha256"
check "its own desktop id ($APP_ID), not the stock one" test "$APP_ID" = ai.storyteller.wordcraft.hebrew
check "both programs start on glibc 2.35 (Ubuntu 22.04)" \
  python3 -I "$ROOT/packaging/linux/glibc-floor.py" --check-only 2.35 "$R/usr/bin/wordcraft" "$R/usr/bin/wordcraft-cli"
check "AppRun is the edition's script" grep -q -e '--install' "$R/AppRun"
check "the menu entry at the AppImage's root validates" desktop-file-validate "$R/$APP_ID.desktop"
check "the menu entry in usr/share/applications validates" desktop-file-validate "$R/usr/share/applications/$APP_ID.desktop"
check "the AppStream file validates" appstreamcli validate --no-net "$R/usr/share/metainfo/$APP_ID.metainfo.xml"
icons_of_every_size() {
  local s
  for s in 16x16 24x24 32x32 48x48 64x64 128x128 256x256 512x512; do
    [[ -f "$R/usr/share/icons/hicolor/$s/apps/$APP_ID.png" ]] || { echo "no $s icon"; return 1; }
  done
  [[ -f "$R/usr/share/icons/hicolor/scalable/apps/$APP_ID.svg" ]] || { echo "no scalable icon"; return 1; }
  ! find "$R/usr/share/icons" -name 'ai.storyteller.wordcraft.png' -o -name 'ai.storyteller.wordcraft.svg' | grep .
}
check "an icon of every size under the edition's id, none under the stock one" icons_of_every_size
pinned_fonts() {
  local sum path
  while read -r sum path; do
    [[ "$path" = *.ttf ]] || continue
    echo "$sum  ${path##*/}"
  done <"$SUMS" | (cd "$R/usr/share/wordcraft/fonts" && sha256sum --check --quiet --strict)
}
check "the bundled fonts are the pinned ones" pinned_fonts
licences() {
  local fam f
  while read -r fam; do
    [[ -s "$DOC/OFL-$fam.txt" ]] || { echo "no OFL-$fam.txt"; return 1; }
  done < <(cut -d' ' -f3 "$SUMS" | cut -d/ -f1 | sort -u)
  for f in NOTICE NOTICE-hebrew-edition.txt BUILD-INFO.txt open-hebrew-fonts.sha256 LICENSE-MIT LICENSE-APACHE \
    localization-he.md OFL-noto-sans-hebrew.txt OFL-Inter.txt OFL-SourceSans3.txt OFL-SourceSerif4.txt OFL-JetBrainsMono.txt \
    epaint_default_fonts-UFL.txt epaint_default_fonts-Hack-Regular.txt epaint_default_fonts-OFL.txt \
    epaint_default_fonts-emoji-icon-font-mit-license.txt; do
    [[ -s "$DOC/$f" ]] || { echo "no $f"; return 1; }
  done
}
check "a licence for every bundled font family and every font the programs embed, and the notices" licences
no_update_information() { [[ -z "$(env -u APPIMAGE_EXTRACT_AND_RUN "$IMAGE" --appimage-updateinformation 2>/dev/null)" ]]; }
check "no update information (AppImageUpdate would fetch the English releases)" no_update_information

# ---- AppRun's options ---------------------------------------------------------------------------
run "$IMAGE" --version >"$T/version" 2>&1 || true
check "--version names the version and the oldest Ubuntu" grep -q "^wordcraft $VERSION .*" "$T/version"
check "--version: Ubuntu 22.04 or newer" grep -q "Ubuntu 22.04 or newer" "$T/version"
run "$IMAGE" --help >"$T/help" 2>&1 || true
help_lists() {
  local w
  for w in --install --uninstall --cli "Ubuntu 22.04" --sample; do
    grep -qF -e "$w" "$T/help" || { echo "no $w"; return 1; }
  done
}
check "--help: --install, --uninstall, --cli, the oldest Ubuntu and the program's own options" help_lists
run "$IMAGE" --cli --version >"$T/cli" 2>&1 || true
check "--cli runs wordcraft-cli" grep -q "^wordcraft-cli $VERSION" "$T/cli"
cli_converts() {
  run "$IMAGE" --cli run --cmd 'text.insert={"text":"שלום עולם"}' --save "$T/home/a.docx" &&
    run "$IMAGE" --cli convert "$T/home/a.docx" "$T/home/a.pdf" && [[ -s "$T/home/a.pdf" ]]
}
check "--cli writes a Hebrew document and converts it to PDF" cli_converts

# ---- --install and --uninstall ------------------------------------------------------------------
# A path a desktop entry has to quote: a space, double quotes, a dollar sign and a percent sign.
P="$T/my apps/Word \"Craft\" \$HOME 100%.AppImage"
mkdir -p "$(dirname "$P")"
ln "$IMAGE" "$P" 2>/dev/null || cp "$IMAGE" "$P"
APPS="$T/home/.local/share/applications"
ICONS="$T/home/.local/share/icons/hicolor"
E="$APPS/$APP_ID.desktop"
# A key of the entry $1, read as the Desktop Entry spec says: string escapes; and for Exec, the
# quoting of arguments and the field codes (the arguments, one per line, without %F).
cat >"$T/entry.py" <<'PY'
import sys


def unescape(v):
    out, i = [], 0
    while i < len(v):
        if v[i] == "\\" and i + 1 < len(v):
            out.append({"s": " ", "n": "\n", "t": "\t", "r": "\r", "\\": "\\"}.get(v[i + 1], "\\" + v[i + 1]))
            i += 2
        else:
            out.append(v[i])
            i += 1
    return "".join(out)


def arguments(v):
    args, cur, i = [], None, 0
    while i < len(v):
        c = v[i]
        if c == '"':
            cur, i = cur or "", i + 1
            while i < len(v) and v[i] != '"':
                if v[i] == "\\" and i + 1 < len(v) and v[i + 1] in '"`$\\':
                    cur, i = cur + v[i + 1], i + 2
                else:
                    cur, i = cur + v[i], i + 1
            i += 1
        elif c == " ":
            if cur is not None:
                args.append(cur)
            cur, i = None, i + 1
        else:
            cur, i = (cur or "") + c, i + 1
    if cur is not None:
        args.append(cur)
    return [a.replace("%%", "%") for a in args if a not in ("%F", "%f", "%U", "%u")]


entry = dict(line.split("=", 1) for line in open(sys.argv[1], encoding="utf-8").read().splitlines() if "=" in line)
value = unescape(entry[sys.argv[2]])
print("\n".join(arguments(value)) if sys.argv[2] == "Exec" else value)
PY
key() { python3 -I "$T/entry.py" "$E" "$1"; }
check "--install" run "$P" --install
check "the installed menu entry validates" desktop-file-validate "$E"
check "its command is the AppImage, from a path with spaces, quotes, \$ and %" test "$(key Exec)" = "$P"
check "its TryExec is the AppImage" test "$(key TryExec)" = "$P"
starts_this_appimage() { run "$(key Exec)" --version >"$T/started" && grep -q '^wordcraft ' "$T/started"; }
check "its command starts this AppImage" starts_this_appimage
in_hebrew() { grep -q '^Name=WordCraft (עברית)$' "$E" && grep -q '^GenericName=מעבד תמלילים$' "$E" && grep -q '^Keywords=.*מסמך;.*docx;' "$E"; }
check "it names WordCraft and describes it in Hebrew" in_hebrew
open_with() {
  local t
  for t in application/vnd.openxmlformats-officedocument.wordprocessingml.document application/msword \
    application/vnd.oasis.opendocument.text application/rtf; do
    grep -q "^MimeType=.*$t;" "$E" || { echo "MimeType lacks $t"; return 1; }
    if command -v update-desktop-database >/dev/null; then
      grep -q "^$t=.*$APP_ID.desktop" "$APPS/mimeinfo.cache" || { echo "mimeinfo.cache lacks $t"; return 1; }
    fi
  done
}
check "Open With offers it for .docx, .doc, .odt and .rtf" open_with
icons_installed() { [[ -f "$ICONS/256x256/apps/$APP_ID.png" && -f "$ICONS/16x16/apps/$APP_ID.png" && -f "$ICONS/scalable/apps/$APP_ID.svg" ]]; }
check "its icons" icons_installed
cp "$E" "$T/first.desktop"
check "--install again" run "$P" --install
check "--install again changes nothing" cmp "$T/first.desktop" "$E"
check "--uninstall" run "$P" --uninstall
all_removed() { [[ ! -e "$E" ]] && ! find "$ICONS" -name "$APP_ID.*" | grep .; }
check "--uninstall removes the entry and the icons" all_removed
check "--uninstall with nothing installed" run "$P" --uninstall
printf '[Desktop Entry]\nType=Application\nName=Not ours\nExec=true\n' >"$E"
if run "$P" --uninstall >"$T/out" 2>&1; then bad "--uninstall succeeded over an entry --install didn't write"; else
  check "--uninstall leaves an entry --install didn't write, and fails" test -f "$E"
fi
rm -f "$E"
follows_xdg_data_home() {
  env -i HOME="$T/home" XDG_DATA_HOME="$T/data" PATH="$PATH" TMPDIR="$T/tmp" APPIMAGE_EXTRACT_AND_RUN=1 "$P" --install &&
    [[ -f "$T/data/applications/$APP_ID.desktop" && ! -e "$E" ]]
}
check "--install follows XDG_DATA_HOME" follows_xdg_data_home

if ((FAILS)); then
  echo "$FAILS checks failed for $IMAGE" >&2
  exit 1
fi
echo "all checks passed for $IMAGE"
