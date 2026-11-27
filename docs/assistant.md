# AI assistant

> **Last reviewed:** 2026-11-27 · **Last updated:** 2026-11-27 · **Change:** major (the assistant changes the document with WordCraft's commands) · **Target:** WordCraft 0.19.1 Hebrew edition

Review › AI › **AI Assistant** opens a side pane where you ask for a change to your document, or a question
about it. The text is sent to the Claude Messages API with your own API key. Nothing is sent until you have
agreed to it in the pane.

The assistant works in one of two ways, chosen with **Let the assistant make changes in the document** (on
by default):

- **It makes the changes itself.** The assistant reads the document and changes it with WordCraft's own
  editing commands (styles, formatting, tables, comments, text). It works on a private copy; when it finishes,
  all of its changes are applied to your document together, as one undo step, and **Undo These Changes** (or
  Undo) takes all of them back. With Track Changes on, the text it adds or deletes is tracked under the name
  AI Assistant; its formatting is applied directly. See [Changes made by the assistant](#changes-made-by-the-assistant).
- **It answers with text.** The reply shows in the pane, and you place it with a button.

## Using it

1. Select the text, or tick **Use the whole document instead of the selection** (text answers only; when the
   assistant makes changes it can read the whole document).
2. Give the key: type it in **API key**, or set the `ANTHROPIC_API_KEY` environment variable before
   starting WordCraft. A typed key is used first. By default the key is kept in memory for this run only.
   To keep it between runs, tick **Remember this key on this computer**: once the service has accepted the
   key, that key is written to the file `assistant-key` in WordCraft's settings folder (on Linux
   `~/.config/wordcraft`), and the next run uses it. The pane says that the file is **not encrypted** before you
   tick the box and after (WordCraft has no system keyring to use), so anyone who can run programs as you can
   read it: do not tick it on a shared account. On Linux and macOS the file is created readable by your user
   alone (mode 0600) and a saved key whose file other users can read is not used, and the pane says so and
   shows where the file is. On Windows nothing is set or checked: the file takes the access rules of its
   folder. **Forget Saved Key** deletes the file and the pane's copy of the key, and unticking the box deletes
   the file and keeps the key for this run; if the file cannot be removed the pane says so and keeps the box
   ticked, so you can try again. A run started with `WORDCRAFT_NO_PREFS` neither reads nor writes the file and
   offers no way to keep a key.
3. Write the **Instruction** and press **Send**.
4. The first time in a run, and again whenever a request would share more than you agreed to, the pane says
   who receives the text, which model reads it, how many characters go and how the text starts. **Agree and
   Send** sends it; **Cancel** sends nothing. Agreeing to the whole document covers a selection too, in any
   document; agreeing to a selection covers later selections, of any size, but a request that sends the whole
   document asks again. The pane says which of the two applies.
5. While the request runs you can press **Stop Waiting**. The request has already been sent and the service may
   still process it; its answer is dropped.
6. The reply shows with **Replace Selection**, **Insert After** and **Copy**. Placing is one undo step, tracked when
   Track Changes is on, and is typed as written: no AutoCorrect, AutoFormat or smart quotes. A reply to the
   selection is placed only while that text is unchanged: if you moved the selection or edited the document
   since the request, nothing is placed and the pane says so. After a move, select the same text again; after
   an edit the reply no longer fits, so ask again. A request made with nothing selected is not tied to the
   caret, so its reply can be placed wherever the caret is.

**New Chat** forgets the conversation. A new document starts a new chat; what you agreed to and the key you
typed stay for the run.

## Changes made by the assistant

- **What it may do.** Only commands that read or edit the open document: text, formatting, paragraphs,
  styles, lists, tables, inserts of content, fields, comments, headers and footers, page setup, find and
  replace. It may not open, save, export or print files, read or write any file (its work runs with every
  file access refused), use the network, change preferences or protection, record or play macros, open
  dialogs, turn Track Changes off, or accept or reject anyone's changes. The engine refuses these inside its
  command runner, so a command that runs other commands cannot reach them either (`crates/engine/src/agent.rs`).
- **How it applies.** Its steps are recorded while it works and replayed on a fresh copy of your document by
  the command `assistant.runSteps`, whose result replaces the document as one undo step. If you edited the
  document while it worked, nothing is applied and the pane says so: ask again.
- **When it does not finish** (it declines, reaches its length limit, uses all the steps one request may take,
  or you press **Stop Waiting**), nothing is applied and the pane says why.
- **Limits.** One request may take up to 25 rounds with the model and 60 tool calls; each tool result is cut at
  30 KB and says so. Each round is a separate request to the service, so a long task costs more than a text
  answer, and the earlier turns are cached to keep that down.
- **The conversation.** The model's turns are sent back to it byte for byte, as the API requires for tool use.

## What leaves the machine

- Your instruction, the earlier turns of this chat (at most 24 turns and 300 KB) and the text of the scope you
  chose (at most 600 KB). Too much is refused with a message, never cut.
- When the assistant makes the changes itself: your instruction, the selected text, an overview of the
  document's paragraphs (at most 60 KB), and whatever its tools read: any part of the document, including
  headers, footers, notes, comments with their authors' names and the document's properties. The agreement
  says so, and is asked once per run for this mode.
- Track Changes is read as accepted: text deleted with tracking on is **not** sent, in either scope. Text
  formatted as hidden **is** sent, and counted. Headers, footers, notes and text boxes are not part of the whole
  document scope.
- The request goes to `api.anthropic.com` over HTTPS and only there: redirects are not followed and a plain HTTP
  endpoint is refused before any connection. The service may have another of its models read the text when the
  first model declines (the request asks for the service's default fallbacks); the agreement step says so.
- WordCraft keeps no copy and writes no log of the text or the reply. The key is held in memory, sent in one
  header, and never printed, logged or put in a document or an error. The pane's long-lived copies (the key
  you typed and the saved one) are wiped when they are dropped, and when you forget the key the key box's undo
  history is cleared too. The copy the provider makes for each request and the buffer the saved file is read
  through are ordinary memory, and a program cannot reach every copy a toolkit or the operating system may
  make, such as swapped-out memory. The key is written to disk
  only if you tick Remember this key (see Using it), and then only in that one file, never in the settings file
  `ui.json`.
- Your agreement is for this run only. It is never saved, and no command, setting or file can give it: only the
  button in the pane.

## For agents and scripts

An agent that drives WordCraft through MCP, the CLI or the control channel brings its own model, so it does not
need the pane. No command, MCP tool or setting can send text to the service or give the agreement. A program that
holds the control channel's token can still press the pane's buttons as pointer input, like a person: the
agreement protects against commands, MCP and settings, not against that kind of GUI automation, which is why
the token is private to the user. Two engine commands serve an agent:

- `assistant.context {"scope": "selection" | "document"}` returns exactly the text the pane would share for that
  scope, with its size. It changes nothing.
- `assistant.apply {"text": "…", "mode": "replaceSelection" | "insertAfter"}` puts text into the document as
  written, as one undo step.
- `assistant.runSteps {"steps": [{"command", "params"}], "author"?}` runs a list of document-editing commands
  on a private copy (only what an assistant may run, with no file access) and, when every step succeeds, puts
  the result in as one undo step; else nothing changes. Text the steps add or delete is tracked when Track
  Changes is on; formatting applies directly.

`view.assistantPane` opens and closes the pane. Sending a request is deliberately not a command: it needs the
person's agreement, which only the pane's button gives (AGENTS.md rule 14 asks that every feature work without the
GUI; this one step cannot, by design).

## What it does not do

- It does not work in the browser version, which cannot send requests: the pane says so and offers nothing else.
- The reply is read whole, within 16,000 tokens, so a very long rewrite of a whole document can be cut off. The
  pane says when a reply was cut off, ended for another reason, or was declined, and nothing is placed from a
  declined reply. Longer replies need streaming, which is not built.
- A request is tried once: a busy or dropped connection is shown as an error and you send again.
- The pane's text is English; translations are added in craft-i18n (the Hebrew edition carries its own Hebrew, see
  [localization-he.md](localization-he.md)). In a right-to-left interface the reply text
  takes the interface's direction, not the text's. The text placed in the document keeps its own direction.
- It was tested with a mock provider and canned replies. No request has been sent to the real service in these
  tests, so the request and reply shapes follow the API documentation and have not been confirmed against it.

## Tests

`crates/assistant` (request building, reply parsing, the mock, the Claude provider against a server on this
machine, including that it refuses plain HTTP and never prints the key), `crates/engine/src/tests/assistant.rs`
(the two commands) and `crates/ui-egui/src/assistant_pane/tests.rs` (agreement, scope, key and saved key, stop,
announcements, focus, placement guard, drawn states, and the pane's side and button order in a right-to-left
interface).

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-27 | major | The assistant changes the document with WordCraft's commands (tools), one undo step; `assistant.runSteps` |
| 2026-11-26 | major | First version: the AI pane, `assistant.context`, `assistant.apply`, `view.assistantPane` |
