//! The AI assistant's commands: which part of the document would be shared, putting a reply into
//! the document, and applying the changes the assistant made with WordCraft's commands.
//!
//! None of them touches the network. The AI pane sends a request only after the person has agreed
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
                r#"{"scope"?: "selection" | "document" (default selection)} → {scope, text, chars, bytes, paragraphs, hasSelection, empty}: exactly the text the AI pane would share for that scope. Both scopes read Track Changes as accepted: text deleted with tracking on is not included. The selection is the selected text; the document is the body text (headers, footers, notes and text boxes are not included). Text formatted as hidden is included. Changes nothing."#,
            )
            .pure(),
        CommandSpec::new("assistant.apply", "Apply Assistant Reply", "Agents", apply).params(
            r#"{"text": string, "mode"?: "replaceSelection" | "insertAfter" (default replaceSelection)}: puts a reply into the document as plain text, exactly as written (no AutoCorrect or AutoFormat). replaceSelection replaces the selection, or inserts at the caret when nothing is selected; insertAfter adds the text as new paragraphs after the paragraph the selection ends in. One undo step, and tracked when Track Changes is on. At most 1,000,000 bytes."#,
        ),
        CommandSpec::new("assistant.runSteps", "Apply Assistant Changes", "Agents", run_steps).params(
            r#"{"steps": [{"command": id, "params"?: {...}}], "author"?: string} → the selection: runs the steps in order on a private copy of the document in which only commands that read or edit the open document are allowed (no files, network, preferences, protection, macros, dialogs or Track Changes controls) and no file can be read or written; when every step succeeds the copy replaces the document as ONE undo step, else nothing changes. Text the steps add or delete is tracked when Track Changes is on, as `author` (default "AI Assistant"); formatting applies directly. At most 500 steps."#,
        ),
    ]
}

/// Apply the steps an AI assistant made (see [`crate::agent`]): replayed on a fresh workspace of the
/// open document, whose document then replaces it. `run` makes that one undo step and rolls back on
/// failure.
fn run_steps(s: &mut Session, v: &Value) -> CmdResult {
    let steps = v
        .get("steps")
        .and_then(Value::as_array)
        .ok_or_else(|| CmdError::Params("assistant.runSteps: `steps` must be a list of {\"command\", \"params\"}".into()))?;
    if steps.is_empty() {
        return Err(CmdError::Params("assistant.runSteps: `steps` is empty".into()));
    }
    let author = p::str(v, "author").unwrap_or(crate::agent::DEFAULT_AUTHOR);
    let mut ws = crate::agent::replay(crate::agent::Seed::of(s, author), steps).map_err(|e| CmdError::Failed(format!("assistant.runSteps: {e}")))?;
    s.doc = std::mem::take(&mut ws.doc);
    s.sel = ws.sel.clone();
    // The body is a new one, any equation being edited is gone, and the bibliography style is the
    // document's (the steps may have changed it).
    s.note_body_replaced();
    s.math = None;
    s.bib_style = crate::doc_state::doc_bib_style(&s.doc);
    s.clamp_selection();
    sel_result(s)
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
        Scope::Selection => selected_final_text(s),
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

/// The selected text as it reads with every tracked change accepted, as [`Session::selected_text`]
/// reads the selection but without the text deleted with Track Changes on: that text is still in the
/// file, marked deleted, and must not go anywhere the person did not mean it to.
fn selected_final_text(s: &Session) -> String {
    let final_text = |a: &Pos, b: &Pos| s.doc.copy_range(a, b).to_document().final_text(StoryRef::Body);
    if let Some(segments) = s.column_segments() {
        return segments.iter().map(|(a, b)| final_text(a.min(b), a.max(b))).collect::<Vec<_>>().join("\n");
    }
    let (a, b) = s.sel.ordered();
    final_text(&a, &b)
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
