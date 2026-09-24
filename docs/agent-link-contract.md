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
| Output folder record | `<link>/output.json` | Slate | Slate |
| Deliverables manifest | `<link>/return.json` | Agents | Slate |
| Deliverables result | `<link>/return.result.json` | Slate | Agents |
| Deliverables record | `<link>/deliverables.json` | Slate | Slate |
| Versions | `<link>/versions.json`, `<link>/versions/` | Slate | Slate |
| Sidecar owner | `<link>/sidecar.pid` | Sidecars | Sidecars, Slate |

`<link>` is the conversation's link folder, `<ai-workspace>/.atlas-ai/agent/<session>/`
by default (a saved portal may point at another one). Everything in it lives
in the AI workspace, never inside a `.slate` workbook (Article IX): the
workbook only links.

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

Optional `output_dir` is the absolute output folder for new files (see
[Output folder](#output-folder)). Absent means Slate has not named one; the
guide then omits the folder sentence.

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

## Repeating messages (`schedule.json`)

The chat card's ellipsis menu (Cursor, Codex) offers Schedule…, which opens a
dialog for the message, a first run date and time, and Once / Every hour /
Every day / Every week. Slate writes `schedule.json` beside `session.json`:

```json
{"start":"2026-10-01T04:55","repeat":"daily","prompt":"Refresh the dashboard","provider":"cursor","cwd":"C:/project","ai_workspace":"C:/ws","model":null}
```

`start` is local wall-clock time. Slate registers a per-user Windows task named
`Slate agent <session>` from task XML (so dates never depend on the Windows
date format) that runs `slate.exe --scheduled-run <link>` with no window, and
runs a start missed while the computer slept as soon as it wakes. While a run
is still ahead, a clock sits in the card's top strip. That process sends the saved
message once through the same request and session files, then exits: Cursor
uses a one-shot sidecar (`ATLAS_AGENT_ONCE=1`), or the sidecar an open Slate
already runs for that folder; Codex uses its usual link. Replies, deliverables
and versions land in the conversation folder, so an opened board shows them.
A failed run leaves `schedule.result.txt`. Nothing is written into a workbook.

The computer must be on and signed in. Running on Cursor's cloud while this
computer is off is not built yet.

## Agent feedback

When a task took too many steps, or an action was unreachable, the agent
writes `feedback.json` beside `session.json`:

```json
{"id":"a-new-id","what":"one sentence","tried":"what you did","missing":"the action you could not reach"}
```

Slate appends it to `feedback-log.json` in that link folder and removes
`feedback.json`. Each field stays under 800 characters and must not contain a
key or token. The person's prompt and file contents are not included.

If `gh` is signed in, Slate also files the note as an issue on
`jmmoser7/slate-agent-feedback` (`ATLAS_FEEDBACK_REPO` names another repo;
`ATLAS_FEEDBACK=0` keeps the note local only). `feedback.result.json` says
whether it was filed and, when it was, the issue URL. No GitHub account is
required for the note to be kept. The same file is what a future MCP tool
will wrap.

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
or run a generation engine.

ComfyUI has its own adapter (`atlas-comfy`). A local install (portable, desktop,
or `COMFY_BIN`) appears as one ComfyUI tile. The adapter talks to
`127.0.0.1:8188`, starts the server with `--disable-api-nodes` when it is not
running, and never downloads weights. Wired inputs arrive in the usual
`InputSnapshot`; a wired 3D model's item carries `images[0]` (shaded view) and
`depth` (inverse depth, near = white), captured by Slate when the run starts.
`AgentRequest.image` carries the seed and live run of a Live generator. Results
are written under `%LOCALAPPDATA%\NativeFileAtlas\comfy\<session>` and listed in
`session.json` like any other image bundle; a frame of the current live run
replaces the previous one, and every other entry is kept.

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
Cursor enumerates the same way through the sidecar `--models` catalog
(`Cursor.models.list`) and forwards the chosen id on `agent.send`. An empty
choice means Codex default or Cursor `auto`. Stop writes `cancel.json` for the
in-flight Cursor request id; the sidecar calls `run.cancel()`. The portal stores
its owned Cursor agent id in `cursor-agent.txt` and resumes only that agent.
The selected value is journaled on the card and inherited by its continuation.
Model catalog and conversation refresh run off the UI thread. Refresh never
replaces an already displayed transcript with an initial loading placeholder.

The portal context beacon now carries routing metadata only (scope is wired;
selection and viewport are empty and board_summary is blank). It is not
concatenated into model prompts. Ordinary left midpoint connectors can explicitly
reference text, completed agent outputs, images, or portal source locators.

## Slate Link (dashboard documents)

An HTML file can be both source text and a drawn page. Opening one from an
agent's changed-documents dot asks which face to show. Text is the snippet
card. Graphic is a web portal pointed at the file.

A chart that should follow a spreadsheet declares one table input in the file:

```html
<script type="application/slate-link+json">
{"version":1,"inputs":[{"name":"data","kind":"table"}]}
</script>
```

The page reads `window.slateLink.inputs.data`, an object `{ columns, rows }`
whose first row of the file is `columns` and the rest are `rows`. Cells are
strings. When nothing is wired, `data` is absent: draw a small sample, and
redraw on the `slate-link` event. Do not copy the spreadsheet into the HTML
as the only data.

A wire from a CSV, TSV, or Excel workbook (`.xlsx`, `.xlsm`, `.xltx`, `.xltm`)
into the left middle of that web portal is the link. The first such wire fills
`data`. Another wire to the same page uses the next free name. Slate does not
write the table back into the file. A cloud placeholder that is not on disk is
reported as `health: "missing"` and is not downloaded.

## Placing a File Atlas portal

An agent can put a folder it created on the board. It writes `place.json`
beside `session.json` in its link folder:

```json
{"id":"penn-station","kind":"file_atlas","path":"penn-station-images"}
```

`path` is relative to the AI workspace, or an absolute path. `id` is a new
name each time a portal should appear. Slate adds a File Atlas portal bound
to that folder, to the right of the agent card, as an undoable agent edit,
then removes `place.json` and writes `place.result.json`. A missing folder
does not create a portal. Do not tell the person to use the changed-documents
dot for a folder they asked to see. That dot remains how a single changed
file is opened.

## Output folder

Each conversation has one folder for the new files the person did not place:

```text
<base>/slate-outputs/<board-slug>/<yyyy-mm-dd>-<title-slug>-<id6>/
  report.html        deliverables at the top
  assets/            supporting files
  scratch/           throwaway files
```

`<base>` is the conversation's working folder: its project folder, or the AI
workspace when it has none. `<board-slug>` is the workbook file stem
(`untitled-board` for an unsaved board). Slugs are lowercase ASCII letters and
digits joined by single hyphens, at most 40 characters (`conversation` when
nothing is left). The date is the UTC day the folder was first named. `<id6>`
is six hex digits hashed from the link folder name. (Slate's folder names share
an `agent-req-<pid>-` prefix, so a leading slice would not be unique.)

The first time the folder is named, Slate records it in `<link>/output.json`
as `{"dir":"<absolute>"}` and creates only the top folder. Later sends reuse
the record, so renaming the board or conversation never moves outputs. Slate
passes the folder as `request.json` `output_dir`, and the guide tells the agent
to use it. Existing files are edited where they are.

Rust owner: `atlas_ai::agent::output_dir(link_dir, base, board, title, now_secs)`.

## Deliverables (`return.json`)

When an agent finishes, it names what the person asked for in `return.json`
beside `session.json`:

```json
{"id":"q3-sales","title":"Q3 sales","items":[
  {"path":"dashboard.html","as":"graphic"},
  {"path":"assets/sales.csv","feeds":"dashboard.html"},
  {"path":"notes.md"}
]}
```

- `id` is new for each manifest. A manifest with an id Slate has already
  recorded replaces that set.
- `as` is how the item is shown: `auto` (default, by file type), `graphic`
  (HTML or SVG rendered as a page), `text` (the source), `images` (a folder or
  several images as a grid), or `folder` (a File Atlas browser).
- `feeds` on a CSV or Excel item names a dashboard to wire it into with a Slate
  Link (see [Slate Link](#slate-link-dashboard-documents)).
- Relative paths resolve against the output folder, then the working folder,
  then the AI workspace; the first place the path exists wins. Absolute paths
  are kept.

The earlier image form is still read. `{"id","kind":"images","title","path"}`
becomes one item with `"as":"images"`. With `"paths":[...]` instead of `path`,
each path becomes its own item with `"as":"images"`.

Slate checks that each path exists (it never reads the files) and appends the
set to `<link>/deliverables.json`:

```json
{"version":1,"sets":[
  {"id":"q3-sales","turn":3,"title":"Q3 sales",
   "items":[{"path":"C:/p/slate-outputs/q3/2026-09-23-sales-a1b2c3/dashboard.html","as":"graphic"}],
   "missing":["notes.md"]}
]}
```

`turn` is the assistant-message index of the reply that wrote the manifest (the
same index as that reply's `artifacts[].turn`). Agents write `return.json`
during their reply, before the reply is in the history, so when the history
ends with the person's message `turn` is the slot the reply will take;
otherwise it is the newest assistant message. A manifest may name any
existing file or folder; agents name it instead of copying it into the output
folder. Protocol writes inside `.atlas-ai/` never appear as outputs. Paths and
`feeds` are absolute. `missing` lists
paths as written that did not exist. Slate then writes
`return.result.json` as `{"id":"q3-sales","ok":true,"count":1,"missing":["notes.md"]}`
and removes `return.json`. An unreadable manifest, or one with no items, is
also removed and reported as `{"ok":false,"error":"..."}`, so the agent can
write a corrected one.

Slate lists these items first on the card, and the person spawns them. Agents
do not place deliverables themselves. `place.json` is only for an explicitly
requested File Atlas folder browser.

Rust owners: `atlas_agent::{ReturnManifest, ReturnItem, Face, Deliverables,
DeliverableSet, Deliverable}`, `atlas_ai::deliverables::{consume_return,
load_deliverables}`. `consume_return` is blocking I/O; the link folder's
session reader worker calls it (`atlas_ai::outputs::OutputWatch`), with the
output folder from `output.json` and then the roots the board gives it (the
conversation's working folder and the AI workspace). It waits until it has
those roots, so a manifest is never resolved against too few folders. The
board reads the result as `AgentSources::outputs`.

## Requested and incidental files

Each message's files split into two lists on the card:

- **Requested**: the items named in that message's `return.json`. When a
  message has no `return.json`, the files it created or modified inside its
  output folder count as requested.
- **Also changed**: every other file the message created, modified, or deleted
  (its `artifacts` with an output kind), listed below the requested items.
  Deleted files are listed but cannot be spawned.

Loose pictures a set names with `"as":"images"` are one image set, titled with
the manifest title. Rust owner: `atlas_ai::outputs::partition` (no I/O).

## Versions

After each message finishes, Slate copies every file it created or modified:

```text
<link>/versions/t003/dashboard.html
<link>/versions/t003/index-2.html    same name from another folder, same message
<link>/versions.json
```

```json
{"version":1,"files":[
  {"source":"C:/p/out/dashboard.html","versions":[
    {"turn":1,"path":"versions/t001/dashboard.html","bytes":5120,"added":120,"removed":0},
    {"turn":3,"path":"versions/t003/dashboard.html","bytes":5302,"added":9,"removed":4},
    {"turn":5,"bytes":70000000,"skipped":"larger than 64 MB"}
  ]}
]}
```

- A message is finished when the session is no longer `thinking`, or when a
  later assistant message exists. Each file is copied once per message
  (`turn`, the assistant-message index), within about a second of it finishing.
  The copy runs on the session reader's worker thread, never the frame loop.
- `path` is relative to the link folder.
- `added` and `removed` count lines against the previous captured version, for
  text files (`html htm css js ts json md txt csv tsv svg xml py rs toml yaml
  yml`) up to 2 MB that decode as UTF-8. Lines are compared as multisets, so a
  moved line counts as unchanged. A created file with no earlier version counts
  all its lines as added. They are absent when unknown.
- Folders, missing files, and relative paths are not copied. Cloud placeholders
  (not downloaded), files over 64 MB, and files changed again by a later message
  before the copy ran are recorded with `skipped` and no `path`. Deleted files
  are not versioned.

Versions live in the AI workspace beside the conversation record, never inside
the workbook (Article IX). Rust owners: `atlas_agent::{Versions, VersionedFile,
Version}`, `atlas_ai::versions::{capture, load_versions}`.

## Sidecar supervision

A link folder has at most one watching sidecar.

- Slate starts the Cursor sidecar with `ATLAS_PARENT_PID` set to its own
  process id.
- At startup the sidecar writes its own pid to `<link>/sidecar.pid`. The newest
  sidecar wins.
- Each watch cycle, the sidecar stops if `sidecar.pid` no longer holds its pid
  (including when the file was removed) or if the `ATLAS_PARENT_PID` process no
  longer exists. It exits through the libuv-safe `finish()` path and removes
  `sidecar.pid` only while the file still names it.
- `atlas_ai::sidecar::stop(link_dir)` stops the sidecar named in `sidecar.pid`
  when that pid is a live `node` process (so a reused pid is left alone), then
  removes the file. Slate calls it when a board closes. It runs system tools;
  call it from a worker.

## Spawning artifacts

What the board places when the person spawns an item. The face comes from `as`,
or the file type when `as` is `auto`.

| Item | Spawns as |
|------|-----------|
| Web page or dashboard (`html`, `htm`, `svg` as graphic) | Web portal 640 × 400. Receives `feeds` wires. |
| Source text (`html`/`htm` as text, `md`, `txt`, `json`, code) | Snippet card at its natural size. |
| Table (`csv`, `tsv`, `xlsx`, `xlsm`) | Table card. When it feeds a dashboard in the same group, it sits immediately left of it, wired with a Slate Link. |
| Image (`png jpg jpeg gif webp bmp tif tiff avif`) | Image node at native aspect, long side 320. |
| Image set (`as: images`: a folder or several images) | A frame with a grid (existing behavior). SVG is excluded. |
| PDF and office documents | Document card with its thumbnail. |
| 3D model (`obj`, `stl`, `gltf`, `glb`, `3dm`) | Model viewport 480 × 360. |
| Video (`mp4`, `webm`, `mov`, `m4v`) | Video node 480 wide with its poster. |
| Folder | File Atlas portal 640 × 420. |
| Other files | Document card. |

Missing or deleted files are not spawned; a notice names them.

**Groups.** Two or more items spawn as one frame titled with the manifest title
(or "<conversation title> outputs"), in manifest order, in rows no wider than
1600 with 24 gaps and 28 padding. One faint provenance wire runs from the
card's output circle to the frame. A second press retracts the group with the
suck-in animation.

**Gestures on the capsule stack.**

- Click a capsule to spawn it, or retract it if it is on the board.
- Shift-click to collect several; the top "Spawn N" capsule then spawns them as
  a group.
- "Spawn all" spawns every requested item, or every item when none is marked
  requested.
- Drag a capsule onto the board to spawn it at the drop point.
- A "vN" badge appears on a capsule when its file has two or more captured
  versions. N is the version that card's own message produced (its capture,
  or the newest one before it), not the count, so a dashboard edited on three
  cards reads v1, v2, v3. A capsule whose file changed on a later card spawns
  its own version's copy from `versions/`, titled "<name> vN"; the newest
  version spawns the live file.
  Clicking the badge spawns the evolution frame: versions left to right, each labeled
  with its version number, the person's message that produced it, and, for
  text files, the lines added and removed.

Commands: `portal.agent.artifacts` (open the stack), `portal.agent.spawn_output`,
`portal.agent.spawn_outputs`, `portal.agent.evolution`. Board owner:
`apps/slate/src/app/board_agent_outputs.rs` (pure `group_layout`, `feed_order`,
and `family`; nodes come from `build_artifact_node`, wires from
`provenance_wire` and `build_connector_with`).

## Guide text

Codex and Ollama receive the Slate Link, File Atlas placement, and file guides
on every send, after the markers `Slate Link (dashboard files)`,
`File Atlas placement`, and `Changed documents`; the chat card hides them
(`atlas_agent::display_prompt`, which also strips transcripts written with the
older image-only guide). The file guide is `atlas_agent::artifact_guide(output_dir)`.
Codex knows its link folder, so it sends `AgentRequest::input_text_in(link_dir)`,
whose guide (`artifact_guide_in`) names `<link>/return.json (beside session.json)`
the way the sidecar does.
The Cursor sidecar builds the same paragraphs into its board preamble
(`artifacts.mjs` `artifactGuide`, TWIN) and names the link folder's absolute
path for `place.json` and `return.json`.

