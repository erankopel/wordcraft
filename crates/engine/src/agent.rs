//! What an AI assistant may do to the open document, and how.
//!
//! The AI pane lets the assistant change the document with WordCraft's own commands. It works on a
//! *workspace* ([`Seed::workspace`]): a private session made from the open document, in which
//! [`Session::run`] refuses every command an assistant may not run ([`refusal`]), the commands other
//! commands run included, and every file read and write is refused ([`confined`]). Its tools
//! ([`Toolbox`]) run commands there. Every call either is recorded as a *step*, when it changed
//! something, or leaves no trace: a call that fails, only asks for a dialog, or makes a change that is
//! refused is put back whole. `assistant.runSteps` replays the steps on a fresh workspace of the open
//! document, from the selection and the view the workspace started with ([`Seed::start`],
//! [`replay`]), and puts the result in as one undo step, so all of one request's changes undo
//! together and land where the assistant made them.
//!
//! Text the assistant adds or deletes is tracked when Track Changes is on: that is a setting of the
//! document, which the workspace carries, and a change that Track Changes cannot record is refused
//! ([`Toolbox`] compares the document as it reads with every change rejected). Its formatting applies
//! directly: the workspace does not track formatting, and `assistant.runSteps` is left out of the
//! formatting record.
//!
//! The assistant reads the document as it reads with every tracked change accepted: its tools leave
//! out text deleted with Track Changes on, and the readers that show it are closed to it.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hasher};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::{Value, json};
use wordcraft_doc::para::OBJ;
use wordcraft_doc::{Block, Document, PartKind, Pos, StoryRef};

use crate::cmd::objects::PictureLook;
use crate::cmd::{parse_pos, pos_json};
use crate::file_access::{AutomationRoots, confine};
use crate::{Selection, Session};

/// The author of the assistant's tracked changes and comments when the caller names none.
pub const DEFAULT_AUTHOR: &str = "AI Assistant";
/// Most tool calls one request may make.
pub const MAX_CALLS: usize = 60;
/// Most commands one request may run, the ones in batches included.
pub const MAX_COMMANDS: usize = 600;
/// Most commands one batch runs.
pub const MAX_BATCH: usize = 50;
/// Most steps one request may record, and `assistant.runSteps` replays.
pub const MAX_STEPS: usize = 500;
/// How far one request may grow the document's text: to this many times its size, or by
/// [`GROWTH_ALLOWANCE`] bytes, whichever is more. A change past that is refused, so a runaway copy and
/// paste cannot fill the memory.
const GROWTH_FACTOR: usize = 3;
const GROWTH_ALLOWANCE: usize = 2_000_000;
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
/// Changes, the assistant itself, commands that open dialogs, read files or replace the document, and
/// the readers that show text deleted with Track Changes on (a paragraph's stored text, the list of
/// tracked changes): the assistant reads the document as it reads with every change accepted.
const NEVER: &[&str] = &[
    "arrange.selectionPane",
    "design.setDefault",
    "developer.designMode",
    "document.paragraph",
    "document.setText",
    "edit.repeat",
    "equation.inputFormat",
    "equation.normalText",
    "file.protect",
    "format.fontDialog",
    "insert.object",
    "insert.picture",
    "insert.textFromFile",
    "link.targets",
    "para.dialog",
    "picture.change",
    "references.notes",
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

/// Parameter names that name a file or folder: refused in every command but the ones in [`NOT_FILES`].
const PATH_PARAMS: &[&str] = &["path", "dir", "folder", "file", "original"];

/// Commands that use a name of [`PATH_PARAMS`] for something that is not a file: a place in an equation or a
/// connector, a direction, the original size of a picture.
const NOT_FILES: &[(&str, &str)] =
    &[("equation.edit", "path"), ("equation.move", "dir"), ("shape.connect", "path"), ("shape.textDirection", "dir"), ("picture.size", "original")];

/// Fields that make a word processor read a file or a web address, or start a program, when the document is
/// opened or its fields are updated (INCLUDE is the old name of INCLUDETEXT; RD names another document for a
/// table of contents or an index).
const REACHING_FIELDS: &[&str] = &["INCLUDEPICTURE", "INCLUDETEXT", "INCLUDE", "LINK", "DDE", "DDEAUTO", "IMPORT", "DATABASE", "RD"];

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
        && PATH_PARAMS.iter().any(|k| map.contains_key(*k) && !NOT_FILES.contains(&(id, *k)))
    {
        return Some("an assistant may not name files or folders");
    }
    let has = |k: &str| params.get(k).is_some_and(|v| !v.is_null());
    let text = |k: &str| params.get(k).and_then(Value::as_str).unwrap_or_default();
    match id {
        "references.sources" if has("import") => Some("importing sources reads a file"),
        "insert.quickParts" | "insert.autoText" if (has("save") || has("delete")) && params.get("in").and_then(Value::as_str) != Some("document") => {
            Some("Quick Parts outside the document are the person's own")
        }
        "review.language" if has("detect") || has("default") => Some("the language preferences are the person's own"),
        "insert.field" if reaches_out(text("instr")) => Some(REACHING),
        "fields.setCode" if reaches_out(text("code")) => Some(REACHING),
        // HTML and RTF can carry fields and linked pictures; the assistant pastes text and formats it with commands.
        "edit.paste" | "edit.pasteMerge" | "edit.pasteSpecial"
            if has("html")
                || has("rtf")
                || matches!(text("as"), "rtf" | "html")
                || crate::cmd::paste::looks_like_rtf(text("text"))
                || crate::cmd::paste::looks_like_html(text("text")) =>
        {
            Some("an assistant pastes text, not HTML or RTF")
        }
        _ => None,
    }
}

/// Why a field that reaches out is refused.
const REACHING: &str = "a field that reads a file or a web address, or starts a program, when the document is opened or updated";

/// Does field code `code` make a word processor read a file or a web address, or start a program, when the
/// document is opened or its fields are updated? Its field name is one that does, or it names a web address or
/// a network path anywhere but in a hyperlink, which is followed only when the reader clicks it.
fn reaches_out(code: &str) -> bool {
    let name = code.split_whitespace().next().unwrap_or_default().to_uppercase();
    if REACHING_FIELDS.contains(&name.as_str()) {
        return true;
    }
    name != "HYPERLINK" && (code.contains("://") || code.contains("\\\\") || code.to_lowercase().contains("www."))
}

/// Commands that only read. Their calls are not steps, unless one changed what later steps depend on
/// (the selection, for one): then it is recorded like any other (see [`Toolbox::call`]).
fn is_reader(id: &str) -> bool {
    matches!(
        id,
        "document.inspect"
            | "document.text"
            | "document.selection"
            | "document.layout"
            | "format.state"
            | "styles.list"
            | "styles.inspect"
            | "list.get"
            | "link.targets"
            | "review.comments"
            | "review.issues"
            | "review.suggestions"
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

/// Commands whose changes Track Changes does not record in WordCraft (the text of a field's result, an
/// equation's content): with Track Changes on, an assistant may not run them.
fn untrackable(id: &str) -> bool {
    matches!(
        id,
        "fields.setCode"
            | "fields.update"
            | "references.updateFields"
            | "references.updateToc"
            | "references.updateIndex"
            | "references.updateFigures"
            | "references.updateAuthorities"
            | "references.citationStyle"
    ) || (id.starts_with("equation.") && !is_reader(id))
}

/// The readers that return the stored text, which holds text deleted with Track Changes on: the model
/// reads through the tool that shows the text as it reads instead.
fn wrapped_by(id: &str) -> Option<&'static str> {
    match id {
        "document.inspect" => Some("inspect_document"),
        "document.selection" => Some("get_selection"),
        "document.text" => Some("get_text"),
        _ => None,
    }
}

/// The fewest letters or digits a piece of deleted text has for results to be checked for it.
const SECRET_MIN_CHARS: usize = 4;

/// Run `f` with every file read and write refused on this thread.
pub fn confined<T>(f: impl FnOnce() -> T) -> T {
    confine(Some(&AutomationRoots::deny_all()), f)
}

/// Is a request a command made for the window only a scroll (harmless), not a dialog or a pane?
fn only_scrolls(request: &Value) -> bool {
    request.get("scrollTo").is_some()
}

/// What a workspace starts from, taken from the open document. It can move to another thread,
/// where [`Seed::workspace`] makes the workspace.
#[derive(Clone)]
pub struct Seed {
    doc: Document,
    sel: Selection,
    view: crate::view::ViewState,
    /// Options › Display › Hidden text: with Show/Hide ¶ in the view, whether hidden text is shown, and so
    /// whether the assistant may read it.
    hidden_text_shown: bool,
    author: String,
    system_region: wordcraft_geom::Region,
    editing_lang: Option<String>,
    new_documents_rtl: bool,
    last_highlight: wordcraft_doc::props::Highlight,
    originals: HashMap<String, String>,
    picture_looks: HashMap<String, PictureLook>,
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
            hidden_text_shown: s.prefs.marks.hidden,
            author: if author.is_empty() { DEFAULT_AUTHOR.to_string() } else { author.chars().take(100).collect() },
            system_region: s.system_region.clone(),
            editing_lang: s.editing_lang.clone(),
            new_documents_rtl: s.new_documents_rtl,
            last_highlight: s.last_highlight,
            originals: s.originals.clone(),
            picture_looks: s.picture_looks.clone(),
            document: s.document_id(),
            rev: s.rev(),
        }
    }

    /// The open document's id and revision when the seed was taken.
    pub fn base(&self) -> (u64, u64) {
        (self.document, self.rev)
    }

    /// The document as it was when the seed was taken: steps made on this seed may be applied only to a
    /// document that still has the same content ([`unchanged_since`]).
    pub fn document(&self) -> &Document {
        &self.doc
    }

    /// Where the workspace starts that is not the document itself: the selection and the view, which
    /// commands that move by lines or pages follow. `assistant.runSteps` takes it as `start`, so the
    /// steps are replayed from where they were made, wherever the person has clicked since.
    pub fn start(&self) -> Value {
        let v = &self.view;
        json!({
            "selection": {"anchor": pos_json(&self.sel.anchor), "focus": pos_json(&self.sel.focus)},
            "view": serde_json::to_value(v).unwrap_or(Value::Null),
            // The geometry the view does not save (the window's width for Web Layout, the reading
            // pages), which lines and pages follow.
            "layout": {
                "webWidth": v.web_width,
                "readWidth": v.read_width,
                "readHeight": v.read_height,
                "readZoom": v.read_zoom,
                "readColumnWidth": v.read_column_width,
                "readLineSpacing": v.read_line_spacing,
            },
        })
    }

    /// This seed with the selection and the view of `start` (as [`Seed::start`] writes it).
    pub fn starting_at(mut self, start: &Value) -> Result<Seed, String> {
        if let Some(sel) = start.get("selection").filter(|v| !v.is_null()) {
            let anchor = sel.get("anchor").and_then(parse_pos).ok_or("the selection has no readable anchor")?;
            let focus = sel.get("focus").and_then(parse_pos).ok_or("the selection has no readable focus")?;
            self.sel = Selection { anchor, focus };
        }
        if let Some(view) = start.get("view").filter(|v| !v.is_null()) {
            let mut v: crate::view::ViewState = serde_json::from_value(view.clone()).map_err(|e| format!("the view is unreadable: {e}"))?;
            // What serde leaves out stays as it is now, unless `start` says what it was.
            let layout = start.get("layout").unwrap_or(&Value::Null);
            let number = |k: &str| layout.get(k).and_then(Value::as_f64).map(|x| x as f32);
            v.web_width = number("webWidth").unwrap_or(self.view.web_width);
            v.read_width = number("readWidth").unwrap_or(self.view.read_width);
            v.read_height = number("readHeight").unwrap_or(self.view.read_height);
            v.read_zoom = number("readZoom").unwrap_or(self.view.read_zoom);
            v.read_column_width = if layout.get("readColumnWidth").is_some() { number("readColumnWidth") } else { self.view.read_column_width };
            v.read_line_spacing = if layout.get("readLineSpacing").is_some() { number("readLineSpacing") } else { self.view.read_line_spacing };
            v.read_page_color = self.view.read_page_color;
            self.view = v;
        }
        Ok(self)
    }

    /// The workspace: a private session of the document that runs only what an assistant may run,
    /// does not track formatting, and starts with no undo history.
    pub fn workspace(self) -> Session {
        let mut w = Session::new(self.doc);
        w.sel = self.sel;
        w.view = self.view;
        w.prefs.marks.hidden = self.hidden_text_shown;
        w.author = self.author;
        w.system_region = self.system_region;
        w.editing_lang = self.editing_lang;
        w.new_documents_rtl = self.new_documents_rtl;
        w.last_highlight = self.last_highlight;
        w.originals = self.originals;
        w.picture_looks = self.picture_looks;
        w.prefs.markup.track_formatting = false;
        w.agent_only = true;
        w.clamp_selection();
        w
    }
}

/// Does `now` have the content `then` had? What a save writes does not count (the time, the name and
/// the revision count of the last save, the creation time when there was none, the zoom kept with the
/// file): saving while the assistant works changes nothing it worked on. Everything else does, the
/// document's properties included. Content only: an Undo and a Redo give the same document back.
pub fn unchanged_since(now: &Document, then: &Document) -> bool {
    let mut then = then.clone();
    then.core.modified = now.core.modified.clone();
    then.core.last_modified_by = now.core.last_modified_by.clone();
    then.core.revision = now.core.revision;
    if then.core.created.is_empty() {
        then.core.created = now.core.created.clone();
    }
    then.settings.zoom = now.settings.zoom;
    now.same_content(&then)
}

/// `doc` with the times of its tracked changes and comments set aside: two runs of the same steps
/// differ only there.
fn timeless(doc: &Document) -> Document {
    let mut d = doc.clone();
    for r in &mut d.revisions {
        r.date.clear();
    }
    for c in d.comments.values_mut() {
        c.date.clear();
    }
    d
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
            let requests = std::mem::take(&mut ws.ui_requests);
            if !requests.iter().all(only_scrolls) {
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

/// The stories whose text Track Changes covers: the body, headers, footers, notes and text boxes.
/// Comments are not tracked changes of the document, in Word either.
fn tracked_stories(doc: &Document) -> Vec<StoryRef> {
    let mut parts: Vec<u32> = doc.parts.iter().filter(|(_, part)| part.kind != PartKind::Comment).map(|(id, _)| *id).collect();
    parts.sort_unstable();
    std::iter::once(StoryRef::Body).chain(parts.into_iter().map(StoryRef::Part)).collect()
}

/// A fingerprint of the document as it reads with every tracked change rejected: its text without
/// tracked insertions, each object as one mark, and each paragraph mark that is not itself a tracked
/// insertion. A change made with Track Changes on leaves it as it was (what it adds is marked inserted,
/// what it removes stays, marked deleted); a change that is not tracked alters it. Markers (where a
/// comment or a bookmark starts and ends, field and content control bounds) are not text: comments and
/// bookmarks are not tracked changes, in Word either.
fn rejected_fingerprint(doc: &Document) -> u64 {
    let mut h = DefaultHasher::new();
    for story in tracked_stories(doc) {
        h.write(format!("\u{0}{story:?}\u{0}").as_bytes());
        for path in doc.para_paths(story) {
            let Some(p) = doc.para(story, &path) else { continue };
            let inserted: Vec<std::ops::Range<usize>> = p.run_ranges().filter(|(_, c)| c.ins.is_some()).map(|(r, _)| r).collect();
            let mut object = 0;
            let mut buf = [0u8; 4];
            for (i, c) in p.text.char_indices() {
                let marker = c == OBJ && {
                    object += 1;
                    p.objects.get(object - 1).is_some_and(|o| o.is_marker())
                };
                if marker || inserted.iter().any(|r| r.contains(&i)) {
                    continue;
                }
                h.write(c.encode_utf8(&mut buf).as_bytes());
            }
            if p.mark.ins.is_none() {
                h.write(b"\n");
            }
        }
    }
    h.finish()
}

/// What a paragraph costs besides its text, in the measure of [`text_bytes`]: so that empty paragraphs and
/// table cells count too.
const PARAGRAPH_COST: usize = 16;

/// The size of `doc`: the bytes of text in every story, and [`PARAGRAPH_COST`] for each paragraph (a table
/// cell has one at least). What bounds how far one request may grow the document.
fn text_bytes(doc: &Document) -> usize {
    let mut stories = vec![StoryRef::Body];
    stories.extend(doc.parts.keys().map(|id| StoryRef::Part(*id)));
    stories
        .iter()
        .map(|story| doc.para_paths(*story).iter().filter_map(|path| doc.para(*story, path)).map(|p| p.len() + PARAGRAPH_COST).sum::<usize>())
        .sum()
}

/// A paragraph's text as the assistant reads it (every tracked change accepted, hidden text left out unless
/// shown: [`crate::cmd::assistant::shared_para_text`]), with, for each of its bytes, where the character it
/// comes from starts and ends in the paragraph's stored text, so a place found in what the model read is the
/// right place in the paragraph, past any text it was not given. An object's text maps to the whole object.
fn shared_with_offsets(styles: &wordcraft_doc::StyleSheet, p: &wordcraft_doc::Paragraph, shown: bool) -> (String, Vec<usize>, Vec<usize>) {
    let deleted = crate::cmd::assistant::unshared_ranges(styles, p, shown);
    let (mut text, mut starts, mut ends) = (String::new(), Vec::new(), Vec::new());
    let mut object = 0;
    for (i, c) in p.text.char_indices() {
        let piece = if c == OBJ {
            object += 1;
            p.objects.get(object - 1).map(|o| o.plain_text().to_string()).unwrap_or_default()
        } else {
            c.to_string()
        };
        if deleted.iter().any(|r| r.contains(&i)) {
            continue;
        }
        starts.extend(std::iter::repeat_n(i, piece.len()));
        ends.extend(std::iter::repeat_n(i + c.len_utf8(), piece.len()));
        text.push_str(&piece);
    }
    (text, starts, ends)
}

/// What a tool call may change in the workspace besides what it is asked to, kept so that a call that
/// fails, or only asks for a dialog, or is refused after it ran, leaves no trace: every call is either
/// recorded as a step, which the replay repeats, or undone, so the replay gives what the model saw.
struct Before {
    snapshot: crate::EditSnapshot,
    doc: Document,
    sel: Selection,
    pending: Option<wordcraft_doc::props::CharProps>,
    column: Option<crate::ColumnBlock>,
    find: crate::FindState,
    clipboard: (Option<wordcraft_doc::edit::Fragment>, String, String, String),
    originals: HashMap<String, String>,
    picture_looks: HashMap<String, PictureLook>,
    math: Option<crate::MathEdit>,
    also_selected: Vec<Pos>,
    member: Option<crate::cmd::canvas::CanvasMember>,
    widened: Option<crate::doc_state::Widened>,
    pending_move: Option<crate::cmd::moves::PendingMove>,
    review_style: Option<crate::cmd::review::StyleTarget>,
    bib_style: String,
    goal_x: Option<f32>,
    hf_return: Option<Selection>,
}

impl Before {
    fn of(ws: &Session) -> Before {
        Before {
            snapshot: ws.edit_snapshot(),
            doc: ws.doc.clone(),
            sel: ws.sel.clone(),
            pending: ws.pending.clone(),
            column: ws.column.clone(),
            find: ws.find.clone(),
            clipboard: (ws.clipboard.clone(), ws.clipboard_text.clone(), ws.clipboard_html.clone(), ws.clipboard_rtf.clone()),
            originals: ws.originals.clone(),
            picture_looks: ws.picture_looks.clone(),
            math: ws.math.clone(),
            also_selected: ws.also_selected.clone(),
            member: ws.member.clone(),
            widened: ws.widened.clone(),
            pending_move: ws.pending_move.clone(),
            review_style: ws.review_style.clone(),
            bib_style: ws.bib_style.clone(),
            goal_x: ws.goal_x,
            hf_return: ws.hf_return.clone(),
        }
    }

    /// Did the call change the document?
    fn document_changed(&self, ws: &Session) -> bool {
        !ws.doc.same_content(&self.doc)
    }

    /// Did the call change anything a later step depends on?
    fn changed(&self, ws: &Session) -> bool {
        self.document_changed(ws) || ws.sel != self.sel || !self.same_besides_selection(ws)
    }

    /// Is everything but the selection's ends as it was? Then a call that moved the selection is recorded as
    /// the place it went to, which the replay reaches whatever the layout or the word lists say then.
    fn same_besides_selection(&self, ws: &Session) -> bool {
        ws.pending == self.pending
            && ws.column == self.column
            && ws.clipboard == self.clipboard.0
            && ws.math == self.math
            && ws.also_selected == self.also_selected
            && ws.member == self.member
            && ws.widened == self.widened
            && serde_json::to_value(&ws.find).ok() == serde_json::to_value(&self.find).ok()
    }

    fn restore(self, ws: &mut Session) {
        ws.restore(self.snapshot);
        ws.pending = self.pending;
        ws.column = self.column;
        ws.find = self.find;
        (ws.clipboard, ws.clipboard_text, ws.clipboard_html, ws.clipboard_rtf) = self.clipboard;
        ws.originals = self.originals;
        ws.picture_looks = self.picture_looks;
        ws.math = self.math;
        // Restoring the snapshot clears these; they are put back as they were.
        ws.also_selected = self.also_selected;
        ws.member = self.member;
        ws.widened = self.widened;
        ws.pending_move = self.pending_move;
        ws.review_style = self.review_style;
        ws.bib_style = self.bib_style;
        ws.goal_x = self.goal_x;
        ws.hf_return = self.hf_return;
        ws.ui_requests.clear();
    }
}

/// The tools the assistant gets, run on a workspace. Their calls that change something are
/// recorded as steps.
pub struct Toolbox {
    ws: Session,
    /// The document the workspace started from.
    seed_doc: Document,
    /// Where the workspace started ([`Seed::start`]).
    start: Value,
    steps: Vec<Value>,
    calls: usize,
    commands: usize,
    /// Most bytes of text the document may grow to in this request.
    size_limit: usize,
    /// With Track Changes on when the request began: the document's fingerprint with every change
    /// rejected, which every change must leave as it is.
    tracked: Option<u64>,
    /// Set when the request is let go: a batch stops between its commands.
    cancel: Option<Arc<AtomicBool>>,
    /// A limit of the request (tool calls, commands, changes) refused a call.
    limited: bool,
    /// What the workspace started from, to check the steps against ([`Toolbox::verify`]).
    seed: Seed,
    /// Text deleted with Track Changes on that the document reads nowhere else, worked out when needed.
    secrets: Option<Vec<String>>,
}

impl Toolbox {
    /// A toolbox on the workspace of `seed`.
    pub fn new(seed: Seed) -> Toolbox {
        let start = seed.start();
        let seed_doc = seed.doc.clone();
        let size = text_bytes(&seed_doc);
        let size_limit = size.saturating_mul(GROWTH_FACTOR).max(size.saturating_add(GROWTH_ALLOWANCE));
        let tracked = seed_doc.settings.track_changes.then(|| rejected_fingerprint(&seed_doc));
        let ws = seed.clone().workspace();
        Toolbox {
            ws,
            seed_doc,
            start,
            steps: Vec::new(),
            calls: 0,
            commands: 0,
            size_limit,
            tracked,
            cancel: None,
            limited: false,
            seed,
            secrets: None,
        }
    }

    /// Stop a batch between its commands once `flag` is set.
    pub fn set_cancel(&mut self, flag: Arc<AtomicBool>) {
        self.cancel = Some(flag);
    }

    /// The steps that change something, in order, for `assistant.runSteps`.
    pub fn steps(&self) -> &[Value] {
        &self.steps
    }

    /// Where the workspace started, for `assistant.runSteps`'s `start`.
    pub fn start(&self) -> &Value {
        &self.start
    }

    /// Has the workspace's document changed?
    pub fn changed(&self) -> bool {
        !self.ws.doc.same_content(&self.seed_doc)
    }

    /// How many tool calls were made.
    pub fn calls(&self) -> usize {
        self.calls
    }

    /// Did a limit of the request (tool calls, commands, changes) refuse a call? Then the assistant stopped
    /// because it had to, and its work may be partial.
    pub fn hit_limit(&self) -> bool {
        self.limited
    }

    /// The workspace, for tests and for reading what was done.
    pub fn workspace(&self) -> &Session {
        &self.ws
    }

    /// The selected text as it reads with every tracked change accepted, hidden text left out unless it is
    /// shown: what the model is told is selected.
    pub fn selection_text(&self) -> String {
        crate::cmd::assistant::selected_shared_text(&self.ws)
    }

    /// Whether the model may read hidden text: it is shown in the view the request was made from.
    fn hidden_shown(&self) -> bool {
        crate::cmd::assistant::hidden_shown(&self.ws)
    }

    /// A story's text as the model reads it ([`crate::cmd::assistant::shared_story_text`]).
    fn story_text(&self, story: StoryRef) -> String {
        crate::cmd::assistant::shared_story_text(&self.ws.doc, story, self.hidden_shown())
    }

    /// The document in brief for the first message: its body paragraphs and tables by block index,
    /// with style names, as the text reads with every tracked change accepted. At most a budget of
    /// bytes; it says when it is cut.
    pub fn overview(&mut self) -> String {
        let mut out = String::new();
        let shown = self.hidden_shown();
        let doc = &self.ws.doc;
        let blocks = &doc.body;
        for (index, block) in blocks.iter().enumerate() {
            let line = match &**block {
                Block::Table(t) => format!("[{index}] table {}x{}\n", t.rows.len(), t.cols()),
                Block::Para(p) => format!(
                    "[{index}] ({}) {}\n",
                    p.props.style.as_deref().unwrap_or("Normal"),
                    crate::cmd::assistant::shared_para_text(&doc.styles, p, shown)
                ),
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
        let pos = json!({"type": "object", "description": "A position: {\"block\": body block index, \"off\": byte offset in that paragraph's stored text}, or {\"story\": \"body\", \"path\": [...], \"off\": n} as find, get_selection and inspect_document report it."});
        let story = json!({"description": "The story to read: \"body\" (the default) or {\"part\": id} for a header, footer, note or text box, as inspect_document lists them with parts: true."});
        vec![
            json!({"name": "get_text", "description": "The text of the body, or of the story you name, as it reads with every tracked change accepted (text deleted with Track Changes on is left out), and without text formatted as hidden unless the person shows hidden text.", "input_schema": {"type": "object", "properties": {"story": story}}}),
            json!({"name": "inspect_document", "description": "The document's structure: body blocks by index (paragraphs with style, text, list, paragraph properties and formatted runs; tables with their cells' text), the selection, sections, whether Track Changes is on, the word count. Text reads as with every tracked change accepted, and without hidden text unless the person shows it. Offsets (runs, selection ends) count the paragraph's stored text, which also holds text deleted with Track Changes on, hidden text and one character for each object, so select by words with select_text rather than by computing offsets. Returns at most 150 blocks from `from`; parts (headers, footers, notes, comments, text boxes) only with parts: true.", "input_schema": {"type": "object", "properties": {"from": {"type": "integer", "minimum": 0, "description": "First body block index (default 0)."}, "count": {"type": "integer", "minimum": 1, "maximum": 150, "description": "How many blocks (default 150)."}, "parts": {"type": "boolean", "description": "Include headers, footers, notes, comments and text boxes."}}}}),
            json!({"name": "get_selection", "description": "The current selection: its ends, its text as it reads with every tracked change accepted (hidden text left out unless the person shows it), and its formatting.", "input_schema": {"type": "object", "properties": {}}}),
            json!({"name": "select_text", "description": "Select an occurrence of text as the document reads (text deleted with Track Changes on, and hidden text you were not given, is skipped), in the story the selection is in, usually the body. 1-based occurrence, default the first.", "input_schema": {"type": "object", "properties": {"text": {"type": "string"}, "occurrence": {"type": "integer", "minimum": 1}}, "required": ["text"]}}),
            json!({"name": "find", "description": "Find text in the story the selection is in, usually the body, and select the first match after the caret. Returns the match ranges (at most 200). It searches the stored text, in which text deleted with Track Changes on and hidden text are still kept, so prefer select_text to select words you read.", "input_schema": {"type": "object", "properties": {"text": {"type": "string"}, "matchCase": {"type": "boolean"}, "wholeWord": {"type": "boolean"}, "regex": {"type": "boolean"}}, "required": ["text"]}}),
            json!({"name": "select_range", "description": "Select from anchor to focus. A collapsed range puts the caret there.", "input_schema": {"type": "object", "properties": {"anchor": pos, "focus": pos}, "required": ["anchor", "focus"]}}),
            json!({"name": "type_text", "description": "Type text at the selection, replacing what is selected, exactly as given (no AutoCorrect). With paragraphs: true each line becomes its own paragraph. Tracked as an insertion (and a deletion of what it replaces) when Track Changes is on.", "input_schema": {"type": "object", "properties": {"text": {"type": "string"}, "paragraphs": {"type": "boolean"}}, "required": ["text"]}}),
            json!({"name": "replace_all", "description": "Replace every match of `text` with `with` in the story the selection is in, usually the body. Tracked when Track Changes is on.", "input_schema": {"type": "object", "properties": {"text": {"type": "string"}, "with": {"type": "string"}, "matchCase": {"type": "boolean"}, "wholeWord": {"type": "boolean"}, "regex": {"type": "boolean"}}, "required": ["text", "with"]}}),
            json!({"name": "insert_table", "description": "Insert a table at the selection from rows of values (at most 1000 rows by 63 columns). With Track Changes on it is refused if the new table cannot be tracked.", "input_schema": {"type": "object", "properties": {"values": {"type": "array", "items": {"type": "array", "items": {"type": ["string", "number", "boolean", "null"]}}}, "header": {"type": "boolean", "description": "The first row is a header row."}, "style": {"type": "string", "description": "A table style id, for example GridTable1Light."}}, "required": ["values"]}}),
            json!({"name": "list_commands", "description": "Search the WordCraft commands you may run with execute, by words in their id, label, ribbon place or parameters. Returns id, label, place and the parameters each takes (at most 40).", "input_schema": {"type": "object", "properties": {"query": {"type": "string"}, "limit": {"type": "integer", "minimum": 1, "maximum": 40}}, "required": ["query"]}}),
            json!({"name": "execute", "description": "Run one WordCraft command on the document, with its parameters as list_commands describes them. Only commands that read or edit the open document are allowed.", "input_schema": {"type": "object", "properties": {"command": {"type": "string"}, "params": {"type": "object"}}, "required": ["command"]}}),
            json!({"name": "batch", "description": "Run up to 50 commands in order. Stops at the first failure unless keepGoing is true; earlier commands stay done.", "input_schema": {"type": "object", "properties": {"commands": {"type": "array", "maxItems": 50, "items": {"type": "object", "properties": {"command": {"type": "string"}, "params": {"type": "object"}}, "required": ["command"]}}, "keepGoing": {"type": "boolean"}}, "required": ["commands"]}}),
        ]
    }

    /// Run tool `name` with `input`. Never panics: an unknown tool, a refused command or a failure
    /// is an error result the model reads.
    pub fn call(&mut self, name: &str, input: &Value) -> ToolResult {
        self.calls += 1;
        if self.calls > MAX_CALLS {
            self.limited = true;
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
            "get_text" => {
                let story = input.get("story").filter(|v| !v.is_null()).cloned().unwrap_or_else(|| json!("body"));
                let params = json!({"story": story});
                // `document.text` reads the text with hidden text; the model reads it as the pane shares it.
                self.run("document.text", params.clone()).map(|mut value| {
                    value["text"] = json!(self.story_text(crate::cmd::story_param(&self.ws, &params)));
                    value
                })
            }
            "get_selection" => self.selection(),
            "inspect_document" => self.inspect(input),
            "find" => self.run("edit.find", pick(&["text", "matchCase", "wholeWord", "regex"])),
            "select_text" => self.select_text(input),
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
                Some(id) if let Some(tool) = wrapped_by(id) => Err(format!("{id} is read through the {tool} tool")),
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
            Ok(value) => {
                let shown = ToolResult::ok(&value);
                let content = shown.content.clone();
                let leaks = self.secrets().iter().any(|secret| {
                    let escaped = serde_json::to_string(secret).unwrap_or_default();
                    content.contains(secret.as_str()) || content.contains(escaped.trim_matches('"'))
                });
                if leaks {
                    return ToolResult::error(
                        "this result would show text deleted with Track Changes on or formatted as hidden, which you may not read; read the document with get_text, inspect_document or get_selection",
                    );
                }
                shown
            }
            Err(why) => ToolResult::error(why),
        }
    }

    /// Run one command in the workspace, file access refused, and record it as a step when it changed
    /// something; or, when it failed, only opened a dialog, would grow the document past its bound or
    /// make a change Track Changes cannot record, put everything back and say why.
    fn run(&mut self, id: &str, params: Value) -> Result<Value, String> {
        if let Some(why) = refusal(id, &params) {
            return Err(format!("{id} is not available to you: {why}"));
        }
        self.commands += 1;
        if self.commands > MAX_COMMANDS {
            self.limited = true;
            return Err(format!("this request has run its {MAX_COMMANDS} commands; stop and say what is left to do"));
        }
        let reader = is_reader(id);
        if !reader && self.steps.len() >= MAX_STEPS {
            self.limited = true;
            return Err(format!("this request has made the most changes one request may make ({MAX_STEPS}); stop and say what is left to do"));
        }
        if self.tracked.is_some() && self.ws.doc.settings.track_changes && untrackable(id) {
            return Err(format!(
                "{id}: with Track Changes on, WordCraft does not track what this command changes (field results, equations), so it is not run. Leave it and say so."
            ));
        }
        let before = Before::of(&self.ws);
        let result = confined(|| self.ws.run(id, &params));
        let requests = std::mem::take(&mut self.ws.ui_requests);
        let value = match result {
            Err(e) => {
                before.restore(&mut self.ws);
                return Err(e.to_string());
            }
            Ok(_) if !requests.iter().all(only_scrolls) => {
                before.restore(&mut self.ws);
                return Err(format!("{id} only opens a dialog when called like this; give it its parameters instead (see list_commands)"));
            }
            Ok(value) => value,
        };
        if before.document_changed(&self.ws) {
            let size = text_bytes(&self.ws.doc);
            if size > self.size_limit {
                before.restore(&mut self.ws);
                self.limited = true;
                return Err(format!(
                    "{id} would make the document's text {size} bytes, more than this request may grow it to ({}), so it was not made",
                    self.size_limit
                ));
            }
            if let Some(fingerprint) = self.tracked
                && self.ws.doc.settings.track_changes
                && rejected_fingerprint(&self.ws.doc) != fingerprint
            {
                before.restore(&mut self.ws);
                return Err(format!(
                    "{id}: with Track Changes on, this change cannot be tracked, so it was not made. Change the text with type_text, replace_all or deletions instead, or leave it and say so."
                ));
            }
        }
        let document_changed = before.document_changed(&self.ws);
        if !document_changed && self.ws.sel != before.sel && before.same_besides_selection(&self.ws) {
            // It only moved the selection: the step is the place it went to, which the replay reaches
            // whatever the layout or the word lists say then.
            let (anchor, focus) = (pos_json(&self.ws.sel.anchor), pos_json(&self.ws.sel.focus));
            self.steps.push(json!({"command": "select.range", "params": {"anchor": anchor, "focus": focus}}));
        } else if !reader || before.changed(&self.ws) {
            self.steps.push(json!({"command": id, "params": params}));
        }
        if document_changed {
            self.secrets = None;
        }
        Ok(self.shown(id, value))
    }

    /// What the model is shown of a command's result: never the copied text in markup (a copy can hold
    /// text deleted with Track Changes on), comments as they read with every change accepted.
    fn shown(&self, id: &str, mut value: Value) -> Value {
        match id {
            "edit.copy" | "edit.cut" => {
                let copied = self.ws.clipboard.as_ref().map_or(0, |c| {
                    let mut d = c.to_document();
                    d.styles = self.ws.doc.styles.clone();
                    crate::cmd::assistant::shared_story_text(&d, StoryRef::Body, self.hidden_shown()).chars().count()
                });
                json!({"done": id, "copied": copied})
            }
            "review.comments" => {
                if let Some(list) = value.as_array_mut() {
                    for entry in list.iter_mut() {
                        let part = entry
                            .get("id")
                            .and_then(Value::as_u64)
                            .and_then(|id| u32::try_from(id).ok())
                            .and_then(|id| self.ws.doc.comments.get(&id))
                            .map(|c| c.part);
                        entry["text"] = json!(part.map(|part| self.story_text(StoryRef::Part(part))).unwrap_or_default());
                    }
                }
                value
            }
            _ => value,
        }
    }

    /// The text deleted with Track Changes on, and the hidden text unless it is shown, that the document
    /// reads nowhere else: no result may carry it to the model. Pieces shorter than [`SECRET_MIN_CHARS`]
    /// letters are left out, as words that short would match too much.
    fn secrets(&mut self) -> &[String] {
        if self.secrets.is_none() {
            let shown = self.hidden_shown();
            let doc = &self.ws.doc;
            let mut stories = vec![StoryRef::Body];
            stories.extend(doc.parts.keys().map(|id| StoryRef::Part(*id)));
            let readable: String = stories.iter().map(|st| crate::cmd::assistant::shared_story_text(doc, *st, shown)).collect::<Vec<_>>().join("\n");
            let mut found: Vec<String> = Vec::new();
            for story in &stories {
                for path in doc.para_paths(*story) {
                    let Some(p) = doc.para(*story, &path) else { continue };
                    for range in crate::cmd::assistant::unshared_ranges(&doc.styles, p, shown) {
                        let piece: String = p.text.get(range).unwrap_or_default().chars().filter(|c| *c != OBJ).collect();
                        let piece = piece.trim();
                        if piece.chars().filter(|c| c.is_alphanumeric()).count() >= SECRET_MIN_CHARS && !readable.contains(piece) {
                            found.push(piece.to_string());
                        }
                    }
                }
            }
            found.sort();
            found.dedup();
            self.secrets = Some(found);
        }
        self.secrets.as_deref().unwrap_or_default()
    }

    /// Replay the steps on a fresh workspace of the seed and compare with this workspace, the times of
    /// tracked changes and comments set aside: what will be applied must be what the model saw and
    /// described. Says why not when it is not.
    pub fn verify(&self) -> Result<(), String> {
        let replayed = replay(self.seed.clone(), &self.steps)?;
        if replayed.sel != self.ws.sel || !timeless(&replayed.doc).same_content(&timeless(&self.ws.doc)) {
            return Err("replaying the steps gives a different document than the assistant worked on".into());
        }
        Ok(())
    }

    /// The selection, its text read as with every tracked change accepted.
    fn selection(&mut self) -> Result<Value, String> {
        let mut value = self.run("document.selection", json!({}))?;
        value["text"] = json!(self.selection_text());
        Ok(value)
    }

    /// Select the `occurrence`th match of `text` in the text as it reads, in the selection's story, through
    /// `select.range`, so the step names the exact place.
    fn select_text(&mut self, input: &Value) -> Result<Value, String> {
        let needle = input.get("text").and_then(Value::as_str).filter(|t| !t.is_empty()).ok_or("select_text needs `text`")?;
        let n = input.get("occurrence").and_then(Value::as_u64).map_or(1, |v| v.max(1) as usize);
        let story = self.ws.sel.focus.story;
        let shown = self.hidden_shown();
        let mut count = 0;
        let mut found = None;
        'paragraphs: for path in self.ws.doc.para_paths(story) {
            let Some(p) = self.ws.doc.para(story, &path) else { continue };
            let (text, starts, ends) = shared_with_offsets(&self.ws.doc.styles, p, shown);
            for (i, _) in text.match_indices(needle) {
                count += 1;
                if count == n {
                    let anchor = Pos { story, path: path.clone(), off: starts[i] };
                    let focus = Pos { story, path: path.clone(), off: ends[i + needle.len() - 1] };
                    found = Some((anchor, focus));
                    break 'paragraphs;
                }
            }
        }
        let Some((anchor, focus)) = found else {
            return Err(format!("`{needle}` not found{}", if count > 0 { format!(" {n} times (it occurs {count} times)") } else { String::new() }));
        };
        self.run("select.range", json!({"anchor": pos_json(&anchor), "focus": pos_json(&focus)}))
    }

    fn inspect(&mut self, input: &Value) -> Result<Value, String> {
        let mut full = self.run("document.inspect", json!({"text": true}))?;
        let shown = self.hidden_shown();
        let doc = &self.ws.doc;
        let para_text = |p: &wordcraft_doc::Paragraph| crate::cmd::assistant::shared_para_text(&doc.styles, p, shown);
        // The text as it reads with every tracked change accepted: deleted text goes nowhere, and hidden
        // text only where it is shown.
        if let Some(blocks) = full.get_mut("blocks").and_then(Value::as_array_mut) {
            for block in blocks.iter_mut() {
                let Some(index) = block.get("index").and_then(Value::as_u64).map(|i| i as usize) else { continue };
                match doc.body.get(index).map(|b| &**b) {
                    Some(Block::Para(p)) => {
                        block["text"] = json!(para_text(p));
                        // The objects in deleted or hidden text (a field's code and result, an equation) are
                        // deleted or hidden text too.
                        if block.get("objects").is_some() {
                            let deleted = crate::cmd::assistant::unshared_ranges(&doc.styles, p, shown);
                            let kept: Vec<Value> = p
                                .text
                                .char_indices()
                                .filter(|(_, c)| *c == OBJ)
                                .zip(p.objects.iter())
                                .filter(|((at, _), _)| !deleted.iter().any(|r| r.contains(at)))
                                .map(|(_, o)| serde_json::to_value(o).unwrap_or(Value::Null))
                                .collect();
                            block["objects"] = json!(kept);
                        }
                    }
                    Some(Block::Table(t)) => {
                        let cells: Vec<Vec<String>> = t
                            .rows
                            .iter()
                            .map(|r| {
                                r.cells
                                    .iter()
                                    .map(|c| c.blocks.iter().filter_map(|b| b.as_para().map(para_text)).collect::<Vec<_>>().join("\n"))
                                    .collect()
                            })
                            .collect();
                        block["cells"] = json!(cells);
                    }
                    None => {}
                }
            }
        }
        if let Some(parts) = full.get_mut("parts").and_then(Value::as_array_mut) {
            for part in parts.iter_mut() {
                if let Some(id) = part.get("id").and_then(Value::as_u64).and_then(|id| u32::try_from(id).ok()) {
                    part["text"] = json!(crate::cmd::assistant::shared_story_text(doc, StoryRef::Part(id), shown));
                }
                // A part's fields are listed with their codes and results as stored.
                if let Some(map) = part.as_object_mut() {
                    map.remove("fields");
                    map.remove("fieldsTruncated");
                }
            }
        }
        if full.get("selection").is_some() {
            full["selection"]["text"] = json!(self.selection_text());
        }
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
        if commands.len() > MAX_BATCH {
            return Err(format!("a batch runs at most {MAX_BATCH} commands; this one has {}: split it", commands.len()));
        }
        let keep_going = input.get("keepGoing").and_then(Value::as_bool).unwrap_or(false);
        let mut results = Vec::new();
        for c in commands {
            if self.cancel.as_ref().is_some_and(|flag| flag.load(Ordering::SeqCst)) {
                results.push(json!({"error": "the request was stopped"}));
                break;
            }
            let Some(id) = c.get("command").and_then(Value::as_str) else {
                results.push(json!({"error": "a command names no `command`"}));
                if keep_going {
                    continue;
                }
                break;
            };
            let params = c.get("params").cloned().filter(|p| !p.is_null()).unwrap_or_else(|| json!({}));
            let outcome = match wrapped_by(id) {
                Some(tool) => Err(format!("{id} is read through the {tool} tool")),
                None => self.run(id, params),
            };
            match outcome {
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
