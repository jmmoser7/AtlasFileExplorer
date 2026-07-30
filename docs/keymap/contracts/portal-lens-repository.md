# Repository Lens portal — interaction contract

Status: **draft** — every row below is `proposed` in `decisions.json` and
awaits accept / alter / reject. Four open questions are live at the bottom.
Family: portal
Portal class: **generated** (Art. V.3 / decision D7) · Type: **lens** ·
Subtype: **repository**
Reference: GitKraken Desktop & GitLens commit graph, GitHub's network graph,
`git log --graph` — see `docs/keymap/research/git-history.md`
Command: `board.portal.repo_lens` (placement) · Key: none in v1 (OQ3) ·
Palette: "repository lens" (aliases: repo lens, git graph, history portal)
Inherits: P0.* (all), P1.node, **P2.DragShape** — deviations flagged below.
Portal-class rules stay L3 here until a second portal contract exists
(P1.portal, promotion rule).

## What it is, and the 10% it implements

A journaled frame on the board whose contents are the commit DAG of one git
repository, drawn as **branching, merging, and forking over time**: time runs
left to right, concurrent lines of work stack as lanes, merges converge, and
every ref that the clone can see — local branches, tags, and the branches of
every configured remote — is labelled at its tip.

The real use (Art. III demands one be named): reading the shape of a
history — *when did these two lines of work diverge, how long did they run
apart, when did they come back together, and which crates existed at that
point* — on a board where the answer can sit next to the drawings, notes, and
code Lens it explains. This repository's own history, built by waves of
parallel agents on `feature/*` branches, is the first subject.

The 90% deliberately not implemented is in D15: this is a reading instrument,
not a git client. Nothing in it stages, commits, checks out, branches,
rebases, or pushes.

## Pushback before agreement (Art. XI)

The request named a GitHub repository as the source. Two clauses bind that,
and the contract answers both rather than quietly complying:

1. **Article I.4 — no capability may require an account with any party.** A
   portal whose source *is* GitHub requires a GitHub account for anything
   private and a token for anything rate-limited. The conforming alternative
   in D19: the source is a **local git worktree or bare repository**, read
   directly from its object database, which needs no account, works offline,
   and is `SourceKind::LocalFs` — no new variant in a closed enum (governance
   rule, decision record). Hosted metadata (the fork network, pull requests)
   is an **optional out-of-process enrichment** (Art. VII.8) whose absence is
   `Unknown`, never a blank and never a guess.
2. **Article IV.2 and false-affordance register row 4 — graphs are extracted,
   never hallucinated.** A clone does not know who forked it. What it knows is
   every ref of every configured remote, and for a multi-remote clone that is
   the honest fork picture; the strangers' forks are not derivable and are not
   drawn. The portal captions what it is showing (`remotes: origin, upstream`)
   so the reader cannot mistake a partial network for the whole one. The same
   rule catches shallow clones: history truncated by `--depth` is marked, not
   presented as a beginning.

Neither point reduces the requested view — the branching, merging, and forking
of this repository over time is exactly what a local clone with its remotes
can prove.

## Dependencies and sequencing

This contract is written ahead of the machinery it needs, deliberately: specs
before code (Roadmap standing priorities), and the durable asset is the
contract.

| Needs | State | Effect on this contract |
|-------|-------|-------------------------|
| S1 portal taxonomy (`docs/portal-contract.md`) | not written | This contract answers S1's six questions **for one generated subtype only** and must conform to S1 when it lands; where they disagree, S1 wins and this file is amended. |
| `NodeKind::Portal` (Roadmap Phase 3) | not built | v1 lands as a portal node. It must **not** ship as a fifth tab-level `ViewKind` — those are legacy by Art. V.1 and each new one is debt to retire. |
| `SourceUri` + tri-state health (T2.1) | specced, not built | D19/D30 use it as specced; until it exists there is no repository binding. |
| `crates/repo-graph` (new pure crate) | not built | Extraction + layout, UI-free, Linux-testable (Art. I.1/I.3), mirroring `code-lens`'s split. |

## Deciding this matrix

The skill's usual surface is an interactive canvas beside the chat; this
contract was drafted by a cloud agent, where that canvas cannot be rendered or
clicked, so the matrix below **is** the decision surface. Reply by row ID —
"D04 accept, D13 alter: <text>, D23 reject" — plus an answer to each open
question; the rows then flip in `decisions.json` (altered text adopted
verbatim at confidence 100), the status moves to agreed, and implementation
starts against the golden paths. Nothing here is built yet.

## Behavior matrix

Rows keyed to `DIMENSIONS.md` in registry order: D01–D17 (`any` scope) answer
the **placement gesture and the frame**; D18–D31 (`portal` scope) answer the
portal itself. Every row is mirrored in `decisions.json`.

| ID | Dimension | Proposed behavior | Source | Conf |
|----|-----------|-------------------|--------|------|
| D01 | Initiation & arming | Palette: type "repository lens" + Enter (aliases: repo lens, git graph, history portal); tools-rail **Portals** flyout; command `board.portal.repo_lens`, board tabs only (P0.7/P0.8). No single-key chord in v1 — portals are placed rarely and the single-key space is spent on drawing tools (OQ3) | guess | 55 |
| D02 | Stickiness & repeat | One-shot: commit returns to Select; Space/Enter re-arms (P2.DragShape, P0.4) | precedent | 90 |
| D03 | Gesture grammar | `Armed → Dragging(rect) → Committed(unbound)`. Press-drag-release defines the frame; release commits an **unbound** portal painting a "Choose repository…" empty state (the Lens view's empty-state precedent). Binding is a separate, non-modal step (D19) — no file dialog ever opens inside a draw gesture | pattern | 75 |
| D04 | Click vs drag rule | Travel > `draft.drag_threshold` (4 px) = drag grammar, frame = dragged rect. Travel below it places a `portal.repo.default_size` frame (960×540 world units) centred on the click, exactly as `board::place_frame_at` does for the Frame tool. **Deviates P2.DragShape**, which discards sub-`MIN_DRAW` releases: for a node this size a click that produces nothing reads as a broken tool | precedent | 85 |
| D05 | Modifiers | Shift during the drag locks **16:9**; unmodified drags are free-aspect. **Deviates the Frame tool**, whose `frame_drag_rect` locks the preset aspect unconditionally, and **deviates P1.shape.aspect**, whose Shift means square: the width of this frame is *how much history is on screen*, so a fixed page aspect would be the wrong default. Ctrl/Alt unassigned in v1 | guess | 60 |
| D06 | Constraints & snapping | F9 grid snap and smart guides apply to the frame rect exactly as for a Frame node (P1.node.move); F8 ortho is `n/a` for an axis-aligned rect. Contents never snap to anything — they are derived geometry | pattern | 80 |
| D07 | Direction / value locks | `n/a` — no directional parameter in a rect placement | pattern | 85 |
| D08 | Numeric / manual entry | `n/a` in v1; typed frame dimensions are listed as a non-goal (D15) rather than invented for one node kind | guess | 55 |
| D09 | Preview & readouts | During the drag: frame outline in `Palette::portal` + live w×h in the dock readout (line D09 precedent). Bound and idle: the dock readout shows repository name · HEAD short ref · commits in window · as-of pin · health (`Ok`/`Unknown`/`Missing`). Focused commit replaces the middle fields with short SHA · author · date · subject | precedent | 80 |
| D10 | Cursor | Crosshair while armed (line D10 precedent). Over an unfocused portal: arrow. Inside a focused portal: arrow, `PointingHand` over a commit dot that has an open action (D22) | precedent | 75 |
| D11 | Commit | One journaled `Add` of a portal node: `{ rect, class: Generated, kind: RepoLens, source: None, query: RepoQuery::default() }`. Frame, source, and query are journaled; contents never are (Art. VI.3). One gesture = one undo (P0.2/P0.3). No style state consumed (D16) | pattern | 85 |
| D12 | Cancel | Esc peels, one layer per press (P0.1): contents focus → drag draft → armed tool → selection. A portal with focus registers the contents-focus layer for that frame only | pattern | 80 |
| D13 | Selected presentation | The frame selects and shows the standard eight resize handles plus rotation, like a Frame node (P1.node). Contents expose **no** grips and are not selectable — a generated portal owns no mutations (Art. V.3). **Resize re-lays-out, it does not scale**: frame size is a layout input, so a wider frame shows more history rather than a stretched picture. Rotation rotates the painted result, as for any node | guess | 60 |
| D14 | Post-edit | Every parameter is re-edited later through the **Portal** sidebar section (visible when a portal is selected) or the `portal.repo.*` commands; each edit is a journaled `Patch` of `source`/`query` on the node, and contents regenerate from it (D21) | pattern | 80 |
| D15 | Non-goals | Cut, and each is a decision (Art. III): staging, committing, checkout, branch/tag create or delete, merge, rebase, cherry-pick, revert, stash, fetch, pull, push — the whole GitKraken context menu, and barred anyway by Art. IX.5 and register row 5; diff and blame views; conflict resolution; issues and pull-request data; authenticated host APIs (Art. I.4); per-file history and Gource-style playback (the animated form belongs to the S2 dynamics layer if it is ever wanted); typed frame dimensions (D08); a minimap of the portal's own (the canvas has one) | stated | 90 |
| D16 | Create-style inheritance | **No.** The frame paints from `Palette::portal` and the portal token block; it does not consume `BoardLastStyle`, and a portal does not become the "last single-node edit" that seeds the next curve. **Deviates P1.shape.style**: chrome-styled analysis surfaces stay identical between boards, and between the two apps (Art. X) | pattern | 75 |
| D17 | Hit-testing & pick | The frame picks on its **rect** (Frame-node semantics), including marquee. Contents are inert until the portal is focused: first click selects the portal node, double-click (or Enter on a selected portal) focuses the contents, after which commit dots and ribbons hit-test with `pick.slop` (4 px). Clicking outside the frame drops contents focus. This is what keeps a portal from swallowing board gestures | guess | 60 |
| D18 | Portal class & authority | **Generated.** No journal owns mutations inside it, because none can be made: contents are a deterministic function of (repository object database, `source`, `query`, frame size) and are never journaled (Art. V.3, VI.3). The only journaled acts are on the frame itself — place, move, resize, rebind, re-query, delete, bake | pattern | 95 |
| D19 | Source binding | One `SourceUri { kind: LocalFs, … }` naming a **git worktree or bare repository directory**, stored relative-first (Art. IX.2) so a workbook survives a moved checkout. Bound by `portal.repo.source` (folder picker, empty-state button, or sidebar) or by dropping a folder containing `.git` onto an unbound portal; rebinding is a journaled `Patch` and discards cached contents. Refused as source kinds in v1: remote URLs and hosted APIs (Art. I.4 — see pushback) | pattern | 85 |
| D20 | Query & parameters (journaled) | `RepoQuery { refs, window, as_of, axis, trunk, max_commits }` — `refs: All \| Heads \| Named(Vec<String>)` plus `include_remotes: bool` and `hidden: Vec<String>` (GitKraken hide/solo, made authored because a board that shows one branch's story must show it to the next reader); `window: Last(n) \| Since(date) \| Range(a..b)`; `as_of: None \| Commit(oid) \| Date(ts)` — the pin that makes an exported board reproducible; `axis: Topological \| Chronological` (OQ1); `trunk: Option<String>` pinned to lane 0 (GitKraken branch pinning), default = the remote HEAD's branch, else `main`/`master` if present, else the first ref by name; `max_commits: u32` default 2000, honoured with a visible "N older commits not shown" band, never a silent truncation | research | 65 |
| D21 | Regeneration & staleness | Recompute on: bind, `query` patch, frame resize, `portal.repo.refresh`, and a debounced watch of the repository's `HEAD`/`refs`/`packed-refs` (`portal.repo.refresh_debounce_ms` = 1000). Work runs on a background thread, generation-tagged; a stale result is discarded, never painted (Art. II.3). While it runs, the last good contents stay painted at `portal.repo.stale_alpha` (0.6) with a progress strip — a portal never blanks while it thinks. A portal pinned by `as_of` does not auto-refresh past its pin, and says `pinned` in its caption | pattern | 80 |
| D22 | Contents interaction | Hover a commit: tooltip = short SHA, author, date, subject, containing refs. Hover a ref label: its reachable commits keep full alpha, the rest dim to `portal.repo.dim_alpha` (0.25) — the Lens's focus convention, one dimming rule across both analysis portals. Click a commit or ref: focus it (`portal.repo.focus`), details to the dock readout. Double-click a commit: open its web page in the OS browser when a configured remote's URL matches a known host pattern, otherwise inert, never an error (P0.8). `Ctrl+C` with a focused commit copies the full SHA. Esc clears focus (D12). **No board tool reaches the contents** — draw, move, and marquee act on the frame. The parent-frame→contents coordinate map (`frame_rect`, `query` → contents space) is defined and used by hit-testing now, so the day a portal gains in-place navigation it is not a rewrite (S1 requires the same of document portals) | research | 70 |
| D23 | Level of detail | Buckets keyed on **world pixels per commit column** (`px_per_commit`), not on camera zoom alone, so a small frame and a zoomed-out camera behave the same. `< portal.repo.lod_ribbon_px` (4): lanes as continuous ribbons, branch/merge chevrons, ref tips labelled, no dots. 4–`portal.repo.lod_detail_px` (18): commit dots, ribbons, tags. `≥ 18`: dots plus short SHA, truncated subject, author initials, date ticks. Ribbon meshes tessellate once per (lane, zoom bucket) and cache (Art. II.2) | research | 70 |
| D24 | Export serialization | `slate-artifact` emits the **regenerated contents** as SVG under the Art. IV.3 ceiling — paths for ribbons, circles for commits, text for labels — from the same layout function the painter uses (two interpreters, one model), plus a provenance caption: repository name, HEAD short SHA, as-of, ref set, commit count, and the generation timestamp. No script, no fetch, nothing that implies the export is live. An unbound or `Missing` portal exports its state card, not an empty rectangle | pattern | 85 |
| D25 | Bake | `portal.repo.bake` emits one journaled `Add` batch of authored nodes — paths, dots, and text matching the current contents, grouped, with a provenance Text node naming repository, HEAD, as-of, and generation time (Art. VI.3's explicit promotion of derived state to authored content). The portal is **left in place**: bake copies, it does not convert, so the live view and the frozen snapshot can sit side by side, which is the actual use (a slide that must not change under you) | guess | 65 |
| D26 | Collaboration & per-peer | Frame, `source`, and `query` sync as ordinary journal deltas. Contents are per-peer and are never transmitted — each peer regenerates from its own clone (wave-3 rule). A peer that cannot resolve the locator paints `Unknown` **naming the locator it tried**, which is stated in the interface so it does not read as a bug (Art. IX.3). Focus, hover, and search are presence: broadcast at most, never journaled, never exported (Art. VIII.5) | pattern | 85 |
| D27 | Agent surface | Every `board.portal.repo_lens` / `portal.repo.*` command is a registry `SPEC` and therefore reachable from the MCP surface (Art. VII.1); agent-issued mutations stage for acceptance (Art. VII.6). The context beacon carries: repository locator, HEAD, visible ref set, window, as-of, and focused commit — the portal is part of the canvas-as-prompt (Art. VIII.1). An agent may **never**: run any git write or network command, or add a commit, ref, or edge to the graph (register row 4, Art. IX.5). Agent-authored *labels* for lanes and epochs are the one contribution the class allows, matched against extracted refs only, on the `docs/lens-agent-contract.md` overlay pattern — named here so it is not invented ad hoc, deferred to a follow-up | pattern | 85 |
| D28 | Determinism & provenance | Same object database + same `query` + same frame size ⇒ byte-identical layout. Commits sort by `(commit_time, oid)` with the oid as tie-break, so equal timestamps cannot reorder between runs; lane assignment is the single forward pass of the research doc §1(c) with first-parent inheritance and `trunk` pinned to lane 0; nothing reads the wall clock except a `generated_at` stamp, which is excluded from `RepoGraph::fingerprint()` (the `code-lens` precedent). The fingerprint gates re-layout, the beacon write, and the golden determinism test | precedent | 85 |
| D29 | Performance envelope | Extraction on a background thread over `crossbeam-channel`, generation-tagged, drained once per frame in the existing pump. Budget: 5 000 commits extracted in < 1 s, layout < 50 ms, paint windowed to the visible time range so a 100 000-commit repository pans at 60 fps (Art. II.1). Layout cached by `(fingerprint, query, frame_size)`; meshes by (lane, zoom bucket). `max_commits` = 2000 by default so first meaningful paint does not scale with repository age (Art. II.4) | pattern | 80 |
| D30 | Failure & honesty states | `Unbound` — "Choose repository…". `Unknown` — initial and unresolved; neutral marker, **not** the missing marker (Art. IX.3). `Analyzing` — last good contents at `stale_alpha` + progress strip. `Ready`. `Missing` — locator named, last-known summary kept, frame intact. `NotARepository` — bound to a folder with no `.git`, says which folder. `Unreadable(msg)` — corrupt or permission-denied objects; the message is shown and **no partial graph is presented as complete**. `Shallow(n)` — `.git/shallow` detected: a banner marks the truncation so the view cannot imply the history began there. `Remotes(list)` caption on every bound state, so the fork surface being drawn is stated (register row 4) | pattern | 85 |
| D31 | View-state ownership | **Journaled** (what a second reader must see): frame rect, rotation, `source`, and every `query` field including hidden refs and the as-of pin. **Derived, per-peer, never journaled or exported**: focus, hover, tooltip, search string and matches, contents-focus mode, analysis status, cached layout, last-good contents. The line: authored intent versus where someone is looking (Art. VI.3, VIII.5) | pattern | 85 |

Source values: stated (user), precedent (approved in `decisions.json` for an
overlapping contract), pattern (catalog or constitution), research (source
app), guess (agent proposal — must be confirmed before Status: agreed).

## The extracted model (sketch, for the implementing card)

Pure crate `crates/repo-graph`, no renderer and no app dependency
(Art. I.1), Linux-testable (Art. I.3), split like `code-lens`:
`extract.rs` (object database → `RepoGraph`), `layout.rs`
(`RepoGraph` + `RepoQuery` + frame size → `RepoLayout`), `model.rs` (frozen
types + `fingerprint()`).

```rust
pub struct Commit {
    pub oid: Oid,               // 20-byte binary, rendered short on demand
    pub parents: Vec<Oid>,      // order is meaning: [0] is the first parent
    pub author: Author,         // name + email, no avatars, no network
    pub time: i64,              // commit time, seconds
    pub summary: String,        // first line only
}

pub struct Ref { pub name: String, pub kind: RefKind, pub target: Oid }
pub enum RefKind { LocalBranch, RemoteBranch { remote: String }, Tag, Head }

pub struct RepoGraph {
    pub commits: Vec<Commit>,   // sorted (time, oid); index is the CommitIx
    pub refs: Vec<Ref>,
    pub remotes: Vec<Remote>,   // name + url, for the caption and web links
    pub shallow: Option<u32>,
    pub generated_at: u64,      // excluded from fingerprint()
}

pub struct RepoLayout {
    pub placed: Vec<PlacedCommit>,   // x from the axis mode, y = lane pitch
    pub ribbons: Vec<Ribbon>,        // one polyline per lane run, stable colour
    pub joins: Vec<Join>,            // branch and merge connectors
    pub labels: Vec<RefLabel>,
    pub elided: Vec<Elision>,        // "N older commits not shown"
    pub bounds: Rectf,
}
```

Extraction backend is OQ4. Whatever it is, it reads the object database
directly — no text parsing of another program's output, which is neither
deterministic across versions nor honest about failure.

## Feel constants

App-side named constants block `portal_repo::consts` (P0.6 permits a block
rather than `ui-tokens.toml` for canvas-content values; the line contract set
that precedent). The frame's colours come from `Palette::portal`, which is
chrome and stays in the theme.

| Token | Meaning | Initial value |
|-------|---------|---------------|
| `portal.repo.default_size` | frame placed by a click, world units (the board's existing 16:9 preset) | 960 × 540 |
| `portal.repo.lane_pitch` | vertical distance between lanes, world units | 24.0 |
| `portal.repo.commit_r` | commit dot radius, world units | 3.5 |
| `portal.repo.ribbon_w` | lane ribbon stroke width, world units | 2.0 |
| `portal.repo.dim_alpha` | alpha of non-focused contents | 0.25 |
| `portal.repo.stale_alpha` | alpha of last-good contents while re-analyzing | 0.6 |
| `portal.repo.lod_ribbon_px` | px per commit below which dots disappear | 4.0 |
| `portal.repo.lod_detail_px` | px per commit above which text appears | 18.0 |
| `portal.repo.max_commits` | default window cap, elided beyond it | 2000 |
| `portal.repo.refresh_debounce_ms` | ref-change watch debounce | 1000 |
| `portal.repo.label_gap` | gap between a ref tip and its label | 6.0 |
| `pick.slop` | contents pick tolerance (reused) | 4.0 |

## Golden paths

Numbered input scripts; each becomes a headless test when the portal ships.
GP2 onward run against a committed fixture repository built by the test (a
trunk, two feature branches, one merge, one tag, one second remote) so the
expected layout is exact rather than descriptive.

- **GP1 (place, unbound):** palette "repository lens" · drag (0,0)→(960,540)
  → one portal node, `source: None`, empty state painted, tool = Select, one
  undo step; scene JSON contains the frame and the default query and **no**
  contents.
- **GP2 (bind):** select the portal · `portal.repo.source` → fixture path →
  one `Patch` in the journal; after the analysis pump, lanes = 3, commits =
  the fixture's count, trunk on lane 0.
- **GP3 (undo is frame-only):** GP2 · Ctrl+Z → source back to `None`, no
  orphan nodes, no journal entry mentioning a commit.
- **GP4 (determinism):** two portals, same source, same query, different
  frame positions → identical `RepoLayout` fingerprints; re-running extraction
  after touching nothing produces an identical `RepoGraph::fingerprint()`.
- **GP5 (missing source):** GP2 · rename the fixture directory ·
  `portal.repo.refresh` → state `Missing`, locator named on the card, frame
  and query intact, no panic and no empty rectangle.
- **GP6 (focus dims):** GP2 · click the merge commit → focused commit and its
  reachable set at full alpha, everything else at `dim_alpha`; Esc → all back
  to full alpha, selection retained.
- **GP7 (as-of pin):** GP2 · pin `as_of = Date(t)` before the last two commits
  · commit twice more in the fixture · refresh → contents unchanged, caption
  reads `pinned`.
- **GP8 (export parity):** GP2 · export the artifact → the SVG contains the
  same lane count, commit count, and ref labels as the painter's layout, plus
  the provenance caption; the golden parity test compares layout outputs, not
  pixels.
- **GP9 (bake):** GP2 · `portal.repo.bake` → one undo step adds a group of
  authored nodes plus a provenance text node; the portal still exists and is
  still live; Ctrl+Z removes the baked group only.
- **GP10 (budget):** synthetic 100 000-commit fixture · bind · pan across the
  window → extraction off the UI thread, layout under 50 ms for the 2000-commit
  window, painted primitives per frame bounded by the visible time range.
- **GP11 (shallow honesty):** shallow fixture (`--depth 5`) → `Shallow(5)`
  banner present; the oldest commit is not drawn as a root.

## Open questions

Four, drawn from the lowest-confidence rows. Each needs an answer before
Status can move to agreed.

1. **Time axis default (D20/D23, conf 65).** *(a)* `Topological` — one column
   per commit, chronological date bands labelled underneath: dense, legible,
   hides quiet periods. *(b)* `Chronological` — x from commit time, with gaps
   longer than a threshold collapsed into a marked elision: shows rhythm and
   bursts, wastes space on a repository with a slow year. Proposal: **(a)** as
   the default, **(b)** available in the query, because the first question
   asked of a history is usually structural.
2. **Fork surface in v1 (D19/D30, conf 85 for the refusal, lower for scope).**
   *(a)* Remotes only, hosted networks never. *(b)* Remotes now, with the
   optional out-of-process enrichment specced now and built later. *(c)*
   Remotes now, enrichment unspecified. Proposal: **(b)** — the seam is cheap
   to name and expensive to retrofit, and Art. I.4 is satisfied as long as the
   enrichment is optional.
3. **Placement binding (D01, conf 55).** *(a)* Palette + rail only. *(b)* A
   `Portals` flyout with its own chord. *(c)* A single key. Proposal: **(a)** —
   portals are placed rarely, and single keys are the scarcest resource in the
   board keymap.
4. **Extraction backend (D28/D29, conf 65).** *(a)* `gix` (gitoxide) — pure
   Rust, no C toolchain, keeps the Linux verification floor cheap, young API.
   *(b)* `git2` (libgit2) — mature, a C dependency in a workspace that has
   none. *(c)* Shell out to `git` — refused above: version-dependent text
   output is not a model. Proposal: **(a)**.
