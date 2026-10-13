//! WordCraft for Android: the native library the APK loads.
//!
//! The Android build is a GameActivity app (`packaging/android`): the activity's Java side starts
//! a thread that calls `android_main` below, and from there it is the same editor as on the
//! desktop: `wordcraft-ui-egui` on eframe, over the engine. What differs is what the system
//! provides, and each piece of that lives in its own module:
//!
//! - [`logging`]: `log::` records go to logcat (`adb logcat -s WordCraft`) and a log file.
//! - [`ime`] and `keyboard`: typing on the on-screen keyboard (winit drops its text, so the
//!   keyboard's state is read and turned into the editor's own input events).
//! - [`storage`]: preferences, the custom dictionary and added fonts in the app's private
//!   folder, and a recovery copy of unsaved work (Android ends a backgrounded app without asking).
//! - `host`: the [`Services`](wordcraft_ui_egui::Services) an Android host offers (the built-in
//!   file browser stands in for the system's file dialog; no printing, no system clipboard yet).
//! - `app`: the eframe app around [`WordApp`](wordcraft_ui_egui::WordApp).
//!
//! Only the entry point and `app` and `host` need Android; `logging` and `storage` are plain code
//! with tests that run anywhere, and the rest is empty on other targets, so `cargo check
//! --workspace` works on a desktop.
//!
//! # Unsafe code
//!
//! The workspace forbids `unsafe_code`, and this is the only crate that doesn't inherit that (see
//! its `Cargo.toml`): the system finds an Android app's entry point by the exported symbol
//! `android_main`, and exporting a symbol is an unsafe attribute. That single item is allowed
//! below; the crate denies unsafe code everywhere else.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

#[cfg(target_os = "android")]
mod app;
#[cfg(target_os = "android")]
mod host;
pub mod ime;
#[cfg(target_os = "android")]
mod keyboard;
pub mod logging;
pub mod storage;

/// The entry point `android-activity` calls on its own thread when the activity starts (and again
/// if the system destroys and recreates the activity). The name and the unmangled symbol are the
/// contract with the glue; nothing else here is exported.
#[cfg(target_os = "android")]
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
fn android_main(android: android_activity::AndroidApp) {
    logging::init();
    log::info!("WordCraft {} starting", env!("CARGO_PKG_VERSION"));
    if let Err(e) = app::run(android) {
        // The activity thread ends here; the log says why (a window that can't be created, say).
        log::error!("WordCraft stopped: {e}");
    }
}
