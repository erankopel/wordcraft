//! What an AI assistant may do to the open document, and how.
//!
//! The AI pane lets the assistant change the document with WordCraft's own commands. It works on a
//! *workspace* ([`Seed::workspace`]): a private session made from the open document, in which
//! [`Session::run`] refuses every command an assistant may not run ([`refusal`]), the commands other
//! commands run included, and every file read and write is refused ([`confined`]). Its tools
//! ([`Toolbox`]) run commands there and record the ones that change something as *steps*.
//! `assistant.runSteps` replays the steps on a fresh workspace of the open document ([`replay`]) and
//! puts the result in as one undo step, so all of one request's changes undo together.
//!
//! Text the assistant adds or deletes is tracked when Track Changes is on: that is a setting of the
//! document, which the workspace carries. Its formatting applies directly: the workspace does not
//! track formatting, and `assistant.runSteps` is left out of the formatting record.

use serde_json::{Value, json};
use wordcraft_doc::Document;

use crate::file_access::{AutomationRoots, confine};
use crate::{Selection, Session};

/// The author of the assistant's tracked changes and comments when the caller names none.
pub const DEFAULT_AUTHOR: &str = "AI Assistant";
/// Most tool calls one request may make.
pub const MAX_CALLS: usize = 60;
/// Most steps `assistant.runSteps` replays.
pub const MAX_STEPS: usize = 500;
/// Most bytes of one tool result; a longer one is cut and says so.
pub const RESULT_BUDGET: usize = 30_000;
/// Most body blocks one `inspect_document` call returns.
const INSPECT_BLOCKS: usize = 150;
/// Most bytes of the document overview the first message carries.
const OVERVIEW_BUDGET: usize = 60_000;
/// Most commands one `list_commands` call returns.
const LIST_LIMIT: usize = 40;

/// Command families that read or edit the open document.
const FAMILIES: &[&str] = &[
    "arrange.",
    "canvas.",
    "caret.",
    "chart.",
    "design.",
    "developer.",
    "document.",
    "equation.",
    "fields.",
    "format.",
    "hf.",
    "insert.",
    "layout.",
    "list.",
    "para.",
    "picture.",
    "references.",
    "select.",
    "shape.",
    "smartArt.",
    "table.",
    "text.",
    "wordArt.",
];

/// Single commands from other families that only read or edit the open document.
const MEMBERS: &[&str] = &[
    "link.targets",
    "object.altText",
    "edit.copy",
    "edit.copyFormat",
    "edit.cut",
    "edit.paste",
    "edit.pasteFormat",
    "edit.pasteMerge",
    "edit.pasteSpecial",
    "edit.pasteText",
    "edit.redo",
    "edit.replace",
    "edit.replaceAll",
    "edit.undo",
    "edit.undoTo",
    "edit.undoHistory",
    "edit.find",
    "edit.advancedFind",
    "edit.findNext",
    "edit.findPrevious",
    "edit.goto",
    "edit.goBack",
    "file.properties",
    "file.accessibility",
    "mailings.insertField",
    "mailings.addressBlock",
    "mailings.greetingLine",
    "mailings.rules",
    "mailings.matchFields",
    "mailings.checkErrors",
    "outline.promote",
    "outline.demote",
    "outline.promoteToHeading1",
    "outline.demoteToBody",
    "outline.level",
    "outline.moveUp",
    "outline.moveDown",
    "review.newComment",
    "review.reply",
    "review.editComment",
    "review.deleteComment",
    "review.resolveComment",
    "review.nextComment",
    "review.previousComment",
    "review.comments",
    "review.changes",
    "review.nextChange",
    "review.previousChange",
    "review.spelling",
    "review.editor",
    "review.issues",
    "review.suggestions",
    "review.applySuggestion",
    "review.wordCount",
    "review.thesaurus",
    "review.language",
    "styles.create",
    "styles.modify",
    "styles.updateToMatch",
    "styles.delete",
    "styles.setVisibility",
    "styles.addToGallery",
    "styles.rename",
    "styles.inspectorClear",
    "styles.list",
    "styles.manage",
    "styles.inspect",
];

/// Never, whatever the lists above say: the window and the views, files, fonts, language packs and
/// the network, speech, preferences, macros and Repeat, protection, the person's control of Track
/// Changes, the assistant itself, and commands that open dialogs, read files or replace the document.
const NEVER: &[&str] = &[
    "arrange.selectionPane",
    "design.setDefault",
    "developer.designMode",
    "document.setText",
    "edit.repeat",
    "equation.inputFormat",
    "equation.normalText",
    "file.protect",
    "format.fontDialog",
    "insert.object",
    "insert.picture",
    "insert.textFromFile",
    "para.dialog",
    "picture.change",
    "references.researcher",
    "review.accept",
    "review.acceptAll",
    "review.addToDictionary",
    "review.combine",
    "review.compare",
    "review.ignoreAll",
    "review.language.preferred",
    "review.proofing",
    "review.readAloud",
    "review.reject",
    "review.rejectAll",
    "review.restrict",
    "review.trackChanges",
    "review.trackingOptions",
    "smartArt.textPane",
    "styles.copyFrom",
    "styles.organizer",
    "table.viewGridlines",
    "text.overtype",
];

/// Families that are never run, however a list above names them.
const NEVER_FAMILIES: &[&str] = &["app.", "assistant.", "draw.", "font.", "help.", "proof.", "readAloud.", "tools.", "ui.", "view."];

/// Parameter names that name a file or folder: refused in every command.
const PATH_PARAMS: &[&str] = &["path", "dir", "folder", "file", "original"];

/// May an assistant run command `id` at all?
pub fn may_run(id: &str) -> bool {
    if NEVER.contains(&id) || NEVER_FAMILIES.iter().any(|f| id.starts_with(f)) {
        return false;
    }
    MEMBERS.contains(&id) || FAMILIES.iter().any(|f| id.starts_with(f))
}

/// Why an assistant may not run `id` with `params`, or `None` when it may.
pub fn refusal(id: &str, params: &Value) -> Option<&'static str> {
    if !may_run(id) {
        return Some("an assistant may only run commands that read or edit the open document");
    }
    if let Some(map) = params.as_object()
        && PATH_PARAMS.iter().any(|k| map.contains_key(*k))
    {
        return Some("an assistant may not name files or folders");
    }
    let has = |k: &str| params.get(k).is_some_and(|v| !v.is_null());
    match id {
        "references.sources" if has("import") => Some("importing sources reads a file"),
        "insert.quickParts" | "insert.autoText" if (has("save") || has("delete")) && params.get("in").and_then(Value::as_str) != Some("document") => {
            Some("Quick Parts outside the document are the person's own")
        }
        "review.language" if has("detect") || has("default") => Some("the language preferences are the person's own"),
        _ => None,
    }
}

/// Commands that only read: their calls are not steps, because replaying them changes nothing.
/// (`edit.find` selects what it finds, so it is a step.)
fn is_reader(id: &str) -> bool {
    matches!(
        id,
        "document.inspect"
            | "document.text"
            | "document.paragraph"
            | "document.selection"
            | "document.layout"
            | "format.state"
            | "styles.list"
            | "styles.inspect"
            | "list.get"
            | "link.targets"
            | "review.changes"
            | "review.comments"
            | "review.issues"
            | "review.suggestions"
            | "review.spelling"
            | "review.editor"
            | "review.wordCount"
            | "review.thesaurus"
            | "edit.undoHistory"
            | "file.accessibility"
            | "mailings.matchFields"
            | "mailings.checkErrors"
            | "equation.get"
            | "equation.gallery"
            | "equation.structureActions"
            | "fields.selected"
            | "design.themes"
            | "references.notes"
    )
}

/// Run `f` with every file read and write refused on this thread.
pub fn confined<T>(f: impl FnOnce() -> T) -> T {
    confine(Some(&AutomationRoots::deny_all()), f)
}

/// What a workspace starts from, taken from the open document. It can move to another thread,
/// where [`Seed::workspace`] makes the workspace.
pub struct Seed {
    doc: Document,
    sel: Selection,
    view: crate::view::ViewState,
    author: String,
    system_region: wordcraft_geom::Region,
    editing_lang: Option<String>,
    new_documents_rtl: bool,
    last_highlight: wordcraft_doc::props::Highlight,
    document: u64,
    rev: u64,
}

impl Seed {
    /// The open document of `s` as it is now, with `author` naming the assistant's tracked changes
    /// and comments (blank: [`DEFAULT_AUTHOR`]).
    pub fn of(s: &Session, author: &str) -> Seed {
        let author = author.trim();
        Seed {
            doc: s.doc.clone(),
            sel: s.sel.clone(),
            view: s.view.clone(),
            author: if author.is_empty() { DEFAULT_AUTHOR.to_string() } else { author.chars().take(100).collect() },
            system_region: s.system_region.clone(),
            editing_lang: s.editing_lang.clone(),
            new_documents_rtl: s.new_documents_rtl,
            last_highlight: s.last_highlight,
            document: s.document_id(),
            rev: s.rev(),
        }
    }

    /// The open document's id and revision when the seed was taken: when either has changed since,
    /// the document changed and steps made on this seed must not be applied to it.
    pub fn base(&self) -> (u64, u64) {
        (self.document, self.rev)
    }

    /// The workspace: a private session of the document that runs only what an assistant may run,
    /// does not track formatting, and starts with no undo history.
    pub fn workspace(self) -> Session {
        let mut w = Session::new(self.doc);
        w.sel = self.sel;
        w.view = self.view;
        w.author = self.author;
        w.system_region = self.system_region;
        w.editing_lang = self.editing_lang;
        w.new_documents_rtl = self.new_documents_rtl;
        w.last_highlight = self.last_highlight;
        w.prefs.markup.track_formatting = false;
        w.agent_only = true;
        w.clamp_selection();
        w
    }
}

/// Replay `steps` (each `{"command", "params"}`) on the workspace of `seed`, refused as a whole at
/// the first step that is not allowed or fails. The workspace comes back with the result.
pub fn replay(seed: Seed, steps: &[Value]) -> Result<Session, String> {
    if steps.len() > MAX_STEPS {
        return Err(format!("{} steps; the most is {MAX_STEPS}", steps.len()));
    }
    let mut ws = seed.workspace();
    let home = ws.document_id();
    confined(|| {
        for (i, step) in steps.iter().enumerate() {
            let n = i + 1;
            let id = step.get("command").and_then(Value::as_str).ok_or_else(|| format!("step {n} names no command"))?;
            let params = step.get("params").cloned().unwrap_or_else(|| json!({}));
            if let Some(why) = refusal(id, &params) {
                return Err(format!("step {n} ({id}): {why}"));
            }
            ws.run(id, &params).map_err(|e| format!("step {n} ({id}): {e}"))?;
            if !ws.ui_requests.is_empty() {
                return Err(format!("step {n} ({id}) needs a dialog"));
            }
        }
        Ok(())
    })?;
    if ws.document_id() != home {
        return Err("the steps changed which document is open".into());
    }
    Ok(ws)
}

/// What a tool call gives back to the model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolResult {
    pub content: String,
    pub is_error: bool,
}

impl ToolResult {
    fn ok(value: &Value) -> ToolResult {
        ToolResult { content: within_budget(&serde_json::to_string(value).unwrap_or_default(), RESULT_BUDGET), is_error: false }
    }

    fn error(text: impl Into<String>) -> ToolResult {
        ToolResult { content: text.into(), is_error: true }
    }
}

/// `text` cut to at most `budget` bytes at a character boundary, saying how much was left out.
fn within_budget(text: &str, budget: usize) -> String {
    if text.len() <= budget {
        return text.to_string();
    }
    let mut end = budget;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{} … [cut: {} more bytes; ask for less, for example a smaller range]", &text[..end], text.len() - end)
}

/// The tools the assistant gets, run on a workspace. Their calls that change something are
/// recorded as steps.
pub struct Toolbox {
    ws: Session,
    steps: Vec<Value>,
    calls: usize,
    start_rev: u64,
}

impl Toolbox {
    /// A toolbox on the workspace of `seed`.
    pub fn new(seed: Seed) -> Toolbox {
        let ws = seed.workspace();
        let start_rev = ws.rev();
        Toolbox { ws, steps: Vec::new(), calls: 0, start_rev }
    }

    /// The steps that change something, in order, for `assistant.runSteps`.
    pub fn steps(&self) -> &[Value] {
        &self.steps
    }

    /// Has anything changed in the workspace's document?
    pub fn changed(&self) -> bool {
        self.ws.rev() != self.start_rev
    }

    /// How many tool calls were made.
    pub fn calls(&self) -> usize {
        self.calls
    }

    /// The workspace, for tests and for reading what was done.
    pub fn workspace(&self) -> &Session {
        &self.ws
    }

    /// The document in brief for the first message: its body paragraphs and tables by block index,
    /// with style names, and the selection. At most a budget of bytes; it says when it is cut.
    pub fn overview(&mut self) -> String {
        let inspected = confined(|| self.ws.run("document.inspect", &json!({"text": true}))).unwrap_or(Value::Null);
        let mut out = String::new();
        let blocks = inspected.get("blocks").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
        for block in blocks {
            let index = block.get("index").and_then(Value::as_u64).unwrap_or(0);
            let line = match block.get("type").and_then(Value::as_str) {
                Some("table") => {
                    let rows = block.get("rows").and_then(Value::as_u64).unwrap_or(0);
                    let cols = block.get("cols").and_then(Value::as_u64).unwrap_or(0);
                    format!("[{index}] table {rows}x{cols}\n")
                }
                _ => {
                    let style = block.get("style").and_then(Value::as_str).unwrap_or("Normal");
                    let text = block.get("text").and_then(Value::as_str).unwrap_or_default();
                    format!("[{index}] ({style}) {text}\n")
                }
            };
            if out.len() + line.len() > OVERVIEW_BUDGET {
                out.push_str(&format!("… [the overview stops at block {index} of {}; use inspect_document with from]\n", blocks.len()));
                break;
            }
            out.push_str(&line);
        }
        out
    }

    /// The tool definitions for the Messages API (`name`, `description`, `input_schema`).
    pub fn definitions() -> Vec<Value> {
        let pos = json!({"type": "object", "description": "A position: {\"block\": body block index, \"off\": byte offset in that paragraph}, or {\"story\": \"body\", \"path\": [...], \"off\": n} as inspect_document reports it."});
        vec![
            json!({"name": "get_text", "description": "The document's body text as plain text, with tracked deletions left out.", "input_schema": {"type": "object", "properties": {}}}),
            json!({"name": "inspect_document", "description": "The document's structure: body blocks by index (paragraphs with style, text, list, paragraph properties and formatted runs; tables with their cells' text), the selection, sections, whether Track Changes is on, the word count. Returns at most 150 blocks from `from`; parts (headers, footers, notes, comments) only with parts: true.", "input_schema": {"type": "object", "properties": {"from": {"type": "integer", "minimum": 0, "description": "First body block index (default 0)."}, "count": {"type": "integer", "minimum": 1, "maximum": 150, "description": "How many blocks (default 150)."}, "parts": {"type": "boolean", "description": "Include headers, footers, notes and comments."}}}}),
            json!({"name": "get_selection", "description": "The current selection: its ends and its text.", "input_schema": {"type": "object", "properties": {}}}),
            json!({"name": "find", "description": "Find text in the document and select the first match. Returns the match ranges (at most 200).", "input_schema": {"type": "object", "properties": {"text": {"type": "string"}, "matchCase": {"type": "boolean"}, "wholeWord": {"type": "boolean"}, "regex": {"type": "boolean"}}, "required": ["text"]}}),
            json!({"name": "select_text", "description": "Select an occurrence of text in the document (1-based occurrence, default the first).", "input_schema": {"type": "object", "properties": {"text": {"type": "string"}, "occurrence": {"type": "integer", "minimum": 1}}, "required": ["text"]}}),
            json!({"name": "select_range", "description": "Select from anchor to focus. A collapsed range puts the caret there.", "input_schema": {"type": "object", "properties": {"anchor": pos, "focus": pos}, "required": ["anchor", "focus"]}}),
            json!({"name": "type_text", "description": "Type text at the selection, replacing what is selected, exactly as given (no AutoCorrect). With paragraphs: true each line becomes its own paragraph. Tracked as an insertion (and a deletion of what it replaces) when Track Changes is on.", "input_schema": {"type": "object", "properties": {"text": {"type": "string"}, "paragraphs": {"type": "boolean"}}, "required": ["text"]}}),
            json!({"name": "replace_all", "description": "Replace every match of `text` with `with` in the document. Tracked when Track Changes is on.", "input_schema": {"type": "object", "properties": {"text": {"type": "string"}, "with": {"type": "string"}, "matchCase": {"type": "boolean"}, "wholeWord": {"type": "boolean"}, "regex": {"type": "boolean"}}, "required": ["text", "with"]}}),
            json!({"name": "insert_table", "description": "Insert a table at the selection from rows of values (at most 1000 rows by 63 columns).", "input_schema": {"type": "object", "properties": {"values": {"type": "array", "items": {"type": "array", "items": {"type": ["string", "number", "boolean", "null"]}}}, "header": {"type": "boolean", "description": "The first row is a header row."}, "style": {"type": "string", "description": "A table style id, for example GridTable1Light."}}, "required": ["values"]}}),
            json!({"name": "list_commands", "description": "Search the WordCraft commands you may run with execute, by words in their id, label, ribbon place or parameters. Returns id, label, place and the parameters each takes (at most 40).", "input_schema": {"type": "object", "properties": {"query": {"type": "string"}, "limit": {"type": "integer", "minimum": 1, "maximum": 40}}, "required": ["query"]}}),
            json!({"name": "execute", "description": "Run one WordCraft command on the document, with its parameters as list_commands describes them. Only commands that read or edit the open document are allowed.", "input_schema": {"type": "object", "properties": {"command": {"type": "string"}, "params": {"type": "object"}}, "required": ["command"]}}),
            json!({"name": "batch", "description": "Run several commands in order. Stops at the first failure unless keepGoing is true; earlier commands stay done.", "input_schema": {"type": "object", "properties": {"commands": {"type": "array", "items": {"type": "object", "properties": {"command": {"type": "string"}, "params": {"type": "object"}}, "required": ["command"]}}, "keepGoing": {"type": "boolean"}}, "required": ["commands"]}}),
        ]
    }

    /// Run tool `name` with `input`. Never panics: an unknown tool, a refused command or a failure
    /// is an error result the model reads.
    pub fn call(&mut self, name: &str, input: &Value) -> ToolResult {
        self.calls += 1;
        if self.calls > MAX_CALLS {
            return ToolResult::error(format!("this request has used its {MAX_CALLS} tool calls; stop and say what is left to do"));
        }
        let pick = |keys: &[&str]| -> Value {
            let mut out = serde_json::Map::new();
            for k in keys {
                if let Some(v) = input.get(*k).filter(|v| !v.is_null()) {
                    out.insert((*k).to_string(), v.clone());
                }
            }
            Value::Object(out)
        };
        let result = match name {
            "get_text" => self.run("document.text", json!({})),
            "get_selection" => self.run("document.selection", json!({})),
            "inspect_document" => self.inspect(input),
            "find" => self.run("edit.find", pick(&["text", "matchCase", "wholeWord", "regex"])),
            "select_text" => self.run("select.text", pick(&["text", "occurrence"])),
            "select_range" => self.run("select.range", pick(&["anchor", "focus"])),
            "type_text" => {
                let mut params = pick(&["text", "paragraphs"]);
                params["raw"] = json!(true);
                self.run("text.insert", params)
            }
            "replace_all" => self.run("edit.replaceAll", pick(&["text", "with", "matchCase", "wholeWord", "regex"])),
            "insert_table" => self.run("insert.spreadsheet", pick(&["values", "header", "style"])),
            "list_commands" => Ok(self.list_commands(input)),
            "execute" => match input.get("command").and_then(Value::as_str) {
                Some(id) => {
                    let params = input.get("params").cloned().filter(|p| !p.is_null()).unwrap_or_else(|| json!({}));
                    self.run(id, params)
                }
                None => Err("execute needs `command`".to_string()),
            },
            "batch" => self.batch(input),
            other => Err(format!("there is no tool `{other}`")),
        };
        match result {
            Ok(value) => ToolResult::ok(&value),
            Err(why) => ToolResult::error(why),
        }
    }

    /// Run one command in the workspace, file access refused; record it as a step when it may
    /// change something. A command that only opens a dialog is a failure, not a success.
    fn run(&mut self, id: &str, params: Value) -> Result<Value, String> {
        if let Some(why) = refusal(id, &params) {
            return Err(format!("{id} is not available to you: {why}"));
        }
        let result = confined(|| self.ws.run(id, &params));
        let asked_window = !self.ws.ui_requests.is_empty();
        self.ws.ui_requests.clear();
        match result {
            Ok(_) if asked_window => {
                Err(format!("{id} only opens a dialog when called like this; give it its parameters instead (see list_commands)"))
            }
            Ok(value) => {
                if !is_reader(id) {
                    self.steps.push(json!({"command": id, "params": params}));
                }
                Ok(value)
            }
            Err(e) => Err(e.to_string()),
        }
    }

    fn inspect(&mut self, input: &Value) -> Result<Value, String> {
        let mut full = self.run("document.inspect", json!({"text": true}))?;
        let from = input.get("from").and_then(Value::as_u64).map_or(0, |v| v as usize);
        let count = input.get("count").and_then(Value::as_u64).map_or(INSPECT_BLOCKS, |v| (v as usize).clamp(1, INSPECT_BLOCKS));
        if let Some(blocks) = full.get_mut("blocks").and_then(Value::as_array_mut) {
            let total = blocks.len();
            let kept: Vec<Value> = blocks.drain(..).skip(from).take(count).collect();
            *blocks = kept;
            full["blockCount"] = json!(total);
            full["from"] = json!(from);
        }
        if !input.get("parts").and_then(Value::as_bool).unwrap_or(false)
            && let Some(map) = full.as_object_mut()
        {
            map.remove("parts");
        }
        Ok(full)
    }

    fn list_commands(&self, input: &Value) -> Value {
        let query = input.get("query").and_then(Value::as_str).unwrap_or_default().to_lowercase();
        let limit = input.get("limit").and_then(Value::as_u64).map_or(LIST_LIMIT, |v| (v as usize).clamp(1, LIST_LIMIT));
        let words: Vec<&str> = query.split_whitespace().collect();
        let all = self.ws.registry.describe();
        let found: Vec<Value> = all
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .filter(|c| c.get("id").and_then(Value::as_str).is_some_and(may_run))
            .filter(|c| {
                let hay = ["id", "label", "location", "params"]
                    .iter()
                    .filter_map(|k| c.get(*k).and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join(" ")
                    .to_lowercase();
                words.iter().all(|w| hay.contains(w))
            })
            .take(limit)
            .map(|c| json!({"id": c["id"], "label": c["label"], "place": c["location"], "params": c["params"]}))
            .collect();
        json!({"commands": found})
    }

    fn batch(&mut self, input: &Value) -> Result<Value, String> {
        let commands = input.get("commands").and_then(Value::as_array).ok_or("batch needs `commands`")?;
        let keep_going = input.get("keepGoing").and_then(Value::as_bool).unwrap_or(false);
        let mut results = Vec::new();
        for c in commands {
            let Some(id) = c.get("command").and_then(Value::as_str) else {
                results.push(json!({"error": "a command names no `command`"}));
                if keep_going {
                    continue;
                }
                break;
            };
            let params = c.get("params").cloned().filter(|p| !p.is_null()).unwrap_or_else(|| json!({}));
            match self.run(id, params) {
                Ok(value) => results.push(json!({"command": id, "result": value})),
                Err(why) => {
                    results.push(json!({"command": id, "error": why}));
                    if !keep_going {
                        break;
                    }
                }
            }
        }
        Ok(json!({"results": results}))
    }
}

#[cfg(test)]
mod tests;
