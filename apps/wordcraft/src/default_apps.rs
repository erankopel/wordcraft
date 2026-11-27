//! File › Options › General › Make WordCraft the default for Word documents (#295).
//!
//! The installers register WordCraft for Word, RTF and ODT files without taking over the
//! defaults (`packaging/windows/wordcraft.wxs`, the `.desktop` file's `MimeType=`, `Info.plist`).
//! This is the button that makes it the default when the user asks:
//! - **Windows** doesn't let apps set defaults: it opens Settings › Default apps on WordCraft's
//!   page (`ms-settings:defaultapps?registeredAppMachine=WordCraft`; Windows 10 ignores the query
//!   and shows the Default apps list), where the user picks WordCraft per type.
//! - **Linux and BSD**: `xdg-mime default <app id>.desktop <types>` for [`WORD_MIME_TYPES`],
//!   which writes the user's `mimeapps.list`. Only when the desktop entry is installed (deb, rpm,
//!   Flatpak; an edition's AppImage after `--install`; a bare AppImage has none), and not from
//!   inside the Flatpak sandbox, which can't change the host's defaults. The app id is
//!   [`crate::APP_ID`]: an edition's own, so its button never makes a stock install the default.
//! - **macOS** has no API for it here: the Options page explains Finder's Get Info › Change All.
//!
//! [`WORD_MIME_TYPES`]: self::WORD_MIME_TYPES

use wordcraft_ui_egui::DefaultApp;

/// The `Services::make_default_app` hook.
pub type Hook = Box<dyn Fn() -> Result<DefaultApp, String>>;

/// The hook for this platform; `None` where there is no button (macOS).
pub fn hook() -> Option<Hook> {
    #[cfg(target_os = "windows")]
    let hook: Option<Hook> = Some(Box::new(windows::open_default_apps));
    #[cfg(all(unix, not(target_os = "macos")))]
    let hook: Option<Hook> = Some(Box::new(xdg::make_default));
    #[cfg(not(any(target_os = "windows", all(unix, not(target_os = "macos")))))]
    let hook: Option<Hook> = None;
    hook
}

/// The desktop entry the Linux packages install (`<app id>.desktop`; the Flatpak's app id).
#[cfg(any(all(unix, not(target_os = "macos")), test))]
fn desktop_id() -> String {
    format!("{}.desktop", crate::APP_ID)
}

/// The types the button makes WordCraft the default for on Linux and BSD: Word documents and
/// templates (also macro-enabled and legacy), RTF and ODT. The same set the Windows installer
/// lists under WordCraft's Capabilities; Markdown is only offered under Open With, and plain
/// text only on Windows and macOS (the `.desktop` file leaves it out, #4137).
/// Every one is in the `.desktop` file's `MimeType=` (a test checks).
#[cfg(any(all(unix, not(target_os = "macos")), test))]
const WORD_MIME_TYPES: &[&str] = &[
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document", // .docx
    "application/vnd.ms-word.document.macroEnabled.12",                        // .docm
    "application/vnd.openxmlformats-officedocument.wordprocessingml.template", // .dotx
    "application/vnd.ms-word.template.macroEnabled.12",                        // .dotm
    "application/msword",                                                      // .doc
    "application/msword-template",                                             // .dot
    "application/rtf",                                                         // .rtf
    "text/rtf",                                                                // .rtf (older name)
    "application/vnd.oasis.opendocument.text",                                 // .odt
];

/// The arguments for `xdg-mime` that make `desktop_id` the default for [`WORD_MIME_TYPES`]: one
/// call sets them all.
#[cfg(any(all(unix, not(target_os = "macos")), test))]
fn xdg_mime_args(desktop_id: &str) -> Vec<String> {
    ["default", desktop_id].into_iter().chain(WORD_MIME_TYPES.iter().copied()).map(str::to_string).collect()
}

/// The `applications` folders the desktop looks for entries in, from `XDG_DATA_HOME` (default
/// `~/.local/share`) and `XDG_DATA_DIRS` (default `/usr/local/share:/usr/share`). Relative paths
/// are ignored, as the XDG Base Directory spec says.
#[cfg(all(unix, any(not(target_os = "macos"), test)))]
fn application_dirs(data_home: Option<&str>, data_dirs: Option<&str>, home: Option<&str>) -> Vec<std::path::PathBuf> {
    use std::path::{Path, PathBuf};
    let absolute = |s: &&str| Path::new(s).is_absolute();
    let home_dir = data_home
        .filter(|s| !s.is_empty())
        .filter(absolute)
        .map(PathBuf::from)
        .or_else(|| home.filter(|s| !s.is_empty()).filter(absolute).map(|h| Path::new(h).join(".local/share")));
    let system = data_dirs.filter(|s| !s.is_empty()).unwrap_or("/usr/local/share:/usr/share");
    home_dir
        .into_iter()
        .chain(system.split(':').filter(|s| !s.is_empty()).filter(absolute).map(PathBuf::from))
        .map(|d| d.join("applications"))
        .collect()
}

#[cfg(target_os = "windows")]
mod windows {
    use wordcraft_ui_egui::DefaultApp;

    /// Settings › Default apps, on the page of the app registered as `WordCraft` under
    /// `HKLM\Software\RegisteredApplications` (the MSI writes it).
    const SETTINGS_URI: &str = "ms-settings:defaultapps?registeredAppMachine=WordCraft";

    pub(super) fn open_default_apps() -> Result<DefaultApp, String> {
        open::that_detached(SETTINGS_URI).map_err(|e| format!("couldn't open Settings: {e}"))?;
        Ok(DefaultApp::SettingsOpened)
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod xdg {
    use std::process::Command;

    use wordcraft_ui_egui::DefaultApp;

    use super::{application_dirs, desktop_id, xdg_mime_args};

    /// The longest error text from `xdg-mime` shown in the status bar.
    const MAX_ERROR: usize = 200;

    pub(super) fn make_default() -> Result<DefaultApp, String> {
        if std::env::var_os("FLATPAK_ID").is_some() || std::path::Path::new("/.flatpak-info").exists() {
            return Err(
                "the Flatpak sandbox can't change default apps; choose WordCraft under Open With in your file manager or system settings".into()
            );
        }
        let var = |k: &str| std::env::var(k).ok();
        let dirs = application_dirs(var("XDG_DATA_HOME").as_deref(), var("XDG_DATA_DIRS").as_deref(), var("HOME").as_deref());
        let id = desktop_id();
        if !dirs.iter().any(|d| d.join(&id).is_file()) {
            // An edition's AppImage adds its own entry (its AppRun's `--install`); `APPIMAGE` is this
            // copy's own here, `appimage_env` having dropped another program's.
            return Err(if wordcraft_ui_egui::i18n::EDITION_LANGUAGE.is_some() && std::env::var_os("APPIMAGE").is_some() {
                "WordCraft isn't in the applications menu yet. Start the AppImage once with --install to add it, then try again.".into()
            } else {
                format!("{id} isn't installed (an AppImage needs desktop integration first)")
            });
        }
        let out = Command::new("xdg-mime").args(xdg_mime_args(&id)).output().map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => "xdg-mime isn't installed (it comes with xdg-utils)".to_string(),
            _ => format!("couldn't run xdg-mime: {e}"),
        })?;
        if out.status.success() {
            return Ok(DefaultApp::MadeDefault);
        }
        let stderr = String::from_utf8_lossy(&out.stderr);
        let first = stderr.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
        let message: String = first.chars().take(MAX_ERROR).collect();
        Err(if message.is_empty() { format!("xdg-mime failed ({})", out.status) } else { format!("xdg-mime failed: {message}") })
    }
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::path::PathBuf;

    use super::*;

    /// The stock desktop entry and the Hebrew edition's (packaging/hebrew), which its AppImage's
    /// `--install` puts in the menu.
    const ENTRIES: [(&str, &str); 2] = [
        ("ai.storyteller.wordcraft", include_str!("../../../packaging/linux/ai.storyteller.wordcraft.desktop")),
        ("ai.storyteller.wordcraft.hebrew", include_str!("../../../packaging/hebrew/ai.storyteller.wordcraft.hebrew.desktop")),
    ];

    /// The value of `key` in a desktop entry.
    fn value<'a>(entry: &'a str, key: &str) -> Option<&'a str> {
        entry.lines().find_map(|l| l.strip_prefix(key)?.strip_prefix('='))
    }

    #[test]
    fn xdg_mime_sets_every_word_type_in_one_call() {
        let args = xdg_mime_args(&desktop_id());
        assert_eq!(args.first().map(String::as_str), Some("default"));
        assert_eq!(args.get(1), Some(&desktop_id()), "the menu entry of this build's app id");
        assert_eq!(&args[2..], WORD_MIME_TYPES);
        for t in [
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            "application/msword",
            "application/rtf",
            "application/vnd.oasis.opendocument.text",
        ] {
            assert!(args.iter().any(|a| a == t), "{t}");
        }
        assert!(!args.iter().any(|a| a == "text/plain" || a == "text/markdown"), "plain text and Markdown are never claimed");
    }

    /// Every type the button claims is one the desktop entry declares, or desktops ignore it. The
    /// edition's AppRun (`--install`) puts its own path in place of `wordcraft`.
    #[test]
    fn the_desktop_entry_declares_every_claimed_type() {
        for (id, entry) in ENTRIES {
            let declared: Vec<&str> = value(entry, "MimeType").map(|v| v.split(';').filter(|s| !s.is_empty()).collect()).unwrap_or_default();
            for t in WORD_MIME_TYPES {
                assert!(declared.contains(t), "{id}: {t} missing from MimeType=");
            }
            assert_eq!(value(entry, "Exec"), Some("wordcraft %F"), "{id}: files are passed on the command line");
        }
    }

    /// Declaring `text/plain` makes WordCraft a handler for every config, script and log on systems with no default set.
    #[test]
    fn the_desktop_entry_does_not_declare_plain_text() {
        for (id, entry) in ENTRIES {
            let declared: Vec<&str> = value(entry, "MimeType").map(|v| v.split(';').filter(|s| !s.is_empty()).collect()).unwrap_or_default();
            assert!(!declared.is_empty(), "{id}: MimeType= line not found");
            assert!(!declared.contains(&"text/plain"), "{id}: text/plain must not be declared");
        }
    }

    /// A desktop matches a window to its entry by the app id, and finds the icon by it: an
    /// entry whose icon or window class were another's would show the other program's icon, or
    /// group the windows of a stock install and the edition together.
    #[test]
    fn each_desktop_entry_names_its_own_icon_and_window_class() {
        for (id, entry) in ENTRIES {
            assert_eq!(value(entry, "Icon"), Some(id), "{id}");
            assert_eq!(value(entry, "StartupWMClass"), Some(id), "{id}");
        }
        assert!(ENTRIES.iter().any(|(id, _)| *id == crate::APP_ID), "this build's app id {} has a desktop entry", crate::APP_ID);
    }

    #[cfg(unix)]
    #[test]
    fn application_dirs_follow_the_xdg_spec() {
        let p = |s: &str| PathBuf::from(s);
        assert_eq!(
            application_dirs(None, None, Some("/home/a")),
            [p("/home/a/.local/share/applications"), p("/usr/local/share/applications"), p("/usr/share/applications")]
        );
        assert_eq!(
            application_dirs(Some("/data"), Some("/opt/share::relative:/usr/share"), Some("/home/a")),
            [p("/data/applications"), p("/opt/share/applications"), p("/usr/share/applications")]
        );
        // Empty or relative values fall back to the defaults; no home at all is fine.
        assert_eq!(application_dirs(Some(""), Some(""), None), [p("/usr/local/share/applications"), p("/usr/share/applications")]);
        assert_eq!(application_dirs(Some("rel"), None, Some("also-rel")), [p("/usr/local/share/applications"), p("/usr/share/applications")]);
    }
}
