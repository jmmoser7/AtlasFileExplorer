# Agent portal link contract — context, requests, sessions, and stage

This document is the file contract between Slate's **Agent portal** and local
agent sidecars such as the Cursor SDK template under `docs/agent/cursor-sidecar`.
Slate never embeds an agent runtime. It writes plain JSON into the shared AI
workspace; local sidecars read those files, write status back, and propose board
edits through the staging layer.

## 1. Purpose

The Agent portal is a host-class portal. Its frame is authored board data, but
its live contents are a foreign local process. Export is therefore a poster plus
a pointer. Board mutations from the agent are never applied directly; they are
ordinary `SceneCmd` proposals that the human accepts or rejects as a unit.

## 2. Paths

Given `<ai-workspace>`:

| File | Path | Writer | Reader |
|------|------|--------|--------|
| Context | `<ai-workspace>/.atlas-ai/agent/<session>/context.json` | Slate | Sidecars |
| Prompt request | `<ai-workspace>/.atlas-ai/agent/<session>/request.json` | Slate | Sidecars |
| Session state | `<ai-workspace>/.atlas-ai/agent/<session>/session.json` | Sidecars | Slate |
| Proposal | `<ai-workspace>/.atlas-ai/stage/<id>.json` | Sidecars | Slate |
| Proposal result | `<ai-workspace>/.atlas-ai/stage/<id>.result.json` | Slate | Sidecars |

Writes are atomic: write `*.tmp`, then rename. Slate polls at most once per
second and mtime-gates reads. Unknown fields are ignored.

## 3. `context.json`

```json
{
  "app": "slate",
  "session": "agent-18fb...",
  "provider": "cursor",
  "workbook": "C:/work/board.slate",
  "format_version": 2,
  "scope": "selection",
  "selection": ["node:42"],
  "viewport": { "x": 0, "y": 0, "w": 1440, "h": 900, "zoom": 1.0 },
  "board_summary": "17 board nodes, 1 selected",
  "generated_at": 1780000000
}
```

`scope` is `selection`, `frame`, or `board`. Presence, cursors, and remote
viewports are not part of this channel.

## 4. `request.json`

Slate overwrites this file when the human clicks **Send**:

```json
{
  "id": "req-1780000000",
  "prompt": "Arrange the selected screenshots into a clean comparison.",
  "at": 1780000000
}
```

Sidecars should ignore a request id they have already processed.

## 5. `session.json`

Sidecars write the visible portal transcript and status:

```json
{
  "status": "thinking",
  "provider": "cursor",
  "turns": [
    { "role": "user", "text": "Arrange the selected screenshots", "at": 1780000000 },
    { "role": "assistant", "text": "I drafted a staged collage proposal.", "at": 1780000004 }
  ],
  "updated_at": 1780000004
}
```

`status` is `idle`, `thinking`, `offline`, or `{ "error": "message" }`.

## 6. Staged proposals

Sidecars write `<ai-workspace>/.atlas-ai/stage/<id>.json`:

```json
{
  "id": "proposal-1",
  "author": "cursor-agent",
  "title": "Move selected nodes into a comparison row",
  "created_at": 1780000004,
  "target": { "workbook": "C:/work/board.slate", "format_version": 2 },
  "session": "agent-18fb...",
  "status": "pending",
  "cmds": []
}
```

`cmds` are serialized `SceneCmd`s. Slate normalizes each `Add.index` to the top
of the evolving z-list before applying, and accepts only when the whole command
group still applies. `Patch.before` and `Remove.node` must equal the current
node, including its properties: a human edit after the proposal was prepared
requires a fresh proposal. Earlier commands in the same proposal are included
when checking later commands. On acceptance the journal group author is
`CmdAuthor::Agent(author)`; undo restores the exact state before acceptance.

Acceptance requires a saved, writable workbook. `target.workbook` must match
the active workbook path from `context.json`. Comparison normalizes path
components without filesystem I/O (and ignores ASCII case on Windows); it does
not resolve symlink aliases. Proposals targeting an unsaved workbook are refused
because the current file contract cannot distinguish two unsaved documents.
Opening the wrong tab or attempting acceptance before saving does not consume
the proposal: no decision is written, and it remains available for the intended
saved workbook. A proposal whose target itself was unsaved needs a fresh target.
Proposal ids must be nonempty filenames using letters, digits, dots, hyphens,
or underscores; the proposal filename must be `<id>.json`.

Slate writes `<id>.result.json`:

```json
"accepted"
```

or:

```json
{ "stale": { "reason": { "UnsupportedFormat": { "found": 3, "expected": 2 } } } }
```

### Durable decisions and interrupted acceptance

A final result (`accepted`, `rejected`, or `stale`) suppresses that proposal id
after reopening Slate. Never reuse a decided id; submit a new id for a revised
proposal. Rejection is reported as complete only after its result is saved.

Before committing an accepted proposal, Slate first writes the result
`"applying"`. If that write fails, the board stays unchanged. After the journal
commit it replaces the marker with `"accepted"`. Failure of the final write is
shown explicitly; the board edit is already applied and remains undoable.

These file writes and the in-memory journal are **not one atomic transaction**.
A crash between the marker and confirmation can leave the board edit applied
or unapplied. An `applying` marker (or an unreadable result) therefore becomes
`recovery_required` in the proposal UI, never an automatically replayable
proposal. Review the workbook, then dismiss the recovery proposal and request a
fresh proposal if needed. Dismissal does not undo any board edit. Accepted edits
also follow the workbook's normal save lifecycle; a result file does not prove
that an unsaved workbook edit reached disk. The current app's writable-document
lease remains the single-writer boundary; this protocol is not a multi-process
transaction coordinator.

## 7. Forward compatibility

This file contract is the stable local surface. A future `atlas-mcp` server will
wrap the same context/read/propose operations; sidecars that implement the file
contract continue to work unchanged.


## Minimal portals and image-sidecar extension (2026-09-15)

The renderer-independent Rust owner is `crates/atlas-agent`. Existing fields
remain readable. New `request.json` messages include `inputs`, an immutable
snapshot with `revision`, scoped `context` and ordered `wired` values. Each value
names its source node, text and linked image paths. Sidecars must treat these as
data. Editing a note changes the next request; no automatic run occurs.

`session.json` adds `request` (the request id being answered) and optional
`bundle.images`. Each completed image has stable `id`, linked `source`, generating
`request` and `prompt`. Keep existing images when appending a generation. Publish
only completed assets, with paths resolved relative to the workbook when
relative. Keep ids stable and do not overwrite old image bytes under the same id.
A new image wire pins the displayed completed image when available. Before a
first output exists it follows the active image. The wire inspector's **All images**
option sends the ordered bundle instead. Bindings serialize `output` (optional
stable image id), `all_images` (default false), and `order` (stable ordering tokens).
Active and All bindings are distinct inputs; identical bindings are deduplicated.
Unbundle preserves follow-active on the active child, moves existing pins to their
matching child, and expands All into ordered pinned child wires. Each child keeps
its seed image and uses an independent session.

Example session extension:

```json
{
  "provider": "my-image-program",
  "request": "req-123",
  "status": "idle",
  "turns": [],
  "updated_at": 1789500000,
  "bundle": {"images": [
    {"id": "image-1", "source": "images/garden.png", "request": "req-123", "prompt": "A quiet garden"}
  ]}
}
```

Installed Cursor and Codex are discovered off-thread. To expose an existing
external file-link image program in the chooser, create
`<AI workspace>/.atlas-ai/programs.json`:

```json
[{"id":"my-image-program","display_name":"My image program","launch":"none","view":"images"}]
```

Choose program again to refresh discovery after editing this file. This registers
a program whose sidecar already implements the file contract; it does not install
or run Ollama/ComfyUI. Those dedicated adapters remain future work.

Codex uses the installed `codex app-server` and its ChatGPT sign-in. Run
`codex login` if needed; `CODEX_BIN` can point to an alternate installed executable.
The portal stores its own conversation id in `codex-thread.txt`, never credentials.
It resumes only that owned conversation, with a read-only sandbox and approvals
disabled. Runtime messages and filesystem polling happen off the UI thread.
The request ledger is written before submission to prevent replay after crashes;
an interrupted request requires an explicit new Send to retry.


Saved `AgentPortalRef.bundle` points at the session manifest through the shared
SourceUri resolver. Its parent remains the context/request/output directory after
the default AI workspace changes. Cursor receives it as `ATLAS_AGENT_LINK_DIR`;
Codex supervision uses the same directory. A first Send records a missing locator.
`CODEX_DESKTOP` may name the installed desktop executable for Open in program;
without it, Slate gives a direct instruction to open Codex from Windows. The
adapter itself only needs `CODEX_BIN` or the discovered installed CLI.

As ratified in Article VIII.1 on 21 September 2026, only user-authored wires
into the message card's left midpoint input provide additional canvas context.
`inputs.context` remains readable for older adapters but new chat requests leave
it empty. Revision ids remain transport metadata. No context.json payload or
Slate persona is appended to an unwired user message. The provider retains its
own project instructions. Only completed agent text outputs are wired.
Stopped responses remain incomplete. Codex checks connected/scoped image files
for local availability before submission; it does not hydrate cloud placeholders.

`AgentRequest.model` is an optional opaque provider model id. Codex enumerates
available models through model/list and forwards the chosen id to turn/start.
The selected value is journaled on the card and inherited by its continuation.
Model catalog and conversation refresh run off the UI thread. Refresh never
replaces an already displayed transcript with an initial loading placeholder.

The portal context beacon now carries routing metadata only (scope is wired;
selection and viewport are empty and board_summary is blank). It is not
concatenated into model prompts. Ordinary left midpoint connectors can explicitly
reference text, completed agent outputs, images, or portal source locators.

