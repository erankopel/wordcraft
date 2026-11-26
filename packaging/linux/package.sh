#!/usr/bin/env bash
# Build and package WordCraft for Linux (<arch> is x86_64 or aarch64, or riscv64 when cross-compiling):
#
#   $DIST/wordcraft-<version>-linux-<arch>.AppImage  any distro with glibc >= the build host's
#   $DIST/wordcraft-<version>-linux-<arch>.AppImage.zsync  delta updates (needs zsyncmake)
#   $DIST/wordcraft-<version>-linux-<arch>.deb       Debian, Ubuntu, Mint, Pop!_OS, ...
#   $DIST/wordcraft-<version>-linux-<arch>.rpm       Fedora, openSUSE, RHEL, ...
#   $DIST/wordcraft-<version>-linux-<arch>.tar.gz    plain FHS-style tree (bin/, share/)
#
# Usage: packaging/linux/package.sh [--skip-build] [--formats "appimage deb rpm tar"]
#
# Needs: cargo; nfpm for deb/rpm (https://nfpm.goreleaser.com); appimagetool for the AppImage
# (downloaded into $CARGO_TARGET_DIR if missing). Build on an old distro (CI: Ubuntu 22.04,
# glibc 2.35) so the binaries run on newer ones. Optional: desktop-file-validate, appstreamcli,
# zsyncmake (the zsync package) for the AppImage's .zsync.
#
# Environment, all optional: WORDCRAFT_EXTRA_FONTS_DIR (folders of .ttf/.otf files, each with its
# OFL.txt, staged to usr/share/wordcraft/fonts), WORDCRAFT_EDITION_REGION (the AppImage starts with
# this WORDCRAFT_REGION unless the person sets one), WORDCRAFT_NO_UPDATE_INFO (no AppImage update
# information). packaging/hebrew/build-appimage.sh sets them for the Hebrew edition.
set -euo pipefail
# shellcheck source=../env.sh
. "$(dirname "${BASH_SOURCE[0]}")/../env.sh"
HERE="$ROOT/packaging/linux"
APP_ID=ai.storyteller.wordcraft

SKIP_BUILD=0
FORMATS="appimage deb rpm tar"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-build) SKIP_BUILD=1; shift ;;
    --formats) FORMATS="$2"; shift 2 ;;
    -h | --help) sed -n '2,17p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

ARCH="${CROSS_ARCH:-$(uname -m)}"
case "$ARCH" in
  riscv64) DEB_ARCH=riscv64 ;;
  x86_64) DEB_ARCH=amd64 ;;
  aarch64 | arm64) ARCH=aarch64; DEB_ARCH=arm64 ;;
  *) echo "unsupported architecture $ARCH" >&2; exit 2 ;;
esac
export WORDCRAFT_MAINTAINER="${WORDCRAFT_MAINTAINER:-WordCraft maintainers <wordcraft@storyteller.ai>}"
BASENAME="wordcraft-$VERSION-linux-$ARCH"

echo "==> WordCraft $VERSION for Linux $ARCH ($FORMATS)"

if [[ "$SKIP_BUILD" = 0 ]]; then
  if [[ -n "${CROSS_TARGET:-}" ]]; then
    (cd "$ROOT" && cargo build --release --locked -p wordcraft -p wordcraft-cli --target "$CROSS_TARGET")
  else
    (cd "$ROOT" && cargo build --release --locked -p wordcraft -p wordcraft-cli)
  fi
fi
BIN="$CARGO_TARGET_DIR/${CROSS_TARGET:+$CROSS_TARGET/}release"
WORK="$CARGO_TARGET_DIR/linux-package"
STAGE="$WORK/root"
rm -rf "$WORK"

# ---- stage an FHS tree (shared by every format) -------------------------------------------------
install -Dm755 "$BIN/wordcraft" "$STAGE/usr/bin/wordcraft"
install -Dm755 "$BIN/wordcraft-cli" "$STAGE/usr/bin/wordcraft-cli"
"${CROSS_COMPILE:-}strip" "$STAGE/usr/bin/wordcraft" "$STAGE/usr/bin/wordcraft-cli" 2>/dev/null || true
install -Dm644 "$HERE/$APP_ID.desktop" "$STAGE/usr/share/applications/$APP_ID.desktop"
install -Dm644 "$HERE/$APP_ID.mime.xml" "$STAGE/usr/share/mime/packages/$APP_ID.xml"
mkdir -p "$STAGE/usr/share/metainfo"
sed -e "s/@VERSION@/$VERSION/g" -e "s/@DATE@/$WORDCRAFT_BUILD_DATE/g" \
  "$HERE/$APP_ID.metainfo.xml.in" >"$STAGE/usr/share/metainfo/$APP_ID.metainfo.xml"
mkdir -p "$STAGE/usr/share/icons"
cp -R "$ROOT/assets/app-icon/hicolor" "$STAGE/usr/share/icons/"
mkdir -p "$STAGE/usr/share/doc/wordcraft"
copy_docs "$STAGE/usr/share/doc/wordcraft"

# Extra fonts: a build input like CRAFT_FONTS_DIR, never committed. The .ttf and .otf files under
# $WORDCRAFT_EXTRA_FONTS_DIR go to usr/share/wordcraft/fonts, which the app scans beside its binary
# (`bundled_font_dirs` in the fonts crate), and each OFL.txt travels as OFL-<folder>.txt.
stage_extra_fonts() {
  local src="${WORDCRAFT_EXTRA_FONTS_DIR:-}" dest="$STAGE/usr/share/wordcraft/fonts" f n=0
  [[ -n "$src" ]] || return 0
  [[ -d "$src" ]] || { echo "error: WORDCRAFT_EXTRA_FONTS_DIR=$src is not a folder" >&2; return 1; }
  mkdir -p "$dest"
  while IFS= read -r -d '' f; do
    cp "$f" "$dest/"
    n=$((n + 1))
  done < <(find "$src" -type f \( -iname '*.ttf' -o -iname '*.otf' \) -print0)
  if ((n == 0)); then
    echo "error: no .ttf or .otf files in $src" >&2
    return 1
  fi
  while IFS= read -r -d '' f; do
    cp "$f" "$STAGE/usr/share/doc/wordcraft/OFL-$(basename "$(dirname "$f")").txt"
  done < <(find "$src" -type f -name 'OFL.txt' -print0)
  echo "staged $n extra font files from $src"
}
stage_extra_fonts

if command -v desktop-file-validate >/dev/null; then
  desktop-file-validate "$STAGE/usr/share/applications/$APP_ID.desktop"
fi
if command -v appstreamcli >/dev/null; then
  appstreamcli validate --no-net --explain "$STAGE/usr/share/metainfo/$APP_ID.metainfo.xml"
fi

has() { local item="$1"; case " $FORMATS " in *" $item "*) return 0 ;; *) return 1 ;; esac; }

# ---- .tar.gz ------------------------------------------------------------------------------------
if has tar; then
  mkdir -p "$WORK/tar"
  cp -R "$STAGE/usr" "$WORK/tar/$BASENAME"
  tar -C "$WORK/tar" -czf "$DIST/$BASENAME.tar.gz" "$BASENAME"
  echo "wrote $DIST/$BASENAME.tar.gz"
fi

# ---- .deb / .rpm --------------------------------------------------------------------------------
if has deb || has rpm; then
  command -v nfpm >/dev/null || { echo "error: nfpm not found (https://nfpm.goreleaser.com/install/)" >&2; exit 1; }
  export VERSION
  export NFPM_ARCH="$DEB_ARCH"
  # nfpm expands env vars in fields like `version` and `arch`, but not in `contents[].src`.
  sed "s|\${STAGE}|$STAGE|g" "$HERE/nfpm.yaml" >"$WORK/nfpm.yaml"
  for fmt in deb rpm; do
    if has "$fmt"; then (cd "$ROOT" && nfpm package -f "$WORK/nfpm.yaml" -p "$fmt" -t "$DIST/$BASENAME.$fmt"); fi
  done
fi

# ---- AppImage -----------------------------------------------------------------------------------
if has appimage; then
  APPIMAGETOOL_VERSION=1.9.1
  # appimagetool would otherwise embed type2-runtime's `continuous` build, unchecked.
  RUNTIME_VERSION=20251108
  case "$ARCH" in
    x86_64)
      APPIMAGETOOL_SHA256=ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0
      RUNTIME_SHA256=2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d
      ;;
    aarch64)
      APPIMAGETOOL_SHA256=f0837e7448a0c1e4e650a93bb3e85802546e60654ef287576f46c71c126a9158
      RUNTIME_SHA256=00cbdfcf917cc6c0ff6d3347d59e0ca1f7f45a6df1a428a0d6d8a78664d87444
      ;;
    *) APPIMAGETOOL_SHA256='' RUNTIME_SHA256='' ;;
  esac
  if [[ -z "$RUNTIME_SHA256" ]]; then
    echo "no pinned AppImage runtime for $ARCH" >&2
    exit 2
  fi
  # Downloads $1 to $3 once and checks it against SHA-256 $2, the cached copy again on every run.
  fetch_pinned() {
    local url="$1" expected_sha="$2" target="$3"
    if [[ ! -x "$target" ]]; then
      curl --proto '=https' -fsSL -o "$target.download" "$url"
      printf '%s  %s\n' "$expected_sha" "$target.download" | sha256sum -c -
      chmod 755 "$target.download"
      mv "$target.download" "$target"
    fi
    printf '%s  %s\n' "$expected_sha" "$target" | sha256sum -c -
    return 0
  }
  APPDIR="$WORK/WordCraft.AppDir"
  cp -R "$STAGE" "$APPDIR"
  if [[ -n "${WORDCRAFT_EDITION_REGION:-}" ]]; then
    # An edition (packaging/hebrew) starts with its region's paper size and units, which the app
    # otherwise takes from the system: an en_US session would start Letter pages. A value the person
    # already set in the environment wins. AppRun is a script, not a link, so it applies to every
    # way of starting the AppImage.
    cat >"$APPDIR/AppRun" <<APPRUN
#!/bin/sh
HERE="\$(dirname "\$(readlink -f "\$0")")"
: "\${WORDCRAFT_REGION:=$WORDCRAFT_EDITION_REGION}"
export WORDCRAFT_REGION
exec "\$HERE/usr/bin/wordcraft" "\$@"
APPRUN
    chmod 755 "$APPDIR/AppRun"
  else
    ln -s usr/bin/wordcraft "$APPDIR/AppRun"
  fi
  cp "$HERE/$APP_ID.desktop" "$APPDIR/$APP_ID.desktop"
  cp "$ROOT/assets/app-icon/hicolor/256x256/apps/$APP_ID.png" "$APPDIR/$APP_ID.png"
  ln -s "$APP_ID.png" "$APPDIR/.DirIcon"

  TOOL="${APPIMAGETOOL:-$(command -v appimagetool || true)}"
  if [[ -z "$TOOL" ]]; then
    if [[ -z "$APPIMAGETOOL_SHA256" ]]; then
      echo "no pinned appimagetool for $ARCH; set APPIMAGETOOL or put appimagetool on PATH" >&2
      exit 2
    fi
    TOOL="$CARGO_TARGET_DIR/appimagetool-$APPIMAGETOOL_VERSION-$ARCH.AppImage"
    fetch_pinned "https://github.com/AppImage/appimagetool/releases/download/$APPIMAGETOOL_VERSION/appimagetool-$ARCH.AppImage" \
      "$APPIMAGETOOL_SHA256" "$TOOL"
  fi
  RUNTIME="$CARGO_TARGET_DIR/appimage-runtime-$RUNTIME_VERSION-$ARCH"
  fetch_pinned "https://github.com/AppImage/type2-runtime/releases/download/$RUNTIME_VERSION/runtime-$ARCH" \
    "$RUNTIME_SHA256" "$RUNTIME"
  # Absolute, because appimagetool runs in $DIST below (CARGO_TARGET_DIR or APPIMAGETOOL may be
  # relative, e.g. target/agent-<name>).
  OUT="$(cd "$DIST" && pwd)/$BASENAME.AppImage"
  APPDIR="$(cd "$APPDIR" && pwd)"
  TOOL="$(cd "$(dirname "$TOOL")" && pwd)/$(basename "$TOOL")"
  RUNTIME="$(cd "$(dirname "$RUNTIME")" && pwd)/$(basename "$RUNTIME")"
  # A .zsync left by an earlier run would hide a missing zsyncmake and describe another file.
  rm -f "$OUT.zsync"
  # Update information: AppImageUpdate, AppImageLauncher and the like read it from the file and
  # fetch only the blocks that changed in a newer release, through the .zsync published next to
  # each AppImage on GitHub Releases. `latest` is the newest published release that is not a
  # pre-release. A fork's builds point at its own releases through GITHUB_REPOSITORY.
  REPO="${GITHUB_REPOSITORY:-storytold/wordcraft}"
  UPDATE_INFO="gh-releases-zsync|${REPO%%/*}|${REPO#*/}|latest|wordcraft-*-linux-$ARCH.AppImage.zsync"
  # Extract-and-run: works without FUSE (containers, CI). The output embeds the pinned static
  # runtime, so users don't need libfuse2 either. With zsyncmake on the host (CI installs the zsync
  # package) appimagetool also writes the .zsync, into its working directory, hence the cd.
  # An edition that is not updated from the upstream releases (WORDCRAFT_NO_UPDATE_INFO=1) carries
  # no update information: it would point AppImageUpdate at releases in another language.
  UPDATE_ARGS=(-u "$UPDATE_INFO")
  [[ -z "${WORDCRAFT_NO_UPDATE_INFO:-}" ]] || UPDATE_ARGS=()
  (cd "$DIST" && ARCH="$ARCH" APPIMAGE_EXTRACT_AND_RUN=1 "$TOOL" --no-appstream \
    --runtime-file "$RUNTIME" "${UPDATE_ARGS[@]}" "$APPDIR" "$OUT")
  echo "wrote $OUT"
  if [[ -s "$OUT.zsync" ]]; then
    echo "wrote $OUT.zsync"
  else
    warn "zsyncmake not found, so $OUT.zsync was not written; AppImage delta updates need it"
  fi
fi

if [[ -z "${CROSS_TARGET:-}" ]]; then
  "$STAGE/usr/bin/wordcraft-cli" --version
elif [[ -n "${EMULATOR:-}" ]]; then
  "$EMULATOR" "$STAGE/usr/bin/wordcraft-cli" --version
fi
echo "==> done"
ls -lh "$DIST"
