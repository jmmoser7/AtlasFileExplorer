# Spec — shared overlays: minimap, palette, history, search, radial (P2)

Stage-2 spec. Research inputs: `../research/miro.md` §3, §9; `../research/grasshopper.md` §3–4;
`../research/rhino.md` §2. Constitution: Art. X (shared chrome), Art. II (cached,
zero cost while closed), Art. VII (palette/radial/history are registry front-ends).

All four surfaces are `atlas-shell` modules; apps supply data models and
handle returned actions. New tunables go in `ui-tokens.toml` (`[minimap]`,
`[palette]`, `[history]` sections) per `docs/ui-tuning-workflow.md`.

## 1. Minimap (`atlas_shell::minimap`, key M — both apps)

```rust
pub struct MinimapModel {
    pub bounds: Rect,                  // world bounds of content
    pub blocks: Vec<(Rect, Color32)>,  // simplified content rects
    pub viewport: Rect,                // current camera rect in world space
    pub generation: u64,               // bump when content changes
}
pub enum MinimapAction { None, JumpTo(Pos2 /*world*/), DragTo(Pos2) }
pub fn minimap_ui(ui, model: &MinimapModel, state: &mut MinimapState) -> MinimapAction;
```

- Squircle panel, lower-right corner (opposite the readouts), ~220 px wide,
  aspect from bounds. Painted from `blocks` into a **cached texture** keyed
  by `generation` (Art. II); the viewport rectangle + interaction paint live.
- Click → `JumpTo` (center camera); drag the viewport rect → `DragTo`
  continuously. Scroll over the minimap zooms the main camera (pointer-
  anchored at the equivalent world point).
- `M` toggles; pinned state persists in `ChromePrefs`. No auto-hide (Miro).
- Slate model: node rects colored by kind (frames outlined — research
  recommends frame outlines as the slide differentiator); Grid/Venn/Lens
  views supply layout bounds/blocks. Atlas model: tree dir/file block rects
  with their existing average-color tint.

## 2. Canvas palette (`atlas_shell::palette` — Slate board P1)

- **Trigger**: double-click empty board (kept: Atlas double-click = zoom).
  Also opened by a dangling wire release (see `connectors.md`) with a
  placeables filter.
- Popup at the click point: text field (focused), result list (max 8),
  arrow keys navigate, Enter executes, Esc dismisses, click-away dismisses.
- Data: `atlas_commands::palette_query` (fuzzy over names + aliases,
  availability-filtered). Two sections implicit in ordering: **placeables**
  (sticky, text, frame, rect, ellipse, connector…) rank above general
  commands on empty query.
- **Placement contract**: the palette carries the invocation's world point;
  a placeable command dispatched from it places at that point (tool arms +
  immediate placement). General commands just dispatch.
- Grasshopper alias culture: specs' `aliases` field feeds this ("wire" →
  connector, "note" → sticky).

## 3. History window (`atlas_shell::history_ui`, key F2 — Slate)

- Anchored panel (right side, above readouts), toggled by F2/command.
- Renders `atlas_commands::History` newest-first: name, detail, author
  chip (Human / agent name — Art. VI visibility), relative time. Cap 500.
- Read-only in P1 (no click-to-rerun yet). Copy button for the log text.
- Atlas keeps F2 = Assign; Atlas history is reachable from Advanced (both
  apps get an Advanced row for it).

## 4. Board search (Ctrl+F — Slate; Atlas focuses its filter box)

- Slate: overlay strip top-center of the canvas: query field + result
  count + prev/next. Searches the active tab: item names, text node
  contents, frame titles, sticky text, tag names.
- Live: matches highlight (existing selection tint), non-matches dim to
  ~35% (Miro §9). Enter / Shift+Enter cycle results, **camera flies** to
  each (existing fit/zoom-to-rect plumbing). Esc closes (restores full
  opacity, keeps camera).
- Dimming is a paint-time modifier — no scene mutation, nothing journaled.
- Atlas: `Ctrl+F` focuses the existing Filters-dock search field (opens the
  dock if collapsed). No new overlay.

## 5. Tab cycling (both apps — no new chrome, listed here for the camera rule)

- Board: Tab/Shift+Tab cycles nodes in **reading order** (top→bottom rows
  by vertical overlap, left→right within a row — Miro's engineering-tested
  traversal, research §6), scoped to visible+unlocked nodes; groups = one
  stop. Selection ring moves; **camera nudges** minimally to keep the
  object in view (the auto-pan Miro lacks — flagged as our win). Enter on
  a text-bearing node starts editing.
- Suppressed while typing/presenting/crop (existing gates).
- Atlas: Tab/Shift+Tab cycles `file_match` entries (filtered set), camera
  follows, selection replaced.

## 6. Radial menu — **P2** (spec now, build later)

- Middle **click** (no drag — middle-drag stays pan) opens 8 sectors around
  the cursor, context-filtered by the registry (`Availability` +
  selection): empty canvas → view ops (fit, zoom sel, grid, snap, minimap,
  palette); node selected → connector, group, lock, hide, z-order, crop;
  connector selected → arrowheads, faint, label, delete.
- Click sector executes; click center/outside or Esc dismisses. Muscle-
  memory positions must stay stable per context (no reflow by relevance).

## New bindings this spec owns

| Chord | Command | Scope |
|-------|---------|-------|
| M | `canvas.minimap` | both |
| Ctrl+F | `canvas.search` | both (Atlas = focus filter) |
| F2 | `app.history` | Slate |
| Tab / Shift+Tab | `canvas.cycle_next/prev` | both |
| Double-click empty | `board.palette` | Slate board |
| PageUp / PageDown | `board.to_front` / `board.to_back` | Slate board (uses existing `reorder_nodes`) |
| Ctrl+B | `board.to_back` alias | Slate board |
| F3 | `app.properties` | both (Slate: selection inspector panel; Atlas: file Details) |
