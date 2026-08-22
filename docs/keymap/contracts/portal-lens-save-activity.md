# Save Activity Lens portal — interaction contract

Status: draft
Family: portal
Portal class: **generated** (proposed — Art. V.3 / P1.portal.frame) · Type:
**lens** · Subtype: **save activity** (name TBD)
Reference: Damon / Rug save journal (`~/.rug/saves.db`) as a *fact source*
only — the browser dashboard at `127.0.0.1:8787` is a retired interpreter
and is not this portal
Command: `board.portal.save_activity` (placement) · Key: none · Palette:
"save activity" (aliases: activity lens, rug, save lens, work journal)
Inherits: P0.* (all), P1.node, **P1.portal**, **P2.PortalPlace** — deviations
flagged below.

This file is a **draft**. Rows are sourced; guesses are marked below 60.
Nothing is Status: agreed until the volatile canvas is accepted or altered
row by row. No `PortalKind` and no crate until then.

## What it is, and the 10% it implements

A journaled frame on a **Slate** board whose contents are an extracted view
of a local **save journal** — timestamped created / modified / moved paths —
so the author can sit next to notes and drawings and read the shape of
recent work: which projects were live, which file families dominated
(Rhino / Grasshopper vs documents vs code), and when the saves clustered,
then filter that map by time, family, folder, or project.

The 90% deliberately not implemented is in D15. This is not File Atlas, not
Repository Lens, not Status Board, not a web iframe, and not a keystroke
logger.

Suggested v1 paint (proposal only): shared `ActivityTimeline` as the
temporal controller (data = save stamps, colored by `atlas-core` family)
plus a tree or compact project/family readout that respects the same query;
honest Unbound / Unknown / Missing / Analyzing / Ready cards; refresh,
bake, and export from one `layout_*` function.

## Pushback before agreement (Art. XI)

The brief itself names the three refusals. They stand even if a later
message says "just embed Damon" or "make it a new tab":

1. **Article V.3 + Status Board.** The dashboard HTML is a disposable
   interpreter. Shipping it as a host / web portal would make the board a
   browser (Wave 5) for a file we already own. Same decision as
   `portal-status-board.md`.
2. **Article IV + I.1.** One extract / layout, two interpreters (egui +
   `slate-artifact`). `app.js` would be a third interpreter, and
   extract/layout cannot live in JS.
3. **Article I.4 / IX.2.** Local journal, relative-first locator. No
   `127.0.0.1` as a source kind, no account, no hosted API.
4. **Article V.1.** A new tab-level `ViewKind::Rug` is debt. The form is a
   portal on the board.
5. **Article III.** Watching all of `$HOME` by default produced ~4 600 rows
   and a hitch. Binding is a human step. Capture roots are authored, not
   implied by home.

The conforming alternative is this generated lens: a `SourceUri` to a local
journal, a pure crate (`save-lens` / `activity-lens` working name) that
extracts and lays out, and the existing ActivityTimeline widget (Art. X).

## PortalKind note

`PortalKind` today includes `RepoLens`, `StatusBoard`, `Agent`, `Web`, and
`FileAtlas`. A **sixth** variant is the honest addition **after** this
contract is agreed — not before. The brief's list of four kinds is stale.

## Behavior matrix

Rows keyed to `DIMENSIONS.md` in registry order. Every row is mirrored in
`decisions.json` as `verdict: proposed` until the canvas settles.

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-----------------|--------|------|
| D01 | Initiation & arming | Slate board only. Command `board.portal.save_activity`, board tabs only. Palette: "save activity" (aliases: activity lens, rug, save lens, work journal). Portals flyout. No single-key chord. `P1.portal.folder-drop`: list this lens only when the dropped folder contains a save journal (`saves.db` or `.jsonl`), not on every folder. Not a File Atlas feature and not a new `ViewKind`. | precedent | 85 |
| D02 | Stickiness & repeat | P1.portal.place / P0.4: one-shot, commit returns to Select; Space/Enter re-arms | pattern | 90 |
| D03 | Gesture grammar | P1.portal.place: `Armed → Dragging(rect) → Committed(unbound)`. Release paints "Choose save journal…". Binding is a separate non-modal step (D19). A folder-drop that picks this lens skips to `Committed(bound)`. | pattern | 90 |
| D04 | Click vs drag rule | P1.portal.place. Travel > `draft.drag_threshold` (4 px) = dragged rect. Below it places `portal.activity.default_size` (960×640 world units) centred on the click. Taller than Repository Lens because v1 stacks a timeline readout over a tree. | precedent | 80 |
| D05 | Modifiers | P1.portal.place: Shift during the drag locks 16:9; unmodified drags are free-aspect. Alt while dropping a folder bypasses the chooser (`P1.portal.folder-drop`). Ctrl unassigned in v1. | precedent | 90 |
| D06 | Constraints & snapping | F9 grid snap and smart guides apply to the frame rect (P1.node.move); F8 ortho is `n/a`. Contents never snap — they are derived geometry. | pattern | 85 |
| D07 | Direction / value locks | `n/a` — no directional parameter in a rect placement | pattern | 85 |
| D08 | Numeric / manual entry | `n/a` in v1; typed frame dimensions stay a non-goal (D15). After bind, query knobs are edited in portal-local UI (D35). | precedent | 85 |
| D09 | Preview & readouts | During the drag: frame outline in `Palette::portal` + live w×h in the dock readout. Bound and idle: journal name · events in window · family mix · health (`Ok` / `Unknown` / `Missing`). A focused path replaces the middle fields with basename · project · time. | precedent | 80 |
| D10 | Cursor | Crosshair while armed. Over an unfocused or focused portal: arrow. No contents-focus cursor set in v1 (D22). Chrome and maximize square: arrow. | precedent | 80 |
| D11 | Commit | One journaled `Add` of a portal node: `{ rect, class: Generated, kind: SaveActivity (after agree), source: None, query: SaveQuery::default() }`. Frame, source, and query are journaled; contents never are (Art. VI.3). One gesture = one undo. No `BoardLastStyle` (D16). | pattern | 90 |
| D12 | Cancel | Esc peels one layer per press (P0.1): maximize → drag draft → armed tool → selection. No contents-focus layer in the v1 proposal (D22). Esc in the inspector timeline clears picks / the window the same way File Atlas does. | pattern | 80 |
| D13 | Selected presentation | P1.portal.pick / P1.node.transform. Windows-style hover resize — no prior selection. Contents expose no grips. Resize re-lays-out. Portals stay axis-aligned: no rotate chrome. Maximize square per P1.portal.chrome. | pattern | 90 |
| D14 | Post-edit | Rebind and authored query knobs through the Portal inspector or `portal.activity.*` commands; each is a journaled `Patch` of `source` / `query`. Hover, typeahead search, and focused path are presence (D31). | pattern | 80 |
| D15 | Non-goals | Cut (Art. III): Damon dashboard HTML/JS; the Python HTTP server; wrapping `http://127.0.0.1:8787` as `PortalKind::Web`; a tab-level `ViewKind::Rug`; File Atlas clone; git history client; Status Board / live CI; keystroke or file-open logging; OneDrive-as-entire-home; home-wide watch as a product default; editing files from the lens; git write-back; agent inserts into the save journal; language-of-the-day heatmaps; on-canvas graph widgets in v1; a capture daemon inside Slate in v1; agent overlay in v1. Not cut: place, bind a local journal, extract+layout crate, shared ActivityTimeline, tree/readout, honest health cards, refresh / bake / export. | stated | 100 |
| D16 | Create-style inheritance | P1.portal.style: **No.** The frame does not consume `BoardLastStyle`. | pattern | 90 |
| D17 | Hit-testing & pick | P1.portal.pick. The frame picks on its rect, including marquee. Contents are not board-selectable. v1 proposal: no contents-focus (OQ4). Double-click selects the frame. Timeline gestures live on the portal inspector widget (D35). | precedent | 70 |
| D18 | Portal class & authority | **Generated lens** (Art. V.3). Contents = `extract(journal)` + `SaveQuery` + frame size; never journaled. Closest sibling is Repository Lens, not Status Board and not File Atlas. Host/web of the Damon dashboard is refused (Art. V.3, IV, I.1, I.4, IX.2). If v1 collapses to caption + KPI + timeline with no tree, it becomes an instrument — that is OQ1. | research | 75 |
| D19 | Source binding | One `SourceUri { kind: LocalFs }` naming a save journal file (`saves.db` or a `.jsonl`) or a folder containing exactly one such journal, stored relative-first (Art. IX.2). Empty state: "Choose save journal…". Bound by `portal.activity.source` or by `P1.portal.folder-drop` when a journal is present. Rebind is a journaled `Patch`. Refused: `http://127.0.0.1:8787`, hosted APIs, accounts, "watch $HOME", a File Atlas window handle, a directory of watch roots (capture — OQ2/OQ3). | guess | 55 |
| D20 | Query & parameters | `SaveQuery { window: Last(n) / Since(date) / Range(a..b) / All; families: All / Named(Vec<Family>); projects: All / Named(Vec<String>); path_prefix: Option<String>; max_events: u32 default 2000; as_of: None / Date(ts) }`. Family shutters use `atlas-core` `types.rs` colors (Art. X). Grasshopper `gh`/`ghx` stays Cad; if it needs its own chip, promote an ExtGroup "Grasshopper" inside Cad — do not fork a second color table. `max_events` paints a visible "N older not shown" band, never a silent crop. Search-as-you-type is presence (D31). | guess | 55 |
| D21 | Regeneration & staleness | Recompute on bind, query patch, frame resize, `portal.activity.refresh`, and a debounced mtime watch of the journal file (`portal.activity.refresh_debounce_ms` = 1000). Generation-tagged; stale results discarded (Art. II.3). Last-good stays painted at `portal.activity.stale_alpha` (0.6). An `as_of` pin does not auto-refresh past the pin. Capture of new saves is out of this portal (OQ2). | pattern | 80 |
| D22 | Contents interaction | v1 proposal (brief §9 cut): no contents-focus navigation, no inner camera, no on-canvas graph widgets. Hover/click/double-click act on the frame. Linked filters are journaled Patches from portal-local UI. Hover, typeahead search, and focused path are presence. No board tool reaches the contents. | guess | 55 |
| D23 | Level of detail | Two named tokens, neither a screen-constant clamp (P0.9). Time LOD: reuse ActivityTimeline days-visible + pixels (`activity-timeline.md`). Tree LOD: type/icons/badges scale with board zoom; when type is too small, drop it. Compact frame: height < `portal.activity.lod_compact` (220) paints caption + timeline readout only; at/above it the tree appears. | guess | 58 |
| D24 | Export serialization | P1.portal.export. `slate-artifact` emits regenerated SVG from the same `layout_*` function the painter uses (Art. IV), plus a provenance caption. No script, no fetch, no poster of a browser. Unbound or Missing exports the state card. | pattern | 90 |
| D25 | Bake | P1.portal.bake. `portal.activity.bake` emits one journaled `Add` of authored nodes matching the current prims, plus a provenance Text node. The portal stays live. | pattern | 85 |
| D26 | Collaboration & per-peer | P1.portal.sync. Frame, source, and query sync as journal deltas. Contents are never transmitted — each peer regenerates from its own journal file. A peer that cannot resolve the locator paints `Unknown` naming the locator. The journal must never sync as contents. | pattern | 90 |
| D27 | Agent surface | `board.portal.save_activity` and `portal.activity.*` are registry SPECs (Art. VII.1). Agent-issued frame/source/query mutations stage for acceptance (Art. VII.6). An agent may **never** insert, edit, or delete save events (Art. IV.2, IX.5). Beacon may carry locator, window, family set, project set, and one focused path — never a dump of every path (Art. VIII). Overlay deferred. | stated | 100 |
| D28 | Determinism & provenance | Same journal bytes + same `SaveQuery` + same quantized frame size ⇒ byte-identical extract fingerprint. Family and project are derived at extract from path + ext via `atlas-core`; db helper columns are not trusted if recomputable. `generated_at` is excluded from `fingerprint()`. Caption states locator + window + `as_of`. | pattern | 85 |
| D29 | Performance envelope | Extract + layout on a background thread, generation-tagged (repo-lens D29). Budget: layout < 50 ms for the default `max_events` window; paint windowed. Default `max_events` = 2000 so first paint does not scale with journal age (Damon hitched at ~4600). Last-good at `stale_alpha`. Two portals, same journal, share one extract; layout may differ by frame size. | precedent | 80 |
| D30 | Failure & honesty states | `Unbound` — "Choose save journal…". `Unknown` — unresolved locator, named. `Analyzing` — last-good at stale alpha + progress. `Ready`. `Missing` — locator named, last-good if any. `NotAJournal` — bound to something that is not a save journal, says why. `Unreadable(msg)` — no partial graph presented as complete. `Truncated` — visible "N older not shown" band. A URL / live webview is refused and stays unbound. | pattern | 85 |
| D31 | View-state ownership | **Journaled:** frame rect, source, window, family/project shutters, `path_prefix`, `max_events`, `as_of` pin. **Derived, never journaled:** hover, search-as-you-type, focused path, contents-focus if later promoted, analysis status, cached extract/layout. Search is "where I am looking" (repo-lens D31). | pattern | 80 |
| D32 | Trust, sandbox & consent | Local journal only. No account, no hosted API, no Python runtime, no webview, no network enrichment (Art. I.4). Binding a journal the workbook can already see needs no extra prompt. The portal executes no code from the journal. No `127.0.0.1` source kind. | stated | 100 |
| D33 | Portal chrome | P1.portal.chrome: no identity tab (web-only). Maximize square on the frame. Right-click: Maximize, Rebind journal, Refresh, Bake. Reuse the existing ActivityTimeline widget (Art. X) — do not grow a second one. | precedent | 85 |
| D34 | Portal maximize | P1.portal.maximize. Fills the Slate canvas pane at the screen aspect; Esc / the square restores. The Damon browser tab goes away; maximize is the immersive reading mode. Node rect is not mutated. | pattern | 90 |
| D35 | Portal-local UI | P1.portal.local-ui. Bind, window, family/project shutters, `path_prefix`, `max_events`, `as_of`, refresh, and the ActivityTimeline controller live on this portal's inspector / empty state. They do not appear on Document Settings. File Atlas mtime scan is a different fact (OQ3). | pattern | 85 |

Source values: stated (user), precedent (approved in `decisions.json` for an
overlapping contract), pattern (catalog or constitution), research (source
app / constitution argument), guess (agent proposal — must be confirmed
before Status: agreed).

## The extracted model (sketch — guess confidence)

Pure crate working name `crates/save-lens` (or `activity-lens`), no renderer
and no app dependency (Art. I.1), Linux-testable (Art. I.3), split like
`code-lens` / `status-board`:

- `extract.rs` — journal / sqlite → `SaveGraph`
- `layout.rs` — `SaveGraph` + `SaveQuery` + frame size → `SaveLayout`
- `model.rs` — frozen types + `fingerprint()` (`generated_at` excluded)

`slate-artifact` consumes the same `layout_*` (Art. IV). Apps only bind,
pump, and paint.

Watcher writes raw facts only. Interpretation (family, color, project
display name, tree layout) is read-time.

```
SaveEvent { ts: i64, path, ext, project, event: Created | Modified | Moved }
SaveQuery { window, families, projects, path_prefix, max_events, as_of }
SaveLayout { tree, timeline_axis, graphs? }
```

`iso` / `day` / `hour` in the Damon db are recomputed. `project` is
re-derived from path markers (`.git`, `Cargo.toml`, `package.json`, …)
when possible.

## Feel constants

App-side named constants; frame chrome colours come from `Palette::portal`.
Time-LOD tokens already live on ActivityTimeline — reuse them.

| Token | Meaning | Initial value |
|-------|---------|---------------|
| `portal.activity.default_size` | frame placed by a click, world units | 960 × 640 |
| `portal.activity.max_events` | default window cap; elide beyond it | 2000 |
| `portal.activity.stale_alpha` | last-good while re-extracting | 0.6 |
| `portal.activity.lod_compact` | frame height below which the tree drops | 220 |
| `portal.activity.refresh_debounce_ms` | journal mtime watch | 1000 |

## Golden-path seeds

Not tests yet. Direction only, from the brief:

- **GP1 (place, unbound):** empty state "Choose save journal…"; scene JSON
  contains frame + default query and **no** Damon contents.
- **GP2 (bind fixture):** committed fixture db → layout fingerprint stable;
  family colors match `atlas-core` types.
- **GP3 (undo bind):** `source: None`, no orphan tree nodes.
- **GP4 (determinism):** two portals, same source + query, different rects →
  identical extract fingerprint; layout may differ by frame size.
- **GP5 (missing):** `Missing`, locator named, last-good if any.
- **GP6 (query window):** timeline and tree agree; one journal `Patch`.

## Open questions

Unresolved until the canvas is accepted. These four are the architecture
forks; the rest of the brief's list is folded into the rows above as
guesses.

1. **OQ1 Class.** Generated lens (proposal) vs generated instrument vs
   host/web of Damon (refused).
2. **OQ2 Capture.** C bind-only (proposal) vs A out-of-process journal that
   Slate only reads vs B move capture into `atlas-core` (defer).
3. **OQ3 SourceUri.** Journal file / folder containing one (proposal) vs
   directory of watched roots vs current Atlas / workbook root (mtime is a
   different fact).
4. **OQ4 v1 body.** Inspector ActivityTimeline + painted readout, no
   contents hit (proposal) vs on-canvas timeline hits while selected vs
   contents-focus like Repository Lens.
