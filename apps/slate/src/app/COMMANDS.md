# Slate — Commands & shortcuts

Same rule as File Atlas: all keyboard bindings, mouse gestures, and navigation
commands are registered in one place so users can look them up in
**Advanced → Commands & shortcuts**.

The default keymap (which keys are bound to what, and why) is governed by
`docs/keymap/KEYMAP.md`; the command registry / tool-mode architecture behind
it is `docs/keymap/ARCHITECTURE.md`, and per-feature specs live in
`docs/keymap/specs/`. Consult those before adding or changing bindings.

## Rule for every change

1. **Register it** in `apps/slate/src/app/commands.rs` → `SPECS` as an
   `atlas_commands::CommandSpec`: stable namespaced `id`
   (`"board.tool.select"`, `"app.save"`, …), `category`, `name`, `binding`
   text, a machine `chord` when key-drivable, the `Repeat` rule (the
   never-repeat set is spec'd in `docs/keymap/specs/command-registry.md`),
   `Availability` flags, and palette `aliases`.
2. **Implement it** as a `dispatch` arm (`app/dispatch.rs`) — key input,
   the canvas palette, menus, and dock buttons all route through
   `SlateApp::dispatch`, which pushes the F2 history entry. View-local keys
   that need a freshly computed layout (`F` fit, `+`/`−` zoom in Grid/Venn/
   Lens) stay in `canvas.rs`/`lens.rs` with `chord: None` doc rows.
3. **Do not** duplicate shortcut lists elsewhere — the Advanced window
   renders `SPECS` via `commands::shortcuts_reference_ui`, and
   `Registry::validate()` runs in a unit test (chord collisions fail CI).
4. **Keep categories stable:** Navigation, Files, Selection, Workbook, Board,
   Presentation, Lens, plus **Commands** (repeat / history / palette meta).

## Module map

| Concern | Location |
|---------|----------|
| Canonical `SPECS` table + reference UI | `commands.rs` |
| Registry dispatch, hotkeys, Esc cancel stack, Space/Enter repeat | `dispatch.rs` |
| Overlays: minimap model, palette, history window, search, Tab cycling | `overlays.rs` |
| Board clipboard (copy/cut/paste, connector remap) | `clipboard.rs` |
| Advanced settings panel | `ui/advanced.rs` |
| Canvas mouse (pan, turbo pan, clicks, tag menu) | `canvas.rs` |
| Board gestures (tools, move/resize, Alt-drag duplicate, marquee) | `board.rs` |
| 3D viewport gestures (orbit / pan / zoom, padlock) | `board.rs` routes into `model3d.rs` |
| Presentation navigation | `present.rs` |
| Lens graph (pan, focus, expand, open source) | `lens.rs` |

## Keymap wave 2a bindings (registry, overlays, small commands)

- **Space (tap) / Enter (idle)** — repeat the last repeatable command
  (Rhino semantics). Space fires on release, only for taps < 250 ms with no
  pointer use while held — Space+drag stays pan. Enter defers to crop mode,
  path drafts, and text editing first.
- **Esc** — pops exactly one cancel layer via
  `atlas_commands::cancel_target`: draft (crop / path) → non-Select tool →
  selection → chrome (menus, adjust popover, inline tag editor). Lens focus
  clear stays first, as before. Overlays with a focused text field (palette,
  search) own their Esc; the inline text editor commits on its own Esc.
- **F1** commands reference (Advanced) · **F2** command history window ·
  **F3** Selection panel toggle · **F4** mark this moment in the session
  activity log · **Ctrl+Shift+P** Advanced ·
  **Ctrl+N** = new tab · **Ctrl+T** = Trim (Board) ·
  **Ctrl+Shift+T** = Split (Board).
  Advanced → Session log shows the path and last stall; open the folder
  from there (`%LOCALAPPDATA%\NativeFileAtlas\session-log\`).
- **M** minimap (all views; pinned state persists in chrome prefs) ·
  **Ctrl+F** canvas search (Enter / Shift+Enter cycle + camera fly, Esc
  closes; non-matches dim to 35 % at paint time) · **Tab / Shift+Tab** cycle
  visible, unlocked objects in reading order with minimal camera nudge.
- **Double-click empty board** — canvas palette at that point (fuzzy search
  over commands + aliases like "note", "box", "circle", "slide").
- **Type-to-command (Board)** — typing a letter with no bare shortcut opens
  the canvas palette seeded with that text. Bare letter shortcuts (`B`,
  `L`, …) wait ~700 ms before committing; a second character inside that
  window promotes the keystroke into the same palette entry mode (so
  typing `brush` is not stolen by the `B` tool binding). Esc cancels a
  pending hold; pointer-down or another chord commits it early.
- **PageUp / PageDown** bring-to-front / send-to-back (**Ctrl+B** = send to
  back) · **C** crop the single selected croppable image · **Ctrl+U** image
  adjust popover · **Ctrl+I** invert image colors (journaled).
- **Ctrl+C / X / V** board clipboard (JSON on the OS clipboard too;
  connectors ride along when both ends are copied, outside anchors degrade
  to Free) · **Ctrl+Shift+V** paste in place · repeated pastes step +24,+24.
- **Align widget** (Grasshopper): with the Select tool and 2+ nodes selected,
  two icon clusters sit outside the group box (bottom and left). Bottom:
  align left / center / right / distribute horizontally. Left: align top /
  middle / bottom / distribute vertically. Press commits `board.align.*` /
  `board.distribute.*` as one undo step. Distribute icons stay inert until
  3+ are selected. The same commands are in the palette. Selection chrome
  is a silhouette outline (no corner or midspan squares) that follows the
  painted geometry.
- **Group reposition** (Ctrl+Alt+Shift): while dragging a group-box edge or
  corner with all three modifiers, members keep their size and only
  translate. The opposite union handle stays put on every corner and
  edge — the same origin rule as a scale. Without that chord, group
  resize still scales (and can squash) each member. See P1.node.transform.
- **Four-dot column** (stacked caption and icon strip, per palette):
  Minimize, layout toggle (dock-wide: Icon strip / Stacked view),
  Advanced (fullscreen catalog canvas of the palette's tools — same
  camera as the board; right-click or linger for toolbar / clipboard /
  Duplicate / Use; linger yields to an open context menu), Drop to canvas (`board.dock.drop` — journals
  a `DockStrip` node; unlimited copies; baseline dock stays independent).
  Hover labels share one chip centered above the dots. A click on a
  canvas-copy icon **arms** the command; click-hold-drag anywhere on
  that node **moves** the copy. The copy is the same fieldset strip as
  the docked flyout (title in the border, contain-scaled). Instant
  actions (join, grid, snaps, color swap) still fire on click. See
  `P1.dock-strip`.
- **F8** ortho toggle · **F9** snap-to-grid · **G / F7** board grid — dock
  Grid/Snap buttons dispatch the same commands. **Object snaps** (End, Mid,
  Center, Near, Intersection, Quadrant, Perpendicular, Tangent) plus
  Snap to grid live under Document Settings → Object snaps; each kind is
  a registered command (`board.osnap.*`). **Smart guides**
  (`board.smart_guides`) align to nearby objects in the same row or
  column; **reach** (`board.snap_reach` / `.tight` / `.nearby` / `.wide`)
  limits how far they look. Alt suspends object snaps and smart guides
  for the current pick. Tan and Perp stay inert until a gesture has a
  prior point. 3D / NURBS snaps are portal-local (a Rhino view), not
  listed here.
  **Wires** (bezier / orthogonal) are the same Document Settings palette
  (`board.wire.bezier`, `board.wire.orthogonal`, `board.wire.routing`);
  orthogonal wraps host geometry and ties go right, then down.
- **Arrows with nothing selected** pan the board canvas (Shift = faster);
  nudge with a selection is unchanged.
- **Agent portal** commands are registered alongside Repository Lens:
  `board.portal.agent`, `portal.agent.bind`, `portal.agent.send`,
  `portal.agent.provider`, `portal.agent.reveal`, `portal.agent.launch`,
  `portal.agent.focus`, `portal.agent.get_key`, `portal.agent.switch_chat`,
  `stage.accept`, and `stage.reject`. Bind journals the project folder on
  `PortalNode.source`. A missing Cursor API key opens the dashboard mint
  page and accepts a paste in the portal — never a dead-end sentence.
  Click selects the frame; double-click / Enter takes contents focus
  (`P1.portal.contents-focus`) so Cover Flow does not steal the board
  wheel. They use the file-link/staging contract, so agent edits remain
  visible and human-accepted.

### P1 simplifications (deliberate, revisit later)

- Palette placement: **frame**, **rect**, **ellipse**, **portals**, **text**,
  and **sticky** place immediately at the invocation point (default size).
  Line / pen / brush still arm — they have no click-default box. On the
  canvas, an armed area tool also click-places; drag still sizes.
- Search dims Board nodes and Grid/Venn thumbnails; **Lens is excluded**
  (it has its own focus dimming), and the Lens minimap model is also
  skipped in P1.
- Tab cycling covers Board (reading order) and Grid/Venn (layout order);
  Lens is excluded in P1.
- F1 opens the Advanced window (the commands reference is a section inside
  it; there is no per-section scroll target yet).
- Path-draft finishes don't push history entries yet (Brush/Eraser/Sticky
  creation commits do — Space repeats re-arm those tools).

## Keymap wave 2b (ink tools, wires, direct selection, scene flags, ortho)

### Color state + ink tools

- **BoardColors { fg, bg }** lives on the app, persisted in
  `slate-settings.json`; defaults are theme ink/paper. **D** resets to the
  theme defaults, **X** swaps. Consumers: Brush strokes and new wires (fg);
  the eyedropper writes fg (Alt: bg). A **Colors** dock button opens the
  fg/bg chip pair (standard color picker) + Swap/Reset.
- **B — Brush**: freehand ink through the same `vector-ink` fitter as the
  Pen, with expressive defaults (round caps/joins, slight end taper), color
  = fg, width = `brush_width` (persisted). The tool **stays armed** after a
  stroke. **Shift+click** draws a straight segment chained from the last
  stroke end (chain breaks when the tool re-arms). Held **Alt**
  spring-loads the eyedropper (click samples fg) — Select-tool Alt-drag
  duplicate is untouched.
- **E — Eraser**: drag scrubs across ink; only Path/Line shape strokes are
  erasable (images, text, frames, and connectors never are). Touched
  strokes render at 30% until release; release = one journal group of
  Removes; **Esc cancels** (nothing was mutated). Hidden/locked strokes are
  skipped.
- **[ / ]** step the brush width — the eraser width while E is armed —
  using the Photoshop tiers in **screen px** converted by zoom
  (`<10:±1 · 10–50:±5 · 50–100:±10 · >100:±25`). A width circle (solid core
  + fainter feather ring) tracks the pointer while Brush/Eraser is armed.
- **I — Eyedropper**: samples the topmost node's salient color
  (shape/path stroke → fill → text color → sticky fill → frame fill; image
  nodes yield only their border stroke — raster sampling is P2). Click →
  fg, **Alt+click → bg**. Sampling ring: outer = hovered candidate,
  inner dot = current fg. Never journaled (tool state).
- **N — Sticky note**: click places a 200×200 Text-node preset (sticky
  yellow fill, dark ink) and the caret enters immediately; the tool stays
  armed. **Tab / Shift+Tab while editing** a sticky commits it and spawns
  an adjacent sibling (24-unit gap) right/left, moving the caret — object
  Tab-cycling stays suppressed while editing. *P1: no autosize — the text
  clips at the note bounds, exactly like the artifact's `overflow:hidden`.*

### Connector wires (Grasshopper grammar)

With the **Select tool**, hovering within ~8 px of a non-connector node's
side midpoint reveals that grip. A press on the grip starts a wire and
beats edge resize at that point; the rest of the edge still resizes.

| Gesture | Behavior |
|---------|----------|
| Drag from grip | Rubber-band bezier; within 14 px of another node's grip (t = 0.5) or edge (t = projected) the preview snaps solid. Release on target → journaled Add. Release on empty canvas → the canvas palette opens there (placeables ranked first); placing a frame/text/sticky auto-connects to its nearest side; dismissing = no connector. Release back on the source node cancels. |
| Shift+drag | Identical add (cursor shows **+**) — whiteboard additive default. |
| Ctrl+drag (grip with wires) | **Detach** the nearest end; it follows the cursor (cursor shows **−**). Release on a grip/edge = journaled rewire Patch; on empty = Patch to Free at that point. |
| Ctrl+Shift+drag | **Move all** ends on that grip; release on a target grip re-anchors all (one Patch group); release on empty cancels. |
| Drag a selected connector's endpoint dot | Same as Ctrl-detach (the discoverable path). |
| Click a wire | Stroke hit-test (8 px). Delete/Backspace removes. Right-click → arrowhead start/end, Default/Faint, Edit label, plus the standard rows. |
| Double-click a wire | Inline label edit at the midpoint (Enter/Esc/click-away commits one Patch). |

Esc during any wire drag cancels (the `ActiveOperation` cancel layer).
Painting goes through the path-mesh cache with the **bezier geometry in the
cache key**, so a moved endpoint node retessellates by construction; Faint
= 40% opacity; arrowheads = filled triangles sized `max(4×width, 10)`;
label = 14-unit sans in the stroke color — all matching the artifact
writer. **If an anchored node is hidden, the whole connector is skipped**
(painter + artifact agree). Deleting a node degrades wires anchored to it
to Free ends *in the same undo group*. Connectors never join frame
membership/slides, are marquee-selected only when their AABB is **fully
inside** the rect, and their `Node.rect` re-syncs to the derived AABB once
per scene generation (never per frame).

### Trim (Ctrl+T) + Split (Ctrl+Shift+T)

Same Rhino two-phase syntax: pick cutting objects (or preselect them),
Enter, then click.

- **Trim** deletes the clicked span (open) or arrangement face (closed).
  Text/images store the remainder in `Node.clip`. Shift+click near an open
  end extends it to the cutter.
- **Split** keeps every piece: an open path becomes one Path per span; a
  closed shape becomes one filled Path per face (a circle inside a rect
  yields the disk **and** the holed outer). Text/images/frames/portals are
  not targets. Each click is one undo.

### Direct selection (A) + Join (Ctrl+J)

- **A** targets `Path` shapes; a **Line promotes to a 2-anchor path on its
  first direct edit** (the promotion rides inside the drag's single Patch).
  Anchors paint as ~7 px hollow squares (selected = filled); selected
  smooth anchors show handle lines + round dots.
- Click / Shift+click selects/toggles anchors; clicking a segment selects
  its two anchors; a marquee over empty canvas selects anchors *of the
  current target path* (P1 narrowing of "across shown paths"); dragging
  anchors moves them (ortho/Shift = 45°); dragging a segment translates
  straight segments or reshapes curved ones with **handle angles
  preserved**; dragging a handle adjusts curvature (**Alt breaks
  symmetry**); double-click an anchor toggles corner ↔ smooth; arrows nudge
  selected anchors (Shift ×10, coalesced). One drag = one journaled Patch.
  Direct edits bake the node's rotation into the path (world shape
  unchanged).
- **Esc order**: anchor selection → node target → tool = Select.
- **Ctrl+J**: two selected endpoints of the A-tool target merge (within the
  snap radius) or close the path; one selected open path closes (merge
  within 24 world units, else a straight seam); 2+ selected open paths join
  at nearest endpoints into one node keeping the **first** path's style
  (one Remove+Add group). If any selected operand is closed (rect, ellipse,
  closed path), Join is a **region union**: closed shapes union as fills;
  open curves become ribbons of their stroke weight (1 world-unit hairline
  if the stroke is none). Frames, portals, text, and images are skipped.

### Scene flags (hidden / locked / groups)

The semantics matrix in `docs/keymap/specs/scene-flags.md` is normative:

- **Hidden** (Ctrl+H / Ctrl+Shift+H show all): skipped by paint, hit-test,
  marquee, Tab cycling, select-all, smart guides, present mode, and the
  artifact. Hiding plays a 150 ms ghost fade. Wires anchored to a hidden
  node are skipped entirely until it returns.
- **Locked** (Ctrl+L / Ctrl+Shift+L unlock all): paints normally, **stays a
  smart-guide snap source**, but leaves selection and every edit path.
  **Ctrl+Shift+click force-selects** a locked node (grayed handles) for
  one-off edits.
- **Groups** (Ctrl+G ≥2 nodes / Ctrl+Shift+G): flat GroupKeys. Click any
  member → whole group (Ctrl+click toggles the group); marquee including a
  member → whole group; **Ctrl+Shift+click → single member**. Group
  moves/resizes ride the existing multi-selection machinery via selection
  expansion (`expand_selection_to_groups` — the single source of truth).
  Duplicates (Ctrl+D / Alt-drag / paste) get fresh GroupKeys. Tab cycling
  counts a group as one stop.
- Right-click an object → Group/Ungroup/Lock/Hide rows; right-click empty
  canvas → "Show all hidden (n)" / "Unlock all (n)" when nonzero. The
  bottom readout shows clickable "n hidden / n locked" chips.
- Connectors may anchor to grouped/locked nodes normally.

### Ortho (F8 + one-shot Shift)

`effective_ortho = board_ortho ^ shift` feeds: node move drags (drag vector
snaps to 45° steps; smart-guide adjustments **project onto** the ortho
line; grid snap suspends), polyline/bezier draft segments, wire add-drags
(F8 only — Shift already means "add" in the wire grammar), and
direct-selection anchor drags. Resize aspect conventions are untouched.
While an ortho drag is live, subtle hash ticks paint through the drag
origin along the snapped axis.

### Zoom tool (Z)

**Z** (Board / Grid / Venn — Lens keeps its own camera keys) arms a
**transient zoom mode**, app-level like Atlas's: the underlying board tool
is untouched and re-arms on disarm. While armed, the primary button belongs
to the tool — **click = ×1.5** at the pointer, **Alt+click = ÷1.5**,
**drag = zoom-window marquee** (release fits that world rect through the
existing fit plumbing; the marquee cancels via Esc as an `ActiveOperation`
layer, the armed mode pops as the `Mode` layer). **Esc or Z again disarms.**
Right-drag / middle-drag / Space+drag still pan; the scroll wheel still
zooms. Crosshair cursor + a "Zoom (Z)" hint chip in the lower-left.
Camera-only — never journaled, never repeatable.

### Wave 2b P1 simplifications / deviations (documented)

- The fg/bg chip pair lives in Slate's dock (this wave could not touch
  `atlas-shell`); lifting it into shared chrome for Atlas is P2.
- Eyedropper samples node styles, not pixels (raster sampling P2). For text
  nodes the text color wins over a sticky fill unless it is transparent.
- Wire drags ignore the Shift-ortho inversion (Shift = additive add per the
  Grasshopper grammar); F8 ortho still constrains them.
- The context beacon does not yet carry connector relations —
  `AiAppContext` lives in `atlas-ai` (P2 with the crate change).
- Multi-selection adornment outlines a connector's AABB; the curve
  highlight + endpoint dots appear when it is the single selection.
- Align/distribute still treat group members individually (groups-as-units
  is P2).
- Direct-select anchor marquee is limited to the active target path.

## Board gesture conventions (reference)

- Single-key tool switches (`V F R O L T`) are **Board-view only** and are
  suppressed while typing or presenting. Grid/Venn keep `F` = fit view; the
  Board uses `Home` for fit because `F` is the Frame tool there.
- **Armed create-tool chrome** (P2.GhostFollow): the moment Frame, Rect,
  Ellipse, Text, Sticky, or a portal tool arms — menu, palette, dock, or
  hotkey — the pointer tints and a 22 px silhouette follows the **snapped**
  world point (osnap + smart-guide forcefield). DragScale snaps both
  corners: the live rubber-band's moving edges, not a naked cursor.
  Click and drag still use the existing place / expand rules; Shift
  aspect goes through one shared `board_place::place_rect` and never
  becomes F8 ortho. Esc disarms with no node.
- **Create toolbar flyouts**: Frame, Portals, Shapes, Text, and Actions
  open a volatile body on single click and pin on double click. A
  single click on an already-pinned icon collapses that palette.
  Hover is bidirectional: the host icon lights its palettes and a
  hovered palette lights its host icon. Hover beside or below the
  icon bar and click to collapse it into a blister on the readout;
  pinned palettes stay. Object
  properties and Document settings use the same model. Hover name chips
  on the primary dock appear only while the pointer is on that icon;
  moving onto the flyout (pinned or volatile) clears them immediately,
  and the chip paints in front of any pinned toolbar. List mode keeps
  the framed popover; a caption control (three squircles) on any list
  switches **all** pinned palettes to a free-space icon strip (65%
  squircles, hex-packed, primary-icon order, dividers between
  categories). One stacked-list glyph sits at the far right of the
  band. Pins and the chosen layout persist in chrome prefs.
- **Alt + drag** duplicates the grabbed selection (Figma convention);
  `Ctrl + D` duplicates in place with a 24px offset.
- One gesture = one undo step: live drags journal their net effect on
  release; inspector slider scrubs coalesce (1.5 s window per node).
- **Hover transform chrome** (Select tool): Windows-style resize is live on
  any rectangular node without a prior selection. Hovering an edge shows a
  bidirectional arrow; hovering a corner shows a 45° arrow. Rotatable kinds
  (shapes, frames, text, images) show a 90° arc cursor just *outside* a
  corner; portals, connectors, and simple lines do not rotate. A press on
  a wire-grip midpoint starts a connector and suppresses resize there;
  the rest of the edge is resize.
- **Resize aspect convention** (single node and group alike): corner drags
  scale proportionally by default; holding `Shift` frees the aspect
  (distortion scaling). Edge drags are single-axis, with `Shift` locking the
  aspect instead. `Ctrl` resizes about the center.
- **Multi-selection group transforms**: with 2+ objects selected the group
  bounding box is an outline (no grip squares) plus rotate zones on hover.
  Corner/edge drag scales every member about the opposite corner/edge
  (aspect convention above); outside-corner drag rotates every member
  about the group center. Journaled as one undo step.
- **Text editing** commits on Escape, focus loss, or clicking anywhere
  outside the text box (the click also performs normal selection).
- **Crop mode** (InDesign-style): double-click an eligible image (or
  right-click → Crop image, or Selection inspector → Edit crop on canvas) to
  edit its crop directly on the canvas. The full uncropped image shows
  ghosted at its content rect with a scrim outside the crop window; dragging
  the eight window handles moves the mask while the content stays put (rect
  and UV crop change together); dragging inside the window (the center
  content-grabber ring) slides the content under the mask. One crop drag =
  one undo step. Finish with Enter, Escape, or a click outside the image —
  the click passes through to normal selection. Eligible media: textured
  images, PDF pages, video posters, and doc thumbnails; 3D viewports and
  text snippet cards have no crop. Rotated nodes are supported by doing the
  window math in the node's local (unrotated) axes.
- **3D viewports** (placed `.3dm` models) invert the drag convention while
  *unlocked*: drag = orbit, Shift+drag = pan, scroll = zoom — Rhino
  semantics inside the node. **Double-click a locked viewport to unlock it**
  (double-click enters crop mode for croppable images and opens the file for
  the remaining kinds); the padlock
  (hover, top-right) toggles the live state too. Orbit drags also select the
  node, so its resize handles stay available while live — handle presses
  always beat orbit. Camera poses journal as one undo step when the viewport
  locks (padlock click, 30 s idle, tab switch, present, or export).

## Board portals

- **Repository Lens** and **Status Board** are generated portals (Art. V.3):
  the frame, source, and query are journaled; contents are derived and never
  stored. Place from the canvas palette or the Portals dock flyout — no
  single-key chord. Click places a default size; drag is free-aspect; Shift
  locks 16:9. Binding is a later, non-modal step (empty-state Browse or
  inspector Choose…).
- **Status Board** binds a local `project-state.json` (or a folder containing
  one). Inspector section toggles are journaled `Patch`es. Refresh reloads
  the file; Bake copies authored Text/Shape nodes and leaves the portal live.
- **File Atlas lens** is a host portal over a local folder (`board.portal.atlas`,
  `portal.atlas.*`). Place from the Portals flyout. Binding is a later
  non-modal step, or a folder drop through the lens chooser (File Atlas is
  the default; Place files on the board dumps images). View-only: no
  filesystem writes. Open in File Atlas uses the existing Slate-hosted
  viewport. Bake writes a poster + provenance; the portal stays live.
- **Web portal** is a host portal: a new one starts at
  `https://www.google.com/` so the page itself can be used as a search
  surface. Rebind it to a URL, an `.html` file, or a folder with an entry
  file. That can embed the standalone HTML dashboard; it is not a substitute
  for the generated Status Board (JSON → native layout). `portal.web.home`
  returns the live page to the authored locator without changing the workbook.
- **Portal chrome**: web portals carry a slim Slate identity tab showing the
  locator. Fold it from the context menu or `portal.chrome.toggle` — the
  tab strip itself only hosts maximize. Other portal kinds have no tab.
  Right-click the portal for type-specific actions. Web: Copy URL / Paste URL
  (`portal.web.copy_url`, `portal.web.paste_url`).
- **Portal maximize** (`portal.maximize`): the four-corner square in the
  upper-right (or the web tab's window-control slot) fills the window at
  the screen aspect and covers Slate chrome. Esc restores. Same for every
  portal kind.

## Lens gestures (reference)

- **Pan / zoom** reuse the Grid/Venn camera: left- or right-drag to pan,
  scroll to zoom at the pointer, Shift+scroll for horizontal pan, Ctrl+right-drag
  for turbo pan, `F` to fit the laid-out graph, `+`/`−` for stepped zoom.
  The camera auto-fits once when analysis first completes.
- **Focus** a node (click its chip or container header); neighbors stay at full
  opacity, everything else dims to ~25%. Click empty canvas or press Escape to
  clear focus.
- **Expand / collapse** an expandable container (workspace, package, module)
  with a double-click on its header.
- **Open source** by double-clicking a file or item leaf (opens via the OS).
- **Code root** is chosen in the Lens sidebar (or the empty-state button);
  Rescan re-runs `code_lens::analyze_workspace` on the current root.
- Edge-kind filters and name search live in the Lens sidebar; depth quick
  buttons set how many hierarchy levels are expanded.

## Tagging gestures (reference)

- **Right-click a thumbnail** (or a selection) → tag menu: one click per tag,
  radio behavior within a group, menu stays open so several tags can be
  assigned in a single right-click instance.
- **In linked Atlas**: the same right-click menu appears on Atlas files under
  "Slate tags"; click-hold-drag carries thumbnails into the Slate window
  (arriving uncategorized).
