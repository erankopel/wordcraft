#!/usr/bin/env bash
# Build the Android APK and sign it:  $DIST/wordcraft-<version>-android-arm64.apk
#
# Usage: packaging/android/package.sh [--install-sdk] [--skip-native]
#
#   --install-sdk   install the Android SDK pieces that are missing (platform, build tools, NDK) with
#                   sdkmanager. That accepts the Android SDK licence agreements for you: read them first
#                   (https://developer.android.com/studio/terms).
#   --skip-native   reuse the native library from the last run instead of building it again.
#
# Needs: a JDK 17 or newer (JAVA_HOME, or java on PATH), rustup with the aarch64-linux-android target
# (rustup target add aarch64-linux-android), git, curl, unzip, and the Android SDK in ANDROID_HOME
# (default ~/Android/Sdk) with platform android-37.0, build-tools 36.1.0 and NDK 29.0.14206865.
# cargo-ndk is installed with `cargo install` when it is missing.
#
# The APK is signed with a key kept outside the repository (WORDCRAFT_KEYSTORE and
# WORDCRAFT_KEYSTORE_PASSWORD, or a key this script creates in ~/.config/wordcraft/android on first
# use). Back that folder up: a build signed with another key cannot be installed over this one.
# Details and sideload steps: packaging/android/README.md.
set -euo pipefail
# shellcheck source=../env.sh
. "$(dirname "${BASH_SOURCE[0]}")/../env.sh"
HERE="$ROOT/packaging/android"

INSTALL_SDK=0
SKIP_NATIVE=0
for arg in "$@"; do
  case "$arg" in
    --install-sdk) INSTALL_SDK=1 ;;
    --skip-native) SKIP_NATIVE=1 ;;
    -h | --help) sed -n '2,20p' "${BASH_SOURCE[0]}"; exit 0 ;;
    *) echo "error: unknown option $arg" >&2; exit 2 ;;
  esac
done

# Versions this build was verified with. Changing one is a build change: re-run and check the APK.
PLATFORM="android-37.0"
BUILD_TOOLS="36.1.0"
NDK_VERSION="29.0.14206865"
MIN_API=35
CARGO_NDK_VERSION="4.1.2"
GRADLE_VERSION="9.8.1"
GRADLE_SHA256="dce76f55f8e251a3a1f130eb120f30b3d271de2b76c9b0729d316b5a1b6dc01f"

WORK="$CARGO_TARGET_DIR/android"
PROJECT="$WORK/project"
TOOLS="$WORK/tools"
mkdir -p "$WORK" "$TOOLS"

# --- Android SDK -------------------------------------------------------------------------------
ANDROID_HOME="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Android/Sdk}}"
export ANDROID_HOME ANDROID_SDK_ROOT="$ANDROID_HOME"
NDK="$ANDROID_HOME/ndk/$NDK_VERSION"
BT="$ANDROID_HOME/build-tools/$BUILD_TOOLS"

missing_sdk() {
  [ -d "$ANDROID_HOME/platforms/$PLATFORM" ] && [ -d "$BT" ] && [ -d "$NDK" ] && return 1
  return 0
}
if missing_sdk; then
  if [ "$INSTALL_SDK" = 1 ]; then
    SDKMANAGER="$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager"
    [ -x "$SDKMANAGER" ] || SDKMANAGER="$(command -v sdkmanager || true)"
    [ -n "$SDKMANAGER" ] || { echo "error: sdkmanager not found; install the Android command line tools into $ANDROID_HOME/cmdline-tools/latest" >&2; exit 1; }
    # `yes` ends with SIGPIPE when sdkmanager stops reading; that is expected.
    (yes 2>/dev/null || true) | "$SDKMANAGER" --sdk_root="$ANDROID_HOME" --licenses >/dev/null || true
    "$SDKMANAGER" --sdk_root="$ANDROID_HOME" "platforms;$PLATFORM" "build-tools;$BUILD_TOOLS" "ndk;$NDK_VERSION"
  else
    echo "error: the Android SDK in $ANDROID_HOME lacks platforms;$PLATFORM, build-tools;$BUILD_TOOLS or ndk;$NDK_VERSION." >&2
    echo "       Set ANDROID_HOME, or run again with --install-sdk (accepts the SDK licences)." >&2
    exit 1
  fi
fi
export ANDROID_NDK_HOME="$NDK"

# --- native library -----------------------------------------------------------------------------
if ! command -v cargo-ndk >/dev/null; then
  cargo install cargo-ndk --locked --version "$CARGO_NDK_VERSION"
fi
rustup target add aarch64-linux-android >/dev/null
JNI="$WORK/jniLibs"
if [ "$SKIP_NATIVE" = 0 ]; then
  rm -rf "$JNI"
  # Release profile of the workspace (opt-level 3, thin LTO). Linked 16 KB page aligned by NDK r28+.
  (cd "$ROOT" && cargo ndk --platform "$MIN_API" -t arm64-v8a -o "$JNI" build --release --locked -p wordcraft-android)
fi
[ -f "$JNI/arm64-v8a/libwordcraft_android.so" ] || { echo "error: $JNI/arm64-v8a/libwordcraft_android.so missing" >&2; exit 1; }

# --- Gradle (a pinned distribution, checked against its published checksum) -----------------------
GRADLE_ZIP="$TOOLS/gradle-$GRADLE_VERSION-bin.zip"
GRADLE="$TOOLS/gradle-$GRADLE_VERSION/bin/gradle"
if [ ! -x "$GRADLE" ]; then
  if [ ! -f "$GRADLE_ZIP" ]; then
    curl --proto '=https' --tlsv1.2 -fSL --retry 3 -o "$GRADLE_ZIP.part" "https://services.gradle.org/distributions/gradle-$GRADLE_VERSION-bin.zip"
    /bin/mv -f "$GRADLE_ZIP.part" "$GRADLE_ZIP"
  fi
  if [ "$(sha256 "$GRADLE_ZIP")" != "$GRADLE_SHA256" ]; then
    echo "error: $GRADLE_ZIP does not match the expected SHA-256 $GRADLE_SHA256" >&2
    rm -f "$GRADLE_ZIP"
    exit 1
  fi
  unzip -q -o "$GRADLE_ZIP" -d "$TOOLS"
fi

# --- signing key --------------------------------------------------------------------------------
if [ -z "${WORDCRAFT_KEYSTORE:-}" ]; then
  KEYDIR="${WORDCRAFT_ANDROID_KEY_DIR:-${XDG_CONFIG_HOME:-$HOME/.config}/wordcraft/android}"
  WORDCRAFT_KEYSTORE="$KEYDIR/wordcraft-release.jks"
  PASSFILE="$KEYDIR/keystore.pass"
  if [ ! -f "$WORDCRAFT_KEYSTORE" ]; then
    command -v keytool >/dev/null || { echo "error: keytool (part of the JDK) not found" >&2; exit 1; }
    mkdir -p "$KEYDIR"
    chmod 700 "$KEYDIR"
    (umask 077 && head -c 24 /dev/urandom | od -An -tx1 | tr -d ' \n' >"$PASSFILE")
    KEYSTORE_PASSWORD="$(cat "$PASSFILE")" keytool -genkeypair -keystore "$WORDCRAFT_KEYSTORE" -storetype PKCS12 -alias wordcraft \
      -keyalg RSA -keysize 4096 -validity 10000 -dname "CN=WordCraft personal build, O=Self" \
      -storepass:env KEYSTORE_PASSWORD -keypass:env KEYSTORE_PASSWORD
    warn "created a new signing key in $KEYDIR; back it up, because a build signed with another key can't be installed over this one"
  fi
  WORDCRAFT_KEYSTORE_PASSWORD="${WORDCRAFT_KEYSTORE_PASSWORD:-$(cat "$PASSFILE")}"
fi
[ -f "$WORDCRAFT_KEYSTORE" ] || { echo "error: signing key $WORDCRAFT_KEYSTORE not found" >&2; exit 1; }
[ -n "${WORDCRAFT_KEYSTORE_PASSWORD:-}" ] || { echo "error: set WORDCRAFT_KEYSTORE_PASSWORD for $WORDCRAFT_KEYSTORE" >&2; exit 1; }
export WORDCRAFT_KEYSTORE WORDCRAFT_KEYSTORE_PASSWORD

# --- APK ----------------------------------------------------------------------------------------
# The version code grows with the build time (minutes since 2026-01-01), so a newer build always
# installs over an older one made with the same key.
export WORDCRAFT_VERSION_NAME="$VERSION"
export WORDCRAFT_VERSION_CODE="${WORDCRAFT_VERSION_CODE:-$(($(date -u +%s) / 60 - 29453760))}"
rm -rf "$PROJECT"
mkdir -p "$PROJECT"
cp -R "$HERE/gradle/." "$PROJECT/"
mkdir -p "$PROJECT/app/src/main/jniLibs"
cp -R "$JNI/arm64-v8a" "$PROJECT/app/src/main/jniLibs/"
(cd "$PROJECT" && "$GRADLE" --no-daemon --console=plain :app:assembleRelease)

BUILT="$PROJECT/app/build/outputs/apk/release/app-release.apk"
[ -f "$BUILT" ] || { echo "error: $BUILT missing" >&2; exit 1; }
OUT="$DIST/wordcraft-$VERSION-android-arm64.apk"
cp "$BUILT" "$OUT"

# --- checks on the result -----------------------------------------------------------------------
"$BT/zipalign" -c -P 16 -v 4 "$OUT" >/dev/null || { echo "error: $OUT is not 16 KB page aligned" >&2; exit 1; }
"$BT/apksigner" verify --verbose --print-certs "$OUT" | grep -E "Verifies|Verified using|certificate SHA-256 digest"
"$BT/aapt2" dump badging "$OUT" | grep -E "^package:|^sdkVersion|^minSdkVersion|^targetSdkVersion|^native-code"
echo "$(sha256 "$OUT")  $(basename "$OUT")" >"$OUT.sha256"
echo "wrote $OUT"
ls -lh "$OUT"
