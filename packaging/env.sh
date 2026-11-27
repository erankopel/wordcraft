# shellcheck shell=bash
# Shared setup for the packaging scripts. Source it: `. "$(dirname "$0")/../env.sh"`.
#
# Exports:
#   ROOT                    workspace root
#   VERSION                 [workspace.package] version from Cargo.toml (override: WORDCRAFT_VERSION)
#   DIST                    output directory for release artifacts (default: $ROOT/dist/release)
#   WORDCRAFT_BUILD_SHA    git commit baked into the binaries (see crates/engine/src/build_info.rs)
#   WORDCRAFT_BUILD_DATE   UTC build date, YYYY-MM-DD
#   CARGO_TARGET_DIR        cargo's target dir (default: $ROOT/target)

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export ROOT

# The version lives in exactly one place: `[workspace.package] version` in the root Cargo.toml.
# (`cargo xtask version` prints the same thing; awk avoids compiling xtask here.)
workspace_version() {
  awk '
    /^\[/ { in_pkg = ($0 == "[workspace.package]") ; next }
    in_pkg && $1 == "version" { gsub(/[" ]/, "", $3); print $3; exit }
  ' "$ROOT/Cargo.toml"
  return $?
}

VERSION="${WORDCRAFT_VERSION:-$(workspace_version)}"
if [[ -z "$VERSION" ]]; then
  echo "error: could not read [workspace.package] version from $ROOT/Cargo.toml" >&2
  exit 1
fi
export VERSION

DIST="${DIST:-$ROOT/dist/release}"
mkdir -p "$DIST"
export DIST

if [[ -z "${WORDCRAFT_BUILD_SHA:-}" ]]; then
  WORDCRAFT_BUILD_SHA="$(git -C "$ROOT" rev-parse HEAD 2>/dev/null || true)"
fi
export WORDCRAFT_BUILD_SHA
export WORDCRAFT_BUILD_DATE="${WORDCRAFT_BUILD_DATE:-$(date -u +%Y-%m-%d)}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"

# Emit a GitHub Actions warning (plain stderr outside Actions).
warn() {
  if [[ -n "${GITHUB_ACTIONS:-}" ]]; then echo "::warning::$*"; else echo "warning: $*" >&2; fi
  return 0
}

# Copy licence and readme files that exist into a package directory, with the licences of every
# font the programs embed.
copy_docs() {
  local dest="$1" f
  for f in README.md LICENSE LICENSE-MIT LICENSE-APACHE COPYRIGHT NOTICE ATTRIBUTION.md; do
    if [[ -f "$ROOT/$f" ]]; then cp "$ROOT/$f" "$dest/"; fi
  done
  # Inter, Source Sans 3, Source Serif 4 and JetBrains Mono (assets/fonts, NOTICE).
  for f in "$ROOT"/assets/fonts/OFL-*.txt; do
    if [[ -f "$f" ]]; then cp "$f" "$dest/"; fi
  done
  copy_font_licences "$dest"
  copy_egui_font_licences "$dest"
  return 0
}

# egui's default fonts, which the desktop and web apps embed (eframe's `default_fonts`): Ubuntu
# Light (Ubuntu Font Licence), Hack (MIT and the Bitstream Vera licence), Noto Emoji (OFL), the
# emoji icon font (MIT) and egui's icons. Their licences come from the epaint_default_fonts crate
# the build used, as epaint_default_fonts-<file>.txt.
copy_egui_font_licences() {
  local dest="$1" manifest f
  manifest="$(cargo metadata --manifest-path "$ROOT/Cargo.toml" --format-version 1 --locked --offline 2>/dev/null |
    grep -o '"manifest_path":"[^"]*/epaint_default_fonts/Cargo.toml"' | head -n 1 | cut -d'"' -f4)" || manifest=""
  if [[ -z "$manifest" ]]; then
    warn "epaint_default_fonts not found with cargo metadata, so the licences of egui's fonts are not in the package"
    return 0
  fi
  for f in "$(dirname "$manifest")"/fonts/*.txt; do
    if [[ -f "$f" ]]; then cp "$f" "$dest/epaint_default_fonts-$(basename "$f")"; fi
  done
  return 0
}

# Builds made with craft-fonts (CRAFT_FONTS_DIR, set by the release workflow) embed its fonts, so
# the package carries their licences as OFL-<family-dir>.txt. Nothing without CRAFT_FONTS_DIR.
copy_font_licences() {
  local dest="$1" f
  [[ -n "${CRAFT_FONTS_DIR:-}" ]] || return 0
  for f in "$CRAFT_FONTS_DIR"/fonts/*/OFL.txt; do
    if [[ -f "$f" ]]; then cp "$f" "$dest/OFL-$(basename "$(dirname "$f")").txt"; fi
  done
}

# Portable SHA-256 of a file (prints just the hash).
sha256() {
  local file="$1"
  if command -v sha256sum >/dev/null; then sha256sum "$file" | cut -d' ' -f1; else shasum -a 256 "$file" | cut -d' ' -f1; fi
  return $?
}
