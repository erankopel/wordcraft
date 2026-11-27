//! The API key kept on this computer, when the person asks for that.
//!
//! One small file in a folder the app chooses (its settings folder). It is not encrypted, and the pane says
//! so before the person turns it on. On Unix the file is created readable and writable by its owner alone
//! (mode 0600) and a file other users can read is not used; that is as far as a program can go without the
//! system's keyring. On Windows nothing is set or checked: the file takes the access rules of its folder.
//! Nothing here prints, logs or returns the key in an error.
//!
//! The file is written to a temporary name and then moved into place, so a reader never sees half a key. A
//! file that is not a key (too long, binary) is not used either.

use std::fmt;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// The name of the file in the folder.
pub const FILE_NAME: &str = "assistant-key";
/// Most bytes a key may have, and so most this reads from the file.
pub const MAX_KEY_BYTES: usize = 512;

/// Why the file could not be read, written or removed. Never holds the key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyFileError {
    /// The key is blank, too long or has characters no key has.
    NotAKey,
    /// The file can be read by other users, so it is not used. Remove it or give it back to its owner.
    TooOpen,
    /// The file system refused.
    Io(String),
}

impl fmt::Display for KeyFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KeyFileError::NotAKey => write!(f, "that is not an API key"),
            KeyFileError::TooOpen => write!(f, "the saved key can be read by other users, so it is not used"),
            KeyFileError::Io(why) => write!(f, "the saved key could not be read or written: {why}"),
        }
    }
}

impl std::error::Error for KeyFileError {}

fn io(error: &std::io::Error) -> KeyFileError {
    KeyFileError::Io(error.kind().to_string())
}

/// Is `key` something to keep: not blank, short, one line of printable characters?
fn is_key(key: &str) -> bool {
    !key.is_empty() && key.len() <= MAX_KEY_BYTES && key.chars().all(|c| !c.is_control() && !c.is_whitespace())
}

/// The saved key's file.
#[derive(Clone, Debug)]
pub struct KeyFile {
    path: PathBuf,
}

impl KeyFile {
    /// The file in `folder`.
    pub fn in_folder(folder: &Path) -> KeyFile {
        KeyFile { path: folder.join(FILE_NAME) }
    }

    /// Where the file is, for telling the person.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The saved key: `None` when nothing is saved, an error when something is there that must not be used.
    pub fn load(&self) -> Result<Option<String>, KeyFileError> {
        let file = match fs::File::open(&self.path) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(io(&e)),
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = file.metadata().map_err(|e| io(&e))?.permissions().mode();
            if mode & 0o077 != 0 {
                return Err(KeyFileError::TooOpen);
            }
        }
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut std::io::Read::take(file, MAX_KEY_BYTES as u64 + 1), &mut bytes).map_err(|e| io(&e))?;
        let text = String::from_utf8(bytes).map_err(|_| KeyFileError::NotAKey)?;
        let key = crate::clean_key(&text);
        if is_key(&key) { Ok(Some(key)) } else { Err(KeyFileError::NotAKey) }
    }

    /// Keep `key` (without surrounding blanks), replacing what was saved. A blank or odd key is refused and
    /// the old file stays.
    pub fn save(&self, key: &str) -> Result<(), KeyFileError> {
        let key = crate::clean_key(key);
        let key = key.as_str();
        if !is_key(key) {
            return Err(KeyFileError::NotAKey);
        }
        if let Some(folder) = self.path.parent() {
            fs::create_dir_all(folder).map_err(|e| io(&e))?;
        }
        let temporary = self.path.with_extension("tmp");
        let _ = fs::remove_file(&temporary);
        let written = (|| {
            let mut options = fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
            let mut file = options.open(&temporary)?;
            file.write_all(key.as_bytes())?;
            file.sync_all()?;
            fs::rename(&temporary, &self.path)
        })();
        if let Err(e) = written {
            let _ = fs::remove_file(&temporary);
            return Err(io(&e));
        }
        Ok(())
    }

    /// Delete the saved key, and a temporary copy a crashed save may have left. Nothing saved is fine.
    pub fn forget(&self) -> Result<(), KeyFileError> {
        let _ = fs::remove_file(self.path.with_extension("tmp"));
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(io(&e)),
        }
    }
}

#[cfg(test)]
mod tests;
