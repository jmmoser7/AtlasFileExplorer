# Live collaboration

**Status: design for review.** Nothing here is implemented. Nothing here amends
`CONSTITUTION.md`. The paste-ready clauses at the end are proposals; only an
explicit edit to the constitution ratifies them.

This is the protocol document Wave 4 was gated on (`docs/workplan/tasks/spikes.md`
S3, `docs/workplan/tasks/wave-3-plus.md` WI-9). The workplan's working title
was `docs/collab-protocol.md`. This file is that document. Decisions D11 and
D18 in `docs/audit/2026-07-25-decisions-flexibility.md` still hold: a
self-hosted relay, server-assigned order, last-writer-wins per property, and
a session log that is not the store of record. Where this file is more
specific than those decisions, this file is the one to review.

The weekly use this capability exists for (Article III): a small team marking
up one board together. Two to eight people is the ordinary meeting. Some of
them have the workbook's folder open through the office share, OneDrive,
Dropbox, or Box. Some are remote and reach the same board through a relay
their firm runs. They see who is looking where, and they do not lose each
other's edits. Twenty people in one session is the design ceiling, carried
forward from D11. A public whiteboard, a text editor, and a file host are
different products.

Roadmap Phase 6 names this work. It sits on the journal Article VI already
requires. It does not wait for a new document model.

## Goals

- Two or more Slate processes show the same board while people edit it, and
  each sees the others' cursors, viewports, and selection outlines.
- Every authored change is still a journaled command with an author, a stable
  node id, and the smallest property that changed (Article VI).
- The `.slate` file remains the document. A relay the user can run is a
  rendezvous for the live session (Article I.4). Quitting the relay, or never
  starting one, leaves a normal local workbook.
- Each machine resolves links itself. A file that machine does not have is
  `Missing`. Nobody downloads a folder to paint someone else's cards
  (Article IX).
- Agents in the session still propose, and a human still accepts
  (Article VII.6).

## Non-goals

These are the other ninety percent. They stay out until a weekly use appears.

- Accounts, sign-in, a directory of users, or a relay operated by this
  project. A firm may run the binary. This project does not host rooms.
- A character-level text CRDT, comments-as-threads, voice, or video.
- Permission regions, per-node ACLs, or a read-only role inside the room.
  Anyone holding the room key can edit. Hidden regions were already refused
  as a false affordance.
- File Atlas as a multi-user app. The in-process Slate⇄Atlas bridge
  (`crates/atlas-session`) is a different channel and stays that way.
- Syncing portal *contents*, web cookies, WebView profiles, playheads,
  simulation, trails, or intent ink. Those are derived (Article VI.3,
  Article VIII.4–5).
- Using OneDrive, Dropbox, or Box as the live merge algorithm. Their whole-file
  last-writer-wins races the JSON. The relay is the live bus; the sync client
  only has to deliver the flushed file and the `assets/` tree.
- Collaborative editing of the tag taxonomy, the Lens root, or brush-wheel
  memory. The board is live. Those are snapshot or per-machine.
- Offline branching. A dropped socket is a short blip with a queued rebase.
  A laptop that edited for an afternoon while disconnected does not merge back
  by timestamp.
- More than twenty simultaneous editors as a supported mode. The relay may
  accept up to 32 and then refuse the join. It does not degrade into a
  spectator protocol.
- Streaming original file bytes through the relay in the first two phases.
  D11 called preview streaming critical for hybrid meetings. This design
  defers that to a possible later phase and asks for ratification below,
  because shipping it now would turn the relay into a second file server.

## What the design builds on

The board is already a flat list of nodes with stable ids. `NodeId` is a
`u64` (`crates/slate-doc/src/scene.rs`). `Scene::alloc_id` bumps
`next_node_id`. `SceneCmd` is `Add` / `Remove` / `Patch` of a whole node, plus
the same three for paint-layer children, and `Scene::apply` returns false when
the index or id is stale. `SceneJournal` is a session-local undo stack. It is
not serialized with the workbook. `CmdAuthor` is `Human` or `Agent(name)`, and
`commit_as` / `amend_top` already refuse to extend a group another author
owns.

That journal cannot yet be the wire format. `Add` and `Remove` address a
`usize` index, and `Patch` replaces the entire `Node`. Two people editing
different fields of one node would overwrite each other, and two inserts would
fight over a list position. That gap is deviation DV-01. The unbounded undo
stack is DV-08. Both close with the convergent-journal work (T1.1a order
keys, T1.1b id-addressed commands, T1.1c property-scoped `SetProp`) described
in `docs/workplan/tasks/wave-1.md`. Phase 2 does not start until those land.
Phase 1 does not need them.

A workbook is pretty-printed JSON. `SlateDoc::save_to` writes a temp file
beside the destination and renames it
(`crates/slate-doc/src/doc.rs`). `load_from` rejects a future
`format_version` and upgrades an older one in memory. Current version is 2.
`SceneJournal` is absent from that JSON on purpose.

Opening a workbook takes a lock file, `board.slate.lock`, via
`create_new` (`crates/slate-doc/src/lease.rs`). A second opener becomes
read-only and is told who holds it. The holder heartbeats every 10 seconds; a
heartbeat older than 30 seconds may be stolen. The lease comment already
records the limit: clocks that disagree by more than that window can steal a
live lease, and anything stronger needs a server. The relay is that server
for a live session. The lock file stays, so a person who is *not* in the
session still opens read-only while someone is in it.

`save_doc_to` (`apps/slate/src/app/mod.rs`) copies the saver's camera into
`doc.view` before writing. `commit_scene_as` (`apps/slate/src/app/board.rs`)
refuses edits when the tab is read-only. Both behaviors are right for a
single writer and wrong if copied blindly into a session: an autosave must
not teleport everyone to the writer's camera, and a peer who lacks the file
lease must still be able to commit scene commands through the relay.

Links already have one resolver. `source_locator` stores a path relative to
the workbook directory when the file lives under it, and `resolve_source`
joins that locator back onto whatever directory the workbook was opened from
(`scene.rs`). Portal frames carry a `SourceUri` of that form. Item records
do not: `SlateItem.path` is an absolute `PathBuf` (DV-03,
`crates/slate-doc/src/item.rs`). Health is `Ok` / `Missing` / `Unknown`,
probed off the UI thread by `LinkHealthCache` (`crates/slate-doc/src/link.rs`).
`Unknown` means the filesystem could not answer. It does not mean deleted.

Secrets stay on the machine (`crates/atlas-core/src/secrets.rs`). A workbook
may name a service. It does not store the value. `is_machine_private` refuses
the WebView2 profile, the secret store, a legacy API-key file, and web-consent
grants under the Atlas data directory. Packaging and HTML export already call
it (`crates/slate-artifact/src/assets.rs`).

Placed media that the board itself creates already lands in
`<workbook>/assets/` (viewport screenshots, media contract D36 in
`docs/keymap/contracts/media.md`). HTML export copies linked material into an
`assets/` tree beside the artifact (`slate-artifact`'s `assets.rs`). The
InDesign-style package (Article IX.4, decision D10, workplan WI-5) is the
same idea pointed at a workbook: copy the linked files next to the `.slate`,
rewrite locators to be relative, write a provenance `manifest.json`. That
package is not built yet. Collaboration should call it, not grow a third
copy path.

## Architecture

One new crate, `crates/atlas-relay`. It is a capability, not core
(Article I). `slate-doc` must not depend on it. It must not depend on `egui`
(Article I.1). Apps paint and forward input; they do not invent a second
merge.

Earlier notes split this into a `slate-relay` binary plus an `atlas-collab`
library (WI-9a / WI-9b). One crate is the owner. The binary and the client
share the protocol types, so the wire cannot drift. The workspace already
names shared infrastructure `atlas-*` (`atlas-session`, `atlas-curl`).
`atlas-curl` stays the model-adapter transport. This crate does not speak
HTTP to Ollama.

Inside the crate:

| Module | Role |
|---|---|
| `protocol` | Message types, serde, size limits. Depends on `serde` only. |
| `log` | Sequence numbers, append-only frames, snapshot pointers, retention. |
| `merge` | Apply a property op onto a scene value. Depends on `slate-doc` once `SetProp` exists. |
| `sim` | In-process relay used by tests. No sockets. |
| `net` | WebSocket server and client. The binary's only entry. |

`protocol`, `log`, and `merge` build and test on Linux with no listener.
`net` is tokio, tokio-tungstenite, and rustls. The server path stores opaque
op bytes. It does not call `Scene::apply`. The only document facts it
understands are the room's epoch, the monotonic sequence, the participant
table, and the id high-water marks it hands out in blocks. Snapshot bytes are
opaque too. The lease-holding client produces them by saving the scene it has
already applied.

That split is deliberate. A relay that interprets `SceneCmd` becomes a second
interpreter of the scene and will drift (Article XII). Compaction is "the
writer uploaded a new snapshot, drop log frames at or below that sequence."

### Transport

WebSocket over TCP, subprotocol `atlas-relay.v1`. One JSON text frame per
message, maximum 256 KiB. Batches are explicit (`Commit` already holds a
group). No second codec.

TLS is the default. On first run the binary writes a self-signed certificate
into its data directory and prints the SHA-256 fingerprint. The invite carries
that fingerprint; the client pins it. There is no account and no public CA.
An operator who already has a certificate passes `--tls-cert` and `--tls-key`.

Plaintext is available only with `--insecure-lan`, and the binary then
refuses to bind anything other than loopback and private ranges. A relay
reached across the public internet is `wss` or it does not start. The room
key is not a substitute for encryption: the key is the authorization, TLS is
what keeps the board off the path.

### Discovery

The address that matters is the one in the invite: `wss://host:port` plus
room id, key, and certificate fingerprint. The relay may also advertise
`_atlas-relay._tcp` on the local network with its port and a human label.
The advertisement does not contain the room key, the room id, or the
fingerprint. Discovery is a convenience for "the relay is that machine."
Joining still requires an invite. A network that blocks multicast changes
nothing about the protocol.

### How someone runs it

The binary is `atlas-relay`. No installer service and no account.

```text
atlas-relay --data C:\AtlasRelay --bind 0.0.0.0:8787
```

`--data` defaults to `%LOCALAPPDATA%\AtlasRelay` on Windows and
`$XDG_DATA_HOME/atlas-relay` elsewhere. The process prints the listen URL
and the certificate fingerprint. Slate's session UI has a field for that
URL; it does not embed a vendor address.

A firm that wants the same binary on a VPS copies the executable and the
data directory's certificate, opens one port, and keeps the data disk. That
is the hosted deployment Article I.4 allows. It is still their process.
Turning it on does not contact this project.

Layout under `--data`:

```text
rooms/<room_id>/epoch
rooms/<room_id>/snapshot.bin
rooms/<room_id>/snapshot.seq
rooms/<room_id>/ops.log
```

`ops.log` is length-prefixed frames. Presence is not in it. The README that
ships with the binary states, in one screen: the relay stores scene
operations and snapshots for the life of the session; it stores no accounts;
it deletes the room directory when the last peer has been gone for the grace
window (default 15 minutes, `--grace-secs`); an operator can read the log
because they hold the disk; do not point `--data` at a backup that outlives
that window if the board is sensitive.

### Messages

| Message | Direction | What it carries |
|---|---|---|
| `Hello` / `Welcome` | client → relay → client | room id, key, client token, display name; assigned participant id, color, epoch, tip sequence |
| `Snapshot` | relay → client | opaque workbook bytes at a sequence, with machine-personal fields already stripped |
| `Ops` | relay → client | frames after a sequence, in order |
| `Commit` | client → relay → clients | one command group, author, the sequence the author based it on |
| `Reject` | relay → client | the group and a reason the relay itself knows (auth, size, rate, full room, stale epoch) |
| `Alloc` | client → relay → client | a block of node, item, and group ids |
| `Presence` | client → relay → clients | cursor, viewport, selection, focused text node |
| `Bye` | either | leave, or the relay closing the room |

Scene rejection ("this no longer applies") is not a relay `Reject`. Every
client runs `apply_all`. A group that fails is skipped, the sequence still
advances, and the author sees the failure. Honest clients skip the same
group because apply is a pure function of the scene at that sequence. A
client that fails a group others accepted reloads the latest snapshot and
shows a session error. It does not try to be clever.

## Identity without accounts

A participant is a display name, a color, and a participant id. The name
defaults to the OS user, the same source `LeaseInfo` already uses, and the
person can override it in local settings. The override is not written into
the workbook. The relay assigns the participant id and a color from a fixed
palette of twelve, stable for the session. The client token is a random
128-bit value stored in local app settings so a rejoin can ask for the same
color. It is not a credential. The room key is the credential.

The room id is random. It is not derived from the file path, because the
path differs per machine (`C:\Users\Ada\OneDrive - Firm\Boards\Deck.slate`
and `D:\Box\Boards\Deck.slate` are the same workbook).

The room key is 128 random bits, shown in the invite as a short group of
characters. Comparison is constant-time. The key is never a field in the
`.slate` JSON, never a `SceneCmd`, and never a thumbnail path.

Default: the key lives in the session UI and in the invite the host copies.
Optional: the host writes `Deck.slate.invite` beside the workbook so a team
that already trusts the share can join without a side channel. That file is
outside the document. Packaging, HTML export, and the relay snapshot must
refuse it. `is_machine_private` does not match it today, because that helper
only recognizes the Atlas data directory. The refusal is a separate check
next to the lock file (`*.slate.invite`, `*.slate.lock`), added when this is
built. Turning the option on means everyone who can read the folder can
enter the room. That matches the trust of the share. Leaving it off means
the key travels only where a person pastes it.

Rotating the key is "start a new room." There is no account to revoke. Peers
still holding the old invite fail `Hello`.

## Phase 1 — presence

Presence ships first, alone. The lease holder is still the only writer. Other
people join the room and see the board they opened from the share (or a copy
they were sent). They do not send commits. `read_only` stays in force for
edits. What they add is the knowledge of who else is here.

This phase is worth shipping by itself. A markup review where only one person
drives, and everyone else can see the driver's cursor and wave their own, is
already the weekly meeting. It also forces the relay, the invite, and the
paint path to exist before merge semantics are trusted with a real board.

### What is broadcast

Per peer, and never written to `SceneJournal`, the `.slate` file, the HTML
export, or `ops.log` (Article VIII.5):

- Pointer position in world coordinates, or an explicit "left the canvas."
- Viewport rectangle in world coordinates.
- Selected node ids, capped at 64. Past that, a count.
- The text node they are editing, if any.
- Display name, when it changes. Not on every cursor packet.

Cursors and name tags are screen-sized, in the same family as pointer-attached
chrome (`P2.GhostFollow`): a person has to be able to see the cursor when the
board is zoomed out. Selection outlines are the node's world rectangle,
stroked with `canvas_scale::px`, because they describe an object. Viewport
rectangles are not drawn as boxes across the board; they show up in the
roster ("Alex is on slide 4") and when you choose Follow.

Follow copies that peer's viewport into the local camera. It is local derived
state. Leaving Follow, or moving the camera yourself, drops it. It is not
restored next session.

The roster is window chrome: a list in the shared shell, names and colors,
not a canvas object. Article X means the widgets live in `atlas-shell` if
both apps ever grow them. Phase 1 paints the roster from Slate only, using
shell primitives, and does not invent a second chrome language.

### Rates

Cursor packets coalesce to 10 per second per peer. Viewport packets wait
200 ms after the last camera change. Selection and "editing this text" send
on change. The client keeps only the latest presence per peer. A stall never
replays a queue of old cursors.

Twenty peers at 10 Hz is two hundred small packets a second into the relay,
and one coalesced update per peer per frame into the painter. That is the
budget. Above eight remote cursors inside the local viewport, name labels
remain on the three closest to the local pointer. The rest are dots. The
roster still lists everyone.

### Paint

Presence is drawn after the scene and before window chrome, from a struct
updated on the UI thread by a channel. The network thread never touches
`egui`. No per-frame allocation: the name galley is cached until the name or
the zoom bucket changes, and cursor geometry is a reused shape. A presence
repaint must stay inside half a millisecond for twenty peers. If it cannot,
the names drop before the frame rate does (Article II).

### Membership notices, deferred

Article V.2 requires that when one person's move changes which frame owns a
node, the session says so with the author's name. Phase 1 has a single
writer, so the only membership change is that writer's, on their own machine,
which the local journal already shows. The notice is a Phase 2 obligation,
specified there, so it is not built twice.

## Phase 2 — co-editing

Phase 2 starts when `SetProp` exists and item locators can be relative
(DV-01, DV-03). Until then the wire would have to carry whole-node patches
and absolute paths, and both are forbidden by the constitution's own
collaboration clauses.

### The operation

A commit is one gesture: a group of commands, one author, applied with
`apply_all` or not at all. The author on the wire is `{ participant, agent: Option<String> }`.
A human gesture has no agent name. An accepted agent proposal has the agent's
name. Locally that maps onto `CmdAuthor::Human` or `CmdAuthor::Agent`, and
the participant id is what the roster uses to say which human accepted it.

Structural commands, addressed by id, never by index:

- `Add` inserts a node with a new `NodeId` and an order key.
- `Remove` tombstones a node id.
- `SetProp` sets one property of one node.

Paint-layer children use the same three, addressed by `(host, layer, child id)`,
matching the layer commands already on `SceneCmd`. Leaving them out would
fork an image the moment two people touched it.

The first property keys are the fields a gesture actually changes: `rect`
components separately (`x`, `y`, `w`, `h`), `rotation_deg`, `opacity`,
`locked`, `hidden`, the z order key, text body, fill, stroke, corner, and
the portal's locator, title, and query. A drag that moves a node and a
color change on that node commute. A whole-node patch does not, which is
why DV-01 blocks this phase.

`PropKey` is a closed enum owned by `slate-doc`, beside the fields. A new
board style still has to land in the painter and the artifact writer
(Article IV). Once it is a `PropKey`, the wire carries it without a
protocol revision. The relay never matches on it.

Id blocks: on join, and whenever a peer is down to 16 unused ids, the peer
asks for a block of 256 from `next_node_id`, `next_item_id`, and
`next_group_key`. The relay is the allocator, which is the one counter it
is allowed to understand. Each peer keeps a spare block so a short
disconnect can still create nodes. An empty spare fails the create with a
toast and does not invent an id locally. Existing workbooks keep their
small ids. New session ids continue the same `u64` sequence. No client bits
are packed into `NodeId`.

During a drag, the moving rect rides along in presence as a ghost. The
commit fires on release, one group, the same moment `SceneJournal::record`
already journals a live gesture. The op log is not a 60 Hz mouse stream.

Text commits on blur, or after 400 ms of quiet, as one `SetProp` of the
whole string. Keystrokes stay local.

### Order

While the socket is up, the relay assigns one `seq: u64` per accepted group.
That sequence is the total order. Clients apply in that order and in no
other.

Each client also keeps a Lamport clock: one integer, bumped when it sends
and when it receives. The clock rides on the message so a log can show
"this peer had already seen seq 40." It is not a merge input. A hybrid
logical clock, which mixes in wall time so disconnected peers can be sorted
without a server, is the wrong tool here. `lease.rs` already records that
two machines' clocks disagree. An HLC would let the laptop with the fast
clock, or the laptop that rejoins last, win a property it has not looked at
in an hour. D11 dropped vector clocks for the same reason: the relay's
sequence is the order, and inventing a second one recreates the problem the
relay exists to remove.

### Conflict

Three models were real candidates.

A CRDT (a last-writer-wins register per field, plus a text CRDT such as
RGA, plus an observed-remove set of nodes) merges peers who cannot talk to
each other, and it can keep both text insertions. It is a second document
model next to `slate-doc`. Tombstones and causal metadata accumulate for
the life of the file. The weekly board is frames, pictures, and short
labels, and Article III does not justify a text engine to protect two
people typing in one label. Article XII forbids a second owner of the scene.

A whole-document merge, including "sync the `.slate` file and hope,"
collides at the file. That is the failure the lease exists to prevent.

The recommendation is a last-writer-wins register per `(node id, property)`,
ordered by relay sequence. The later sequence is the value. Different
properties of one node both survive. The same property edited in the same
moment keeps the later gesture. The command that lost was applied and then
overwritten; it is not dropped on the floor. If the author's value was
visible and a different participant replaced it within two seconds, that
author gets one line: who replaced which property. A drag ghost does not
toast. This is D11's model, and it is the one that matches `SetProp`.

Deletes are the exception that makes the register safe. `Remove` writes a
tombstone at its sequence. A later `SetProp` on that id fails apply on every
client, and the author is told the node is gone. Tombstones live until the
writer uploads a snapshot past them. The snapshot omits the dead node, and
the relay drops log frames at or below the snapshot sequence. An op that
still names the dropped id fails apply and is surfaced. There is no general
CRDT library inside that scheme, and there should not be one.

Text is one register: the whole string. Two people in the same text node see
each other's "editing" presence from Phase 1. The product does not try to
weave their keystrokes. The last flush wins the string. If simultaneous
typing in one label becomes a weekly complaint, that is the moment to
reopen a text CRDT, as an amendment to this design, for that property only.

### Undo

Undo reverts the local user's last group, not the latest group in the room.
The local `SceneJournal` already stores groups by author and refuses
`amend_top` from a different author. Collaboration keeps that stack local
and unsent, and turns Undo into a new commit: the inverse commands, in a
new group, naming the sequence they invert.

The inverse is submitted only when this participant is still the last writer
of every register the group touched, and the nodes are not tombstoned.
Otherwise Undo does nothing to the scene and says who changed it, or that
the node was deleted. Redo is the same stack in the other direction. The
relay log is not an undo history. Peers do not see "undo"; they see the
inverse edits arrive like any other edit.

A foreign undo never rolls back someone else's work, because the inverse is
just a `SetProp` of the earlier value, and it is rejected when the register
has moved on.

### Offline and rejoin

A dropped socket keeps the local scene and queues groups the user makes
during the blip. The UI says the session is reconnecting. On reconnect the
client sends `Hello` with the last sequence it applied. The relay replies
with a snapshot only if the client's sequence is at or below a compacted
prefix, then the ops after that.

Queued groups are not ordered by their old Lamport clock against the
meeting. Each one is submitted only if every register it writes still sits
at the sequence the user edited from. A register that moved is left as the
room left it, and the group is listed back to the user as not applied. A
laptop that was asleep through the review cannot clobber the review by
rejoining. That is stricter than last-writer-wins, and it applies only to
the queued blip. Two people connected at the same time still resolve by
sequence, as above.

The spare id block covers creates during the blip. An exhausted block
refuses creates until `Alloc` succeeds.

If the grace window has expired, the relay has deleted the room. The client
loads the `.slate` from disk, discards the dead queue with a clear notice,
and may start or join a new room. There is no silent stitch of the dead log
onto the file.

### Who writes the file

The relay log is the live session. The `.slate` file is the document
(Article IX, decision D18). Exactly one participant writes it: the process
that holds `board.slate.lock`. Slate already knows how to take that lease
and how to refuse a second writer.

Rules:

- Peers who do not hold the lease set the tab read-only for `save_to` and
  for anything that is not a session commit. Session commits go through the
  relay even though `commit_scene_as` would refuse them today. The file
  lease and the session are different gates, and the code has to grow that
  distinction instead of overloading `read_only`.
- The holder autosaves the converged scene 30 seconds after the last applied
  remote or local commit, and again when the session ends or the holder
  leaves. The write is the existing `save_to` (temp file, then rename).
- An autosave preserves the `view` that was on disk when the session
  started, including the camera. It does not copy the holder's live camera.
  An explicit Save still does, and that pose is where a later cold open
  lands. Peers do not move their cameras because a snapshot arrived.
- Autosave and the relay snapshot strip machine-personal portal fields:
  `AgentPortalRef.channel`, `AgentPortalRef.session`, and any secret,
  cookie, or consent. `instruction`, model name, and the locator stay.
  Those channel fields are journaled on the node today. Broadcasting them,
  or flushing them onto a shared folder, would hand the writer's provider
  conversation to every peer. Solo use outside a session keeps today's file.
- Other peers do not watch the file in order to merge. While the session is
  up, the relay is the authority. A cloud client rewriting the `.slate`
  underneath them must not trigger a reload over the live scene. Cold open,
  after the session, reads the file as it does now.
- When the holder leaves they flush first, then drop the lease. The relay
  keeps the log for the grace window. The next peer who can `create_new`
  the lock file becomes the writer and flushes. The relay pins that
  participant as the only snapshot uploader.
- If nobody can take the lease (the share is down, everyone remaining is
  remote), the log survives until grace, and then the last flushed file is
  the document. Ops that never reached a flush are gone. The 30 second
  autosave is the bound on that loss. The UI says so when the writer
  disappears.

The lease is strong on SMB, where `create_new` is atomic, and weak on
OneDrive, Dropbox, and Box, which replicate a lock file whenever they get
around to it. In a session the relay's pinned writer is the authority.
The lock file is still taken, so a Slate that is not in the session keeps
the read-only behavior it has today. Two session members both calling
`save_to` is a bug in the client, not a conflict policy.

Snapshot upload is the writer's scene at a sequence, same JSON `save_to`
would produce, personal fields stripped, camera preserved as above. The
relay stores it and discards older frames. A late joiner applies the
snapshot, then the ops after it. They do not need the original holder to be
awake (D18).

On relay restart the on-disk log and snapshot are the session, and the same
room key resumes it. If the data directory is gone, the epoch bumps, clients
fall back to the `.slate` file, and unflushed ops that existed only in the
dead process are lost. Same bound: the last autosave.

## Linked assets

Each machine resolves a locator against the directory it opened the workbook
from. `resolve_source` is that function, and it is the one portals already
call. A session must not replace it with "ask the host for the bytes."

Worked example. The workbook is `Boards\Deck.slate`. Ada opens it from
`C:\Users\Ada\OneDrive - Firm\Boards`. Ben opens the same synced tree from
`D:\Box\Boards`. A photo that lives at `Boards\photos\north.png` is stored
as `photos/north.png` by `source_locator`. Both machines join that onto
their own copy of `Boards` and get `Ok` once the sync client has the file.
A file that exists only on Ada's desktop is stored absolute. Ben's
`LinkHealthCache` reports `Missing`. The card shows the missing state the
app already has. The session does not upload Ada's desktop, and it does not
read the file to invent a preview.

Cloud placeholders make this sharp. On the firm's OneDrive, most files are
dehydrated: the directory entry is real and the bytes are not.
`link_status` uses `try_exists`, so a placeholder is `Ok`, not `Missing`.
The thumbnail policy already refuses to read those bytes, because a one-byte
read downloads the whole file. A shared session inherits that rule without
a special case. Ben may see a type icon for a file his client has not
hydrated. That is an honest card. Hydrating the folder to improve his
meeting is the bug `docs/performance.md` and the cloud guard exist to
prevent.

`<workbook>/assets/` is how bytes travel with the board when they need to.
Screenshots already write there. The package work copies linked sources
there and rewrites locators to be relative, with origins in `manifest.json`.
In a session, placing a file that is not already under the workbook
directory should offer that same copy — one file, the existing package
primitive, provenance included — so the sync client replicates a relative
locator instead of an absolute path only Ada can resolve. It is a human
action. It is not automatic on every place, and an agent cannot invoke it
(Article IX.5: write-back and copying into the workbook are explicit and
human). Peers then resolve `assets/...` like any other relative locator.
Until the copy finishes syncing, the peer's health is `Missing` or
`Unknown`, and the board stays responsive (Article IX.3).

DV-03 is a gate because the writer flushes `SlateItem` records. If those
records still contain Ada's absolute path, her autosave publishes paths Ben
cannot use, and the next cold open on his machine breaks cards that were
fine a minute earlier. The flush has to persist the relative locator
`source_locator` already computes. Portal `SourceUri` is already in that
shape.

What does not move across the wire in Phase 2: file bytes, thumbnail bytes,
WebView captures, model meshes, PDF bytes. A peer with `Missing` sees the
missing card. A peer with `Ok` builds their own thumbnail through the
existing pool.

A later phase can add content-addressed *previews* if hybrid meetings where
nobody shares a folder turn out to be weekly. The shape, if ratified, is
narrow: the writer serves bytes already in the local thumbnail cache, keyed
by the existing cache key, never by reading a dehydrated file, never an
original, retained with the same grace window as the log, and the card stays
`Unknown` until those bytes arrive. That is D11's asset note, postponed.
It is not part of the first co-editing milestone. The path for "Ben has no
share at all" remains Package: a folder he can open cold, which Article IX.4
already specifies.

## Portals

A portal frame syncs. Its contents do not. Article V.3's three classes:

**Generated** (File Atlas on the board, and any future deterministic view).
The journaled frame syncs: rect, title, `SourceUri`, `AtlasPortalQuery`
(sort and the explicit file list). Each peer that can resolve the locator
regenerates the tree locally. Each peer that cannot shows the empty portal
and `Missing` or `Unknown`. The inner camera, the scan, and the thumbnails
are derived and stay derived. Shipping the scanned tree over the relay
would freeze one machine's directory listing into everyone else's board and
would walk a share on their behalf.

**Document** (a Slate portal onto a child workbook). The frame and the child
locator sync. The child has its own journal and, if anyone ever shares it
live, its own room. Phase 2 does not edit the child in place. Double-click
opens the child in a tab on that machine, which is the rule already chosen
for the first version of document portals. A cycle is still refused by
`slate_embed_refusal`.

**Host** (web). The locator, viewport mode, export mode, and the other
fields on `WebPortalRef` sync. Cookies, scroll position, the WebView2
profile, origin consent, and the live texture do not. Consent is per user
and is already forbidden from the file. A peer who has not allowed the
origin sees the consent gate, not Ada's signed-in page. The poster is
derived. Each machine captures its own, or shows the unbound state. The
export form stays a poster plus a pointer for a remote page.

Membership follows the frame that contains the node's center, as
`Scene::frame_of` does now. Because that is derived, one person's move can
change which slide a node belongs to. Phase 2 announces that when a remote
commit changes `frame_of` for a node the local user has selected, or for a
node on the slide they are presenting: "Alex moved North elevation onto
slide 3." The notice is suppressed when the local user authored the commit,
and when the node was not selected and not on the current slide. It is not a
journal entry.

## Agents

An agent does not open a relay connection. The human's Slate is the only
client (Article VII: one command surface, no parallel mutation path).

Proposals stay in the staging layer they already use
(`crates/slate-doc/src/stage.rs`, applied from `apps/slate/src/app/board_agent.rs`).
The proposal files live in the AI workspace, which is per machine. Other
participants do not see a pending proposal. Reject stays local.

Accept is a human action. It commits the proposal's commands through the
relay as one group whose author is that agent, attributed to the participant
who accepted. Everyone then sees ordinary ops, labeled with the agent name.
A proposal that no longer applies is already `Stale` locally; the relay
never receives it.

Solo autonomy ("this agent may commit without asking") does not extend into
a shared session. The room has other people in it, and an unattended agent
committing at machine speed is the failure WI-9d already flags. While a
session is connected, acceptance is required even if the workspace grant is
on. That narrowing of Article VII.6 is one of the amendment drafts below.
Until it is ratified, the implementation should still stage, and the draft
is the honest way to avoid quietly shrinking a grant the constitution
allows.

Agent portals sync authored fields (instruction, model, view). They do not
sync `channel`, `session`, or the live runtime, as the snapshot rules above
require. Two people do not share one provider conversation by sharing a
board.

## Security and privacy

The threat model is a firm that trusts the people it invited and trusts the
machine the relay runs on. It is not a threat model for strangers.

What the room key plus TLS actually does: keeps a non-invitee from speaking,
and keeps the path from reading the board. What it does not do: prove that
the person who typed the name "Ada" is Ada, stop an invitee from publishing
the key, or stop the relay operator from reading `ops.log`. Display names
are self-asserted. Say that in the session UI in one line, not a policy
document.

Concrete obligations:

- The relay logs sequence numbers and participant ids at info level.
  Payloads are debug-only and off by default.
- Presence is memory-only. It is not in `ops.log`, and it is dropped on
  disconnect. Article VIII.5 also forbids restoring it. A review of "where
  did Alex look" is not a feature.
- Rate limits per participant: presence 20 messages a second, commits 30
  groups a second, `Alloc` 4 a minute. A client that exceeds them is
  disconnected. Joins past 32 are refused. Messages over 256 KiB are
  refused before they are stored.
- Absolute paths and any path `is_machine_private` would refuse are stripped
  before a commit is sent. A locator that cannot be made relative is omitted,
  and the sender keeps the local node. The wire carries `photos/north.png`,
  not `C:\Users\Ada\...`.
- The relay snapshot and the session autosave null `channel` and `session`
  on agent portals. They omit the invite file, the lock file, the WebView
  profile, and the secret store.
- Snapshot uploads are accepted only from the pinned writer. Everyone may
  submit commits. That is the product: the room is trusted to edit. A
  malicious invitee can delete the board. The lease holder's last flushed
  file, and ordinary undo of their own work, are the recovery. Building
  ACLs to stop an invitee is the permission-region feature already refused.
- The grace-window directory is the sensitive artifact. The operator
  statement in the relay README is part of the feature, not a comment.
- Stolen invite: start a new room, new key. Old `Hello` fails.
- Clock skew does not grant a lease while the relay is up, because the
  pinned writer, not the heartbeat timestamp, decides who saves.

## Performance

Article II applies to the session the same way it applies to a scan.

- The UI thread never reads the socket and never calls `save_to`. A
  channel delivers already-decoded groups. `save_to` and snapshot upload
  run on a worker.
- Apply at most 32 groups or 2 ms of apply work per frame, whichever comes
  first. The rest stays queued and the frame requests another paint. This
  is the same shape as the watcher budget in File Atlas.
- Presence coalesces to the latest packet per peer per frame.
- Presence paint stays under 0.5 ms for twenty peers, names dropped first.
- A commit is one group per gesture, not per mouse move. A text body is one
  set per flush, so a paragraph is one register write.
- Joining applies the snapshot off the UI thread and swaps it in. The first
  paint may be the board they already had from disk; it must not be a modal
  spinner across the window (Article II.4).
- No thumbnail, metadata, or file-byte work is added to the apply path.
  Link health keeps the cache it has.

Twenty editors changing properties is a small amount of JSON. The costs that
will actually show up are a snapshot that is a large pretty-printed workbook,
and a board full of missing-file cards if the assets were not in the synced
tree. Both are reasons to keep snapshots infrequent (the 30 second flush, and
compaction when the log passes 2,000 groups) and to make `Missing` cheap to
paint, which it already is.

## Tests

The merge and the simulated relay run in `crates/atlas-relay` with
`cargo test` on Linux and on Windows. No display, no listener, required in
CI. The socket tests bind `127.0.0.1` only and are also required; they do
not need a LAN.

Named tests, all in-process, many clients, one simulated relay:

- `twenty_peers_converge_under_random_interleaving` — a fixed schedule, not
  a time-based race. After the same groups, every client hashes equal and
  matches a single-threaded apply of the sequenced log.
- `distinct_properties_both_survive` and `same_property_keeps_later_seq`.
- `delete_tombstone_rejects_a_later_set`.
- `presence_never_enters_the_journal` — feed presence, assert the journal
  and the serialized `SlateDoc` are unchanged.
- `undo_own_group_when_still_last_writer` and
  `undo_refused_after_a_foreign_write`.
- `offline_queue_drops_a_register_that_moved` and
  `offline_queue_keeps_a_register_nobody_touched`.
- `late_joiner_applies_snapshot_then_tail`.
- `only_the_lease_holder_writes_the_file` — a second client commit changes
  its scene and does not call `save_to`.
- `autosave_does_not_replace_the_saved_camera`.
- `room_key_and_agent_channel_absent_from_snapshot_bytes`.
- `relative_locator_round_trip_across_two_workbook_dirs` — the same locator
  resolves under two different parent directories; an absolute desktop path
  is `Missing` in the second.
- `malformed_group_skips_on_every_client_and_advances_seq`.

A test that needs a real shared folder or a cloud placeholder stays
`#[ignore]`, the way `folder_probe` and the cloud guard already do. The
convergence tests must not.

## Milestones

Each phase is done only when its acceptance lines are true. Later phases do
not start early to "save a rewrite."

**Gate, before Phase 2 code.** User has ratified this document, including the
asset-streaming deferral and whichever amendment drafts they want. T1.1a–c
have closed DV-01 and DV-08. Item save persists a relative locator when the
file is under the workbook (DV-03 for the fields a flush writes). `PropKey`
lives in `slate-doc`.

**Phase 1 — presence.** A second Slate on another machine, given the invite,
sees the first user's cursor, selection, and viewport within a frame or two,
and sees them disappear on disconnect. The second user cannot save and
cannot change the scene. The `.slate` bytes are identical before and after
the session, apart from the lock file appearing and disappearing. The room
directory is gone within the grace window after the last disconnect.
`presence_never_enters_the_journal` passes. Killing the relay leaves both
apps editing locally, as they do today.

**Phase 2 — co-editing.** Three clients (the tests use twenty) edit one
board through a relay on `127.0.0.1`. Moves and color changes on the same
node both stick. A same-property race keeps the later sequence and tells the
loser. Undo reverts only the undoer's last untouched group. A late joiner
matches without the original holder connected, as long as a snapshot exists.
Only the lease holder creates the temp file `save_to` uses. After the holder
disconnects and grace expires, a cold open shows the last autosave, and a
queued edit from a sleeping client is reported as not applied rather than
merged by timestamp. A file that is not on the second machine is `Missing`.
A file under `assets/` with a relative locator resolves on the second
machine once the bytes are there. An agent proposal is invisible to the
other clients until accepted, and the accepted group carries the agent name.

**Later, only if ratified.** Preview-byte service for `Missing` cards, from
the writer's existing thumbnail cache, under the retention rules above.
Not scheduled by this document.

## Open questions

These need an explicit answer before the corresponding code. The rest of
this document is the recommendation and can be reviewed as written.

1. **Adopt this as the S3 protocol** that Wave 4 is gated on, and treat
   `docs/collab.md` as the only copy (the workplan's `collab-protocol.md`
   name is not a second document).
2. **One crate, `crates/atlas-relay`.** This retires the `slate-relay` plus
   `atlas-collab` split in the workplan. Confirm the name.
3. **Defer D11's preview streaming** out of the first co-editing release.
   Phase 2 uses the shared folder, `assets/`, relative locators, and
   `Missing`. Confirm that the weekly hybrid meeting actually has the
   folder, or say that streaming is still on the critical path and Phase 2
   must include the narrow preview service.
4. **Offline policy.** Connected conflicts are last-writer-wins by relay
   sequence. A reconnect queue drops any register that moved while the peer
   was gone. Confirm that a longer offline merge is out of scope.
5. **Text.** The whole string is one register, with an "is editing" presence
   mark. Confirm that a character CRDT is out of scope.
6. **Agent autonomy.** A connected session always stages, even when the
   workspace has a solo autonomy grant. That needs the Article VII.6 draft
   below if it is going to be law rather than a quiet narrowing.
7. **Article VI.3 draft** below, so presence is allowed on the wire without
   a later "fix" that journals cursors. Article IX.6 draft, so a session log
   cannot grow into a second database. Ratify, edit, or reject each one.
8. **Defaults to confirm:** grace window 15 minutes, autosave 30 seconds,
   compaction at 2,000 groups, join cap 32 with a design target of 20,
   cursor presence at 10 Hz.

## Amendment drafts

Unratified. Not applied to `CONSTITUTION.md`.

### Draft — Article VI.3, presence exception

Article VI.3 ends: "Where derived state is shared between participants it
must be deterministic, so that peers reproduce it from the journal rather
than receiving it over a wire." Article VIII.5 says cursors, viewports,
selections, and membership are broadcast and never journaled. Those two
sentences disagree. Presence is shared, derived, and not a function of the
journal. Portal contents, simulated motion, playheads, and trails are.

Replace the last sentence of VI.3 with:

> Where derived state is a function of authored intent — portal contents,
> simulated transforms, playheads, trails — peers reproduce it from the
> journal rather than receiving it over a wire. Ephemeral presence
> (Article VIII.5) is the exception: it is broadcast because it is not a
> function of the journal, and it is still never journaled, never exported,
> and never restored.

This supersedes only that sentence. It does not move presence into the
document.

### Draft — Article VII.6, shared sessions stage

Article VII.6 allows an explicit autonomy grant to skip human acceptance.
A live room makes that grant surprising: the other participants did not
grant it.

Add to VII.6:

> A connected multi-person session suspends autonomy for that workspace.
> Agent mutations in the session enter the staging layer and require a
> human in the session to accept them. The grant resumes when the session
> ends.

### Draft — Article IX.6, the session log is not a document

Article IX.1 forbids Slate from quietly becoming a database. A relay that
stores ops for a grace window is easy to "improve" into the store of record.

> **IX.6 — A session log is not a document.** A relay may store a session's
> operation log and snapshots so a peer can catch up. That store is deleted
> with the session, after a bounded grace window. It is not a second copy of
> the workbook, it is not opened as a workbook, and it does not outrank the
> `.slate` file the user saved. No capability may require the log to exist
> in order to open, edit, or export a workbook.

This records decision D18 as law. It does not require a relay for local use,
which Article I.4 already protects.
