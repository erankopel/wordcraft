//! Where the Android app keeps its files, and the recovery copy of unsaved work.
//!
//! Everything lives under one folder the system gives the app (`AndroidApp::internal_data_path`,
//! `/data/user/0/<package>/files`): preferences (`ui.json`), the custom dictionary, fonts the user
//! added, logs, and `recovery/unsaved.docx`. Nothing here needs a storage permission, and
//! uninstalling the app removes it all.
//!
//! Android ends a backgrounded app when it needs the memory, without calling the app first, so a
//! document with unsaved changes would be lost for good. [`Recovery`] decides when to keep a copy
//! of it on disk (shortly after edits, and at once when the app loses focus), and the app offers
//! it back on the next start. The planner is plain code with tests that run on any host.

use std::io::Write;
use std::path::{Path, PathBuf};

/// How often an edited document's recovery copy is refreshed while the app stays in front.
pub const RECOVERY_INTERVAL_MS: f64 = 10_000.0;
/// A recovery copy or preferences file bigger than this is ignored (hostile or corrupt).
pub const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;
/// The recovery copy's file name; the extension picks the format it is written in.
pub const RECOVERY_FILE: &str = "unsaved.docx";

/// The app's folders, derived from the one the system gave it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dirs {
    root: PathBuf,
}

impl Dirs {
    /// `files` is the app's private folder; everything goes in `wordcraft` inside it, the name the
    /// settings folder has on every platform (`wordcraft_control_key::settings_dir`).
    pub fn new(files: &Path) -> Dirs {
        Dirs { root: files.join("wordcraft") }
    }

    /// The settings folder.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// `ui.json`, which the desktop app keeps in the same place.
    pub fn prefs(&self) -> PathBuf {
        self.root.join("ui.json")
    }

    /// Fonts the user added.
    pub fn fonts(&self) -> PathBuf {
        self.root.join("fonts")
    }

    /// The custom dictionary.
    pub fn dictionary(&self, file_name: &str) -> PathBuf {
        self.root.join(file_name)
    }

    /// The log files.
    pub fn logs(&self) -> PathBuf {
        self.root.join("logs")
    }

    /// The recovery copy of unsaved work.
    pub fn recovery(&self) -> PathBuf {
        self.root.join("recovery").join(RECOVERY_FILE)
    }
}

/// Read a file, or `None` when it is missing, unreadable or bigger than `max` bytes.
pub fn read_limited(path: &Path, max: u64) -> Option<Vec<u8>> {
    let len = std::fs::metadata(path).ok()?.len();
    if len > max {
        return None;
    }
    std::fs::read(path).ok()
}

/// Write `bytes` to `path` so a reader (or a crash, or the system ending the app) never sees half
/// of them: to a sibling file first, then renamed over `path`. Creates the folder.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "no folder"))?;
    std::fs::create_dir_all(dir)?;
    let mut name = path.file_name().map(std::ffi::OsString::from).unwrap_or_default();
    name.push(".tmp");
    let tmp = dir.join(name);
    let mut file = std::fs::File::create(&tmp)?;
    let written = file.write_all(bytes).and_then(|()| file.sync_all());
    drop(file);
    if let Err(e) = written.and_then(|()| std::fs::rename(&tmp, path)) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

/// Delete a file; one that is already gone is fine.
pub fn remove_quietly(path: &Path) {
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => log::warn!("can't remove {}: {e}", path.display()),
    }
}

/// What to do about the recovery copy after a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Plan {
    Nothing,
    /// Write the copy now, then tell [`Recovery::wrote`].
    Write,
    /// The document has no unsaved changes: delete the copy.
    Clear,
}

/// Keeps a recovery copy of unsaved work current without writing on every frame.
#[derive(Debug, Default)]
pub struct Recovery {
    /// The (document, revision) the copy on disk reflects; `None` while there is no copy.
    on_disk: Option<(u64, u64)>,
    last_write_ms: f64,
}

impl Recovery {
    /// A copy is on disk from an earlier run (it was just restored): clear it once the document
    /// is saved or discarded.
    pub fn restored(&mut self) {
        self.on_disk = Some((u64::MAX, u64::MAX));
    }

    /// What to do now. `stamp` is the document's identity and revision, `leaving` is true when the
    /// app has just lost focus (the last chance before the system may end it), `now_ms` any clock
    /// that counts milliseconds.
    pub fn plan(&mut self, dirty: bool, stamp: (u64, u64), now_ms: f64, leaving: bool) -> Plan {
        if !dirty {
            return if self.on_disk.take().is_some() { Plan::Clear } else { Plan::Nothing };
        }
        if self.on_disk == Some(stamp) {
            return Plan::Nothing;
        }
        if leaving || now_ms - self.last_write_ms >= RECOVERY_INTERVAL_MS {
            Plan::Write
        } else {
            Plan::Nothing
        }
    }

    /// The copy for `stamp` is on disk.
    pub fn wrote(&mut self, stamp: (u64, u64), now_ms: f64) {
        self.on_disk = Some(stamp);
        self.last_write_ms = now_ms;
    }

    /// The copy couldn't be written: try again after the interval, not on the next frame.
    pub fn failed(&mut self, now_ms: f64) {
        self.last_write_ms = now_ms;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn temp_dir(tag: &str) -> PathBuf {
        static N: AtomicUsize = AtomicUsize::new(0);
        let d = std::env::temp_dir().join(format!("wordcraft-android-storage-{tag}-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn every_file_lives_under_one_wordcraft_folder() {
        let d = Dirs::new(Path::new("/data/user/0/ai.storyteller.wordcraft/files"));
        assert_eq!(d.root(), Path::new("/data/user/0/ai.storyteller.wordcraft/files/wordcraft"));
        for p in [d.prefs(), d.fonts(), d.dictionary("custom-dictionary.txt"), d.logs(), d.recovery()] {
            assert!(p.starts_with(d.root()), "{}", p.display());
        }
        assert_eq!(d.recovery().file_name().and_then(|n| n.to_str()), Some(RECOVERY_FILE));
    }

    #[test]
    fn atomic_writes_replace_the_file_and_leave_no_temporary() {
        let dir = temp_dir("atomic");
        let path = dir.join("nested").join("ui.json");
        write_atomic(&path, b"one").unwrap();
        write_atomic(&path, b"two").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"two");
        let names: Vec<_> = std::fs::read_dir(dir.join("nested")).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(names, vec![std::ffi::OsString::from("ui.json")]);
        // A path with no parent is an error, not a panic.
        assert!(write_atomic(Path::new(""), b"x").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_write_keeps_the_old_file() {
        let dir = temp_dir("keeps");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ui.json");
        write_atomic(&path, b"old").unwrap();
        // The temporary's name is taken by a folder, so the write fails before the rename.
        std::fs::create_dir_all(dir.join("ui.json.tmp")).unwrap();
        assert!(write_atomic(&path, b"new").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"old");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reads_are_capped_and_missing_files_are_none() {
        let dir = temp_dir("read");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a");
        std::fs::write(&path, b"12345").unwrap();
        assert_eq!(read_limited(&path, 5), Some(b"12345".to_vec()));
        assert_eq!(read_limited(&path, 4), None);
        assert_eq!(read_limited(&dir.join("missing"), 5), None);
        remove_quietly(&path);
        remove_quietly(&path);
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unsaved_edits_are_copied_after_a_pause_and_at_once_when_the_app_is_left() {
        let mut r = Recovery::default();
        // Nothing to keep while the document is saved.
        assert_eq!(r.plan(false, (1, 0), 0.0, false), Plan::Nothing);
        // The first edit after a long idle time is copied; edits right after it wait.
        assert_eq!(r.plan(true, (1, 1), 30_000.0, false), Plan::Write);
        r.wrote((1, 1), 30_000.0);
        assert_eq!(r.plan(true, (1, 1), 31_000.0, false), Plan::Nothing, "already on disk");
        assert_eq!(r.plan(true, (1, 2), 31_000.0, false), Plan::Nothing, "too soon");
        assert_eq!(r.plan(true, (1, 2), 31_000.0, true), Plan::Write, "leaving the app doesn't wait");
        r.wrote((1, 2), 31_000.0);
        assert_eq!(r.plan(true, (1, 3), 31_000.0 + RECOVERY_INTERVAL_MS, false), Plan::Write, "the interval passed");
        // A write that failed isn't retried on the next frame.
        r.failed(41_000.0);
        assert_eq!(r.plan(true, (1, 3), 41_001.0, false), Plan::Nothing);
        assert_eq!(r.plan(true, (1, 3), 41_000.0 + RECOVERY_INTERVAL_MS, false), Plan::Write);
        r.wrote((1, 3), 51_000.0);
        // Saving (or discarding) the document removes the copy once.
        assert_eq!(r.plan(false, (1, 3), 52_000.0, false), Plan::Clear);
        assert_eq!(r.plan(false, (1, 3), 53_000.0, false), Plan::Nothing);
    }

    #[test]
    fn a_restored_copy_is_cleared_when_the_document_is_saved_or_discarded() {
        let mut r = Recovery::default();
        r.restored();
        // Still unsaved and unchanged since the restore: the copy on disk is the older state, so it
        // is refreshed on the next edit (the stamp differs), never skipped.
        assert_eq!(r.plan(true, (7, 0), 5_000.0, true), Plan::Write);
        r.wrote((7, 0), 5_000.0);
        assert_eq!(r.plan(false, (7, 0), 6_000.0, false), Plan::Clear);
        // Restored, then saved before any edit.
        let mut r = Recovery::default();
        r.restored();
        assert_eq!(r.plan(false, (7, 0), 1.0, false), Plan::Clear);
    }
}
