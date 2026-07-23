# Canvas command project — change log

One entry per delivery wave. The governing docs are `KEYMAP.md` (what is
bound and why) and `ARCHITECTURE.md` (how it is built); per-app binding
tables live in each app's `commands.rs` (`SPECS`) and render in
**Advanced → Commands & shortcuts**.

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
