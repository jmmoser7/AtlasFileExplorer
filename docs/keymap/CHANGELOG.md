# Canvas command project — change log

## 2026-09-20 — PDF Pages album and unbundle

- A selected multi-page PDF or PowerPoint gains a Pages squircle on the
  shared property strip. The same control browses pages and unbundles.
- Page browse is a thin `image_album` pallet hovering over the document,
  not a hover grid or a second Cover Flow painter.
- `board.media.unbundle` keeps the original node id, lays the full deck
  on the board as a selected grid, and journals one undo group.
- Hover page-picker chrome and the old explode path are removed.

## 2026-09-18 — Image photo-filter capsule

- Image selections gain a Filters squircle on the shared property strip.
- The editor is the fillet-height capsule: colored radios (B&W, Invert,
  Clarendon, Juno, Lark) and one intensity slider. Hover previews; click
  or scrub journals `ImageAdjust` through `board.shape.edit`.
- Recipes live on `slate-doc::scene::PhotoFilter` and compile to the
  existing CSS-filter model. `invert` is now an amount (legacy bool
  documents still load). 3D model viewports omit the control.

## 2026-09-18 — Shared property strip beyond shapes

- Fill, Stroke and Corners follow scene capabilities, so the same strip
  serves frames, text sticky-note fills, portals, images and wires.
- The forked frame popup is gone. Deck order, tags, add-images and present
  use the geometry-node squircles.
- Selecting expands the icons; one empty-canvas click commits, collapses
  and deselects (no second click).
- The fillet capsule is 30% taller than the 17-unit wire capsule
  (`CORNER_HEIGHT` / `CAPSULE_HEIGHT`).

## 2026-09-18 — Dock family primaries open the flyout only

- Clicking Frame, Shapes, Portals, Text, Media, or Actions on the dock
  opens that palette. It does not arm a subtype. Nested flyout icons
  still arm or run the chosen command.
- Frame primary and size glyphs share one page-and-dog-ear sheet at
  the proposed size's true aspect (Letter 8.5×11, Tabloid 11×17,
  16:9, Custom 1:1).

## 2026-09-18 — Selection yields to property previews

- Opening a Fill, Stroke, Corners or Wire adjustment fades selection/hover
  decoration out over 120 ms. Switching editors keeps it hidden; closing or
  cancelling restores it. Property controls and stringers remain visible.
- The shared shell selection painter handles silhouettes, path highlights and
  endpoint grips without changing authored paint, selection, or undo history.
- GP14 covers native board paint output for rectangles and wires in both themes.

## 2026-09-17 — Curved dashes and stroke mesh quality

- Dashes retain every intervening curve sample instead of becoming straight endpoint chords, including runs crossing a closed path's seam.
- Round caps advance along noncrossing boundary rails; round and bevel joins hold their inner intersection fixed. This removes internal triangle overlap and translucent buildup. Cap and join detail is tolerance-driven.
- Tapered strokes retain adaptive curve samples instead of resampling to a maximum of 64 stations. SVG stroke outlines use the same cap/join/dash boundary owner as the native mesh.
- Path fills, path strokes and wires refine with zoom to keep curve error below 0.15 logical px. Meshes stay cached by zoom bucket.
- Regression targets cover dash curvature, closed seams, odd dash arrays, capsule/join area, tapered samples, export outlines and zoom-dependent fill caching.

## 2026-09-17 — Trim follows authored outlines

- Trim uses the actual fillet/chamfer boundary of both rectangles, including percentage corners, instead of substituting square boxes. The board painter and trim share the pure outline owner in `slate-doc::scene`.
- Ellipse and curve sampling use bounded geometric error rather than a fixed 48-sided ellipse. Cutter highlights follow their actual outlines.
- Trim/Split results bake world-space rotation once; legacy rotated line endpoints and rotated text/image clips now use the correct coordinate space.
- Added rounded/chamfered overlap, rotation, serialization and undo regressions. Slate, slate-doc and slate-artifact test targets compile; Windows denied execution of the slate-doc regression executable (`os error 5`), so test execution is not claimed.

## 2026-09-17 — Document settings palette

- Document settings is a toggle palette (icon strip) like the other board
  tools — grid, snaps, reach, and kinds stay on the strip instead of a
  stacked form that ignored palette mode.

## 2026-09-17 — Wire property palettes and short stringers

- Dimension labels that cannot fit between their ticks move beyond the stringer end and remain editable in place.
- Wires share the shape property palette, with capsule routing, weight, dash and arrow controls. Routing is authored per wire and exported faithfully; legacy wires retain their default until edited.
- Shift/Ctrl selection and crossing marquee support wire batches. Marquee checks the actual route, avoiding both full-AABB containment and empty-box false hits.

## 2026-09-17 — Selection, arc bulge, last style, stringer clearance

- Shift+click (and Shift+marquee) add to the selection; rectangles no longer
  lose the set because hover-resize stole the press. Empty Shift+click keeps
  the current set. `P1.node.select`.
- Selection chrome is a per-shape silhouette (faint fill + outline, or the
  path itself) instead of a painted union bounding box.
- Arc grammar is start → end → middle. The last pick is the through-point, so
  dragging it changes curvature only; endpoints stay put. `arc` D03.
- Inherit-style creates (`CreateStyle::Inherit`, the kit default) take the
  last single-node stroke and fill. Stroke-only creates do not wipe fill
  memory. `P1.shape.style` / `P1.curve.create-style`.
- The align widget's bottom cluster sits past the width stringer
  (`STRINGER_GAP × zoom`) so the two do not overlap at some zoom levels.

## 2026-09-17 — Trim stroke and slimmer corner slider

- Miter stroke cross-sections follow the corner bisector; bevels and miter-limit fallbacks preserve both edge normals. Trimmed paths retain uniform-width edges instead of triangular slivers. Shared geometry fix; authored stroke settings and native SVG export stay unchanged.
- Fillet/Chamfer capsule height is halved to 17 board units. Its slider now has the reference's enclosing capsule and outlined pill thumb in both themes; metrics remain transient at the pointer.

## 2026-09-17 — Closed polyline hits the stroke

- Unfilled closed paths pick, marquee, Near-snap, wire ports, and
  smart guides on the path itself (including the closing seam), not the
  AABB. Stroke hit-testing walks each contour separately and honors
  `ClosePath`, so a phantom segment cannot fire outside the box (the
  classic close-to-origin ghost). Bounding-box resize chrome stays off
  for unfilled paths even after select. `P1.curve.pick` / `P1.wire.ports`.

## 2026-09-17 — Approved compact shape palettes

- Fill/Stroke share a low-profile picker with full-width buffers, transient cursor metrics, inline RGB percentages and document-local recent colors.
- Squircle selection controls and a single-row Fillet/Chamfer strip use shared light/dark theme slots.
- Exterior stringers edit at their rotated labels. The entire assembly follows board pan/zoom without screen-position caching or viewport relocation.
- Recent-color usage metadata records successful committed RGB changes and persists separately from scene undo/export.

## 2026-09-17 — Sharp web viewers and unified idle chrome

- Monitor/DPI-sized capture tiers restore legible text on enlarged and
  maximized web viewers while retaining valid stills during upgrades.
- Mouse coordinates track the physical capture scale.
- Plain centered title bar; native scrollbars hide with it without reflow.
  Fullscreen chrome uses the Slate index top-bar dimensions.
- Windows denied the File Atlas dependency build script and contract checker
  (OS error 5). Slate compilation, runtime tests, and a new executable remain
  blocked; these source changes have not been visually verified.


## 2026-09-16 — File Atlas portal follows board scale

- Keep the inner folder camera in portal-local units and compose it with
  the board transform for painting, LOD, and picking. Leaving contents
  preserves the local view while board zoom/pan carries the entire map.
- Initial fit is independent of board zoom; focused pan/zoom converts
  pointer input to local units. Maximized Atlas uses the same input handler.
- Corrected `portal-atlas-lens` D23's contradictory clip-only scaling rule
  and added GP9 regression coverage for rendered cards and navigation.
- Validation: release build and final regression test compilation passed.
  Windows denied launching the tests and `cargo xtask contracts` (OS error 5),
  so execution and live interaction remain unverified.

## 2026-09-16 — Retain valid web portal stills

- Low-resource portals retain the last valid frame at full opacity. Reject
  transparent and uniform black/white clears at native capture and upload.
- Eviction, same-source viewport regeneration, and recapture preserve the
  texture; rebinding to a different source still discards the old image.
- Validation: release build, test compilation, and contract audit passed.
  Windows denied launching the focused test executable (OS error 5), so
  executable tests and live zoom behavior remain unverified.


## 2026-09-15 — Retracting web portal chrome

- Slimmed and inset the URL blister. The bar overlays the page and retracts
  after 1.2 seconds of idle time; interaction or the top edge reveals it.
- Page bounds remain full-frame when chrome hides, reveals, or folds.


## 2026-09-15 — Web portal stability and hover preferences

- Camera zoom no longer resizes live browser captures or destroys a visible
  portal's session. Captures are bounded; GPU readback never waits for a copy.
- Animation sampling targets 30 fps without focus; texture uploads reuse the
  existing texture. Native Escape restores maximized portals through the
  command cancel stack.
- `board.hover_highlight`: local passive-outline preferences for the seven
  primary node types, under Preferences → Advanced settings.
- Validation: `cargo check --release -p slate --tests` passed, and the release
  test executable built. Windows denied launching both that executable and
  `cargo xtask contracts` (OS error 5); test outcomes and live animation feel
  remain unverified.

## 2026-08-24 — File Atlas portal drag-out

- Contents-focus left-drag on a File Atlas card calls `atlas_core::shell_drag`
  (same CF_HDROP as standalone File Atlas / Explorer). Copy or link, never
  move. `portal.atlas.drag_out`.

## 2026-08-22 — Article XII (one owner of knowledge)

- Constitution **Art. XII**: knowledge has one owner; incidental copies
  wait for the third; parallel interpreters stay two. Agents must not
  paste a working portal to emulate another.
- Pattern **P2.PortalHost** (locators, empty CTA, bake, focus prelude,
  paint shell, `border_hit_px`, folder-map session). Host contracts
  inherit it.
- Agent rule: `.cursor/rules/dry.mdc`. Known twins: DV-18, DV-19, DV-20.

## 2026-08-22 — File Atlas portal reuses folder_map; contents-focus click-out

- **`atlas-shell::folder_map`** is the one folder map (camera, orthogonal
  leaders, collapse grips, cards). File Atlas and the Slate File Atlas
  portal both call it. Slate still must not import `AtlasApp`.
- **`P1.portal.contents-focus`**: a primary click outside the focused
  host-portal body peels focus; entering one portal peels any other;
  wheel/pan never reach an unfocused portal. `portal-atlas-lens` D12 /
  D17 / D22 / D29 updated.

## 2026-08-21 — File Atlas lens portal

- **`portal-atlas-lens`** agreed. Host portal on the Slate board over
  `atlas-core` (not a File Atlas app feature). Portals flyout +
  `board.portal.atlas`. Contents-focus, maximize, bake poster, Open in
  File Atlas (existing hosted viewport).
- **`P1.portal.folder-drop`**: dropping a folder opens a chooser (File
  Atlas default, other honest lenses, or place files on the board). Alt
  keeps today's drop.

One entry per delivery wave. The governing docs are `KEYMAP.md` (what is
bound and why) and `ARCHITECTURE.md` (how it is built); per-app binding
tables live in each app's `commands.rs` (`SPECS`) and render in
**Advanced → Commands & shortcuts**.

## 2026-08-16 — Trim / Split / Join geometry honesty

- Boolean results snap back onto source vertices, H/V lines, and edges
  (`vector-ink` `clean.rs`). Uncut axis-aligned borders stay axis-aligned;
  collinear mid-edge vertices drop. Overlay is the topology oracle only.
- **Join** of objects that do not share area is a no-op ("Objects do not
  touch"). It no longer packs disjoint islands into one uneditable
  compound path. Group (Ctrl+G) is the grouping command. Connected
  components still union independently.

## 2026-08-22 — Dock strip paint is shared

- Canvas `DockStrip` nodes paint through `atlas_shell::dock::paint_icon_strip_card`
  — the same fieldset strip as the docked flyout, title in the outer
  border. Resize contain-scales the measured card; it does not reflow
  icons. `P1.dock-strip.chrome` / `.select` updated.

## 2026-08-16 — Dock strip (dropped toolbar)

- **`P1.dock-strip`** in `PATTERNS.md`. A canvas `DockStrip` click arms
  the command; click-hold-drag anywhere on the node moves it. Icon strip
  keeps the vertical four-dot column; stacked captions use a horizontal
  three-dot ellipsis. Icon-strip bodies use fieldset groups of circular
  secondary icons; tertiary toggles stack two-high on that datum and
  the knob slides. Hover chips sit above the dots. Selection chrome
  follows the painted fillet.

## 2026-08-16 — Split (keep every piece)

- **`board.tool.split`** on **Ctrl+Shift+T**. Same pick-cutters-then-click
  syntax as Trim; the click keeps every span/face as its own Path.
  Type "split" / Actions dock row. Contract: `contracts/split.md`.

## 2026-08-16 — Join (open paths + region union)

- **`board.path.join`** on **Ctrl+J** (already the chord) now also unions
  closed shapes and treats an open curve in that set as a stroke-weight
  ribbon. Type "join" / Actions dock row. Contract: `contracts/join.md`.
- Open+open is unchanged: nearest endpoints, first selected style, one undo.

## 2026-08-16 — Trim (2D Rhino subset)

- **`board.tool.trim`** on **Ctrl+T**. New workbook tab is **Ctrl+N** only.
  Contract: `contracts/trim.md` (`P2.RhinoTrim`). Actions dock chip between
  objects and properties.
- Pick cutters, Enter, click the dying piece. Each click is one undo.
  Line cutters are infinite. No Untrim — geometry is rewritten.
- Closed shapes become compound even-odd paths (a circle punch is a hole).
  Text and images keep the node and store the remaining region in
  `Node.clip` (SVG/CSS `clip-path`). Frames and portals are never targets.

## 2026-08-16 — Status Board portal (second generated portal)

- **`portal-status-board`** lands as a generated portal: journaled frame +
  local `project-state.json` source + section query; contents come from
  `crates/status-board` (`layout_status`) and are never journaled. Placement
  reuses `drag_rect` (click = 960×720, drag free-aspect, Shift locks 16:9).
  Commands: `board.portal.status_board`, `portal.status.source` /
  `refresh` / `bake`. No single-key chord.
- **`P1.portal` extended.** Generated-portal rules (place, bind, pick, bake,
  sync) sit beside the host-portal rules promoted with `portal-web-embed`
  (health, enter, determinism, export-honesty).

## 2026-08-08 — Web portal hardening (post-ship)

- **Deferred WebView2 admit no longer sticks on Loading.** Admission is
  re-issued every frame while a portal is eligible, so an environment that
  finishes creating after the first admit still starts the page; an environment
  that fails reports `NoRuntime` instead of pretending forever.
- **Local source probes left the UI thread.** `metadata` for Missing/mtime runs
  on a worker and is generation-tagged; a live portal whose file changes on disk
  reloads in place (D21) rather than waiting to be evicted.
- **`index.htm`-only folders bind correctly.** Drop/bind records the entry the
  folder actually holds.
- **Popups and downloads are denied** in the composition host (D15, D32).
- **`portal.web.source` accepts a detail** (URL or path) so agents share the
  human command path (GP11 / D27). `live_min_px` is 160 so a normally zoomed
  board page is live rather than looking broken.

## 2026-08-08 — Atlas mouse buttons: left acts, right navigates

- **Right-drag pans from anywhere, cards included.** It previously pans only on
  empty canvas, because a right-drag off a card handed the files to Windows —
  which meant pan failed wherever the folder was full, exactly where it is
  needed most. Ctrl+right-drag turbo pan and middle-drag pan are unchanged.
- **The shell drag-out moved to the left button**, joining the other things the
  left button already did to the card under the cursor: filesystem move/copy in
  Edit mode, and the carry-to-Slate in a linked session.
- **Left-drag on empty canvas now rubber-band selects** instead of panning.
  Shift+left-drag still forces a band from on top of a card, which is the only
  way to start one in a dense folder.
- Tests: `right_drag_pans_even_when_it_starts_on_a_card` and
  `left_drag_on_empty_canvas_sweeps_a_selection`.

## 2026-08-01 — One time axis: the activity timeline

- **The stacked pair became one control.** File Atlas' contribution graph and
  its date-window slider were two widgets with two independent scales for one
  piece of state; they are now `atlas_shell::timeline::ActivityTimeline`, where
  cells, handles, and ticks are all placed by the same `x(t)`. The Filters
  dock's duplicate slider is gone, replaced by a readout plus *clear
  selection*. Spec: `specs/activity-timeline.md`.
- **Semantic zoom instead of a scrub bar.** Wheel pans, Ctrl+wheel zooms at the
  cursor, and the 7×N weekday block morphs — staggering into per-day slots
  around a month of span, then expanding the focused day into an adaptive
  bucket strip and finally per-file dashes down to seconds. Thresholds, LOD,
  and wheel feel are tokens, not constants.
- **Discrete picks generalized to the grain in force** (`TimePicks`, a
  normalized disjoint interval set): Ctrl+click toggles the bucket you are
  looking at — a day zoomed out, an hour or a minute zoomed in — so punching a
  hole in a range is possible at any depth. Timeline reset earns its own cancel
  layer (`CancelLayer::Readout`) below canvas selection, so Esc walks the
  canvas first and the time window last.

## 2026-08-01 — Type-to-command vs bare-letter shortcuts

- **Board type-to-command**: typing opens the canvas palette with the query
  pre-seeded (same UI as double-click empty board). Bare A–Z shortcuts hold
  ~700 ms before committing so a following character can promote into
  command entry instead of stealing the first letter of a typed name
  (`B` vs `brush`). Esc cancels the hold; pointer-down / other chords
  commit early so tool-then-click stays snappy.

## 2026-08-01 — Repository Lens portal on the board

- **Board portal ships a usable v1**: `NodeKind::Portal` (generated /
  repo_lens) with journaled source + query; palette / tool placement
  (`board.portal.repo_lens`); empty-state bind; async `repo-graph`
  extract→layout paint; focus dimming; refresh / bake; inspector controls.
  Git write-back commands remain stubbed (toast) until IX.5 wiring lands.
- Earlier the same day: contract moved to **agreed**, `repo-graph` scaffolded,
  and SPECS registered.

## 2026-07-30 — The contract system covers portals (first portal contract)

- **`portal-lens-repository.md`** — the first contract for something that is
  not a canvas tool: a **generated** portal (Art. V.3 / decision D7) of type
  **lens**, subtype **repository**, drawing one git repository's branching,
  merging, and forking over time. Status: **draft** — all 31 rows are
  `proposed` in `decisions.json` and four open questions are live (time-axis
  default, fork-surface scope, placement binding, extraction backend).
- **Two constitutional refusals recorded in the contract, not silently
  complied with** (Art. XI): the source is a local git worktree, never a
  hosted account (Art. I.4), and the fork surface drawn is the one a clone can
  prove — configured remotes — with hosted fork networks left to an optional
  out-of-process enrichment rather than inferred (Art. IV.2, false-affordance
  register row 4).
- **`DIMENSIONS.md` grows a Scope column and D18–D31.** Dimensions now declare
  which contract families must answer them (`tool` / `portal` / `any`), so a
  gesture tool is not made to write `n/a` about export serialization and a
  portal is not made to invent a numeric-entry story. The fourteen new axes
  are the portal questions: class and authority, source binding, query,
  regeneration, contents interaction, level of detail, export, bake,
  collaboration, agent surface, determinism, performance envelope, failure
  states, and view-state ownership.
- **`PATTERNS.md` gains `P1.portal`** as a named but empty class: with one
  portal contract, its rules stay L3 by the promotion rule.
- **`docs/keymap/research/git-history.md`** — source research: GitKraken and
  the GitLens commit graph, GitHub's network graph, `git log --graph`'s
  first-parent lane rule, and what those tools do that this portal will not.
- **`cargo xtask contracts`** — the framework's rule ("silence is not an
  answer") becomes machine-checked: every contract answers every dimension its
  family is scoped to, every matrix row is mirrored in `decisions.json`, and a
  contract may claim `agreed`/`shipped` only when no row is proposed and no
  open question remains. Runs in `cargo test --workspace`; contracts now carry
  a `Family:` header line that the check reads.

## 2026-07-23 — Tool interaction contracts (method, not code)

- New project skill **`.cursor/skills/tool-contract`**: the codified
  communication method for pinning a tool's interaction and feel before
  building. Flow: one-line user prompt → agent research → **behavior
  matrix** in chat (best-guess defaults, row IDs, sources) → terse
  corrections by row ID → contract doc → golden-path tests.
- New catalog **`docs/keymap/contracts/`**:
  - `DIMENSIONS.md` — the **permanent matrix**: an append-only registry
    of every behavior dimension ever used (`D01`–`D15` seeded from the
    Line request). Stable IDs, never renumbered; per-tool matrices must
    account for every dimension (answer, pattern reference, or `n/a`).
    New axes discovered during any tool request are appended and persist
    for all future requests.
  - `PATTERNS.md` — hierarchical pattern vocabulary (L0 universal →
    L1 object-class → L2 archetypes → L3 tool-specific) with the
    promotion rule: a rule appearing in two contracts moves up, never
    duplicates down.
  - `TEMPLATE.md` — the contract template; matrix rows come from
    `DIMENSIONS.md` in registry order.
  - `line.md` — first worked contract (Rhino Line, status: draft).
    Flags the gap: today's Line is a drag-only bbox shape; the contract
    specifies a parametric two-point line with endpoint grips under the
    new `P2.RhinoDraft` archetype.
- **Volatile matrix canvas** — per tool request, the matrix now renders
  as an interactive Cursor canvas beside the chat
  (`<tool>-tool-contract.canvas.tsx`): Accept / Alter / Reject per
  dimension, option pills for open questions, a "propose new dimension"
  input feeding the permanent registry. Decisions persist to the canvas
  data sidecar, which the agent reads back to update the contract.
  First instance: `line-tool-contract`.
- **Decisions database** — `decisions.json`: every tool × dimension
  decision (behavior, source, confidence, verdict, date). Approved rows
  are **precedent**: a future overlapping tool (e.g. bezier after line)
  seeds its matrix from them at 85–95% confidence instead of re-guessing.
  Rows flip `proposed → approved` as completion bookkeeping, alongside
  appending user-added dimensions to `DIMENSIONS.md`.
- **Confidence column** — every matrix row (canvas, contract, database)
  carries a score: 100 stated by the user · 85–95 approved precedent ·
  75–90 cataloged pattern · 60–80 source-app research · <60 guess. Open
  questions are drawn from the lowest-confidence rows.
- **Line contract agreed** (same day): the user accepted all 15 matrix
  rows as proposed and resolved all four open questions (dock readouts ·
  45° ortho · length-only numeric entry · legacy bbox lines convert to
  parametric on load). `line.md` → Status: agreed; all 15 `decisions.json`
  rows → approved (now precedent for arc/polyline/bezier); no new
  dimensions proposed, so `DIMENSIONS.md` is unchanged at D01–D15.
  `KEYMAP.md` gains the **L** binding (🟢 adopt) and the Tab
  direction-lock note. Next step: implementation to contract (golden
  paths GP1–GP6 become headless input-script tests).
- **Line tool shipped to contract** (same day): new
  `apps/slate/src/app/board_line.rs` — draft state machine (both
  grammars, `draft.drag_threshold` disambiguation), Tab direction lock,
  typed-length numeric entry (digits/Backspace mid-gesture, Enter
  commits), F8 ortho (Shift inverts) + F9 grid + endpoint object snap,
  dock length/angle readout, crosshair + lock glyph, fg-color commit as
  one journaled Add. Committed lines are open single-segment **Path**
  nodes, so Direct Selection, Ctrl+J join, and stroke picking work
  unchanged (D14); selected lines show endpoint grips instead of a
  resize bbox (D13), grip drags journal one point-edit Patch. Legacy
  bbox lines (`ShapeKind::Line` + `flip`) migrate to parametric paths
  on load (`Scene::migrate_legacy_lines`). Feel constants pinned in
  `board_line::draft_tokens` (P0.6). Golden paths GP1–GP6 are headless
  tests (`line_gp1`–`line_gp6`); GP3's expected point corrected to
  (97,0) — the board's ortho projection convention, not a rotation.
  `line.md` + `decisions.json` → Status: shipped; `KEYMAP.md` L row →
  ✅ exists. Palette alias "segment" registered in SPECS.
- **Tool-contract skill hardened** (same day): the volatile canvas's
  "Send decisions to agent" button now dispatches `openAgent` at the
  building conversation (focuses the working agent on the taskbar —
  never `newComposerChat`, which lost context in a fresh chat), and
  step 7 (Implement + pin) is explicitly not optional: a contract
  flipping to agreed triggers implementation in the same task unless
  the user defers it.
- **Line contract amendments** (2026-07-24): Square end caps on draft
  curves (`default_curve_stroke`, distinct from round expressive ink);
  **P1.curve.create-style** — last single-node edit seeds stroke +
  opacity on the next Line commit (`board_style.rs`); D13 extended to
  multi-select (endpoint grips on every simple line, no per-line or
  group bbox). Golden paths GP7–GP8; registry gains **D16** (create-style
  inheritance).
- **Line stroke-precise pick** (2026-07-24): open curves (including simple
  lines and legacy `ShapeKind::Line`) click- and marquee-select on stroke
  geometry via `board_path::hit_shape_stroke` / `marquee_hits_node` — never
  the node AABB alone (**P1.curve.pick**, D17). Registry + template updated;
  GP9 / `line_pick_stroke_not_bbox` test.

## 2026-07-22 — P1 delivery

### New crate

- **`crates/atlas-commands`** (pure, zero-dependency — Art. I): commands as
  data. `CommandSpec` (id, name, category, chord, repeat policy,
  availability, palette aliases), `Registry` with chord lookup +
  collision validation, `History` (500-entry ring, author-attributed per
  Art. VI) with Rhino-style `last_repeatable` (never-repeat entries are
  skipped over), the `CancelLayer` cancel-stack contract
  (ActiveOperation → Draft → Mode → Selection → Chrome), and
  `palette_query` fuzzy search. This registry is the Phase-4 MCP command
  surface arriving early (Art. VII).

### Document model (`slate-doc`, with full `slate-artifact` parity — Art. IV)

- Nodes gained `hidden`, `locked`, and `group` (serde-defaulted; old
  `.slate` files load unchanged).
- New `NodeKind::Connector` — wires between board nodes. Endpoints anchor
  to a node side at a fraction (`Anchored{node, side, t}`) or float free;
  the bezier is **derived at paint/export time** from live node rects,
  never stored stale. Arrowheads, midpoint labels, Default/Faint display.
  Exports as SVG path + triangles + text.
- `TextNode.fill` (the sticky-note base) and `ImageAdjust.invert`
  (CSS `invert(1)`, mirrored in `imagefx.rs` pixel math and the artifact).

### Geometry (`vector-ink`)

- New `edit` module: anchor/handle model over bezier paths —
  `anchors_from_bezpath`/back (lossless), move anchor/handle (Alt breaks
  smooth symmetry), angle-preserving segment translation (Illustrator
  "constrain path dragging"), corner↔smooth conversion, `join_endpoints`
  (merge / close / bridge), anchor and segment hit-testing.

### Shared chrome (`atlas-shell` — Art. X)

- **Minimap** (`minimap.rs`): squircle overlay, content rendered to a
  generation-keyed cached texture (Art. II), viewport rectangle,
  click/drag/scroll navigation. Both apps.
- **Canvas palette** (`palette.rs`): anchored fuzzy-search popup, keyboard
  navigation, zero cost while closed.
- **History window** (`history_ui.rs`): read-only command log with author
  chips and copy-to-clipboard.
- New `[minimap]` and `[palette]` token sections in `ui-tokens.toml`.

### Slate

- **Registry migration**: `ENTRIES` → `SPECS`; keys dispatch through the
  registry; every dispatch and major mutation pushes attributed history.
  Space (tap) / Enter (idle) = **repeat last command**; Esc = formal
  cancel stack; F1 help, F2 history window, Ctrl+Shift+P preferences,
  Ctrl+N new tab.
- **Tools**: **B** Brush (fg color, sticky tool, Shift+click straight
  chain, `[`/`]` Photoshop-tier width stepping, width-circle cursor),
  **E** Eraser (whole-stroke, live 30% preview, one undo group),
  **I** Eyedropper (+ spring-loaded Alt from Brush; Alt+click samples
  background), **N** Sticky note (Tab-while-editing spawns the next
  sticky), **A** Direct Selection (anchors/handles/segments via
  vector-ink, double-click toggles corner/smooth), **D**/**X** color
  reset/swap with persisted fg/bg state + dock chips, **C** enter crop.
- **Wires**: hover-edge grips; drag to connect (snap-solid preview);
  Shift = add, **Ctrl = detach/rewire**, **Ctrl+Shift = move all wires**
  (Grasshopper grammar); release on empty opens the palette and
  auto-connects the placed node; labels via double-click; arrowhead/faint
  controls in the context menu.
- **Flags**: Ctrl+G/Ctrl+Shift+G group/ungroup (click selects the group,
  Ctrl+Shift+click picks a member), Ctrl+H/Ctrl+Shift+H hide/show-all,
  Ctrl+L/Ctrl+Shift+L lock/unlock-all (locked nodes still feed smart
  guides — Rhino), readout chips for hidden/locked counts.
- **Constraints**: **F8** ortho (45° steps; held Shift *inverts* it —
  Rhino), F9 snap, G/F7 grid; ortho feeds moves, drafts, wires, anchor
  drags with DominantOrtho snap projection.
- **Overlays**: **M** minimap, double-click empty board = canvas palette,
  **Ctrl+F** board search (dim non-matches, Enter cycles, camera flight),
  Tab/Shift+Tab reading-order object cycling with camera follow.
- **Clipboard**: Ctrl+C/X/V (paste at pointer, +24 stepping, connector
  bridging with anchor degradation), **Ctrl+Shift+V paste in place**.
- **Misc**: PageUp/PageDown/Ctrl+B z-order, Ctrl+J join paths, Ctrl+U
  image-adjust popover, Ctrl+I invert image, F3 inspector toggle,
  arrows pan when nothing is selected, **Z** zoom tool (click in,
  Alt+click out, drag = zoom window).
- **Join** (Ctrl+J): merge coincident endpoints, close open paths, bridge
  nearest endpoints across two paths (first path's style wins).

### File Atlas

- Registry migration + Space/Enter repeat + Esc cancel stack (existing
  order preserved), history log surfaced in Advanced.
- **M** minimap over the folder tree (avg-color file tints), **Ctrl+F**
  focuses the filter search, Tab/Shift+Tab cycles filtered files with
  camera follow, **Z** zoom tool, arrows pan (Shift ×4), **Ctrl+C** copies
  selected file paths, Ctrl+N new tab, F1/F3/Ctrl+Shift+P.
- Unchanged by design: F2 = Assign, Shift+click = range select,
  double-click empty = zoom-to-point, Ctrl+right-drag = turbo pan.

### Deliberately rejected (see `KEYMAP.md` for reasons)

Ctrl+RMB zoom (turbo pan wins), Ctrl+T trim (new-tab + Art. III),
Ctrl+W zoom window (Z-drag covers it), F11 attributes (fullscreen),
F12 DigClick, Ctrl+P print (deferred to Roadmap Phase 5), Delete/Ctrl+S
in Atlas.

### Deferred to P2 (specced in `specs/`, not built)

Radial menu (middle-click), scale tool (S), rulers/guides (Ctrl+R),
brush preset cycling (,/.) + F6 color panel, graphic styles (Shift+F5),
Shift+letter tool-family cycling, segment-splitting eraser, image-pixel
eyedropper, nested groups, show-hidden picker, Atlas command palette,
shared fg/bg chrome primitive, connector relations in the AI beacon.

## 2026-09-15 — Agent portal refinement

Approved minimal icon picker, hover chrome, semantic input wires, in-node image albums and journaled Unbundle. Added `portal.agent.unbundle` and `portal.agent.stop`. No new shortcuts. See `contracts/portal-agent-link.md`.


## 2026-09-17 — native shape property implementation

Implemented geometry-gated circular selection palettes; transaction previews; desktop RGB sampling; centered dimension stringers; persistent percent/absolute fillet/chamfer geometry with export parity. Ordered input preserves moving line/polyline/arc picks and all freehand event samples; draft polyline endpoints/segments participate in snapping and start closure. Added command registry entries and regression coverage. Shared ownership passed Article XII review. See the shape contracts and desktop sampler acceptance notes.
