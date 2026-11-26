//! The AI assistant's commands: which part of the document would be shared, and putting a reply into
//! the document.
//!
//! Neither command touches the network. The AI pane sends a request only after the person has agreed
//! to what leaves the machine, and an agent that drives WordCraft through MCP or the CLI brings its own
//! model, so nothing here can send text out or spend money. See `docs/assistant.md`.

use serde_json::{Value, json};
use wordcraft_doc::{Pos, StoryRef};

use super::{paste_text, sel_result};
use crate::{CmdError, CmdResult, CommandSpec, Selection, Session, p};

/// Most bytes `assistant.apply` takes in one call.
pub const MAX_APPLY_BYTES: usize = 1_000_000;

/// The assistant commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("assistant.context", "Assistant Context", "Agents", context)
            .params(
                r#"{"scope"?: "selection" | "document" (default selection)} → {scope, text, chars, bytes, paragraphs, hasSelection, empty}: exactly the text the AI pane would share for that scope. The selection is the selected text; the document is the body text with Track Changes read as accepted (headers, footers, notes and text boxes are not included). Changes nothing."#,
            )
            .pure(),
        CommandSpec::new("assistant.apply", "Apply Assistant Reply", "Agents", apply).params(
            r#"{"text": string, "mode"?: "replaceSelection" | "insertAfter" (default replaceSelection)}: puts a reply into the document as plain text, exactly as written (no AutoCorrect or AutoFormat). replaceSelection replaces the selection, or inserts at the caret when nothing is selected; insertAfter adds the text as new paragraphs after the paragraph the selection ends in. One undo step, and tracked when Track Changes is on. At most 1,000,000 bytes."#,
        ),
    ]
}

/// What `assistant.context` reads.
#[derive(Clone, Copy)]
enum Scope {
    Selection,
    Document,
}

fn scope_of(v: &Value) -> Result<(Scope, &'static str), CmdError> {
    match p::str(v, "scope").unwrap_or("selection") {
        "selection" => Ok((Scope::Selection, "selection")),
        "document" => Ok((Scope::Document, "document")),
        other => Err(CmdError::Params(format!("assistant.context: `scope` is `selection` or `document`, not `{other}`"))),
    }
}

fn context(s: &mut Session, v: &Value) -> CmdResult {
    let (scope, name) = scope_of(v)?;
    let text = match scope {
        Scope::Selection => s.selected_text(),
        Scope::Document => s.doc.final_text(StoryRef::Body),
    };
    let paragraphs = text.split('\n').filter(|line| !line.trim().is_empty()).count();
    Ok(json!({
        "scope": name,
        "chars": text.chars().count(),
        "bytes": text.len(),
        "paragraphs": paragraphs,
        "hasSelection": !s.sel.is_collapsed(),
        "empty": text.trim().is_empty(),
        "text": text,
    }))
}

/// The end of the paragraph the selection ends in, where `insertAfter` adds its text.
fn end_of_selection_paragraph(s: &Session) -> Pos {
    let end = s.sel.ordered().1;
    let len = s.doc.para_at(&end).map_or(end.off, |para| para.len());
    Pos { off: len, ..end }
}

fn apply(s: &mut Session, v: &Value) -> CmdResult {
    let text = p::req_str(v, "text")?;
    if text.trim().is_empty() {
        return Err(CmdError::Params("assistant.apply: `text` is empty".into()));
    }
    if text.len() > MAX_APPLY_BYTES {
        return Err(CmdError::Params(format!("assistant.apply: `text` is {} bytes; the limit is {MAX_APPLY_BYTES}", text.len())));
    }
    // Paragraph breaks are carriage returns in a paste, as `edit.pasteText` writes them.
    let text = text.replace("\r\n", "\r").replace('\n', "\r");
    match p::str(v, "mode").unwrap_or("replaceSelection") {
        "replaceSelection" => paste_text(s, &text)?,
        "insertAfter" => {
            s.sel = Selection::caret(end_of_selection_paragraph(s));
            paste_text(s, &format!("\r{text}"))?;
        }
        other => return Err(CmdError::Params(format!("assistant.apply: `mode` is `replaceSelection` or `insertAfter`, not `{other}`"))),
    }
    sel_result(s)
}
