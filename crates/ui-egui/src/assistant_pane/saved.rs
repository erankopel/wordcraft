//! The AI pane's choices and the person's agreement, kept between runs.
//!
//! They live in `assistant.json` in WordCraft's settings folder, beside the key file (`assistant-key`), and
//! only when the app gives the pane that folder: a run without preferences (`WORDCRAFT_NO_PREFS`) gives none,
//! so its choices and agreement last for the run. Nothing secret is in the file. A file that cannot be read,
//! or a value of the wrong kind in it, reads as the defaults: the pane then asks for the agreement again.
//!
//! Several WordCraft processes can run at once (opening a file from the file manager starts another), and
//! each reads the file when it starts. So a change is written over what the file holds when it is made, one
//! choice at a time ([`save`]): a process never writes back a choice it did not change, such as an agreement
//! another process withdrew. Before a request is sent without asking, the pane reads the agreement again
//! ([`load`]), so a withdrawal in another process holds in this one too.

#[cfg(not(target_arch = "wasm32"))]
use serde_json::{Value, json};

/// The file's name in the settings folder.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) const FILE_NAME: &str = "assistant.json";
/// Most bytes read from the file: it holds a few short values.
#[cfg(not(target_arch = "wasm32"))]
const MAX_BYTES: u64 = 64 * 1024;

/// Who the person agreed may receive the text: the request's recipient and model, as the agreement step named
/// them. A request to another recipient or model asks again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Agreed {
    pub(crate) recipient: String,
    pub(crate) model: String,
}

/// What the pane keeps between runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Choices {
    /// The assistant makes the changes itself, with WordCraft's commands, instead of answering with text.
    pub(crate) edits: bool,
    /// A text answer is about the whole document, not the selection.
    pub(crate) whole_document: bool,
    /// The person agreed that requests may go to this recipient and model; `None` until they agree.
    pub(crate) agreed: Option<Agreed>,
}

impl Default for Choices {
    fn default() -> Choices {
        Choices { edits: true, whole_document: false, agreed: None }
    }
}

/// One of the choices, for a save that writes only the one that changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Choice {
    Edits,
    WholeDocument,
    Agreement,
}

#[cfg(not(target_arch = "wasm32"))]
impl Choices {
    /// Take `which` from `from`, keeping the others.
    pub(crate) fn take(&mut self, which: Choice, from: &Choices) {
        match which {
            Choice::Edits => self.edits = from.edits,
            Choice::WholeDocument => self.whole_document = from.whole_document,
            Choice::Agreement => self.agreed = from.agreed.clone(),
        }
    }

    /// The choices in `text` (the file's JSON). A missing or odd value keeps its default.
    pub(crate) fn from_json(text: &str) -> Choices {
        let mut choices = Choices::default();
        let Ok(value) = serde_json::from_str::<Value>(text) else { return choices };
        if let Some(edits) = value.get("edits").and_then(Value::as_bool) {
            choices.edits = edits;
        }
        if let Some(whole) = value.get("wholeDocument").and_then(Value::as_bool) {
            choices.whole_document = whole;
        }
        let agreed = value.get("agreed");
        let field = |key: &str| agreed.and_then(|a| a.get(key)).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty() && s.len() <= 256);
        if let (Some(recipient), Some(model)) = (field("recipient"), field("model")) {
            choices.agreed = Some(Agreed { recipient: recipient.to_owned(), model: model.to_owned() });
        }
        choices
    }

    /// The file's JSON.
    pub(crate) fn to_json(&self) -> String {
        let agreed = self.agreed.as_ref().map(|a| json!({"recipient": a.recipient, "model": a.model}));
        let value = json!({"edits": self.edits, "wholeDocument": self.whole_document, "agreed": agreed});
        serde_json::to_string_pretty(&value).unwrap_or_default()
    }
}

/// The choices saved at `path`: `None` when nothing is saved there, the defaults when the file cannot be read.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn load(path: &std::path::Path) -> Option<Choices> {
    let file = std::fs::File::open(path).ok()?;
    let mut text = String::new();
    Some(match std::io::Read::read_to_string(&mut std::io::Read::take(file, MAX_BYTES), &mut text) {
        Ok(_) => Choices::from_json(&text),
        Err(_) => Choices::default(),
    })
}

/// Keep `which` of `choices` at `path`, over what the file holds now, whose other choices stay as they are
/// there (another process may have changed them). The file is written to a temporary name, then moved into
/// place, so a reader never sees half of it. The error is the file system's, for the pane to report.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn save(path: &std::path::Path, choices: &Choices, which: Choice) -> Result<(), String> {
    use std::io::Write;
    let mut kept = load(path).unwrap_or_default();
    kept.take(which, choices);
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder).map_err(|e| e.kind().to_string())?;
    }
    let temporary = path.with_extension("json.tmp");
    let written = (|| {
        let mut file = std::fs::File::create(&temporary)?;
        file.write_all(kept.to_json().as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&temporary, path)
    })();
    written.map_err(|e| {
        let _ = std::fs::remove_file(&temporary);
        e.kind().to_string()
    })
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    /// What is saved comes back, and a file a person or another version wrote reads value by value: what is
    /// missing or odd keeps its default, and an agreement without both names is no agreement.
    #[test]
    fn saved_choices_come_back_and_odd_values_keep_their_defaults() {
        let choices = Choices { edits: false, whole_document: true, agreed: Some(Agreed { recipient: "api.example".into(), model: "m-1".into() }) };
        assert_eq!(Choices::from_json(&choices.to_json()), choices);
        assert_eq!(Choices::from_json("not json"), Choices::default());
        let odd = Choices::from_json(r#"{"edits": "no", "wholeDocument": true, "agreed": {"recipient": "api.example", "model": 5}}"#);
        assert_eq!(odd, Choices { edits: true, whole_document: true, agreed: None });
        assert_eq!(Choices::from_json(r#"{"agreed": {"recipient": " ", "model": "m"}}"#).agreed, None);
    }

    /// A save writes the choice that changed over what the file holds, so a process that changes an option
    /// does not bring back an agreement another process withdrew, nor undo that process's options.
    #[test]
    fn a_save_writes_only_the_choice_that_changed() {
        let folder = std::env::temp_dir().join(format!("wordcraft-assistant-choices-{}", std::process::id()));
        let path = folder.join(FILE_NAME);
        let agreed = Some(Agreed { recipient: "api.example".into(), model: "m-1".into() });
        // Two processes read the same agreement.
        let first = Choices { agreed: agreed.clone(), ..Choices::default() };
        save(&path, &first, Choice::Agreement).unwrap();
        let mut other = load(&path).unwrap();
        let mut this = load(&path).unwrap();
        // The other one withdraws it and turns on the whole document.
        other.agreed = None;
        save(&path, &other, Choice::Agreement).unwrap();
        other.whole_document = true;
        save(&path, &other, Choice::WholeDocument).unwrap();
        // This one, which still holds the agreement, turns off the changes.
        this.edits = false;
        save(&path, &this, Choice::Edits).unwrap();
        let kept = load(&path).unwrap();
        let _ = std::fs::remove_dir_all(&folder);
        assert_eq!(kept, Choices { edits: false, whole_document: true, agreed: None });
    }
}
