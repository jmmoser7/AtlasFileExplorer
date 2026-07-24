# Canvas command project — change log

One entry per delivery wave. The governing docs are `KEYMAP.md` (what is
bound and why) and `ARCHITECTURE.md` (how it is built); per-app binding
tables live in each app's `commands.rs` (`SPECS`) and render in
**Advanced → Commands & shortcuts**.

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
