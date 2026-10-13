# WordCraft for Android (sideload)

An unofficial Android build of WordCraft for your own device, tested on nothing yet: it has been
compiled, linked, packaged and checked statically (see Status), but not run on a phone, a tablet or
an emulator, because none was available where it was built. It targets a Samsung Galaxy Z Fold7 on
Android 17: arm64-v8a only, minSdk 35, targetSdk 34, compileSdk 37. It is not on Google Play and is
not part of the GitHub releases.

The app is the same editor as the desktop one (`wordcraft-ui-egui` over `wordcraft-engine`, eframe
with wgpu on Vulkan). The Android part is the crate [`apps/wordcraft-android`](../../apps/wordcraft-android),
loaded by Google's GameActivity. This folder holds only what turns that library into an APK; there
is no Java or Kotlin of ours in it.

## Build the APK

```sh
packaging/android/package.sh --install-sdk
```

writes `dist/release/wordcraft-<version>-android-arm64.apk` and its `.sha256`. It needs a JDK 17 or
newer, `rustup` (the script adds the `aarch64-linux-android` target), `git`, `curl` and `unzip`, and
the Android SDK in `ANDROID_HOME` (default `~/Android/Sdk`) with platform `android-37.0`, build tools
`36.1.0` and NDK `29.0.14206865`. `--install-sdk` installs the SDK pieces that are missing with
`sdkmanager`; that accepts the Android SDK licence agreements for you, so read them first
(<https://developer.android.com/studio/terms>). The script also installs `cargo-ndk` and downloads a
pinned Gradle (checked against its published SHA-256). A first build compiles the whole app for
arm64 in release mode and takes a while (about twenty minutes on four cores).

`--skip-native` reuses the library of the last run, to repackage after a change to this folder only.
Setting `CRAFT_FONTS_DIR` embeds the optional craft-fonts, as for the desktop builds (the APK then
grows by about 70 MB; the system already has CJK, Arabic, Thai and Hebrew fonts).

### The signing key

Android installs an update only over an app signed with the same key. The first run creates a key in
`~/.config/wordcraft/android/` (`wordcraft-release.jks` and `keystore.pass`, readable by you only)
and signs with it from then on. **Back that folder up.** With another key, the new APK refuses to
install over the old one, and you would have to uninstall WordCraft first, which removes its
settings and the recovery copy (your documents in shared storage stay). Use your own key with
`WORDCRAFT_KEYSTORE`, `WORDCRAFT_KEYSTORE_PASSWORD` and optionally `WORDCRAFT_KEY_ALIAS` (default
`wordcraft`). The key is never part of the repository.

The version code is the number of minutes since 2026-01-01 at build time, so each build is newer than
the one before it. The version name is the workspace version.

### Build on GitHub

The workflow `.github/workflows/android.yml` ("Android", run from the Actions tab) builds the same
APK and attaches it to the run as an artifact. Without secrets it signs with a key created for that
run, so each run's APK has a different signature and can't update the last one: save the key you want
to keep as the repository secrets `ANDROID_KEYSTORE_BASE64` (the `.jks` file, base64 encoded) and
`ANDROID_KEYSTORE_PASSWORD`, and the workflow uses it. Do not publish an APK built by a public
repository with a key you care about; the artifact is visible to anyone who can read the repository.

## Install on the Fold

1. Copy the APK to the phone (USB, cloud drive, or `adb install -r wordcraft-<version>-android-arm64.apk`).
2. Open it from a file manager. Android asks to allow that app to install unknown apps: allow it
   (Settings, Apps, Special app access, Install unknown apps). Google Play Protect may say it has not
   seen the app: choose More details, then Install anyway. The app is signed by your own key, which
   Play Protect can't know.
3. Open WordCraft once, then give it access to files: Settings, Apps, WordCraft, Allow access to
   manage all files (also under Special app access, All files access). The built-in file browser
   needs it to list and save documents in shared storage. Without it the browser shows a hint and
   the app still works inside its own folder.

Android's developer verification (announced for 2026 and 2027) is meant for apps installed on
certified devices. By the October 2026 schedule on developer.android.com it applies first in Brazil,
Indonesia, Singapore and Thailand, and worldwide from 2027, with an advanced flow for installing apps
from unverified developers and free limited accounts for hobbyists and students (up to 20 devices).
If a later Android update blocks installing this APK, that is the reason: read the current pages
before changing anything.

## Using it

- **Scroll** with one finger; the page coasts after you let go. **Tap** to place the caret,
  **double tap** to select a word (then drag to select by words), **long press** for the context
  menu, **pinch** to zoom.
- The **keyboard button** at the right end of the status bar brings the on-screen keyboard back
  after you dismissed it. Tapping the page shows it by itself. A hardware keyboard works as on the
  desktop.
- The interface starts at 125 percent; File, Options, General changes it.
- **Files**: Open and Save use the built-in file browser, which starts in the device's shared
  storage (`/storage/emulated/0`).
- **Your work**: while a document has unsaved changes, WordCraft writes a recovery copy about every
  ten seconds and at once when you leave the app, because Android can end a background app without
  asking. The next start opens it again, marked unsaved. Save to keep it.

## Not there yet

A system clipboard bridge (copy and paste work inside WordCraft only), printing, opening documents
from other apps, the system file picker, a TalkBack bridge, Read Aloud, Zotero, hunspell
dictionaries for Polish and Portuguese (English spelling works), and touch sized controls (the
ribbon, menus and dialogs keep their desktop sizes).

## Logs

`adb logcat -s WordCraft` shows the log. The same lines go to
`Android/data/ai.storyteller.wordcraft/files/logs/wordcraft.log` (shared storage; the previous run's
log is `wordcraft.1.log`), which the system file manager can open, so you can attach it to a bug
report without a computer. Document text is never logged.

## Status

What was checked, and how (nothing was run on a device):

- `libwordcraft_android.so` links for `aarch64-linux-android` with NDK r29; its load segments are
  16 KB aligned (`llvm-readelf -l`), and it needs only `liblog`, `libdl`, `libandroid`, `libm` and
  `libc`.
- The APK passes `zipalign -c -P 16`, `apksigner verify` and `aapt2 dump badging` (package, SDK
  levels, native code arm64-v8a).
- The pure parts of the Android crate (logging, storage, the keyboard bridge's text diffing) and the
  touch scrolling have tests that run on a desktop.

Everything else is unverified until it has run on the device: the GPU start, typing with the
on-screen keyboard, the file browser over shared storage, folding and unfolding, and long documents
on the 2 MiB stack Android gives the app thread. If something fails, the log above is what to look at.
