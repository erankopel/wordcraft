//! The [`Services`] an Android host offers the editor.
//!
//! Left out on purpose, each one a hook the UI hides or works without when it is `None`:
//! printing (`printers`, `print_native`: Android prints through its own service), the system
//! clipboard's pictures, formatted text and format list (egui keeps text copied inside the app),
//! and Make WordCraft the default app. The animation setting is not read, so effects stay on.

use std::sync::mpsc::{Receiver, channel};

use wordcraft_ui_egui::{FileDialogAnswer, FileDialogRequest, Services};

/// The services of the Android build.
pub fn services() -> Services {
    Services { file_dialog: Some(Box::new(no_system_dialog)), ..Default::default() }
}

/// Android's own file picker (the Storage Access Framework) is reached through Java, which this
/// build doesn't call yet, so every request is answered at once with an error and the app shows its
/// built-in file browser over the device's shared storage.
fn no_system_dialog(_request: FileDialogRequest) -> Receiver<FileDialogAnswer> {
    let (answer, rx) = channel();
    // The receiver is held by the app; if it is gone there is nobody to tell.
    let _ = answer.send(Err("Android's file picker isn't used by this build".to_owned()));
    rx
}
