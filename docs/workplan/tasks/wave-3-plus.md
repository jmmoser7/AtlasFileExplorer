# Waves 3–5 — sketches

These are **not task cards**. They are scoped intents with their decisions
already made, so that Waves 0–2 do not accidentally foreclose them. Each becomes
a full card only when its gate opens, because the details depend on what the
earlier waves teach.

Writing these out now costs an hour and prevents the two expensive mistakes:
building a Wave 2 API that Wave 4 cannot use, and re-litigating settled
decisions when the wave finally starts.

---

## Wave 3 — share

**Gate:** Wave 2 merged, plus one week of daily use of the collage and the lease
guard. The week is not a formality: WI-5 and WI-8 are both shaped by what
actually breaks when the tool is shared every day.

### T3.0 · Delete the `SlateItem.path` mirror
Small cleanup card. T2.1 kept `path`, `size`, `mtime`, and `cache_key` as
documented mirrors of `uri` / `content` to avoid touching forty call sites in
three lanes at once. This card converts every reader to `item.uri` /
`item.content` and deletes the mirrors. Mechanical, one lane, best done while
nobody else is in `board.rs`. **Acceptance:** the fields are gone; no behaviour
changes; `cargo test --workspace` green.

### WI-5 · Package — the InDesign model
**Why:** decision D10, and D1 makes "send this board to someone" a weekly act.
Amendment B / IX.4 as revised: **a package is a permanent fork, not a
synchronised mirror.**

Shape — deliberately smaller than the original spec:

- **Package** copies every linked file into `<name> Package/Links/`, rewrites
  the workbook's locators to package-relative, and optionally zips the folder.
  The result is an ordinary workbook that owns its assets. No sealed mode, no
  "sources not verified" state machine, no re-link command, no diffing against
  origins, no divergence tracking.
- `manifest.json` records the origin `SourceUri` of each asset. Pure provenance
  so a human can find the original by hand — that is what satisfies IX.4's
  honesty requirement, and it is a text file rather than a synchronisation
  system.
- Expect 2–8 GB for a real competition board; the transport is a share or a
  drive, not email. A **thumbnails-and-previews-only** variant (20–50 MB,
  presents perfectly, cannot be zoomed to pixel level) is worth building second
  if the full package proves unwieldy — decide after the first month.
- Reuses `slate-artifact`'s existing asset pipeline (`assets.rs` already copies
  and dedupes); this is that pipeline pointed at a `.slate` instead of an
  `index.html`.

Also in this card, because it is the natural moment: **the board-vs-artifact
golden parity test** that closes DV-07.

**Acceptance sketch:** package a board with 40 linked images; open it on a
machine with no access to any source; present it fullscreen with no missing
card; the manifest names all 40 origins.

### WI-4 · Extract `Edge` from `NodeKind::Connector` (format v5)
**Why:** tldraw shipped arrows-with-embedded-bindings and then migrated the
bindings out; you are at their pre-migration design at a fraction of the cost
(audit F8). A connector has no independent extent — its geometry is derived —
which is the definition of an edge, not a node.

Shape:

- `Scene.edges: Vec<Edge>`; `Edge { id, role: EdgeRole, from: Endpoint, to: Endpoint, presentation, priority }`.
- **`EdgeRole` ships with `Connector` only.** Decision D3 removed `Membership`
  from scope permanently; `Context` and `Provenance` arrive with Mode 2,
  `Control` with control surfaces, and the **force roles** (`Gravity`, `Spring`,
  `Repulsion`) with the dynamics layer that S2 specifies — which is the first
  real use for typed edges beyond wires, and the reason this migration is worth
  doing rather than merely correct.
- `connector_bezier` and friends stay exactly as they are — derived geometry was
  already right. **Fold in the routing consolidation** that closes DV-11: an
  `EdgeRouting` enum (`Bezier`, `Orthogonal`, `Straight`) living beside the
  geometry in `slate-doc`, with Lens abandoning its private
  `lens_orthogonal_route`. Per the open/closed rule in `EXTENDING.md`, routing
  is taste and its enum is open.
- An adjacency index on `Scene` so "what connects to this node" is not a linear
  scan.
- v5 migration moves every connector node into an edge, preserving id and z.

The blast radius is real and known: roughly sixty `NodeKind::Connector` match
sites across board paint, the wire tool, clipboard, overlays, present mode, and
the artifact writer. Split the card by consumer if it exceeds one session.

**Acceptance sketch:** every existing connector test passes against the new
representation; a v4 → v5 fixture round-trips; the artifact writer emits
byte-identical SVG for a scene of connectors before and after.

### WI-8 · Product B, the shared file
**Why:** decision D1 — this is the workhorse. Most of the office value of
"collaboration" is sequential: I mark up the board, you open it tomorrow and see
my markup, attributed.

Shape, building directly on the Wave 0 lease:

- Lease upgrade: **request the lease** from the holder ("Jane is editing — ask
  for control?"), and hand it over cleanly.
- **Reload with changes:** detect that the file on disk changed under a
  read-only viewer and offer a reload that keeps camera and selection.
- **Authorship display:** the journal already carries `CmdAuthor`; surface "last
  edited by" per session and in the history overlay.
- **Membership-change notices** (decision D3, edge case E6): when a reload
  brings in someone else's move that changed a slide's membership, say so with
  the author's name. Never silent — Amendment E / V.2.

**Explicitly not in Product B:** simultaneous editing. That is C1.

---

## Wave 4 — live collaboration

**Gate:** Wave 3 merged **and** the S3 protocol document adopted by the user.
This is no longer gated on a named unmet need: decision D11 makes simultaneous
multi-user editing a core capability of the product.

This is the largest body of work in the plan — months, not weeks — and it should
be split into four cards once S3 is adopted, roughly along these lines.

### WI-9a · The relay
`slate-relay`, a small separate binary with no document knowledge beyond
ordering and fan-out: accept connections, authenticate a session code, assign
monotonic sequence numbers, broadcast, hold the delta log for late joiners. TLS.
Deployable by a firm on any box or VPS today, and by this project as a hosted
service later, **from the same source** (Amendment F / I.4). No accounts.

### WI-9b · The session client
`crates/atlas-collab` grows a transport and the app grows a session lifecycle:
start, join by code, leave, reconnect, save (the workbook lease from T0.4 decides
who owns the file on disk — reuse it rather than inventing a second authority).
Commits go out as property-scoped command groups; rejections are surfaced, never
dropped.

### WI-9c · Presence and the interface
Cursors, selections, viewports, participant list, and per-author colour.
**Never journaled** (Amendment B / VIII.5). Includes the two interface problems
that the merge mathematics do not solve: keeping twenty cursors legible, and
announcing membership changes with their author when someone's move re-slides
your deck (Amendment E / V.2).

### WI-9d · Session asset delivery
The piece neither audit caught, and the one that decides whether hybrid works at
all. Remote peers cannot resolve the host's paths, so they see an empty board
unless thumbnails and previews stream over the session. Request by `cache_key`
(already project-relative), serve from the host's existing cache tiers, cache on
receipt, show an honest `Unknown` card while in flight (Amendment A / IX.3).

Two rules carried forward from the audits: portal contents are per-peer and are
not synced — the frame syncs, the contents regenerate locally (S1 covers this
per portal class) — and agent proposals stay in the staging layer, per-peer
until accepted, so an agent cannot rearrange a board under twenty people at
machine speed.

**Acceptance sketch:** a dozen participants, some remote, join one session
through a firm-run relay; all of them edit; all of them see each other's
cursors and changes within a frame or two; all of them see the images; killing
the relay leaves every participant with a coherent local copy.

---

## Wave 5 — after audit №3

Listed so that nothing in Waves 0–4 accidentally blocks them. None is scheduled,
and none should be specified further until audit №3 re-scores them against real
usage. The first three come from the flexibility decisions and each has a spike
or a contract behind it already.

- **Web-view host portals** (D15, S1). Rendered offscreen to a texture so they
  are canvas-native — they rotate, zoom, hit-test, and export as a poster plus a
  pointer. This is the only host-portal tier planned; foreign OS application
  windows are not (they would cost rotation, zoom, occlusion, export, and
  visibility to remote peers).
- **The dynamics layer** (D12, S2): the canvas clock, dynamics components, force
  edges, journaled impulses, derived motion, and baking. Depends on WI-4 for
  typed edges. Its first demonstration is the orbital toy — two circles, a
  gravity edge, a fling, and a trail you can bake into a path.
- **Filmstrip video scrubbing** (D13): hover-scrub across a video node driven by
  ~100 keyframes extracted into the existing thumbnail cache tier, click to play
  with real decode. The filmstrip cache can land earlier than the rest since it
  reuses the thumbnail pool.
- **The extension package format** (D14): a manifest and install path for
  declarative assets — themes, keymaps, skills, brushes, board templates — plus
  documentation of the MCP-server-as-plugin path that Wave 2 already built.
- **Generative image nodes** (D16) as authored content carrying their provenance
  (prompt, model, seed).
- **Agent Mode 2 — agent nodes on the canvas.** Needs typed edges (`Context`,
  `Provenance`), durable journaled memory with the pinned/learned split
  (Amendment C / VII.5), and the held-back Article VII.8 ratified.
- **`ControlSurface` + `PanelSpec` registry** (audit §7, Amendment D). Deliberately
  unratified: it is the third interpreter of one model, it is the right idea,
  and it should not become law until something implements it. Requires the
  deterministic-resolution and flatten-on-export rules to land *with* it, plus
  the golden parity test WI-5 introduces.
- **Portals** (Roadmap Phase 3): `NodeKind::Portal`, Grid → Venn → Lens
  migration. The graph work in WI-4 is its prerequisite, which is why `Portal`
  belongs in the taxonomy before it exists in the code.
- **Canvas painting of staged proposals** — the tinted, attributed overlay
  deferred from T2.3.
- **Journal persistence**, if and only if a named use appears. Today the journal
  is session-local and that is a feature, not a gap.

---

## Audit №3 — what to bring

For the next audit to diff rather than re-impress:

1. `cargo xtask metrics` snapshot at that date, against the 2026-07-25 baseline.
2. `docs/audit/deviations.md` — what closed, what opened, what stayed open and
   why.
3. Which cards shipped, which were escalated, and which cards turned out to be
   wrong (the last list is the most valuable one for improving this process).
4. Answers to the questions Wave 3 and Wave 4 raise in practice: did the
   sync-client shortcut hold? Did the collage get used weekly, or was it a
   one-week novelty? How many people actually edited at once, and what broke
   first — the merge, the assets, or the cursors?
5. If the repository is public by then: what strangers asked for, and which
   flexibility class each request landed in. That distribution is the real test
   of whether `EXTENDING.md` and the enum rule are doing their job.
