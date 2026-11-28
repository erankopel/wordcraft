# AI assistant

> **Last reviewed:** 2026-11-28 · **Last updated:** 2026-11-28 · **Change:** minor (review fixes: the empty instruction box's caret, a chat's history bounded and let go a run at a time, the token use of failed and stopped requests, a choice that could not be saved said outside Settings, a cleared saved key) · **Target:** WordCraft 0.19.1 Hebrew edition

Review › AI › **AI Assistant** opens a side pane, docked on the right of the window, where you ask for a change
to your document, or a question about it. The text is sent to the Claude Messages API with your own API key.
Nothing is sent until you have agreed to it in the pane, once.

The pane opens on the window's right in every interface language, the Hebrew one included, where Comments and
the other panes that open at the end edge open on the left. You can move it like any pane (Dock Left, Dock Right,
Float, or drag it), and the place you choose is kept. Only a place you chose is saved in `ui.json`
(`panePlaces.assistant.dock`; resizing the pane saves no place), so an AI pane you never moved opens on the
right after updating from a build that opened it on the left.

The assistant works in one of two ways, chosen with **Let the assistant make changes in the document** (on
by default):

- **It makes the changes itself.** The assistant reads the document and changes it with WordCraft's own
  editing commands (styles, formatting, tables, comments, text). It works on a private copy; when it finishes,
  all of its changes are applied to your document together, as one undo step, and **Undo These Changes** (or
  Undo) takes all of them back. With Track Changes on, the text it adds or deletes is tracked under the name
  AI Assistant, and a change that Track Changes cannot record is not made; its formatting is applied directly.
  See [Changes made by the assistant](#changes-made-by-the-assistant).
- **It answers with text.** The reply shows in the pane, and you place it with a button.

## Using it

Until the one-time setup is done, the pane shows it first: an introduction, the key and its notes, and the
options. The setup is done when a key needs no typing (a saved key that could be read, or
`ANTHROPIC_API_KEY`) and you have agreed (step 4), and, within a run, as soon as the service has accepted a
key (the first request that got an answer other than an authentication error). From then on the pane shows
only the instruction box, **Send** (and **Stop Waiting** while a request runs), **New Chat**, what the
conversation needs (the status line, notices, the agreement when one is needed, errors, the reply and its
buttons) and a **Settings** button. Settings shows, in place, the key box and its notes, **Remember this key
on this computer**, **Forget Saved Key**, the two options, what you agreed to with **Withdraw Agreement**, and
the last request's token use. Its state (expanded or collapsed) is exposed to screen readers, and drawn as an
arrow that points down while it is open. When the setup goes, the keyboard stays where it was (Send after Stop Waiting, or the
instruction box when it was on a control that goes) and the status line, a live region, says that the key and
the options are now under Settings. A key the service refuses, a problem with the saved key's file, or a request
that finds no key at all (a saved key cleared from the key box, with no `ANTHROPIC_API_KEY`) brings the setup
back with its message.

1. Select the text, or tick **Use the whole document instead of the selection** (text answers only; when the
   assistant makes changes it can read the whole document). The two options are kept between runs (see
   step 4 for where).
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
3. Write the **Instruction** and press **Send**. In a right-to-left interface every paragraph of the
   instruction box is right to left, whatever letter it starts with, and its text, hint and caret start at
   the box's right edge; in a left-to-right interface each paragraph takes the direction of its first strong
   letter, and a right-to-left one ends at the right edge. Mixed Hebrew, English, numbers and punctuation are
   ordered by the Unicode bidirectional algorithm, and the arrow keys move on screen, Home and End to the
   row's start and end, as in the document.
4. Before the first request, the pane says who receives the text, which model reads it, everything a request
   may send in either way of working (your instruction, the earlier messages of the chat, the selection or the
   whole document, and, when the assistant makes changes, the whole document and its other parts), that a
   fallback model may read it if the model declines, and that WordCraft keeps no copy; and, for this request,
   how many characters go and how the text starts. **Agree and Send** sends it; **Cancel** sends nothing.
   The agreement is kept, with the two options, in the file `assistant.json` in WordCraft's settings folder
   (on Linux `~/.config/wordcraft`), so later requests, documents and runs send without asking, each when you
   press Send. A run with `WORDCRAFT_NO_PREFS` keeps them for the run only, and so does a run that cannot
   write the file: the pane then says so under the status line, also while Settings is closed (agreeing ends
   the setup, so Settings is closed then), and a withdrawal that could not be written is said too, since a
   later run may then still send without asking. The file is not `ui.json`: resetting WordCraft's settings by
   removing `ui.json` keeps the agreement and the options; withdraw it in the pane, or remove `assistant.json`
   too. (The choices are kept apart because each is written the moment it changes and read again before a
   request goes without asking, while `ui.json` is mostly written when WordCraft quits.) The agreement names the
   recipient and the model: a request to
   another one asks again. Settings says what you agreed to, and **Withdraw Agreement** takes it back: the
   next request asks again. When several WordCraft processes run at once (opening a file from the file
   manager starts another), each change is written over what the file holds, one choice at a time, so one
   process never brings back an agreement another withdrew; and a request that would be sent without asking
   reads the agreement from the file first, so a withdrawal in one process holds in the others.
5. While the request runs you can press **Stop Waiting**. The request has already been sent and the service may
   still process it; its answer is dropped, and nothing is sent again, also when the request was waiting to be
   retried.
6. The reply shows with **Replace Selection**, **Insert After** and **Copy**, paragraph by paragraph in each
   paragraph's own direction (from its first strong letter: a Hebrew paragraph from the right, an English one
   from the left, in either interface). Placing is one undo step, tracked when
   Track Changes is on, and is typed as written: no AutoCorrect, AutoFormat or smart quotes. A reply to the
   selection is placed only while that text is unchanged: if you moved the selection or edited the document
   since the request, nothing is placed and the pane says so. After a move, select the same text again; after
   an edit the reply no longer fits, so ask again. A request made with nothing selected is not tied to the
   caret, so its reply can be placed wherever the caret is.

**New Chat** forgets the conversation. A new document starts a new chat; what you agreed to and the key you
typed stay.

When a request that changes the document finishes while the pane is closed, the pane opens again (without taking
the keyboard) so what happened is shown and spoken, and the status bar says it too.

## Changes made by the assistant

- **What it may do.** Only commands that read or edit the open document: text, formatting, paragraphs,
  styles, lists, tables, inserts of content, fields, comments, headers and footers, page setup, find and
  replace. It may not open, save, export or print files, read or write any file (its work runs with every
  file access refused), use the network, change preferences or protection, record or play macros, open
  dialogs, turn Track Changes off, or accept or reject anyone's changes. The engine refuses these inside its
  command runner, so a command that runs other commands cannot reach them either (`crates/engine/src/agent.rs`).
- **What it reads.** The document as it reads with every tracked change accepted: its tools leave out text
  deleted with Track Changes on (and fields or equations inside it), the commands that would show it (a
  paragraph's stored text, the list of tracked changes, notes and headings as stored, a copy in markup) are
  closed to it or answer in the same terms, and any result that would still carry a piece of deleted text is
  withheld. Its search commands still match the stored text, so it could learn that a phrase it guesses is
  there.
- **How it applies.** Each command it runs is either recorded as a step, when it changed something, or leaves
  no trace: a command that fails, only opens a dialog, or is refused is put back whole. A command that only
  moves the selection is recorded as the place it reached. When it finishes, the steps are replayed once on a
  fresh copy and compared with what it worked on; if they differ, nothing is applied. Then the command
  `assistant.runSteps` replays them on a fresh copy of your document, from the selection and the view the
  assistant started with, so its changes land where it made them even if you clicked elsewhere meanwhile; the
  result replaces the document as one undo step. If the document's content or properties changed while it
  worked, nothing is applied and the pane says so: ask again. A click, a save, or an edit you undid meanwhile
  does not count.
- **Track Changes.** With Track Changes on, a change that Track Changes cannot record (a new table, removing a
  header, a field's code or result, an equation's content) is refused, and the assistant is told to do it
  another way or say it could not. Comments and bookmarks go in: they are not tracked changes, in Word either.
- **What it may not write.** Fields that make a word processor read a file or a web address, or start a
  program, when the document is opened or updated (INCLUDEPICTURE, INCLUDETEXT, LINK, DDE, DDEAUTO, IMPORT,
  DATABASE, RD, or any field but a hyperlink that names a web address or a network path), and HTML or RTF
  pastes, however they are passed, which can carry such fields.
- **Undo These Changes** is offered while the assistant's last changes are the newest step of the document's
  undo history, and undoes those and nothing else. Repeat and macro recording leave its changes out.
- **When a limit stops it** (its rounds, tool calls, commands or changes), it is told so and sums up: the
  changes it made so far are applied as one undo step, the pane says the work may be partial, and its reply
  says what is left.
- **When it does not finish** (it declines, its reply reaches its length limit, the conversation grows too
  long, or you press **Stop Waiting**), nothing is applied and the pane says why. A request too large to send
  at all is refused before anything is sent.
- **Limits.** One request may take up to 25 rounds with the model, 60 tool calls, 600 commands (a batch runs at
  most 50) and 500 changes, and may grow the document's text to three times its size or by 2 MB, whichever is
  more; the assistant is told when it reaches one. Each tool result is cut at 30 KB and says so. Each round is a
  separate request to the service, so a long task costs more than a text answer, and the earlier rounds are
  read from the service's cache to keep that down (see [Prompt caching](#prompt-caching)). With the results of
  the round before the last, the model is told to call no more tools and sum up; the last round keeps the same
  tool choice (a different one would start the cache again), and a tool it still asks for then is not run.
- **The conversation.** The model's turns are sent back to it byte for byte, as the API requires for tool use.

## Prompt caching

Every request asks the service to cache what it shares with the next one, which then reads that part at a
small part of the price of fresh input (writing it costs more than fresh input, once). Caching never changes
what the model reads.

- **What is cached.** When the assistant makes changes, the tools and the system prompt (the same in every
  request, and the largest fixed part) are cached for **an hour**, by a breakpoint on the system
  prompt: a request you make minutes after the last reads them from the cache, although its conversation is
  new. The conversation of one request (its rounds) is cached for **five minutes**, by the request-level
  breakpoint, and each round also marks the message where the round before it ended, so it reads that round's
  cache however long the model's turn in between was. An hour costs twice the input price to write against
  1.25 times for five minutes, which pays only for what is read again minutes later: the fixed part; the
  rounds of one request follow each other within seconds, so five minutes keeps them, each read renewing it.
  For text answers the conversation is cached for five minutes: a follow-up sends the earlier turns exactly as
  they went, each instruction with the document text it carried, so it starts with the last request and reads
  it from the cache; when the selected text (or the whole document) is the same as the last instruction's, it
  is not sent again (the new turn says it is unchanged). The earlier turns sent are bounded: at most 24 turns,
  300 KB of their own text (instructions and replies) and 600 KB of the document text they carry again (as much
  as one request may share, so the last exchange always goes with its text). Past a bound the oldest turns go a
  run at a time, not one exchange with every request: the chat is cut into runs from its oldest exchange on, a
  run ending where it reaches half of a bound, and whole runs go, so the requests after a cut start with the same
  turns and read them from the cache again (dropping one exchange each time would change every request's first
  message, and the whole history would be written to the cache again each time, at more than it costs
  uncached). The first instruction kept carries its text in full, and the pane keeps only the turns a request
  can still send.
- **What keeps it working.** The system prompt, the tools (in a fixed order, with their keys in a fixed
  order), the thinking and effort settings and the tool choice are the same in every request and every
  round, and earlier messages go again byte for byte; the messages come last in the body, so each request
  starts with the one before it. The service caches a prefix of 512 tokens or more on Claude Opus 5.5, so a
  short exchange is not cached at all.
- **Where to see it.** Settings shows the last request's token use: the tokens in, how many of them were read
  from the cache and how many were written to it, and the tokens out, summed over the rounds of a request that
  changed the document. A request that changes the document and fails after some of its rounds were answered
  shows what those rounds used, and says so; a request that got no answer, or that was let go (**Stop
  Waiting**, or another document opened while it ran), says that its token use is not known (never the
  request before's numbers).

## What leaves the machine

- Your instruction, the earlier turns of this chat (at most 24 turns and 300 KB of their own text), and the text
  of the scope you chose (at most 600 KB). Too much is refused with a message, never cut. For text answers the
  earlier instructions go again with the document text each carried (at most 600 KB of it in all; a text the same
  as the one before it goes once), so that a follow-up repeats the last request and is read from the cache (see
  [Prompt caching](#prompt-caching)); the oldest turns past these bounds are not sent.
- When the assistant makes the changes itself: your instruction, the selected text, an overview of the
  document's paragraphs (at most 60 KB), and whatever its tools read: any part of the document, including
  headers, footers, notes, comments with their authors' names and the document's properties. The one-time
  agreement says so, whichever way of working is chosen when it is given.
- Track Changes is read as accepted: text deleted with tracking on is **not** sent, in either scope or mode.
- Text formatted as hidden (Font › Hidden, directly or through a style) is **not** sent either, in either scope
  or mode, nor counted, unless hidden text is shown in the document (Show/Hide ¶, or Options › Display › Hidden
  text): then it is sent as you see it. A paragraph that is hidden mark and all takes no line. When the
  assistant makes changes, its readers leave hidden text out the same way, the tools that find words place
  the selection past any hidden text between them, and a result that would show hidden text is refused, as
  for deleted text. Replace Selection replaces the whole selection, hidden text in it included, as typing
  over a selection does. Headers, footers, notes and text boxes are not part of the whole document scope.
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
- Your agreement is kept in `assistant.json` in the settings folder, with the recipient and the model it names
  (see Using it); `WORDCRAFT_NO_PREFS` keeps it for the run only. No command, MCP tool or setting can give it:
  only the button in the pane. A program that can write your files could write that file, as it could your
  settings or the key file.

## For agents and scripts

An agent that drives WordCraft through MCP, the CLI or the control channel brings its own model, so it does not
need the pane. No command, MCP tool or setting can send text to the service or give the agreement (it is kept
in `assistant.json`, which no command writes). A program that
holds the control channel's token can still press the pane's buttons as pointer input, like a person: the
agreement protects against commands, MCP and settings, not against that kind of GUI automation, which is why
the token is private to the user. Three engine commands serve an agent:

- `assistant.context {"scope": "selection" | "document"}` returns exactly the text the pane would share for that
  scope, with its size. It changes nothing.
- `assistant.apply {"text": "…", "mode": "replaceSelection" | "insertAfter"}` puts text into the document as
  written, as one undo step.
- `assistant.runSteps {"steps": [{"command", "params"}], "author"?, "start"?}` runs a list of
  document-editing commands on a private copy (only what an assistant may run, with no file access), from the
  selection and view in `start` (default the current ones) and, when every step succeeds, puts the result in
  as one undo step; else nothing changes. Text the steps add or delete is tracked when Track Changes is on;
  formatting applies directly. Repeat and macro recording leave it out.

`view.assistantPane` opens and closes the pane. Sending a request is deliberately not a command: it needs the
person's agreement, which only the pane's button gives (AGENTS.md rule 14 asks that every feature work without the
GUI; this one step cannot, by design).

## What it does not do

- It does not work in the browser version, which cannot send requests: the pane says so and offers nothing else.
- The reply is read whole, within 16,000 tokens, so a very long rewrite of a whole document can be cut off. The
  pane says when a reply was cut off, ended for another reason, or was declined, and nothing is placed from a
  declined reply. Longer replies need streaming, which is not built.
- A busy or failing service, or a connection that could not be made, is tried again at most twice, waiting
  as the service asks (at most 30 seconds); a spent usage limit is not. After that the error is shown and you
  send again.
- The pane's text is English; translations are added in craft-i18n (the Hebrew edition carries its own Hebrew, see
  [localization-he.md](localization-he.md)). The reply text is laid out paragraph by paragraph in its own
  direction, and the text placed in the document keeps its own direction.
- It was tested with a mock provider and canned replies. No request has been sent to the real service in these
  tests, so the request and reply shapes follow the API documentation and have not been confirmed against it.

## Tests

`crates/assistant` (request building, reply parsing, the tool conversation, the mock, the Claude provider
against a server on this machine, including retries, Stop during a retry wait, that it refuses plain HTTP and
never prints the key; and the caching, without the network: the breakpoints of each body, that each round's
body starts with the last one's, the last round included, that a follow-up starts with the last request and
does not repeat unchanged text, that a dropped turn leaves no tag pointing at text no longer sent, that a long
document's text does not cost a follow-up its history, that past each bound most follow-ups still start with the
request before them, the cache fields of the usage, and the usage of the rounds before a failed one),
`crates/engine/src/tests/assistant.rs` (the commands), `crates/engine/src/agent/tests.rs`
(what the assistant may run, the replay, the limits, Track Changes, fields, what it reads),
`crates/ui-egui/src/assistant_pane/tests.rs` (the one-time agreement and its withdrawal, kept between runs and
honoured across processes, the setup and Settings, the key's acceptance and refusal, scope, key and saved key, stop, announcements, focus,
placement guard, changes made by the assistant, drawn states, the pane's side, a long chat let go a run at a
time, the last request's token use, an agreement that could not be saved, a cleared saved key, and on real
layout in the Hebrew interface the instruction box (its text, and the caret of the empty box at its right edge)
and a reply, right to left), `crates/ui-egui/src/assistant_pane/saved.rs` (the choices file, written one choice
at a time) and `crates/ui-egui/src/rtl.rs` (a field's paragraphs).

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-28 | minor | Hidden text is not sent (either scope, and the assistant's readers when it makes changes) unless hidden text is shown in the document; words found across hidden text are placed in the stored text |
| 2026-11-28 | minor | Review fixes: the empty instruction box's caret at its right edge in a right-to-left interface; a chat's earlier document text bounded on its own (600 KB) and the oldest turns let go a run at a time, so follow-ups keep reading the cache; the token use of a failed or stopped request; an agreement or a withdrawal that could not be saved said outside Settings; a saved key cleared from the key box brings the setup back |
| 2026-11-27 | major | The pane opens on the right in every interface; after the one-time setup it shows the conversation and a Settings button; the agreement is asked once and kept (`assistant.json`, one choice at a time, read again before sending), with a way to withdraw it; the instruction box and the reply lay out right-to-left text; prompt caching (an hour for the tools and system prompt, five minutes for the conversation, follow-ups that repeat the last request) and the cache's token use in Settings |
| 2026-11-27 | minor | Second review: moves recorded as places, a final replay check, the whole selection kept when a call fails, the Web Layout width replayed, more readers kept from deleted text, comments allowed and field changes refused under Track Changes, properties count as changes, no keyboard taken by a reopened pane |
| 2026-11-27 | minor | Review fixes: replay from where the assistant started, calls that fail leave no trace, untracked changes refused, per-document agreement, limits, fields and pastes that reach out refused, tracked deletions never read, Stop ends a retry wait |
| 2026-11-27 | major | The assistant changes the document with WordCraft's commands (tools), one undo step; `assistant.runSteps` |
| 2026-11-26 | major | First version: the AI pane, `assistant.context`, `assistant.apply`, `view.assistantPane` |
