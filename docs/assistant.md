# AI assistant

> **Last reviewed:** 2026-11-26 · **Last updated:** 2026-11-26 · **Change:** major (first version) · **Target:** WordCraft main after 0.19.1

Review › AI › **AI Assistant** opens a side pane where you ask for a change to your text, or a question
about it, and put the reply into the document. The text is sent to the Claude Messages API with your own
API key. Nothing is sent until you have agreed to it in the pane, and a reply is placed only when you press
a button.

## Using it

1. Select the text, or tick **Use the whole document instead of the selection**.
2. Give the key: type it in **API key** (kept in memory for this run, never saved), or set the
   `ANTHROPIC_API_KEY` environment variable before starting WordCraft. A typed key is used first.
3. Write the **Instruction** and press **Send**.
4. The first time in a run, and again whenever a request would share more than you agreed to, the pane says
   who receives the text, which model reads it, how many characters go and how the text starts. **Agree and
   Send** sends it; **Cancel** sends nothing. Agreeing to the whole document covers the selection too.
5. While the request runs you can press **Stop Waiting**. The request has already been sent and the service may
   still process it; its answer is dropped.
6. The reply shows with **Replace Selection**, **Insert After** and **Copy**. Placing is one undo step, tracked when
   Track Changes is on, and is typed as written: no AutoCorrect, AutoFormat or smart quotes. A reply to the
   selection is placed only while that text is unchanged: if you moved the selection or edited the document
   since the request, nothing is placed and the pane says so (select the same text again to place it).

**New Chat** forgets the conversation. A new document starts a new chat; what you agreed to and the key you
typed stay for the run.

## What leaves the machine

- Your instruction, the earlier turns of this chat (at most 24 turns and 300 KB) and the text of the scope you
  chose (at most 600 KB). Too much is refused with a message, never cut.
- Track Changes is read as accepted: text deleted with tracking on is **not** sent, in either scope. Text
  formatted as hidden **is** sent, and counted. Headers, footers, notes and text boxes are not part of the whole
  document scope.
- The request goes to `api.anthropic.com` over HTTPS and only there: redirects are not followed and a plain HTTP
  endpoint is refused before any connection. The service may have another of its models read the text when the
  first model declines (the request asks for the service's default fallbacks); the agreement step says so.
- WordCraft keeps no copy and writes no log of the text or the reply. The key is held in memory, wiped when it is
  dropped, sent in one header, and never printed, logged, saved in settings or put in a document or an error.
- Your agreement is for this run only. It is never saved, and no command, setting or file can give it: only the
  button in the pane.

## For agents and scripts

An agent that drives WordCraft through MCP, the CLI or the control channel brings its own model, so it does not
need the pane and cannot use it to send text anywhere. Two engine commands serve it:

- `assistant.context {"scope": "selection" | "document"}` returns exactly the text the pane would share for that
  scope, with its size. It changes nothing.
- `assistant.apply {"text": "…", "mode": "replaceSelection" | "insertAfter"}` puts text into the document as
  written, as one undo step.

`view.assistantPane` opens and closes the pane. Sending a request is deliberately not a command: it needs the
person's agreement, which only the pane's button gives (AGENTS.md rule 14 asks that every feature work without the
GUI; this one step cannot, by design).

## What it does not do

- It does not work in the browser version, which cannot send requests: the pane says so and offers nothing else.
- The reply is read whole, within 16,000 tokens, so a very long rewrite of a whole document can be cut off. The
  pane says when a reply was cut off, ended for another reason, or was declined, and nothing is placed from a
  declined reply. Longer replies need streaming, which is not built.
- A request is tried once: a busy or dropped connection is shown as an error and you send again.
- The pane's text is English; translations are added in craft-i18n. In a right-to-left interface the reply text
  takes the interface's direction, not the text's. The text placed in the document keeps its own direction.
- It was tested with a mock provider and canned replies. No request has been sent to the real service in these
  tests, so the request and reply shapes follow the API documentation and have not been confirmed against it.

## Tests

`crates/assistant` (request building, reply parsing, the mock, the Claude provider against a server on this
machine, including that it refuses plain HTTP and never prints the key), `crates/engine/src/tests/assistant.rs`
(the two commands) and `crates/ui-egui/src/assistant_pane/tests.rs` (agreement, scope, key, stop, announcements,
placement guard, drawn states, right-to-left layout).

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-26 | major | First version: the AI pane, `assistant.context`, `assistant.apply`, `view.assistantPane` |
