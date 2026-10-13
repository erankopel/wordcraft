//! `log::` records on Android: logcat and a log file in the app's folder on shared storage.
//!
//! Without a logger every `log::warn!` in the workspace (an unreadable part of a .docx, a font a
//! PDF export had to leave out) vanishes, and an Android app has no terminal. Records go to logcat
//! (`adb logcat -s WordCraft`) and, once [`attach_dirs`] has run, to `wordcraft.log` in the first
//! folder it can write: `Documents/WordCraft/logs` on shared storage (when All files access is
//! granted, so every file manager can open and share it), else `Android/data/<package>/files/logs`
//! (which newer Android versions hide from the file manager), else the private settings folder's
//! `logs`. Each start moves
//! the previous log to `wordcraft.1.log`, so the log of a run that was killed survives the next
//! launch. Levels follow the desktop app: `info` for WordCraft's own crates, `warn` for the rest
//! (wgpu and naga are chatty).
//!
//! Writing never panics: a file that can't be created or written leaves logcat as the only sink.
//! The pieces that need no Android (the file sink, the filter, the panic text) are plain code with
//! tests that run on any host.

use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::{Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use log::LevelFilter;

/// The current log file's name inside the log directory.
pub const LOG_FILE: &str = "wordcraft.log";
/// The previous run's log.
pub const PREVIOUS_LOG: &str = "wordcraft.1.log";
/// The log file stops growing past this size (a runaway warning can't fill the storage).
pub const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;

/// The level that applies to a record's target: `info` for WordCraft's crates, `warn` otherwise.
pub fn level_for(target: &str) -> LevelFilter {
    if target.starts_with("wordcraft") { LevelFilter::Info } else { LevelFilter::Warn }
}

/// The log file: appends lines until it reaches [`MAX_FILE_BYTES`], then drops them.
pub struct FileSink {
    file: Mutex<Option<(File, u64)>>,
}

impl FileSink {
    /// Start a new log in `dir` (created if needed), keeping the last run's as `wordcraft.1.log`.
    pub fn open(dir: &Path) -> std::io::Result<FileSink> {
        std::fs::create_dir_all(dir)?;
        let current = dir.join(LOG_FILE);
        if current.exists() {
            // A failed rename leaves the old log to be overwritten below; not worth stopping for.
            let _ = std::fs::rename(&current, dir.join(PREVIOUS_LOG));
        }
        let file = File::create(current)?;
        Ok(FileSink { file: Mutex::new(Some((file, 0))) })
    }

    /// Append one line; a write error or a full file turns the sink off.
    pub fn write_line(&self, line: &str) {
        let mut guard = self.file.lock().unwrap_or_else(PoisonError::into_inner);
        let Some((file, written)) = guard.as_mut() else { return };
        let bytes = line.len() as u64 + 1;
        if *written + bytes > MAX_FILE_BYTES {
            *guard = None;
            return;
        }
        if writeln!(file, "{line}").is_err() {
            *guard = None;
        } else {
            *written += bytes;
        }
    }
}

/// One record as a log file line: seconds since the epoch, level, target, message.
pub fn format_record(record: &log::Record) -> String {
    let since = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    format!("{}.{:03} {:<5} {}: {}", since.as_secs(), since.subsec_millis(), record.level(), record.target(), record.args())
}

/// Replace text a panic message quotes between backticks with `…`: std quotes the string it was
/// slicing (`byte index 5 is not a char boundary; … of `<the string>``), and that string can be
/// the user's writing. An unpaired backtick leaves the rest of the message out too.
pub fn redact_quoted(message: &str) -> String {
    let mut out = String::with_capacity(message.len());
    for (i, part) in message.split('`').enumerate() {
        if i % 2 == 1 {
            out.push_str("`…");
        } else {
            if i > 0 {
                out.push('`');
            }
            out.push_str(part);
        }
    }
    out
}

/// The text the panic hook logs: where it happened, the redacted message and, when captured, the
/// backtrace.
pub fn panic_report(location: Option<&str>, message: Option<&str>, backtrace: Option<&str>) -> String {
    let message = message.map_or_else(|| "(no message)".to_owned(), redact_quoted);
    let mut report = format!("panic at {}: {message}", location.unwrap_or("an unknown place"));
    if let Some(trace) = backtrace {
        report.push('\n');
        report.push_str(trace.trim_end());
    }
    report
}

#[cfg(target_os = "android")]
mod android {
    use std::path::Path;
    use std::sync::OnceLock;

    use log::{LevelFilter, Log, Metadata, Record};

    use super::{FileSink, format_record, level_for, panic_report};

    /// logcat plus the file.
    struct Tee {
        android: android_logger::AndroidLogger,
        file: OnceLock<FileSink>,
    }

    impl Log for Tee {
        fn enabled(&self, metadata: &Metadata) -> bool {
            metadata.level() <= level_for(metadata.target())
        }

        fn log(&self, record: &Record) {
            if !self.enabled(record.metadata()) {
                return;
            }
            self.android.log(record);
            if let Some(file) = self.file.get() {
                file.write_line(&format_record(record));
            }
        }

        fn flush(&self) {}
    }

    static LOGGER: OnceLock<Tee> = OnceLock::new();

    /// Install the logger and the panic hook; a second call does nothing.
    pub fn init() {
        let config = android_logger::Config::default().with_max_level(LevelFilter::Info).with_tag("WordCraft");
        let logger = LOGGER.get_or_init(|| Tee { android: android_logger::AndroidLogger::new(config), file: OnceLock::new() });
        if log::set_logger(logger).is_ok() {
            log::set_max_level(LevelFilter::Info);
            install_panic_hook();
        }
    }

    /// Also write the log to the first of `dirs` that can be written, once the settings folder is
    /// known. The first attach wins: opening a second sink would rotate the log the first writes.
    pub fn attach_dirs(dirs: &[&Path]) {
        let Some(logger) = LOGGER.get() else { return };
        if logger.file.get().is_some() {
            return;
        }
        for dir in dirs {
            match FileSink::open(dir) {
                Ok(sink) => {
                    let _ = logger.file.set(sink);
                    log::info!("log file in {}", dir.display());
                    return;
                }
                Err(e) => log::info!("no log file in {}: {e}", dir.display()),
            }
        }
    }

    /// Report every panic (caught by `Session::run`'s guard or not) through the logger.
    fn install_panic_hook() {
        std::panic::set_hook(Box::new(|info| {
            let location = info.location().map(ToString::to_string);
            let trace = std::backtrace::Backtrace::force_capture().to_string();
            log::error!("{}", panic_report(location.as_deref(), info.payload_as_str(), Some(&trace)));
        }));
    }
}

#[cfg(target_os = "android")]
pub use android::{attach_dirs, init};

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        static N: AtomicUsize = AtomicUsize::new(0);
        let d = std::env::temp_dir().join(format!("wordcraft-android-log-{tag}-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn wordcraft_crates_log_info_and_the_rest_warnings() {
        assert_eq!(level_for("wordcraft_docx::package"), LevelFilter::Info);
        assert_eq!(level_for("wordcraft_android::app"), LevelFilter::Info);
        assert_eq!(level_for("wgpu_core::device"), LevelFilter::Warn);
        assert_eq!(level_for("naga"), LevelFilter::Warn);
    }

    #[test]
    fn a_panic_message_never_carries_the_text_it_quotes() {
        assert_eq!(
            redact_quoted("byte index 5 is not a char boundary; it is inside 'é' of `Dear Anna, my secret`"),
            "byte index 5 is not a char boundary; it is inside 'é' of `…`"
        );
        assert_eq!(redact_quoted("no quotes"), "no quotes");
        // An unpaired backtick leaves the rest out.
        assert_eq!(redact_quoted("oops `private text"), "oops `…");
        let report = panic_report(Some("src/x.rs:1:2"), Some("bad `private`"), Some("frame 0\nframe 1\n"));
        assert_eq!(report, "panic at src/x.rs:1:2: bad `…`\nframe 0\nframe 1");
        assert_eq!(panic_report(None, None, None), "panic at an unknown place: (no message)");
    }

    #[test]
    fn each_start_keeps_the_last_log_and_the_file_stops_at_its_cap() {
        let dir = temp_dir("rotate");
        let first = FileSink::open(&dir).unwrap();
        first.write_line("run one");
        drop(first);
        let second = FileSink::open(&dir).unwrap();
        second.write_line("run two");
        assert_eq!(std::fs::read_to_string(dir.join(PREVIOUS_LOG)).unwrap(), "run one\n");
        assert_eq!(std::fs::read_to_string(dir.join(LOG_FILE)).unwrap(), "run two\n");
        // A line that would pass the cap is dropped, and so is everything after it.
        let big = "x".repeat(MAX_FILE_BYTES as usize);
        second.write_line(&big);
        second.write_line("after");
        assert_eq!(std::fs::read_to_string(dir.join(LOG_FILE)).unwrap(), "run two\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_log_directory_that_cannot_be_made_is_an_error_not_a_panic() {
        let dir = temp_dir("blocked");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("not-a-dir");
        std::fs::write(&file, b"x").unwrap();
        assert!(FileSink::open(&file.join("logs")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn records_are_one_line_with_level_and_target() {
        let line = format_record(&log::Record::builder().level(log::Level::Warn).target("wordcraft_docx").args(format_args!("bad part")).build());
        assert!(line.ends_with("WARN  wordcraft_docx: bad part"), "{line}");
        assert!(!line.contains('\n'));
    }
}
