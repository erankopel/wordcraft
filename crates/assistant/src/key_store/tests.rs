use std::sync::atomic::{AtomicU32, Ordering};

use super::*;

const KEY: &str = "sk-ant-test-0123456789";

/// A folder of its own for one test, removed afterwards.
struct Folder(PathBuf);

impl Folder {
    fn new() -> Folder {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!("wordcraft-assistant-key-test-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::SeqCst)));
        fs::create_dir_all(&path).unwrap();
        Folder(path)
    }

    fn file(&self) -> KeyFile {
        KeyFile::in_folder(&self.0)
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_saved_key_comes_back_and_nothing_saved_is_none() {
    let folder = Folder::new();
    let file = folder.file();
    assert_eq!(file.load(), Ok(None), "nothing saved");
    file.save(&format!("  {KEY} \n")).unwrap();
    assert_eq!(file.load(), Ok(Some(KEY.to_owned())), "without the blanks around it");
    file.save("sk-ant-another-key").unwrap();
    assert_eq!(file.load(), Ok(Some("sk-ant-another-key".to_owned())), "a new key replaces the old");
    assert!(!folder.0.join("assistant-key.tmp").exists(), "no temporary file is left behind");
}

#[test]
fn forgetting_removes_the_key_and_forgetting_nothing_is_fine() {
    let folder = Folder::new();
    let file = folder.file();
    assert_eq!(file.forget(), Ok(()));
    file.save(KEY).unwrap();
    assert_eq!(file.forget(), Ok(()));
    assert_eq!(file.load(), Ok(None));
    assert!(!file.path().exists());
}

/// A leftover temporary copy from a save that died is removed with the key, and a removal that fails says so.
#[test]
fn forgetting_also_removes_a_leftover_copy_and_reports_a_failed_removal() {
    let folder = Folder::new();
    let file = folder.file();
    let leftover = folder.0.join("assistant-key.tmp");
    fs::write(&leftover, KEY).unwrap();
    file.save(KEY).unwrap();
    assert_eq!(file.forget(), Ok(()));
    assert!(!leftover.exists() && !file.path().exists(), "neither copy stays");
    // A path that cannot be removed (here a folder with something in it) is an error, not a success.
    fs::create_dir_all(file.path().join("inside")).unwrap();
    assert!(matches!(file.forget(), Err(KeyFileError::Io(_))), "the key file could not be removed");
}

/// A blank or odd key is refused, and a key that was saved stays.
#[test]
fn a_key_that_is_not_a_key_is_refused_and_the_old_one_stays() {
    let folder = Folder::new();
    let file = folder.file();
    file.save(KEY).unwrap();
    for bad in ["", "   \n", "two words", "line\nbreak", "tab\there", "bell\u{7}", &"k".repeat(MAX_KEY_BYTES + 1)] {
        assert_eq!(file.save(bad), Err(KeyFileError::NotAKey), "{bad:?}");
    }
    assert_eq!(file.load(), Ok(Some(KEY.to_owned())));
}

/// What is in the file is not trusted: long, binary or multi-line content is not a key.
#[test]
fn a_file_that_is_not_a_key_is_not_used() {
    let folder = Folder::new();
    let file = folder.file();
    for content in [vec![b'k'; MAX_KEY_BYTES + 100], vec![0xff, 0xfe, 0x00, 0x01], b"first\nsecond".to_vec(), b"   \n".to_vec()] {
        fs::write(file.path(), &content).unwrap();
        #[cfg(unix)]
        fs::set_permissions(file.path(), std::os::unix::fs::PermissionsExt::from_mode(0o600)).unwrap();
        assert_eq!(file.load(), Err(KeyFileError::NotAKey), "{content:?}");
    }
}

/// The file is the owner's alone, and a file that others can read is not used.
#[cfg(unix)]
#[test]
fn the_file_is_private_and_a_file_others_can_read_is_not_used() {
    use std::os::unix::fs::PermissionsExt;
    let folder = Folder::new();
    let file = folder.file();
    file.save(KEY).unwrap();
    let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(file.path()) & 0o077, 0, "only its owner can read or write it: {:o}", mode(file.path()));
    fs::set_permissions(file.path(), fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(file.load(), Err(KeyFileError::TooOpen), "readable by others: not used");
    assert_eq!(file.load().map(|k| k.is_none()), Err(KeyFileError::TooOpen));
    // Saving again makes it private again, whatever it was.
    file.save(KEY).unwrap();
    assert_eq!(file.load(), Ok(Some(KEY.to_owned())));
    assert_eq!(mode(file.path()) & 0o077, 0);
}

#[test]
fn errors_never_hold_the_key() {
    let folder = Folder::new();
    // A folder that is a file: saving fails, and the key is in no message.
    let blocker = folder.0.join("blocker");
    fs::write(&blocker, b"x").unwrap();
    let file = KeyFile::in_folder(&blocker);
    let error = file.save(KEY).unwrap_err();
    for text in [format!("{error}"), format!("{error:?}")] {
        assert!(!text.contains(KEY) && !text.contains("0123456789"), "the key leaked into: {text}");
    }
}
