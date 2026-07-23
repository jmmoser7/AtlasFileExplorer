# Canvas command architecture

Companion to `docs/keymap/KEYMAP.md` (what is bound). This document decides
*how* the keymap project is built: where code lives, what the contracts are,
and which constitutional articles bind each piece.

## The central decision: commands become values

Today a "command" in this repo is a static documentation row
(`atlas_shell::commands::CommandEntry`) plus an ad-hoc `if key_pressed`
handler. Four requested features break that model at once:

- **Space/Enter = repeat last command** needs to know *what the last
  command was* and whether it may repeat.
- **F2 = command history** needs an execution log with authorship (Art. VI).
- **Double-click = canvas palette** needs to enumerate, fuzzy-search, and
  dispatch commands at a canvas point.
- **Middle-click = radial menu** needs context-filtered command lists.

So Stage 1's core deliverable is a **command registry where commands are
data**: an ID, metadata, availability, and a dispatch path. This is
Roadmap Phase 4's MCP surface arriving early — the human front-ends
(keyboard, palette, radial) and the future agent front-end are the same
surface (Art. VII).

## Crate layout

| Piece | Location | Why there |
|-------|----------|-----------|
| Command registry, history, repeat rules, cancel-stack contract, key-chord model | **new pure crate `crates/atlas-commands`** | Art. I: no egui. Both apps + future MCP server consume it. |
| Palette overlay, radial menu, minimap, history window, shortcuts reference (upgraded) | `crates/atlas-shell` (new modules `palette.rs`, `radial.rs`, `minimap.rs`, `history_ui.rs`) | Art. X: shared chrome, built once. Tunable values in `ui-tokens.toml`. |
| Scene model growth: connectors, group/lock/hide flags, image invert | `crates/slate-doc` (`scene.rs`, new `connector` support) | The durable model. SVG ceiling holds (Art. IV). |
| Artifact parity for every new visual | `crates/slate-artifact` | Art. IV: two interpreters of one model, or not at all. |
| Path editing (direct selection, join), stroke erase hit-testing | `crates/vector-ink` + `apps/slate/src/app/board_path.rs` | Geometry stays pure; the board interprets. |
| Constraint layer (ortho / grid snap / one-shot Shift inversion) | `apps/slate/src/app/board_snap.rs` (extended) | Already the snap home; stays app-side because it binds gestures to geometry. |
| Tool state, color state (fg/bg), brush presets, keymap dispatch wiring | `apps/slate/src/app/` (`commands.rs` grows into the registry adapter) | Apps supply data and handle actions. |
| Atlas adoption (minimap, Ctrl+F focus, Tab cycling, Z zoom tool, palette on a later pass) | `apps/file-atlas/src/app/` | Same registry crate, Atlas-sized command set. |

### `atlas-commands` contracts (sketch)

```rust
pub struct CommandId(pub &'static str);          // "board.tool.brush"

pub struct CommandSpec {
    pub id: CommandId,
    pub name: &'static str,                      // "Brush tool"
    pub category: &'static str,                  // existing category names
    pub bindings: &'static [Chord],              // for reference UI + dispatch
    pub repeatable: Repeat,                      // Repeatable | NeverRepeat
    pub when: Availability,                      // Board-only, needs-selection, …
}

pub struct Registry { /* specs + lookup by id/chord */ }

pub struct History {                             // Art. VI-adjacent: execution log
    // (id, author, timestamp, human summary) — the F2 window's data,
    // and the "last repeatable command" source for Space/Enter.
}
```

Rules encoded in the crate, not in apps:

- **Never-repeat list** (Rhino's model): undo, redo, repeat itself, save,
  open, escape/cancel, delete-with-nothing-selected. Everything else
  repeats by re-dispatching its ID (parametric commands re-arm the tool,
  they do not replay coordinates).
- **The cancel stack** is a contract: `CancelLayer` = ActiveOperation →
  Draft → Mode(tool) → Selection → Chrome. Apps register which layers are
  live each frame; one Esc pops exactly one layer. This formalizes the
  existing ad-hoc Escape cascades in both apps.
- **Chords are data** so the Advanced reference, the palette, and the
  dispatcher can never disagree (today `ENTRIES` strings can drift from
  `hotkeys` code — the registry ends that class of bug).

Migration: each app's `ENTRIES: &[CommandEntry]` becomes `SPECS:
&[CommandSpec]`; `shortcuts_reference_ui` renders from specs. Dispatch is
incremental — existing `hotkeys` arms move behind registry lookups without
rewriting handler bodies.

## The scene model grows (slate-doc)

All Board mutations remain `SceneCmd::{Add,Remove,Patch}` — the three
variants already cover every new capability because flags and connectors
are node data:

1. **`Node` gains** `locked: bool`, `hidden: bool`, `group: Option<GroupKey>`
   (serde-defaulted so existing `.slate` files load unchanged).
   - *hidden*: skipped by paint, hit-test, export (present/HTML), and
     marquee. `Ctrl+Shift+H` shows all.
   - *locked*: painted normally + skipped by click/marquee/drag; still a
     smart-guide snap source. Grayed handles if force-revealed.
   - *group*: flat membership key (nesting deferred). Click selects the
     whole group; `Ctrl+Shift+click` selects a member. Group transforms
     reuse the existing multi-selection bbox machinery.
2. **`NodeKind::Connector`** — the Grasshopper adoption. Endpoints are
   `ConnectorEnd::Anchored { node, side, t } | Free { point }`; style is a
   stroke + arrowheads under the SVG ceiling (`<path>` + `marker-end`).
   Geometry (the bezier between anchors) is *derived at paint/export time*
   from the two anchored nodes — never stored stale. Wire gestures
   (plain/Shift/Ctrl/Ctrl+Shift drag on grips) compile to Add/Patch/Remove
   of connector nodes. Connectors are semantic relations the `atlas-ai`
   context beacon can carry (Art. VIII).
3. **`ImageAdjust` gains `invert: bool`** (CSS `invert()`); `Ctrl+U` opens
   the existing adjust controls as a popover. Both land in the egui painter
   and `slate-artifact` in the same change (Art. IV) — `imagefx.rs` mirrors
   the pixel math.
4. **Sticky notes are a preset, not a kind**: `N` places a Text node with a
   fill, padding, and autosize behavior. No new NodeKind; no new export
   path beyond what Text+fill already serializes.

## The input layer (Slate board)

1. **Tool families.** `BoardTool` gains `Brush`, `Eraser`, `Eyedropper`,
   `DirectSelect`, `Connector`, `ZoomTool` (+ later `ScaleTool`). Single-key
   switches stay Board-only and typing/present-suppressed (existing gate).
2. **Color state.** `BoardColors { fg: Rgba, bg: Rgba }` in app state,
   persisted in prefs. `D` resets, `X` swaps, `I`/Alt+click sample. Brush,
   shape, text, sticky creation consume `fg`.
3. **Brush presets** are declarative assets (Art. VII.3): a small TOML/JSON
   list (width profile, feather, color-slot) in user space; `[`/`]` step
   width, `,`/`.` cycle presets (P2). Painting uses the existing
   vector-ink pipeline (`fit_polyline` → `Path` node, cached `InkMesh`).
4. **Constraint layer.** One struct consulted by every draw/move gesture:
   persistent `ortho: bool` (F8), `snap_grid: bool` (F9), `show_grid`
   (G/F7) + **one-shot Shift** which *inverts* ortho while held (Rhino
   semantics — Shift constrains when ortho is off, frees when on).
5. **Direct selection (A)** operates on `Path` nodes: anchors as hollow
   squares (filled when selected), drag anchors/segments/handles, marquee
   over anchors, Alt breaks handle symmetry. Every drag journals one Patch.
   `Ctrl+J` joins nearest endpoints of selected open paths (vector-ink
   geometry, journaled).
6. **Eraser (E)** P1 semantics: whole-stroke delete on drag-over
   (hit-tested via `vector-ink::hit_stroke`), journaled Removes; segment
   splitting is P2.
7. **Zoom tool (Z)**: click = step in at point, Alt+click = step out,
   drag = zoom-window (fit dragged rect). Purely camera — never journaled.

## Shared overlays (atlas-shell)

1. **Minimap (M, both apps).** A squircle overlay (dock corner opposite the
   tools dock): downscaled bounds view + viewport rectangle; click/drag to
   move the camera. Content comes from a `MinimapModel` the app supplies
   (Slate: node rects by kind color; Atlas: tree block rects) — shell owns
   painting/interaction (Art. X). Cached texture, regenerated on content
   change, not per frame (Art. II).
2. **Canvas palette (double-click empty board).** Grasshopper's beloved
   gesture: popup at the click point, fuzzy search over (a) placeable
   things — sticky, text, frame, shapes — and (b) registered commands.
   Enter places/executes *at that canvas point*. Atlas keeps double-click =
   zoom (shipped); Atlas reaches the palette via `Ctrl+K` later (P2).
3. **Radial menu (middle-click, P2).** Context commands (selection vs
   empty canvas) in eight sectors around the cursor; fed by the registry;
   middle-*drag* stays pan.
4. **History window (F2, Slate).** Renders `atlas-commands::History` —
   named commands with author attribution; doubles as the undo-stack
   inspector. Atlas keeps F2 = Assign (muscle memory), reaches history via
   Advanced.
5. All new tunables (minimap size, palette width, radial radius…) go in
   `ui-tokens.toml` per `docs/ui-tuning-workflow.md`.

## Atlas adoption set

Atlas is a *file* canvas — no authored geometry, so drawing/board tools are
Slate-only. Atlas adopts: registry + Space repeat + Esc stack, minimap (M),
Ctrl+F (focus filter search), Tab/Shift+Tab (cycle filtered matches with
camera follow), Z zoom tool, F1/F3/Ctrl+Shift+P, arrows pan, Ctrl+C (copy
selected paths), Ctrl+N (new tab). Rejections in `KEYMAP.md`.

## Performance holds (Art. II)

- Brush strokes tessellate on stroke-end (live preview uses the incremental
  draft mesh, same as Pen today); cached by path+style+zoom bucket.
- Connector beziers recompute only when an anchored node's rect changes;
  meshes cached like other paths.
- Minimap renders to a cached texture on content-generation change.
- Palette/radial/history are overlay UI — zero cost while closed.

## Delivery order

- **P1 (this project):** registry + repeat/Esc/history + palette; scene
  flags (group/lock/hide) + connectors + wire gestures; brush/eraser/
  eyedropper/color state; direct selection + join; constraint layer keys
  (G/F7/F8/F9); Ctrl+F, Tab cycling, minimap, Z tool, clipboard
  (Ctrl+C/X/V, paste-in-place), z-order keys, Ctrl+U/Ctrl+I, C crop key,
  F1/F3, Ctrl+N, Ctrl+Shift+P; Atlas adoption set.
- **P2 (specced now, built later):** radial menu, scale tool, rulers/
  guides, brush preset cycling + F6 color panel, graphic styles, tool-
  family cycling (Shift+key), segment-splitting eraser, image-pixel
  eyedropper, nested groups, show-hidden picker, Atlas palette.

## Spec index (Stage 2 outputs)

| Spec | Covers |
|------|--------|
| `specs/command-registry.md` | atlas-commands crate, repeat, cancel stack, history, palette data |
| `specs/connectors.md` | Connector model, grips, wire gestures, artifact serialization |
| `specs/brush-color.md` | Brush/eraser/eyedropper, color state, presets, `[ ]` `, .` keys |
| `specs/direct-selection.md` | A tool, anchors/handles, join, sub-object selection |
| `specs/scene-flags.md` | group/lock/hide semantics across paint, hit-test, export |
| `specs/overlays.md` | Minimap, palette UI, radial (P2), history window, search (Ctrl+F) |
| `specs/constraints.md` | Ortho/snap/grid layer + Shift one-shot semantics |
| `specs/atlas-adoption.md` | The Atlas-side command set |
