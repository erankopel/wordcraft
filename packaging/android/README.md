# WordCraft for Android (sideload)

An unofficial Android build of WordCraft for your own device, tested on nothing yet: it has been
compiled, linked, packaged and checked statically (see Status), but not run on a phone, a tablet or
an emulator, because none was available where it was built. It targets a Samsung Galaxy Z Fold7 on
Android 17: arm64-v8a only, minSdk 35, targetSdk 36, compileSdk 37. It is not on Google Play and is
not part of the GitHub releases.

The app is the same editor as the desktop one (`wordcraft-ui-egui` over `wordcraft-engine`, eframe
with wgpu on Vulkan). The Android part is the crate [`apps/wordcraft-android`](../../apps/wordcraft-android),
loaded by Google's GameActivity. This folder holds only what turns that library into an APK: a Gradle
project, the manifest, a launcher icon and one small Java class (`MainActivity`, which turns the
window insets, the system bars and the keyboard into padding; everything else is Rust).

## Build the APK

```sh
packaging/android/package.sh --install-sdk
```

writes `dist/release/wordcraft-<version>-android-arm64.apk` and its `.sha256`. It needs a JDK 17 or
newer, `rustup` (the script adds the `aarch64-linux-android` target), `git`, `curl` and `unzip`, and
the Android SDK in `ANDROID_HOME` (default `~/Android/Sdk`) with platform `android-37.0`, build tools
`36.1.0` and NDK `29.0.14206865`. `--install-sdk` installs the SDK pieces that are missing with
`sdkmanager` from the Android command line tools, which must already be in
`ANDROID_HOME/cmdline-tools/latest` (<https://developer.android.com/studio#command-line-tools-only>);
that accepts the Android SDK licence agreements for you, so read them first
(<https://developer.android.com/studio/terms>). Since version 23 of those tools, `sdkmanager` is a
shim that downloads Google's Android CLI on first use, which collects usage data unless you turn it
off with that CLI's own `--no-metrics` flag; if that matters to you, install the SDK pieces by hand
and run the script without `--install-sdk`. The script also installs `cargo-ndk` and downloads a
pinned Gradle (checked against its published SHA-256). It checks the JDK and the signing key first,
so a mistake there shows at once rather than after the build. A first build compiles the whole app
for arm64 in release mode and takes a while (about twenty minutes on four cores).

`--skip-native` reuses the library of the last run, to repackage after a change to this folder only
(or to retry after a signing error); it warns when that library was built from another commit or a
changed tree. Setting `CRAFT_FONTS_DIR` embeds the optional craft-fonts, as for the desktop builds
(the APK then grows by tens of megabytes, not measured here, and the fonts' licence texts are not
added to it; the system already has CJK, Arabic, Thai and Hebrew fonts). The APK also contains
Google's Apache-2.0 licensed AndroidX and GameActivity libraries: keep that in mind before sharing
it beyond your own devices.

### The signing key

Android installs an update only over an app signed with the same key. The first run creates a key in
`~/.config/wordcraft/android/` (`wordcraft-release.jks` and `keystore.pass`, readable by you only)
and signs with it from then on. **Back that folder up.** With another key, the new APK refuses to
install over the old one, and you would have to uninstall WordCraft first, which removes its
settings and the recovery copy (your documents in shared storage stay). Use your own key with
`WORDCRAFT_KEYSTORE` (an absolute path), `WORDCRAFT_KEYSTORE_PASSWORD` and optionally
`WORDCRAFT_KEY_ALIAS` (default `wordcraft`). The key is never part of the repository, and only
Gradle gets its password (the Rust build, which runs other people's build scripts, does not).
After each build the certificate's SHA-256 is printed: compare it with the one of the APK you
installed to know that an update will go over it.

The version code is the number of minutes since 2026-01-01 at build time, so each build is newer than
the one before it. The version name is the workspace version.

### Build on GitHub

The workflow `.github/workflows/android.yml` (named Android, run from the Actions tab) builds the same
APK and attaches it to the run as an artifact. Without secrets it signs with a key created for that
run, so each run's APK has a different signature and can't update the last one: save the key you want
to keep as the repository secrets `ANDROID_KEYSTORE_BASE64` (the `.jks` file, base64 encoded) and
`ANDROID_KEYSTORE_PASSWORD`, and the workflow uses it. Anyone who can run workflows on a branch of
the repository can make a run that reads those secrets, so keep a key you care about out of a
repository other people can write to; the artifact is visible to anyone who can read it. This
workflow has not been run yet, so the first dispatch may need fixing.

## Install on the Fold

1. Copy the APK to the phone (USB, cloud drive, or `adb install -r wordcraft-<version>-android-arm64.apk`).
2. Open it from a file manager. Android asks to allow that app to install unknown apps: allow it
   (Settings, Apps, Special app access, Install unknown apps). Google Play Protect may say it has not
   seen the app: choose More details, then Install anyway. The app is signed by your own key, which
   Play Protect can't know.
   Samsung's Auto Blocker (Settings, Security and privacy) blocks installs from outside the stores
   and USB commands: if the install is refused, or adb is ignored, switch Auto Blocker off for the
   install and on again afterwards.
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

- **Scroll** with one finger (or two); the page coasts after you let go, and touching it stops it
  (without moving the caret). **Tap** to place the caret (the page gets the press when the finger
  lifts, so a swipe never edits a picture or a table it starts on), **double tap** to select a word
  (then drag to select by words), **rest a finger** a moment to give the page its press and about
  half a second in all to select the word and open the context menu (it has Select All; a picture
  or table border held that way can be dragged), **pinch** to zoom (two fingers moving together
  scroll). A finger on the scroll bar at the page's right edge drags the bar. Hyperlinks can't be followed (that is
  Ctrl+click); a Bluetooth mouse or DeX pointer counts as a finger, so click-dragging scrolls and
  the wheel does nothing.
- In a narrow window (the Fold's outer screen) the ribbon's tabs scroll sideways, Share, Editing and
  Comments fold into the menu at the end of the tab strip, and dialogs scroll inside the window.
- The **keyboard button** at the right end of the status bar brings the on-screen keyboard back
  after you dismissed it. Tapping the page shows it by itself. The keyboard is told the text
  before the caret, so Backspace, capital letters and suggestions follow the document; words reach
  the editor with their spaces and punctuation separate, so AutoCorrect, smart quotes and lists
  work as when typing on a desktop. A swipe on the space bar restarts the keyboard's buffer at the
  caret instead of moving it. In a dialog's text field the keyboard gets blank filler instead of
  the document's text (the first letter may come capitalised). A hardware keyboard types, with the
  limits under Not there yet.
- The interface starts at 125 percent on a screen 600 dp wide or more (the unfolded Fold) and 100
  percent on a narrower one (the folded cover screen); File, Options, General changes it, and the
  choice is kept.
- **Files**: Open and Save use the built-in file browser, which starts in the device's shared
  storage (`/storage/emulated/0`).
- **Your work**: while a document has unsaved changes, WordCraft writes a recovery copy a moment
  after you stop typing (and at the latest ten seconds into continuous typing), because Android can
  end a background app without asking. The next start opens it again, marked unsaved. Save to keep
  it. File, Close after saving or discarding removes it; a copy that can't be opened is set aside as
  `unreadable.docx` in the app's private folder instead of being deleted (it can't be reached from
  outside the app, but a later version may be able to read it).
- **Closing**: File, Close quits the app, and its process ends with it, so the next launch starts
  from nothing (the native side can run only once per process). The system's Back gesture either
  sends the app to the background or finishes it, which ends the process the same way; which of
  the two happens on Android 17 is not known. Either way, unsaved work comes back from the recovery copy, which is at
  most 1.5 seconds old while you pause and at most 10 seconds while you type without a break. Back
  does not close dialogs or menus: the app has no Back handling yet.

## Not there yet

A system clipboard bridge (copy and paste work inside WordCraft only), printing, opening documents
from other apps, the system file picker, a TalkBack bridge, Read Aloud, Zotero, hunspell
dictionaries for Polish and Portuguese (English spelling works), and finger sized controls (the
ribbon, menus and dialogs keep their desktop sizes, and there are no selection handles).

Hardware keyboards type, and the arrow, Home, End and Page keys work, but shortcuts that use Ctrl
or Shift (Ctrl+B, Ctrl+C, Shift+arrows) do not: winit's Android backend never reports modifier
keys. Use the ribbon and the context menu. Fixing it means carrying a small patch to winit.
While the on-screen keyboard is up, Android keeps the arrow, Home and End keys for the keyboard's
own buffer, so with a hardware keyboard the page does not raise the on-screen one by itself (the
keyboard button and a tap on the page do).

## Logs

`adb logcat -s WordCraft` shows the log. The same lines go to `wordcraft.log` (the previous run's is
`wordcraft.1.log`) in `Documents/WordCraft/logs/` once All files access is granted, so you can open
and share it with any file manager; before that, and if that folder can't be written, in
`Android/data/ai.storyteller.wordcraft/files/logs/` (newer Android versions keep the system file
manager out of that folder, and on a computer `adb pull` still reaches it) or the private folder.
Document text is never logged. A failure before the log starts (the library not loading, the
window not opening) is not in it: from a computer, `adb logcat -b crash` and `adb logcat -d '*:W'`
show those, and are what to attach to a bug report.

## Status

What was checked, and how (nothing was run on a device):

- `libwordcraft_android.so` links for `aarch64-linux-android` with NDK r29; its load segments are
  16 KB aligned (`llvm-readelf -l`), and it needs only `liblog`, `libdl`, `libandroid`, `libm` and
  `libc`.
- The APK passes `zipalign -c -P 16`, `apksigner verify` and `aapt2 dump badging` (package, SDK
  levels, native code arm64-v8a).
- The pure parts of the Android crate (logging, the recovery copy and its restore, the keyboard
  bridge's text diffing against a model of Android's text buffer) and the touch handling (a gate
  that holds a finger's press until it is a tap, a scroll or a hold, with frame by frame tests of a
  swipe over a picture, a table border, a coasting page, two fingers and a long press) have tests
  that run on a desktop.

Everything else is unverified until it has run on the device: the GPU start, typing with the
on-screen keyboard, the file browser over shared storage, folding and unfolding, and long documents
on the roughly 1 MiB stack Android gives the app thread (the engine and UI test suites pass with that
stack size, so it is a caution, not a known problem). If something fails, the log above is what to look at.
