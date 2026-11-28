//! The AI assistant's commands: which part of the document would be shared, putting a reply into
//! the document, and applying the changes the assistant made with WordCraft's commands.
//!
//! None of them touches the network. The AI pane sends a request only after the person has agreed
//! to what leaves the machine, and an agent that drives WordCraft through MCP or the CLI brings its own
//! model, so nothing here can send text out or spend money. See `docs/assistant.md`.

use std::ops::Range;

use serde_json::{Value, json};
use wordcraft_doc::{Block, Blocks, Document, Paragraph, Pos, StoryRef, StyleSheet};

use super::{paste_text, sel_result};
use crate::{CmdError, CmdResult, CommandSpec, Selection, Session, p};

/// Most bytes `assistant.apply` takes in one call.
pub const MAX_APPLY_BYTES: usize = 1_000_000;

/// The assistant commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("assistant.context", "Assistant Context", "Agents", context)
            .params(
                r#"{"scope"?: "selection" | "document" (default selection)} → {scope, text, chars, bytes, paragraphs, hasSelection, empty}: exactly the text the AI pane would share for that scope. Both scopes read Track Changes as accepted: text deleted with tracking on is not included. The selection is the selected text; the document is the body text (headers, footers, notes and text boxes are not included). Text formatted as hidden is left out unless hidden text is shown (Show/Hide ¶ or Options › Display › Hidden text). Changes nothing."#,
            )
            .pure(),
        CommandSpec::new("assistant.apply", "Apply Assistant Reply", "Agents", apply).params(
            r#"{"text": string, "mode"?: "replaceSelection" | "insertAfter" (default replaceSelection)}: puts a reply into the document as plain text, exactly as written (no AutoCorrect or AutoFormat). replaceSelection replaces the selection, or inserts at the caret when nothing is selected; insertAfter adds the text as new paragraphs after the paragraph the selection ends in. One undo step, and tracked when Track Changes is on. At most 1,000,000 bytes."#,
        ),
        CommandSpec::new("assistant.runSteps", "Apply Assistant Changes", "Agents", run_steps).params(
            r#"{"steps": [{"command": id, "params"?: {...}}], "author"?: string, "start"?: {"selection"?: {"anchor": Pos, "focus": Pos}, "view"?: {...}}} → the selection: runs the steps in order on a private copy of the document in which only commands that read or edit the open document are allowed (no files, network, preferences, protection, macros, dialogs or Track Changes controls) and no file can be read or written, starting from `start` (the selection and the view the steps were made from; default the current ones); when every step succeeds the copy replaces the document as ONE undo step, else nothing changes. Text the steps add or delete is tracked when Track Changes is on, as `author` (default "AI Assistant"); formatting applies directly. At most 500 steps. Repeat and macro recording leave it out."#,
        ),
    ]
}

/// Apply the steps an AI assistant made (see [`crate::agent`]): replayed on a fresh workspace of the
/// open document, from where they were made (`start`), whose document then replaces it. `run` makes
/// that one undo step and rolls back on failure.
fn run_steps(s: &mut Session, v: &Value) -> CmdResult {
    let steps = v
        .get("steps")
        .and_then(Value::as_array)
        .ok_or_else(|| CmdError::Params("assistant.runSteps: `steps` must be a list of {\"command\", \"params\"}".into()))?;
    if steps.is_empty() {
        return Err(CmdError::Params("assistant.runSteps: `steps` is empty".into()));
    }
    let author = p::str(v, "author").unwrap_or(crate::agent::DEFAULT_AUTHOR);
    let mut seed = crate::agent::Seed::of(s, author);
    if let Some(start) = v.get("start").filter(|x| !x.is_null()) {
        seed = seed.starting_at(start).map_err(|e| CmdError::Params(format!("assistant.runSteps: `start`: {e}")))?;
    }
    let mut ws = crate::agent::replay(seed, steps).map_err(|e| CmdError::Failed(format!("assistant.runSteps: {e}")))?;
    s.doc = std::mem::take(&mut ws.doc);
    s.sel = ws.sel.clone();
    // The pictures the steps restyled keep what Reset Picture and the next restyle need: the workspace
    // started from these maps, and only the steps changed them.
    s.originals = std::mem::take(&mut ws.originals);
    s.picture_looks = std::mem::take(&mut ws.picture_looks);
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
        Scope::Selection => selected_shared_text(s),
        Scope::Document => shared_story_text(&s.doc, StoryRef::Body, hidden_shown(s)),
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

/// Is hidden text on screen, so the assistant may read it: Show/Hide ¶ or Options › Display › Hidden
/// text, as the layout shows it.
pub(crate) fn hidden_shown(s: &Session) -> bool {
    s.view.marks || s.prefs.marks.hidden
}

/// The byte ranges of `p` the assistant is not given: text deleted with Track Changes on (still in the
/// file, marked deleted) and, unless hidden text is `shown`, text whose formatting resolves to hidden.
pub(crate) fn unshared_ranges(styles: &StyleSheet, p: &Paragraph, shown: bool) -> Vec<Range<usize>> {
    let style = p.props.style.as_deref();
    p.run_ranges().filter(|(r, c)| !r.is_empty() && (c.del.is_some() || (!shown && styles.resolve_char(style, c).hidden))).map(|(r, _)| r).collect()
}

/// A paragraph's text as the assistant reads it: without the [`unshared_ranges`].
pub(crate) fn shared_para_text(styles: &StyleSheet, p: &Paragraph, shown: bool) -> String {
    p.text_without(&unshared_ranges(styles, p, shown))
}

/// Does a paragraph go from what the assistant reads altogether: its mark is hidden (and not shown) and
/// none of its text is shared, so it takes no line, as on screen.
fn para_left_out(styles: &StyleSheet, p: &Paragraph, text: &str, shown: bool) -> bool {
    !shown && text.is_empty() && styles.resolve_char(p.props.style.as_deref(), &p.mark).hidden
}

/// A story's text as the assistant reads it, laid out as [`Document::final_text`] lays it out (a line a
/// paragraph, a table row's cells joined by tabs): every tracked change accepted and hidden text left out
/// unless `shown`.
pub(crate) fn shared_story_text(doc: &Document, story: StoryRef, shown: bool) -> String {
    let mut out = String::new();
    if let Some(blocks) = doc.story(story) {
        shared_blocks_text(&doc.styles, blocks, &mut out, 0, shown);
    }
    if out.ends_with('\n') {
        out.pop();
    }
    out
}

fn shared_blocks_text(styles: &StyleSheet, blocks: &Blocks, out: &mut String, depth: usize, shown: bool) {
    if depth > 16 {
        return;
    }
    for b in blocks {
        match &**b {
            Block::Para(p) => {
                let text = shared_para_text(styles, p, shown);
                if !para_left_out(styles, p, &text, shown) {
                    out.push_str(&text);
                    out.push('\n');
                }
            }
            Block::Table(t) => {
                for row in &t.rows {
                    let cells: Vec<String> = row
                        .cells
                        .iter()
                        .map(|c| {
                            let mut s = String::new();
                            shared_blocks_text(styles, &c.blocks, &mut s, depth + 1, shown);
                            s.trim_end_matches('\n').replace('\n', " ")
                        })
                        .collect();
                    out.push_str(&cells.join("\t"));
                    out.push('\n');
                }
            }
        }
    }
}

/// The selected text as the assistant reads it, as [`Session::selected_text`] reads the selection but
/// without the text deleted with Track Changes on (still in the file, marked deleted, and it must not go
/// anywhere the person did not mean it to) and without hidden text unless it is shown.
pub(crate) fn selected_shared_text(s: &Session) -> String {
    let shown = hidden_shown(s);
    let shared = |a: &Pos, b: &Pos| {
        let mut d = s.doc.copy_range(a, b).to_document();
        // The copy's runs resolve through the document's styles, which may be what hides them.
        d.styles = s.doc.styles.clone();
        shared_story_text(&d, StoryRef::Body, shown)
    };
    if let Some(segments) = s.column_segments() {
        return segments.iter().map(|(a, b)| shared(a.min(b), a.max(b))).collect::<Vec<_>>().join("\n");
    }
    let (a, b) = s.sel.ordered();
    shared(&a, &b)
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
