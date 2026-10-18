# Slate — Commands & shortcuts

Same rule as File Atlas: all keyboard bindings, mouse gestures, and navigation
commands are registered in one place so users can look them up in
**Advanced → Commands & shortcuts**.

The default keymap (which keys are bound to what, and why) is governed by
`docs/keymap/KEYMAP.md`; the command registry / tool-mode architecture behind
it is `docs/keymap/ARCHITECTURE.md`, and per-feature specs live in
`docs/keymap/specs/`. Consult those before adding or changing bindings.

## Rule for every change

Agent portal presentation changes live in the card's ellipsis menu and the
Selection inspector. Name-only and summary controls are removed. Full-conversation
cards support inline replies, with the Message placeholder shown only when selected.
Compact train grips create a continuation; full-conversation and single-window grips
create two sibling fork drafts. Fill, Stroke, Bundle and Unbundle use the shared
selection squircles. Unbundle expands one nested layer and restores horizontal train
layout. History rails cannot be disconnected; deletion prunes the downstream view
and Undo restores references without rerunning inference.

`board.hover_highlight` opens the hover preferences. With a primary node
kind as its detail (`frame`, `image`, `shape`, `text`, `connector`, `portal`,
`dock_strip`), it toggles that kind's passive hover outline. The checkboxes
are under Preferences → Advanced settings → Board hover highlights and
persist in `slate-settings.json`.

Browser pictures paste as linked image nodes, using offered PNG/DIB pixels,
promised image contents, an HTML image source, or a direct image URL. Image
intent wins over the hosting-page URL; each placement preserves aspect ratio
and undoes in one step. Image transfer/decode runs in the background.

The bottom **Tags** capsule shows each tag's color disc and readable name.
Hover reveals Show on the top half and Hide on the bottom half. Shift/Ctrl-click
selects several tags; Show/Hide on a selected tag applies to that batch.
`board.tags.visibility` receives `{tags:[id,...],visible:true|false}` and
journals the existing hidden flag in one undo step. Matching any selected
tag is the current implementation choice. Linked-image assignments and
tagged-frame content participate; hidden frame membership remains available
for Show. It never changes tags or source files.

The **+** in the Tags capsule opens tag creation. `board.tags.add` receives
`{name:"Tag name",color:[r,g,b],group?:id}` and journals its metadata in one
undo step. Without an explicit facet, each tag gets an independent group so
several UI tags can apply together; an existing group keeps facet exclusivity.

Drag a tool capsule's primary icon onto the canvas to place a journaled
toolbar copy at the release point, including Tags. Existing context-menu
Drop remains available. Once a generator has an explicit upstream image and
Prompt, its collapsed blister offers **Generate** directly; hover reveals
the count. `portal.agent.run` receives `{node:id}` and
`portal.agent.image_count` receives `{node:id,count:1..8}`. The expanded
generator reuses the same controls and keeps image-history chips on their
own full-width row.

Blank-wire menus offer **Artifact** as the fourth choice. Its Web host uses
shared generator prompt, model, Play/Stop and history controls. `portal.agent.run`
receives `{node:id}` for both images and artifacts. Artifact commands receive
`{node:id}`: `portal.artifact.accept`, `.reject`, `.fork`, `.memory_disconnect`,
`.memory_inspect`, and `.source_open`. `portal.artifact.revision` additionally
receives `revision`. Apply journals the completed revision; Discard restores the
previous view. Content and its pinned revision stay independent of the removable
memory trace. Showing an earlier revision never rewrites later history.

Every portal blister offers **Capture**, shared with the existing 3D viewport
capture control. `portal.capture` opens still-image and silent-video choices;
`portal.capture.still.canvas` / `.file` save PNG, JPEG, or WebP, and
`portal.capture.video.start` starts one private MP4 recording, up to 1080p at
30 fps. Its exterior timer stays visible after dismissing the launch palette
or changing selection. `portal.capture.pause` freezes capture and opens
`portal.capture.save.file` / `.save.canvas`; `portal.capture.resume` continues
without including the paused interval. Cancelling the save picker retains the
paused draft. `portal.capture.cancel` discards it. The old
`portal.capture.video.canvas` / `.file` IDs now start the same deferred-output
flow, and `portal.capture.stop` is a compatibility alias for Pause.
Canvas output adds one linked media node beside its source in one undo step.
Recording leaves source interaction live and shows Starting, Recording, Paused
or Saving with elapsed time. Pause exposes Save File, Place Board, Resume and
Discard. Capture follows the ordinary attached blister: its host, controls and
hit regions pan and scale together. Right/middle drag keeps the panel open;
a completed outside click follows the existing property-panel dismissal rule.
Maximized capture fits the source and its board-unit apron together without
changing the board camera.
Esc closes the menu before leaving portal focus. Switching away
from the source tab, deleting/rebinding its source, or closing the tab cancels
the draft. Existing `board.model_screenshot*` IDs remain aliases and preserve
3D view metadata. No capture hotkeys are added.

Web portals accept dropped page links and selected HTTP(S) URL text on the
Board: empty canvas creates a portal; an unlocked web portal receives a new
URL. Both are undoable. Native Chrome/Edge tab-strip docking is not supported;
drag the address-bar URL or a page link instead.

1. **Register it** in `apps/slate/src/app/commands.rs` → `SPECS` as an
   `atlas_commands::CommandSpec`: stable namespaced `id`
   (`"board.tool.select"`, `"app.save"`, …), `category`, `name`, `binding`
   text, a machine `chord` when key-drivable, the `Repeat` rule (the
   never-repeat set is spec'd in `docs/keymap/specs/command-registry.md`),
   `Availability` flags, and palette `aliases`.
2. **Implement it** as a `dispatch` arm (`app/dispatch.rs`) — key input,
   the canvas palette, menus, and dock buttons all route through
   `SlateApp::dispatch`, which pushes the F2 history entry. Board-local gestures remain documented with `chord: None` rows.
3. **Do not** duplicate shortcut lists elsewhere — the Advanced window
   renders `SPECS` via `commands::shortcuts_reference_ui`, and
   `Registry::validate()` runs in a unit test (chord collisions fail CI).
4. **Keep categories stable:** Navigation, Files, Selection, Workbook, Board,
   Presentation, plus **Commands** (repeat / history / palette meta).

## Module map

Board right-click menus use the approved recipes in
`apps/slate/assets/context-menus.json`; studio checkpoints and measurements
are under `design/right-click-menu-studio/`. Eligibility remains with native
capability owners. Object properties opens the existing object-local panel,
Edit spreadsheet enters the existing editor, and Agent rename opens its
inline title field. Right-clicking a layer also offers Add image as layer:
the asynchronous picker adds a separate layer in one undo step. Existing
image-drop behavior still places its pixels on the active layer.

| Concern | Location |
|---------|----------|
| Canonical `SPECS` table + reference UI | `commands.rs` |
| Registry dispatch, hotkeys, Esc cancel stack, Space/Enter repeat | `dispatch.rs` |
| Overlays: minimap model, palette, history window, search, Tab cycling | `overlays.rs` |
| Board clipboard (copy/cut/paste, connector remap) | `clipboard.rs` |
| Advanced settings panel | `ui/advanced.rs` |
| Canvas entry and camera helpers | `canvas.rs` |
| Board gestures (tools, move/resize, Alt-drag duplicate, marquee) | `board.rs` |
| 3D viewport gestures (orbit / pan / zoom, measure picks, Esc / click-outside lock) | `board.rs` and `dispatch.rs` route into `model3d.rs` |
| Presentation navigation | `present.rs` |

## Keymap wave 2a bindings (registry, overlays, small commands)

- **Selection properties** — edited on the object strip below the actual selection silhouette, with clearance for the strip width.
  The bottom dock does not host a Selection icon. `app.properties` does not
  open a dock panel.
- **Space (tap) / Enter (idle)** — repeat the last repeatable command
  (Rhino semantics). Space fires on release, only for taps < 250 ms with no
  pointer use while held — Space+drag stays pan. Enter defers to crop mode,
  path drafts, and text editing first.
- **Esc** — pops exactly one cancel layer via
  `atlas_commands::cancel_target`: draft (crop / path; polyline Esc commits two or more placed points and
  discards an incomplete draft) → non-Select tool → selection → chrome (menus, adjust popover, inline tag editor). Lens focus
  clear stays first, as before. Overlays with a focused text field (palette,
  search) own their Esc; the inline text editor commits on its own Esc.
- **F1** commands reference (Advanced) · **F2** command history window ·
  **F4** mark this moment in the session
  activity log · **Ctrl+Shift+P** Advanced ·
  **Ctrl+N** = new tab · **Ctrl+T** = Trim (Board) ·
  **Ctrl+Shift+T** = Split (Board).
  Advanced → Session log shows the path and last stall; open the folder
  from there (`%LOCALAPPDATA%\NativeFileAtlas\session-log\`).
- **M** minimap (all views; pinned state persists in chrome prefs) ·
  **Ctrl+F** canvas search (Enter / Shift+Enter cycle + camera fly, Esc
  closes; non-matches dim to 35 % at paint time) · **Tab / Shift+Tab** cycle
  visible, unlocked objects in reading order with minimal camera nudge.
- **Tab while drawing a segment** (`board.draft.direction_lock`; Line,
  Polyline, Arc, Bézier span, and the Pen / Brush / Eraser Shift line) locks the
  direction from the last point toward the pointer, as Rhino does: the
  pointer then changes only the length, and snaps land where they project
  onto that ray. Tab again releases; placing the point, Esc, or a tool
  change ends it. A padlock rides beside the pointer while locked. Held
  **Shift** inverts ortho (F8): 45° steps from the last point. The board
  keeps that Tab, so it does not move keyboard focus to a panel field.
- **Double-click empty board** — canvas palette at that point (fuzzy search
  over commands + aliases like "note", "box", "circle", "slide").
- **Type-to-command (Board)** — typing a letter with no bare shortcut opens
  the canvas palette seeded with that text. Bare letter shortcuts (`B`,
  `L`, …) wait ~700 ms before committing; a second character inside that
  window promotes the keystroke into the same palette entry mode (so
  typing `brush` is not stolen by the `B` tool binding). Esc cancels a
  pending hold; pointer-down or another chord commits it early.
  Auto-repeat of the held letter does not open command entry and does not
  start a new hold: holding `A` arms Direct Select once (user,
  28 September 2026: "general issue using charicters in this way as it
  launches the search fetur on the canvas").
- **`"` or `'` then Enter (Board)** — the Grasshopper panel entry. A leading
  quote opens the same canvas palette with the quote as its query and
  **Media: new text document** (`board.media.text_new`) as the only row.
  Typing after the quote places a fitted linked text card immediately,
  preserving case. Enter places a blank linked `.txt` card with a blinking
  caret (one undo step); blank cards can be reopened after click-away. Esc
  closes the palette and creates nothing. No key
  chord is bound; the quote arrives as typed text, so it never fires while a
  text field, draft, or page holds the keyboard.
- **PageUp / PageDown** bring-to-front / send-to-back (**Ctrl+B** = send to
  back) · **C** crop the single selected croppable image · **Ctrl+U** image
  adjust popover · **Ctrl+I** invert image colors (journaled).
- **Actions → Mirror horizontal / Mirror vertical** (`board.mirror.horizontal`,
  `board.mirror.vertical`) — mirror selected pictures, paths, and lines
  across the board axis through each center, one undo step. Dragging a
  picture's edge or corner past its opposite also mirrors it (P1.node.transform).
- **Ctrl+C / X / V** board clipboard (Slate's own format on the OS
  clipboard; copied pictures also go out as the bitmap shown plus their
  files, anything else as plain text, never JSON; connectors ride along
  when both ends are copied, outside anchors degrade to Free). Ctrl+V also
  places a copied image or files at the pointer.
  **Ctrl+Shift+V** paste in place · repeated pastes step +24,+24. One physical press pastes once; the Windows bitmap/file fallback does not also handle clipboard text.
- **Select** — click replaces the selection; **Shift+click** or **Ctrl+click**
  adds (a second Shift/Ctrl+click on the same object toggles it off).
  Shift+click empty canvas keeps the set. Shift+marquee adds. Rectangles
  honor this even when the press lands on the hover-resize band.
  Selection chrome is a subtle highlight of each shape (not a union box).
- **Corner-radius grip** (`board.shape.fillet`) — on one selected rectangle,
  regular polygon, line polyline, frame, portal, or placed-media node, the
  square grip sits on the border at the corner's tangent point (the top edge
  from the top-left corner on boxes). Drag it along that edge: away from the
  corner rounds more, toward it less, changing smoothly from the current
  amount (no jump on grab), and the held grip stays under the pointer. On
  release it settles to its resting place, a small inset from a square
  corner.
  Shift snaps to whole world units; Esc cancels the active drag; release
  records one undo step. Click the grip to type the amount: Enter applies it
  as one undo step, Esc cancels. With several such nodes selected, each shows
  its grip; dragging or typing on any one sets them all, each clamped to what
  it can show, still as one undo step. A single selected line polyline shows
  a grip near each corner instead, on the segment arriving at it; dragging or
  typing on one rounds or cuts that corner alone. The Corners panel value
  still sets every corner.
- **Polygon sides** (`board.shape.sides`) — hover a vertex of a selected
  regular polygon: a **+** shows outside it and a **−** inside it. Click +
  to add a side (up to 12) while that vertex stays put and the others
  re-space around it; click − to remove one (down to 3) and keep a
  vertex where it was. Each click is one undo step.
  The glyphs grow and shrink with the board zoom and drop when too small.
  This replaces the Corners panel's Sides number.
- **Polygon rotation and dimensions** — hover outside any polygon vertex,
  clear of the vertex pick and +/− controls, to rotate with the usual cursor
  and drag. Hold Shift to lock the gripped vertex ray to a cardinal direction
  about the polygon center; Alt suspends snapping. Stringers follow the
  current polygon's measured local axes throughout the live drag;
  typed sizes preserve its measured center. Pick vertices for local Stroke
  width/color/opacity edits; click a side away from vertices to edit the whole stroke.
- **Align widget** (Grasshopper): with the Select tool and 2+ nodes selected,
  two icon clusters sit outside the group box (bottom and left). Bottom:
  align left / center / right in a row with distribute below center; the T sits past
  the width stringer so the two do not overlap as zoom changes. Left: align
  top / middle / bottom in a column with distribute left of middle. Press commits
  `board.align.*` / `board.distribute.*` as one undo step. Distribute icons
  stay inert until 3+ are selected. The same commands are in the palette.
- **Group reposition** (Ctrl+Alt+Shift): while dragging a group-box edge or
  corner with all three modifiers, members keep their size and only
  translate. The opposite union handle stays put on every corner and
  edge — the same origin rule as a scale. Without that chord, group
  resize still scales (and can squash) each member. See P1.node.transform.
- **Two-dot column** (stacked caption and icon strip, per palette):
  Minimize, Drop to canvas (`board.dock.drop` — journals
  a `DockStrip` node; unlimited copies; baseline dock stays independent).
  Hover labels share one chip centered above the dots. A click on a
  canvas-copy icon **arms** the command; click-hold-drag anywhere on
  that node **moves** the copy. The copy is the same unlabeled fieldset
  strip as the docked flyout (contain-scaled). Instant
  actions (join, grid, snaps, color swap) still fire on click. See
  `P1.dock-strip`.
- **F8** ortho toggle · **F9** snap-to-grid · **G / F7** board grid — dock
  Grid/Snap buttons dispatch the same commands. **Object snaps** (End, Mid,
  Center, Near, Intersection, Quadrant, Perpendicular, Tangent), Grid,
  Snap to grid, smart guides and reach live under the home menu →
  Preferences → Snaps; each kind is a registered command
  (`board.osnap.*`). **Smart guides**
  (`board.smart_guides`) align to nearby objects in the same row or
  column; **reach** (`board.snap_reach` / `.tight` / `.nearby` / `.wide`)
  limits how far they look. Alt suspends object snaps and smart guides
  for the current pick. Tan and Perp stay inert until a gesture has a
  prior point. 3D / NURBS snaps are portal-local (a Rhino view), not
  listed here.
  **Wires** use their selection popup (`board.wire.edit`): Bezier/Square,
  stroke weight, Solid/Dashed and None/Arrows. Shift-click and crossing
  marquee select multiple wires. `board.wire.bezier`, `.orthogonal` and
  `.routing` edit selected wires, or set the creation default if none are
  selected. Square routing wraps host geometry and ties go right, then down.
  Crosstalk wires take the same commands; they have no selection popup, so
  their capsule runs them for the whole conversation (`{"wires": [...]}`).
- **Bumper cars** (optional tool, `contracts/bumper-cars.md`): off until
  Preferences → Configure → Tools → Bumper cars (`app.optional.bumper_cars`,
  a local preference). Then shapes and sticky notes show a Bumper squircle
  whose capsule sets On/Off, buffer and friction through `board.shape.edit`.
  Dragging a bumper node pushes other bumper nodes (`board.bumper.push`);
  Alt passes through, Esc mid-drag restores everything, and the release
  glide plus every push is one undo. No new key.
- **Suggestion box** (`app.feedback.open`) — the cherry-red singleton at the far
  left of the Board toolstrip opens a bug / feature form that writes a local
  report folder (no network). Dropped or pasted images attach while the
  form is open and do not land on the board. **Enter** ends a Reproduce
  recording (`app.feedback.finish_recording`); recording also stops itself
  after 60 s. Steps are dispatched command ids only, never their text or
  paths. Shared with File Atlas through `atlas_shell::feedback`.
- **Crash reports** (`app.crash_reports.open`) — Advanced → Crash reports →
  Open crash reports opens local diagnostic folders. Reports capture unexpected
  exits and Rust panics with recent session activity where available; some
  failures leave only partial information. Reports are never uploaded
  automatically. The next launch offers to view new reports.
- **Arrows with nothing selected** pan the board canvas (Shift = faster);
  nudge with a selection is unchanged.
- **Agent portal** commands:
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
  stroke. **Shift+drag** previews a straight segment from the last stroke
  end (else the press) and the release commits it, extending that stroke;
  the drag takes 45° steps and **Tab** locks its direction. **Shift+click**
  connects the last stroke end to the click at any angle (the start point
  resets when the tool re-arms). Shift never steps opacity;
  **Shift+right-drag** scrubs it. Held **Alt**
  spring-loads the eyedropper (`board.brush.sample`): a click samples the
  screen color under the cursor into fg (opacity kept) and the recent
  colors, and releasing Alt paints again. The sampling cursor's swatch is a
  ring, so the hotspot pixel shows the canvas. Select-tool Alt-drag
  duplicate is untouched.
- **P — Pen Shift line** (`board.pen.line`): with the Pen armed,
  **Shift+drag** previews a straight segment from the end of the last Pen
  stroke (else the press) in 45° steps, and the release extends that stroke
  as one path; **Tab** locks its direction. **Shift+click** connects the
  last Pen stroke's end to exactly the click point. One undo step each. A
  stroke that cannot grow (locked, closed, several contours) gets a new
  Pen path from its end. The Pen stays one-shot.
- **E — Eraser**: drag or click over ink; only Path/Line shape strokes are
  erasable (images, text, frames, and connectors never are). Painted
  (stamped) brush strokes are spot-erased: the pass erases just the ink it
  covers (soft, partial, and Shift-straight like the Brush) and each
  reached stroke takes one `Patch` storing the erase mark; a stroke left
  with no ink is removed. Vector strokes the eraser crosses render at 30%
  until release and are then removed whole. One pass is one journal group;
  **Esc cancels** (nothing was mutated). Hidden/locked strokes are skipped.
- **[ / ]** step the brush width — the Eraser or Smooth width while that
  tool is armed — using the Photoshop tiers in **screen px** converted by
  zoom (`<10:±1 · 10–50:±5 · 50–100:±10 · >100:±25`). An idle width
  circle (solid core + fainter feather ring) is the cursor while
  Brush/Eraser is armed; during Alt+right-drag the size circle stays
  pinned where the right button went down and grows about that point.
- **Tip HUD** — **Alt+right-drag** (`board.brush.size_hud`: size, and
  softness for Brush, Eraser, and Smooth), **Shift+right-drag**
  (`board.brush.opacity_hud`: opacity, or Eraser / Smooth strength), and
  **Ctrl+right-drag** (`board.brush.color_wheel`: Brush and curve tools
  only). They run while Brush, Eraser, Smooth, or a curve tool is armed,
  and on a committed curve: its picked vertices (Direct Select anchors or
  Select-tool grips), else the vertex under the pointer, else the whole
  selected curve under the Select tool or Direct Select (see *Picked
  vertices* below). With a drawing tool armed or a curve targeted,
  Ctrl+right-drag is the color wheel instead of turbo pan; Smooth and the
  Eraser show no wheel. The size HUD's style row restyles the whole curve,
  except that with an end grip of an open curve picked it sets that end's
  cap, arrowhead, or narrowing only (user, 28 September 2026, tl5).
- **Alt / Shift / Ctrl + right-click** (`board.brush.tip_entry`): the
  same chord released without travel (within 4 px and 0.8 s) opens that
  HUD's numeric entry (user, 28 September 2026): a small panel beside the
  HUD with size in board units (and softness), opacity or strength, or
  hue / saturation / value. Tab and Shift+Tab apply the field and move,
  Enter applies and closes as one undo step, a click away applies and
  closes, Esc restores. While it is open, digits go to it rather than to
  type-to-command. Eraser and Smooth have no Ctrl entry.
- **Alt+right-drag** (`board.brush.size_hud`) with Pen, Line, Arc,
  Polyline, or Bézier armed opens the Brush size HUD for that tool's own
  width (horizontal only; these strokes are always hard). During a line,
  arc, polyline, or Bézier draft it sets the width of the point being
  placed, and the curve tweens from the points already placed; the color
  and opacity HUDs do the same for color and opacity (P1.curve.tip-chord).
  During a Pen or Brush stroke a HUD draws nothing and the rest of the
  stroke blends into the new tip, stored as one tip per vertex. Esc
  restores the width.
- **I — Eyedropper**: samples the topmost node's salient color
  (shape/path stroke → fill → text color → sticky fill → frame fill; image
  nodes yield only their border stroke — raster sampling is P2). Click →
  fg, **Alt+click → bg**. Sampling ring: outer = hovered candidate,
  inner dot = current fg. Never journaled (tool state).
- **N — Sticky note**: click places one 200×200 Text-node preset (white
  fill, subtle drop shadow, dark ink, center alignment) and returns to Select. A black
  blinking caret is already in the middle of the note, so typing starts
  with no further click. The fill/text strip stays hidden while that caret
  is up. Clicking off commits and leaves the note. Double-click opens that
  caret again. **Tab / Shift+Tab while editing** a sticky
  commits it and spawns an adjacent sibling (24-unit gap) right/left, moving
  the caret — object Tab-cycling stays suppressed while editing. *P1: no
  autosize — the text clips at the note bounds, exactly like the artifact's
  `overflow:hidden`.*

### Connector wires (Grasshopper grammar)

With the **Select tool**, hovering within ~8 px of a non-connector node's
side midpoint reveals that grip. A press on the grip starts a wire and
beats edge resize at that point; the rest of the edge still resizes.

| Gesture | Behavior |
|---------|----------|
| Drag from grip | Rubber-band bezier; within 14 px of another node's grip (t = 0.5) or edge (t = projected) the preview snaps solid. Release on target → journaled Add. Release on empty canvas → journaled Add with a free end at that point (same curve as a detached end). The tool-search palette stays closed. Release back on the source node cancels. |
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

Shared Slate two-phase syntax: pick cutting objects (or preselect them),
Enter, then click. Ctrl+click removes a picked cutter before Enter.
Rhino Split uses target-first input; Slate retains its approved shared flow.

- **Trim** deletes the clicked span (open) or arrangement face (closed).
  Text/images store the remainder in `Node.clip`. Shift+click near an open
  end extends it to the cutter.
- **Split** keeps every piece: an open path becomes one Path per span; a
  closed shape becomes one filled Path per face (a circle inside a rect
  yields the disk **and** the holed outer). Text/images/frames/portals are
  not targets. Each click is one undo.
- Cutters work between open and closed forms both ways: an open cutter
  (line, polyline, arc, Bézier, pen) divides a closed shape along its whole
  path, and a closed shape divides an open curve. Only two-point lines
  extend infinitely. A cut rectangle, ellipse or polygon becomes closed,
  filled Paths.

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
  symmetry**, **Shift** keeps its direction, **Ctrl** scales both handles
  of the anchor — `board.curve.handle_scale`); double-click an anchor toggles corner ↔ smooth; arrows nudge
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

### Curve grips with the Select tool (`board.curve.grips`)

- One selected line, polyline (open or closed), arc, Bézier curve, or Pen
  path shows its grips through the same path-edit overlay: line end points,
  polyline vertices, arc start / through / end, Bézier and Pen anchors and
  handles. Dragging one moves only that point through object snaps; one
  drag = one journaled Patch. A filleted polyline keeps its authored radius
  on the new corners.
- Click a grip to pick that point; Shift+click adds or removes it. Picked
  points paint filled and are never journaled. Shift decides by travel: a
  Shift click without travel toggles a pick (a handle knob toggles its
  anchor); a Shift drag on a handle locks its direction. Two quick clicks
  on grips are two picks, not the canvas palette.
- **Handle keys** (`board.curve.handle_scale`, Select grips, Direct Select,
  and a span being drawn; user, 28 September 2026): **Shift+drag** keeps a
  handle's direction and changes only its length (snaps project onto its
  ray); **Ctrl+drag** scales the anchor's other handle by the same ratio,
  keeping that handle's own direction; **Ctrl+Shift+drag** does both;
  **Alt+drag** moves only the dragged handle and beats Ctrl. One undo step
  per drag; Esc restores. A Ctrl click keeps its meaning.
- **Ctrl+Shift+click a segment** (`board.subselect`; user, 28 September
  2026) picks that edge and its two end vertices; more Ctrl+Shift+clicks
  add edges or toggle one off. **Ctrl+Shift+drag a window** adds complete
  segments left-to-right, or crossed segments right-to-left, on the active
  curve (including closed paths and primitive outlines). The explicit chord
  owns a real edge even under overlapping property or dimension controls.
  Dragging a picked edge moves its vertices
  through object snaps as one journaled Patch (Esc restores), and
  **Delete** removes the picked segments: an open curve splits in two, a
  closed curve opens there. One Ctrl+Z restores.
- One selected rectangle or regular polygon takes the same picks on its
  vertices (P1.shape.vertex-style, user 28 September 2026). A rectangle
  vertex drag moves that vertex and promotes the primitive to a closed Path;
  Delete on picked vertices reconnects their neighbors and promotes likewise.
  A polygon side +/− wins on its glyph. Direct Select (A) offers the same
  vertex edit. Style-only edits keep the primitive. Each edit is one Undo.

### Picked vertices (A anchors or Select grips)

User decision, 27 September 2026 (`docs/keymap/specs/direct-selection.md`):

- **Alt / Ctrl / Shift+right-drag** edit the width, color, and opacity of
  the picked vertices only. With nothing picked, the vertex under the
  pointer is edited. With no grip picked and no point hovered, the HUD
  edits the whole selected curve under the Select tool or Direct Select
  (user, 28 September 2026, ed1), painted brush strokes included. Those
  whole-curve edits shift the curve rather than overwrite it: width scales
  every tip by one factor (tapers keep), the color wheel shifts every tip's
  hue, saturation, and value by the change from the curve's first color to
  the pick (a gradient stays a gradient), and opacity scales the curve's,
  so each vertex keeps its share, down to 0 % (a 0 % curve still selects
  on its line). The style row under the size circle restyles the whole
  curve too. One undo step per HUD; Esc restores.
- The shape property strip stays absolute: with nothing picked, its width
  scales every vertex and keeps the taper, and its color sets every vertex.
- The shape property strip sits beside the picked points and edits them.
  It offers only Stroke (width, color, opacity) and, where a picked vertex
  turns a polyline corner, Corners, which then writes only the picked
  corners (user, 28 September 2026).
- A dragged handle keeps its grab offset and snaps its tip, never onto its
  own anchor. The same applies to a Bézier span being drafted.
- **Delete / Backspace** (`board.delete`), under any tool, removes the
  picked vertices and rejoins their neighbors. Too few left (fewer than two
  open, three closed) removes the curve. One Ctrl+Z restores. Picks on a
  rectangle or regular polygon make Delete do nothing.

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
  member → whole group; **Ctrl+Shift+click → single member** (on a curve
  segment it also picks that edge; see Curve grips above). Group
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
  Media, Frame, Portals, Shapes, Text, and Actions do not arm a
  subtype on that primary click — pick one from the flyout.
  Hover is bidirectional: the host icon lights its palettes and a
  hovered palette lights its host icon. Hover beside or below the
  icon bar and click to collapse it into a blister on the readout;
  pinned palettes stay. Object
  properties and Document settings use the same unlabeled icon-strip
  capsules. Hover name chips
  on the primary dock appear only while the pointer is on that icon;
  moving onto the flyout (pinned or volatile) clears them immediately,
  and the chip paints in front of any pinned toolbar. Pins persist in
  chrome prefs.
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
- **Multi-selection group transforms**: with 2+ objects selected each shape
  keeps its own silhouette; the union box is hover-resize only (no painted
  frame, no grip squares) plus rotate zones on hover.
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
  one undo step, across every selected image. Handle zones are generous
  screen-space discs (corners) and full-length bands (edges) that reach
  just outside the box; corners win. Finish with Enter, Escape, or a click
  outside every image and handle zone — the click passes through to normal
  selection. After a crop, Space tap / idle Enter re-enters crop mode on the
  next selected image (repeat-last). Eligible media: textured
  images, PDF pages, video posters, and doc thumbnails; 3D viewports and
  text snippet cards have no crop. Rotated nodes are supported by doing the
  window math in the node's local (unrotated) axes.
- **3D viewports** (placed `.3dm` models) invert the drag convention while
  *unlocked*: drag = orbit, Shift+drag = pan, scroll = zoom — Rhino
  semantics inside the node. **Double-click a locked viewport to unlock it**
  (double-click enters crop mode for croppable images and opens the file for
  the remaining kinds). Esc or a primary press outside the node locks it
  again. Orbit drags also select the
  node, so its resize handles stay available while live — handle presses
  always beat orbit. Camera poses journal as one undo step when the viewport
  locks (Esc, a click outside, 30 s idle, tab switch, present, or export).
  The node carries no in-frame chrome: its selection strip adds **Viewport
  display** (`board.model_display`, one undo step) and **Measure**
  (`board.model_measure`, enters the viewport, two picks, then back to
  Navigate). Display offers Shaded, Arctic, Hidden line, Material mask, and
  Z-buffer. In Shaded and Hidden line, **Materials off / on**
  (`board.model_materials`) toggles imported surface colors as one undo step;
  Arctic remains white, and the diagnostic passes are unchanged. Materials
  default to On for older workbooks. Esc inside a live viewport peels the pending point, then
  Measure, then the shown measurements, then the viewport.

## Board portals

- **File Atlas lens** is a host portal over a local folder (`board.portal.atlas`,
  `portal.atlas.*`). Place from the Portals flyout. Binding is a later
  non-modal step, or a folder drop through the lens chooser (File Atlas is
  the default; Place files on the board dumps images). View-only: no
  filesystem writes. In contents-focus, folder cards and their incremental/full
  grips expand or collapse through the same implementation as standalone Atlas.
  Click a file to select it; Ctrl+click toggles selection. Drag a box on empty
  canvas (or Shift-drag) to marquee multiple files; Ctrl keeps the current
  set. Left-drag files onto the primary Slate board to link and place them
  (including frame tags and undo). Esc cancels the carry; releasing inside
  the source portal adds nothing. Drag outside the window to another
  application the same way File Explorer does (`portal.atlas.drag_out`,
  `atlas_core::shell_drag` — copy/link, never move). Unauthored fill follows
  the card slot (slightly lighter than the board) and a theme hairline
  outlines the window. Fill/Stroke squircles author the window fill and
  border. Formatting (Display icon) is the filter
  menu plus Zoom to fit (`portal.atlas.fit`). Open in File Atlas uses the
  existing Slate-hosted viewport. Bake writes a poster + provenance; the
  portal stays live.
- **Web portal** is a host portal: a new one starts at
  `https://www.google.com/` so the page itself can be used as a search
  surface. Rebind it to a URL, an `.html` file, or a folder with an entry
  file. `portal.web.home`
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

## Tagging gestures (reference)

- **Object properties → Tags** edits tag groups and assigns tags to selected media.
  Frame tag controls remain attached to frames. Tags have no separate dock toggle.
- **In linked Atlas**: the same right-click menu appears on Atlas files under
  "Slate tags"; click-hold-drag carries thumbnails into the Slate window
  (arriving uncategorized).

### Minimal Agent portals (15 September 2026)

New placements show the installed/configured program icon grid. `portal.agent.provider` returns to that grid. Codex uses a portal-owned conversation and installed ChatGPT sign-in. Connect Text or Image nodes with existing wire gestures, then Send/Generate to capture their inputs. `portal.agent.unbundle` splits completed images in one undo group. `portal.agent.stop` interrupts the active Codex or Cursor turn. Image albums use contents-focus for scroll and arrow navigation; hover reveals Generate and shared Maximize.


## Media

The primary Media palette contains Image, 3D, Video, and Text. These dispatch
`board.media.image`, `board.media.model`, `board.media.video`, and
`board.media.text`; each opens a filtered file picker and places the selected
files through the existing board placement path. Results are scoped to the
requesting tab; cancel adds nothing. Image includes raster/vector pictures,
PDF, and PowerPoint. Text includes Word, Excel, CSV, other spreadsheets, and
source code: the board and the HTML artifact show the same excerpt when the
file can be read, and a linked document card when it cannot. 3D uses the
existing model viewer. Video places the file on the board: hover across the
node to scrub its full trim window without playing, and click to play from
that frame. A codec Windows cannot open stays a poster. Web-safe files still
export as `<video>`.

`board.media.text_new` (palette: `"` / `'` then Enter, or "new text")
creates a blank text document instead of picking one. It links a new
`text-<ms>.txt` in `slate-outputs/<board>/text/` beside the saved workbook,
or in the AI workspace's `slate-outputs/untitled-board/text/` when the
workbook is unsaved; with neither, it refuses with a toast. The card opens
with the caret in it. Typed words go to that file through the off-thread
text writer, not the journal. Esc or click-away finishes typing.

PowerPoint stays linked to its source and renders a derived PDF locally using
installed PowerPoint. Word (`.doc`, `.docx`) does the same through Word, and
Unbundle writes those page images beside a saved workbook under
`assets/documents/<stem>-<hash>/`, one folder per source path (or the Atlas
data directory when unsaved). Until
Unbundle, Word stays a text excerpt. Conversion and PDF page counting run on
bounded workers; an Unbundle click while that work is still running finishes
when the preview arrives. Cloud-only files are not downloaded. A selected PDF,
deck, or Word document shows a Pages squircle on the shared property strip.
That control opens a thin Cover Flow album pallet over the document
(`board.media.page`) and Unbundle (`board.media.unbundle`) spreads every page
onto the board as a selected grid in one journal group. Without PowerPoint or
Word, place an exported PDF. Outlook `.msg` files are text excerpts.
See `docs/keymap/contracts/media.md`.

`board.assets.collect` (Collect assets into workbook folder) copies pasted and
generated images that still live in the Atlas data directory into
`assets/pasted` and `assets/generated/<provider>/<yyyy-mm>` beside the saved
workbook, and rewrites those links to workbook-relative locators in one undo
step. It leaves user-linked files outside that data directory alone, refuses
machine-private folders and cloud placeholders, and reports what it filed.
The folder layout is `atlas_core::workbook_assets`, described in
`docs/keymap/contracts/media.md`. Save and Save As run the same filing for
images placed before the workbook had a folder, and Save As also copies the
assets this workbook already keeps under its own `assets/` tree.


Text Formatting lists Replica, Arial and Courier New first, and provides independent Bold, Italic and Underline glyph toggles. These use `board.shape.edit` (`TextBold`, `TextItalic`, `TextUnderline`) and preserve the same block style through inline editing, undo, HTML and SVG export.

Selection strip: squircle Fill/Stroke/Corners/Filters/Pages/Formatting (and wire) controls dispatch `board.shape.edit` / `board.wire.edit` / `portal.atlas.fit` for any node those scene helpers support — shapes, frames, text fills, portals, images and wires. File Atlas Formatting is the Display squircle and the filter+fit editor in `selection_tools`. Image Filters is the photo-filter capsule (low-resolution filtered thumbnails, hover preview, intensity slider). A 3D model's Viewport display and Measure squircles dispatch `board.model_display` / `board.model_measure`. Frame deck/tags/images/present actions share that strip. External dimension stringers dispatch `board.shape.dimension`. Palette previews commit on icon change/outside click; an empty-canvas click also deselects. Esc cancels. Numeric dimensions edit directly in their rotated stringers and commit on Enter/outside click. RGB percentages also edit in place; slider metrics appear only during adjustment. All eyedroppers use `board.color.desktop` / the shared desktop sampler (RGB only; existing alpha preserved). The color editor's eyedropper has two modes: click it, move, and click the color; or press it, drag to the color, and release to commit (the magnifier shows a live swatch; Escape cancels). Polyline, Arc and Bezier are also discoverable as `board.tool.polyline`, `board.tool.arc`, and `board.tool.bezier`, without new default shortcuts.

Agent chat titles support double-click editing (`portal.agent.rename`) across the current linear branch segment. The output handle invokes `portal.agent.continue` by click or drag; Escape cancels placement. `portal.agent.chat` presents the complete train as a single chat window per fork segment; `portal.agent.train` restores exchange cards. Bundle/expand actions appear in the shared selection toolbar only when applicable.

Agent composer: Enter sends the message; Shift+Enter inserts a newline. Newly placed continuations focus this field automatically. Presentation changes live only in the ellipsis menu; single chat windows have no summary/identity or Unbundle action.

Media agents: a selected picture, video, 3D model, text document, note or
generated frame shows the Agent control. Its editor offers Text or Image, the
model menu (ComfyUI checkpoints with Auto, ChatGPT through the Codex sign-in,
GPT Image with an API key, or a local language model), the prompt, and for
images the count, aspect and Live. Generating on a placed image enables the
same image editor on that frame, retaining the original as its first album
image and retaining its layers. Other media use `portal.agent.spawn` to create
a downstream frame. The output-port menu still creates downstream cards.

The output-port and blank-wire menus also offer **Artifact**, an interactive
dashboard in the existing browser portal. Its attached generator uses an
installed Cursor or Codex harness and runs only on Play (`portal.agent.run`);
`portal.agent.stop` cancels. Completed output previews before Apply
(`portal.artifact.accept`) or Discard (`portal.artifact.reject`). Settings
offers Fork (`portal.artifact.fork`), Fresh eyes
(`portal.artifact.memory_disconnect`), Inspect memory
(`portal.artifact.memory_inspect`) and Open source
(`portal.artifact.source_open`). History chips select a preserved revision
(`portal.artifact.revision`). These commands add no keyboard bindings.
Play shows preparation, progress or a specific failure on the attached editor.
Pending approvals use its Allow once / Deny controls (`portal.agent.approval`).
A downstream Artifact receives separate content and Memory wires. Deleting
Memory preserves the pinned source and gives the next run a fresh agent.
Revisions preserve HTML, embedded data and assets; live filters and controls
remain browser session state. Enter/double-click interacts with the page;
Esc returns to the board.

Images sit naked on the board until selected. Their image-local blister has
one active tool panel, a technical strip above its content, and an expanding
prompt below the generation controls. Empty-canvas clicks hide it. Painting
and selection editing keep the active image pinned. Each generated album
image owns its own layers. The wheel advances one image per mouse detent;
trackpad momentum advances once until the gesture settles.

Dragging an image's left input port onto empty canvas keeps the input menu
open after release. Choose Image → image or Text → image to create an upstream
card feeding the original frame. A new image input is gray until generation
starts. Esc cancels; one Undo removes the new card and wire and restores any
capability change on the original.

`board.image.selection.lasso` starts a polygonal image selection. Click to
place vertices, hold Shift for 45° directions, press Delete for the previous
vertex, Enter to close, or Esc to cancel. Automatic object previews can be
pinned and refined with click to include and Alt-click to exclude. Right-click
and choose Create layer from selection (`board.image.selection.layer`). A
pinned selection constrains painting; committed ink retains that clip after
the selection is cleared. Dragging a layer or album result onto the canvas
creates an independent copy and retains the original.

Pictures and notes an agent makes: selected or hovered, they show typed input
ports on the left edge (picture: Media, Prompt, Style; note: Image, Prompt)
and one output port on the right. `portal.agent.spawn` is the modality menu
(Text, Image, Agent). Click the output port, or drop a wire on empty board from
any media that shows the Agent squircle, then choose Text (a sticky an agent
writes), Image (a picture an agent generates) or Agent (an agent portal in
chat train presentation, one message per card, with the source wired into its
midpoint context input as its first input; it shows the program grid until a
program is picked). Esc closes the menu and adds nothing; one Undo removes
the new node and its wire. The new wire keeps the grip it left from. Generate, Run and Enter in the docked prompt use
`portal.agent.send`; Stop uses `portal.agent.stop`. On these media the Agent
squircle edits the attached agent: model (`set_flow_model`), count, aspect,
seed and Live apply at once, and Submit runs it again. A click on a square
under a picture picks that result as its file.

Agent coding sidecars: `portal.agent.artifacts` opens the output capsule stack (requested files first, then "Also changed"); `portal.agent.open_artifact` explicitly creates the linked portal for a reference; `portal.agent.approval` answers a pending provider request once. Coding conversations are linear; `portal.agent.continue` remains a local-agent gesture. Generator model menus route advertised speed/thought choices through `portal.agent.model_parameter`; `portal.artifact.full_access` sets the machine-local default for all dashboard builders. Public Activity expands through the shared chat fold/follow owner and is separate from Memory.

Agent cards: `portal.agent.model` chooses the next Codex or Ollama response model from the
installed provider catalog. The title menu also exposes conversation rename.
`portal.agent.continue` accepts click or drag on the terminal top output handle
for coding agents. Dragging that handle, or Fork chat here, starts a branch with its own session. Midpoint ports
remain ordinary context/artifact ports. Drag-select transcript text and Copy to
copy it without moving the card. Additional context requires an explicit wire.

Agent headers have two single-click targets: conversation name renames the branch;
model name opens its picker. Agent contents focus does not capture canvas wheel
zoom. Provider/project/conversation choices activate on one click over the complete
visible tile or row.

Agent train cards (23 September 2026):

| Command | Binding | Effect |
|---------|---------|--------|
| `portal.agent.model` | Model name, tail card only | Sent cards show "Title · Model" as text; only the tail (or an unsent draft) opens the dropdown. |
| `portal.agent.collapse` | Chevrons left of the ellipsis | A single chevron steps one level, a double steps two (ledger CT1). Maximized offers a double (to collapsed) and a single (to partial); collapsed offers a single (to partial) and a double (to maximized); partial offers one single chevron each way. Each chevron owns the pointer, so a press never starts a resize, and hover is a highlight with no tooltip. Detail `step-expand`, `jump-expand`, `step-collapse`, or `jump-collapse` (older `one`, `full`, `partial`, `collapse` still parse). With no detail, selected cards toggle between full and the three-line capsule. The tail keeps its composer. One undo step. |
| `portal.agent.schedule` | Ellipsis → Schedule… / Edit schedule… / Stop schedule (Cursor, Codex) | The dialog takes the message (the composer's text or the last message sent), a date (tonight, tomorrow, Oct 1, 10/1, 2026-10-01) and a time (1am, 4:55 am, 13:30), Once / Every hour / Every day / Every week, and a required end (Until next month, Until next year, or a date). Windows Task Scheduler stores that end (`EndBoundary`) and will not register a task without one. A schedule saved before an end was required stays, and the dialog asks for one. Replies land in the conversation's `session.json`. The top strip carries no clock (ledger CT5); a pending run reads Edit schedule… in the ellipsis menu. Detail JSON: `{"open":true}`, `{"stop":true}`, or `{"start":"2026-10-01T04:55","end":"2026-11-01T04:55","repeat":"once|hourly|daily|weekly","prompt":"…"}`. |
| `portal.agent.fit` | Ellipsis → Fit to text | Appears after a resize. Resizing records the card's size in the same undo step; the text rewraps and scrolls with the wheel inside. |
| `portal.agent.fork` | Ellipsis → Fork chat here; drag the top output circle | Creates one forked chat at that checkpoint, journaled as one command and joined by the history rail. Each drag is one fork. A click without a drag still continues. A coding-agent branch gets its own session, seeded with the transcript up to that card; continuing the unbranched tail keeps the session. |
| `portal.agent.full_access` | Ellipsis → Full access (Cursor, Codex) | Toggles the person's grant for this conversation: from the next message Codex runs with approval policy `never` and a full-access sandbox, and Cursor runs without auto-review. The model name turns deep red on every card of that conversation. Stored per user in the Atlas data folder (`agent-access.json`), never in the workbook, so a received `.slate` never arrives trusted. |
| `board.delete` | Delete / Backspace; Ellipsis → Delete card | A selected chat card deletes while its composer is idle. With an empty composer focused, Delete deletes and Backspace stays with the field. With typed text, both keys edit. Unsent text is discarded. |
| `portal.agent.pocket` | Click the lower (human) context handle | Deleting context a card already sent hides it into every card that used it. A click shows it beside the card; a second click pockets it again. |
| `portal.agent.stop` | Click Stop on a streaming card's top output circle; Ellipsis → Stop response | While a reply streams, the top output circle becomes Stop (a disc holding a square) in the same place, so it stays put as the card grows downward. It stops that card's run. The card has no other Stop button. When the reply ends, the circle continues the train again. |
| `portal.agent.artifacts` | Click the right output circle | Opens the output capsule stack beside the circle. It closes when the pointer leaves the circle and stack, on Esc, or on a press elsewhere. |
| `portal.agent.spawn_output` | Click a capsule; drag a capsule onto the board | Spawns that output at the protocol size beside the card, or at the snapped drop point, with a faint provenance wire. A second click retracts it. Shift-click collects capsules instead. Deleted files are disabled. A file a later card changed spawns the copy of this card's version ("<name> vN"); the newest version spawns the live file. |
| `portal.agent.spawn_outputs` | Click Spawn all / Spawn N | Spawns the collected capsules, else every requested output, else every output, as one frame (one Undo). A table that feeds a dashboard sits left of it, wired with a Slate Link. A second press retracts the group. |
| `portal.agent.evolution` | Click a capsule's vN badge (N = the version this card's message made) | Spawns "<name> evolution": the file's captured versions left to right, each labeled with its number, the message that produced it, and its line changes. A second click retracts it. |

A click in a tail card's "Message" area focuses that card's composer on press
and does not move or select-drag the card.

Agent crosstalk (27 September 2026, contract `portal-agent-crosstalk`). A
Cursor or Codex chat card has two crosstalk ports, the midpoints of its top and
bottom edges, revealed near the card and red once wired. Every crosswire has
a blister at its mid-span that opens the wire's capsule (28 September 2026).
New cards in a crosstalk arrive collapsed. None of these has a default hotkey;
all are in the palette.

| Command | Binding | Effect |
|---------|---------|--------|
| `portal.agent.crosstalk.link` | Drag from a crosstalk port onto another conversation's card; or select two chat cards and run it | Draws the deep-red owner wire (one Undo) and opens its capsule with the Start editor. Released anywhere else, or Esc mid-drag, journals nothing. Drawing sends nothing. |
| `portal.agent.crosstalk.capsule` | Click the blister at a crosswire's mid-span | Expands it into the wire's capsule, which the wire runs through. Esc or a click anywhere else folds it. View state; nothing is journaled. |
| `portal.agent.crosstalk.start` | Start in the wire's editor | Journals each side's transcript length and starts relaying. Refuses a reviewer with Full access, a Cursor builder with Full access unless the trust tick is on, and two different project folders. |
| `portal.agent.crosstalk.send` | The Send to … pill beside the newest wire's blister, or Send in the capsule | Step mode: the finished reply becomes the other agent's next message, on a new collapsed card with a red crosswire, one Undo authored by the sending agent. A long reply's pill opens the capsule, which offers Send in full / Send the first 30,000. |
| `portal.agent.crosstalk.skip` | Skip and pause in the capsule | The reply stays on its card, unrelayed, and the crosstalk pauses. |
| `portal.agent.crosstalk.pause` / `.resume` | Pause / Resume in the capsule | Derived state; nothing is journaled. A reopened board opens paused. |
| `portal.agent.crosstalk.stop` | Stop in the capsule; Delete on a crosswire | Ends the crosstalk (journaled; Undo restores). A deleted crosswire hides and keeps its message's provenance. |
| `portal.agent.crosstalk.edit` | Edit… in the capsule | Opens the stop-rule editor beside the capsule: turns, minutes and goal, from that wire on; "Use inherited rule" clears an override. Roles change only while paused. |
| `portal.agent.crosstalk.more` | 5 more turns in a stopped capsule | Raises the turn limit of the rule in force. |
| `board.wire.orthogonal` / `.bezier` | Square wires / Curved wires in the capsule | Routes every wire of that crosstalk square or curved, one Undo, through the same command as any selected wire. Square crosswires meeting at one card bundle on nested lanes (the File Atlas rule) so they never touch. |
| `portal.agent.crosstalk.trust` | The tick in the editor, when the builder has Full access | Lets the builder act on the other agent's messages without asking. Stored per user in `agent-access.json`, never in the workbook. |

### Board collaborators (17 October 2026)

The top-right Participants capsule has separate portrait and name/caret targets.
Portrait opens the persistent conversation; name/caret opens the right-aligned
profile. `presence.photo` changes your local picture and `presence.name` edits
your name. Self-chat uses `presence.send` without starting a provider.
`presence.draft` retains unsent text; `presence.retry` explicitly retries a failed
message with its delivery identity. `presence.people` opens encrypted human
messaging setup; it grants no shared-board authority.

Double-click a canvas avatar (`presence.address`) to pause its board work, show
listening and open recent messages beneath it. `presence.pause` explicitly
resumes it; dismissal alone does not. `presence.cancel` stops request-owned work.
The avatar, hat, prop, name and farewell all scale with the board camera.

Conversation controls use `presence.provider.switch`, `presence.provider.cancel`
and `presence.model` for validated harness changes and advertised model choices.
Busy changes offer after-task or stop-and-switch; selecting the same harness is
a no-op and never resends a task. `presence.schedule` arms a bounded live-board
schedule; reopening does not restore a running job or grant.

Advanced access uses `presence.grant`, `presence.file_inputs` and `presence.own_chats` independently. The last permits only conversations created by that live agent after an actual witnessed or accepted commit. It defaults off and does not grant unrelated conversations, file inputs or Full access. `presence.helper` adds a scoped temporary helper; `presence.clone` copies configuration into a fresh identity. `presence.retire` revokes work before its farewell; `presence.restore` restores identity/history with fresh authority.

The targeted collaborator catalog includes `board.objects.create_train`, `board.objects.create_generator` and `board.objects.place_images`. Construction does not start a provider, import arbitrary files or change grants. Existing `portal.agent.run` starts an eligible generator separately. `presence.read_image` prepares one explicitly permitted source through the existing image-input pool; ordinary board reads remain bounded metadata. All authored board changes keep the normal journal, proposal, stale-input and witnessed-motion checks.

### Canvas wire settings and plain-image inputs (13 October 2026)

Right-click empty Board canvas → **Canvas wire settings** (`board.wire.canvas`). The shared color/opacity/width controls and **Bézier / Orthogonal** toggle apply edited properties to every unlocked connector, including hidden connectors, in one Undo step. Selection, untouched styles, and new-wire defaults are retained. Close or Escape discards the settings draft.

Dragging a wire near any placed image reveals **Primary image**, **Text**, and **Style image** inputs. Text, sticky notes and imported text documents default to Text on a body drop. A compatible connection enables the existing image generator in place, keeping the linked original and layers. The wire and capability change undo together. Connecting does not run a model; Run and Live retain their existing semantics.

Image-palette click-away recovery lasts **1.5 seconds from dismissal**. Returning to the same image within that window restores the expanded sub-palette without rearming Brush or lasso; returning later shows the collapsed strip. See `P1.image.palette-return`.

## Authored text and basic style commands

`presence.read_text` pages one explicitly readable authored text field using a UTF-8 byte cursor (4–4096 bytes per page). Continuations echo the returned scene generation. Text nodes, eligible shape text and an existing card `text_override` are supported; linked documents are not opened and a missing override never falls back to source bytes. Empty authored text remains readable.

`board.objects.set_text` takes explicit targets and `{text:"..."}` arguments, replacing one authored text field through the same text transformations and world-layout growth as normal composition. The direct human dispatch receives the real UI context; scoped collaborators use the existing witnessed proposal/approval/journal path. Replacement `@text` from an agent is authored content rather than a human message.

Targeted `board.shape.edit` / `board.wire.edit` take `{edits:[...]}` and reuse the existing Property transform with no vertex scope. Supported edits are FillRgb, StrokeRgb, StrokeWidth (0–10000), TextFamily, TextSize (1–512), TextAlign, TextRgb, TextBold, TextItalic and TextUnderline. Each call contains 1–16 typed edits; unsupported target/property pairs, unknown arguments, locked targets and net no-ops are refused. The ordinary human property palette keeps its existing full property request. These commands do not modify linked files, select objects, open a caret, or reposition the camera.
