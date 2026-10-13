//! Where the Android app keeps its files, and the recovery copy of unsaved work.
//!
//! Everything lives under one folder the system gives the app (`AndroidApp::internal_data_path`,
//! `/data/user/0/<package>/files`): preferences (`ui.json`), the custom dictionary, fonts the user
//! added, logs, and `recovery/unsaved.docx`. Nothing here needs a storage permission, and
//! uninstalling the app removes it all.
//!
//! Android ends a backgrounded app when it needs the memory, without calling the app first, so a
//! document with unsaved changes would be lost for good. [`Recovery`] decides when to keep a copy
//! of it on disk (a moment after the last edit, and at the latest [`RECOVERY_INTERVAL_MS`] into
//! continuous editing), and the app offers it back on the next start through [`restore_with`].
//! Nothing here waits for the app to be told it is leaving: a paused app draws no frames, so the
//! copy is written while the user is still typing. The planner and the restore protocol are plain
//! code with tests that run on any host.

use std::io::Write;
use std::path::{Path, PathBuf};

/// The most an edited document goes without a recovery copy while the user keeps typing.
pub const RECOVERY_INTERVAL_MS: f64 = 10_000.0;
/// A pause in typing this long writes the recovery copy.
pub const RECOVERY_IDLE_MS: f64 = 1_500.0;
/// After a failed write, the wait before the next try.
pub const RECOVERY_RETRY_MS: f64 = 10_000.0;
/// A recovery copy or preferences file bigger than this is ignored (hostile or corrupt).
pub const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;
/// The recovery copy's file name; the extension picks the format it is written in.
pub const RECOVERY_FILE: &str = "unsaved.docx";
/// While a recovery copy is being opened it is called this, so a start that dies in the middle
/// finds the file in the same place and doesn't try it again.
const RESTORING_FILE: &str = "unsaved.docx.restoring";
/// A recovery copy that couldn't be opened is kept under this name, never deleted.
pub const UNREADABLE_FILE: &str = "unreadable.docx";

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

    /// Where a recovery copy that couldn't be opened is kept.
    pub fn unreadable(&self) -> PathBuf {
        self.root.join("recovery").join(UNREADABLE_FILE)
    }

    fn restoring(&self) -> PathBuf {
        self.root.join("recovery").join(RESTORING_FILE)
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
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Plan {
    Nothing,
    /// A copy is due in this many milliseconds: make sure a frame runs then.
    Wait(f64),
    /// Write the copy now, then tell [`Recovery::wrote`] (or [`Recovery::failed`]).
    Write,
    /// The document has no unsaved changes: delete the copy.
    Clear,
}

/// Keeps a recovery copy of unsaved work current without writing on every frame: a moment after
/// the last edit, and at the latest [`RECOVERY_INTERVAL_MS`] after the first edit the copy lacks.
#[derive(Debug, Default)]
pub struct Recovery {
    /// The (document, revision) the copy on disk reflects; `None` while there is no copy.
    on_disk: Option<(u64, u64)>,
    /// The newest (document, revision) a frame saw, and when it first did.
    seen: Option<(u64, u64)>,
    changed_ms: f64,
    /// When the first edit the copy on disk lacks was seen.
    pending_since_ms: Option<f64>,
    /// No write before this, after one failed.
    retry_at_ms: f64,
}

impl Recovery {
    /// A copy is on disk from an earlier run and `stamp` is the document it was opened into: it
    /// stays until the document is saved or discarded, and is refreshed once that is edited.
    pub fn restored(&mut self, stamp: (u64, u64)) {
        self.on_disk = Some(stamp);
    }

    /// What to do now. `stamp` is the document's identity and revision, `leaving` is true when
    /// the app has just lost focus (a bonus chance: the system may end it next), `now_ms` any
    /// clock that counts milliseconds and never goes backwards.
    pub fn plan(&mut self, dirty: bool, stamp: (u64, u64), now_ms: f64, leaving: bool) -> Plan {
        if !dirty {
            self.seen = None;
            self.pending_since_ms = None;
            return if self.on_disk.take().is_some() { Plan::Clear } else { Plan::Nothing };
        }
        if self.on_disk == Some(stamp) {
            self.seen = Some(stamp);
            self.pending_since_ms = None;
            return Plan::Nothing;
        }
        if self.seen != Some(stamp) {
            self.seen = Some(stamp);
            self.changed_ms = now_ms;
            self.pending_since_ms.get_or_insert(now_ms);
        }
        if leaving {
            return Plan::Write;
        }
        let first = self.pending_since_ms.unwrap_or(now_ms);
        let due = (self.changed_ms + RECOVERY_IDLE_MS).min(first + RECOVERY_INTERVAL_MS).max(self.retry_at_ms);
        if now_ms >= due { Plan::Write } else { Plan::Wait(due - now_ms) }
    }

    /// The copy for `stamp` is on disk.
    pub fn wrote(&mut self, stamp: (u64, u64)) {
        self.on_disk = Some(stamp);
        self.pending_since_ms = None;
        self.retry_at_ms = 0.0;
    }

    /// The copy couldn't be written: try again after [`RECOVERY_RETRY_MS`], not on the next frame.
    pub fn failed(&mut self, now_ms: f64) {
        self.retry_at_ms = now_ms + RECOVERY_RETRY_MS;
    }
}

/// Run `f`, turning a panic into an error: the document writer and reader see user content, and a
/// panic in them must cost one failed recovery copy, not the app (the panic hook has logged it).
pub fn guarded<T>(f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_or_else(|_| Err("internal error (see the log)".to_owned()))
}

/// What became of the recovery copy at start.
#[derive(Debug, PartialEq, Eq)]
pub enum Restore<T> {
    /// There was no copy to open.
    Nothing,
    /// The copy opened as `T`; it stays on disk until the document is saved or discarded.
    Opened(T),
    /// A copy exists but couldn't be opened: it was moved here, not deleted.
    Unreadable(PathBuf),
}

/// Open the recovery copy a previous run left with `open`, so that it can never fail twice: the
/// file is renamed to a `.restoring` name first, which an interrupted start (a crash, the system
/// ending the app while it reads) leaves behind and the next start moves aside instead of trying
/// again. A copy that doesn't open, or whose reader panics, is kept as [`UNREADABLE_FILE`], since
/// it may be the only copy of someone's writing.
pub fn restore_with<T>(dirs: &Dirs, open: impl FnOnce(&[u8]) -> Result<T, String>) -> Restore<T> {
    let (path, restoring, unreadable) = (dirs.recovery(), dirs.restoring(), dirs.unreadable());
    let set_aside = |from: &Path| match std::fs::rename(from, &unreadable) {
        Ok(()) => Restore::Unreadable(unreadable.clone()),
        Err(e) => {
            log::warn!("can't keep the unreadable recovery copy: {e}");
            Restore::Nothing
        }
    };
    if restoring.exists() {
        log::warn!("the last start died while opening the recovery copy: keeping it aside");
        let moved = set_aside(&restoring);
        // A newer copy can't exist (nothing ran after the failed start), but if one does, it is
        // tried below.
        if !path.exists() {
            return moved;
        }
    }
    if !path.exists() {
        return Restore::Nothing;
    }
    if let Err(e) = std::fs::rename(&path, &restoring) {
        log::warn!("can't open the recovery copy: {e}");
        return Restore::Nothing;
    }
    let opened = match read_limited(&restoring, MAX_FILE_BYTES) {
        Some(bytes) => guarded(|| open(&bytes)),
        None => Err("unreadable or too big".to_owned()),
    };
    match opened {
        Ok(value) => match std::fs::rename(&restoring, &path) {
            Ok(()) => Restore::Opened(value),
            Err(e) => {
                // The document is open but the copy can't go back: it will be written again on the
                // next edit.
                log::warn!("can't put the recovery copy back: {e}");
                remove_quietly(&restoring);
                Restore::Opened(value)
            }
        },
        Err(e) => {
            log::warn!("the recovery copy can't be opened: {e}");
            set_aside(&restoring)
        }
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
    fn unsaved_edits_are_copied_a_moment_after_the_last_one() {
        let mut r = Recovery::default();
        // Nothing to keep while the document is saved.
        assert_eq!(r.plan(false, (1, 0), 0.0, false), Plan::Nothing);
        // An edit is copied once typing pauses: a frame is asked for at that moment, so the copy
        // doesn't wait for another input event.
        assert_eq!(r.plan(true, (1, 1), 30_000.0, false), Plan::Wait(RECOVERY_IDLE_MS));
        assert_eq!(r.plan(true, (1, 1), 30_500.0, false), Plan::Wait(RECOVERY_IDLE_MS - 500.0));
        assert_eq!(r.plan(true, (1, 1), 30_000.0 + RECOVERY_IDLE_MS, false), Plan::Write);
        r.wrote((1, 1));
        assert_eq!(r.plan(true, (1, 1), 32_000.0, false), Plan::Nothing, "already on disk");
        // Another edit starts the wait again, from that edit.
        assert_eq!(r.plan(true, (1, 2), 40_000.0, false), Plan::Wait(RECOVERY_IDLE_MS));
        assert_eq!(r.plan(true, (1, 3), 40_900.0, false), Plan::Wait(RECOVERY_IDLE_MS));
        assert_eq!(r.plan(true, (1, 3), 42_400.0, false), Plan::Write);
        r.wrote((1, 3));
        // Saving (or discarding) the document removes the copy once.
        assert_eq!(r.plan(false, (1, 3), 52_000.0, false), Plan::Clear);
        assert_eq!(r.plan(false, (1, 3), 53_000.0, false), Plan::Nothing);
    }

    #[test]
    fn continuous_typing_is_copied_at_least_every_interval() {
        let mut r = Recovery::default();
        let mut now = 0.0;
        let mut writes = Vec::new();
        // An edit every 200 ms for a minute: never idle long enough for the pause rule.
        for rev in 1..=300_u64 {
            now += 200.0;
            if r.plan(true, (1, rev), now, false) == Plan::Write {
                writes.push(now);
                r.wrote((1, rev));
            }
        }
        assert!(writes.len() >= 5, "{writes:?}");
        let gaps: Vec<f64> = writes.windows(2).map(|w| w[1] - w[0]).collect();
        assert!(gaps.iter().all(|g| *g <= RECOVERY_INTERVAL_MS + 200.0), "{gaps:?}");
    }

    #[test]
    fn leaving_the_app_writes_at_once_as_a_bonus() {
        let mut r = Recovery::default();
        assert_eq!(r.plan(true, (1, 1), 1_000.0, true), Plan::Write);
        r.wrote((1, 1));
        assert_eq!(r.plan(true, (1, 1), 1_001.0, true), Plan::Nothing, "already on disk");
    }

    #[test]
    fn a_failed_write_is_retried_later_not_on_the_next_frame() {
        let mut r = Recovery::default();
        assert_eq!(r.plan(true, (1, 1), 0.0, false), Plan::Wait(RECOVERY_IDLE_MS));
        assert_eq!(r.plan(true, (1, 1), 2_000.0, false), Plan::Write);
        r.failed(2_000.0);
        assert_eq!(r.plan(true, (1, 1), 2_016.0, false), Plan::Wait(RECOVERY_RETRY_MS - 16.0));
        assert_eq!(r.plan(true, (1, 1), 2_000.0 + RECOVERY_RETRY_MS, false), Plan::Write);
        r.wrote((1, 1));
        assert_eq!(r.plan(true, (1, 1), 20_000.0, false), Plan::Nothing);
    }

    #[test]
    fn a_restored_copy_is_cleared_when_the_document_is_saved_or_discarded() {
        let mut r = Recovery::default();
        r.restored((7, 0));
        // Unchanged since the restore: the copy on disk is this document, nothing to write.
        assert_eq!(r.plan(true, (7, 0), 5_000.0, false), Plan::Nothing);
        // Edited: refreshed after a pause.
        assert!(matches!(r.plan(true, (7, 1), 6_000.0, false), Plan::Wait(_)));
        assert_eq!(r.plan(true, (7, 1), 8_000.0, false), Plan::Write);
        r.wrote((7, 1));
        assert_eq!(r.plan(false, (7, 1), 9_000.0, false), Plan::Clear);
        // Restored, then saved before any edit.
        let mut r = Recovery::default();
        r.restored((7, 0));
        assert_eq!(r.plan(false, (7, 0), 1.0, false), Plan::Clear);
    }

    #[test]
    fn hostile_clock_values_never_panic() {
        let mut r = Recovery::default();
        for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0, 0.0, f64::MAX] {
            let _ = r.plan(true, (u64::MAX, u64::MAX), now, false);
            r.failed(now);
            let _ = r.plan(true, (0, 0), now, true);
        }
    }

    #[test]
    fn a_panic_while_writing_or_reading_becomes_an_error() {
        let r: Result<(), String> = guarded(|| {
            let v: Vec<u8> = Vec::new();
            let i = v.len() + 1;
            #[allow(clippy::indexing_slicing)]
            let _ = v[i];
            Ok(())
        });
        assert!(r.is_err());
        assert_eq!(guarded(|| Ok(5)), Ok(5));
        assert_eq!(guarded::<u8>(|| Err("no".into())), Err("no".to_owned()));
    }

    fn dirs_with_copy(tag: &str, bytes: Option<&[u8]>) -> (Dirs, PathBuf) {
        let base = temp_dir(tag);
        let dirs = Dirs::new(&base);
        if let Some(bytes) = bytes {
            write_atomic(&dirs.recovery(), bytes).unwrap();
        }
        (dirs, base)
    }

    #[test]
    fn a_good_copy_opens_and_stays_until_the_document_is_saved() {
        let (dirs, base) = dirs_with_copy("good", Some(b"hello"));
        let got = restore_with(&dirs, |b| Ok(b.to_vec()));
        assert_eq!(got, Restore::Opened(b"hello".to_vec()));
        assert_eq!(std::fs::read(dirs.recovery()).unwrap(), b"hello");
        assert!(!dirs.restoring().exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn no_copy_means_nothing_to_restore() {
        let (dirs, base) = dirs_with_copy("none", None);
        assert_eq!(restore_with(&dirs, |_| Ok(1)), Restore::Nothing);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_copy_that_does_not_open_is_kept_aside_not_deleted() {
        let (dirs, base) = dirs_with_copy("bad", Some(b"precious words"));
        let got: Restore<u8> = restore_with(&dirs, |_| Err("unsupported".into()));
        assert_eq!(got, Restore::Unreadable(dirs.unreadable()));
        assert_eq!(std::fs::read(dirs.unreadable()).unwrap(), b"precious words");
        assert!(!dirs.recovery().exists(), "not offered again at the next start");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_reader_that_panics_is_kept_aside_and_the_start_goes_on() {
        let (dirs, base) = dirs_with_copy("panic", Some(b"x"));
        let got: Restore<u8> = restore_with(&dirs, |bytes| {
            let n = bytes.len() + 5;
            #[allow(clippy::indexing_slicing)]
            Ok(bytes[n])
        });
        assert_eq!(got, Restore::Unreadable(dirs.unreadable()));
        assert!(!dirs.recovery().exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_start_that_died_while_reading_does_not_try_the_copy_again() {
        // The dead start renamed the copy and never came back: the next start finds only that.
        let (dirs, base) = dirs_with_copy("died", None);
        write_atomic(&dirs.restoring(), b"half read").unwrap();
        let mut tried = false;
        let got: Restore<u8> = restore_with(&dirs, |_| {
            tried = true;
            Ok(1)
        });
        assert!(!tried, "a copy that killed the app once is not opened again");
        assert_eq!(got, Restore::Unreadable(dirs.unreadable()));
        assert_eq!(std::fs::read(dirs.unreadable()).unwrap(), b"half read");
        assert!(!dirs.restoring().exists());
        // The start after that has nothing to do.
        assert_eq!(restore_with(&dirs, |_| Ok(1)), Restore::Nothing);
        let _ = std::fs::remove_dir_all(&base);
    }
}
