# Interaction pattern hierarchy

The shared vocabulary for tool contracts. A contract
(`docs/keymap/contracts/<tool>.md`) **never restates** a pattern from this
file — it declares what it inherits and lists only deviations and additions.

Levels (specificity increases downward):

- **L0** — universal: every tool, both apps.
- **L1** — object-class: every tool producing/editing that class of object.
- **L2** — tool-family archetypes: a shared gesture grammar several tools arm.
- **L3** — tool-specific: lives only inside the tool's contract file.

**Promotion rule (anti-bloat):** when the same rule appears in two contracts,
it moves UP to the lowest level that covers both, gets a pattern ID here, and
both contracts replace their copy with a reference. Never copy a pattern
downward. When an L2 rule turns out to apply to a whole object class, promote
it to L1, etc.

**Deviation rule:** a contract may override an inherited pattern, but the
override must be written as `deviates P2.x: <what changes>` so the exception
is searchable.

---

## L0 — Universal (both apps, every tool)

- **P0.1 Cancel stack.** Esc peels exactly one layer per press:
  ActiveOperation → Draft → Mode (armed tool → Select) → Selection → Chrome.
  (`atlas-commands::CancelLayer`.) A drag that edits nodes live — move
  (single, multi, group, Alt copy), resize, rotate, crop, endpoint grip,
  fillet — is an ActiveOperation: Esc returns every node to its press-time
  state, drops staged Alt copies, keeps the selection, and journals
  nothing. egui aborts its own drag on Esc, so no release follows; the drag
  owner (`SlateApp::cancel_node_drag`) restores instead of waiting for one.
  A release that commits something other than the drag (a saved view onto
  a model, a picture into an image) rewinds the same way first
  (`restore_press_nodes`), so its own command is the only journal entry;
  a release with no pointer position (outside the window) rewinds too.
- **P0.2 One gesture = one undo.** Everything a single user gesture produced
  reverts with a single Ctrl+Z (journal command grouping).
- **P0.3 Journal-only mutation.** Commits go through the journal with an
  author; tools never mutate the document directly (Constitution Art. VI).
- **P0.4 Repeat-last.** Space (tap, not held) or Enter while idle re-runs the
  last repeatable command — so any tool is re-armed by Space/Enter if it was
  the most recent command.
- **P0.5 Camera never blocked.** Scroll-zoom and Space-drag pan work during
  any draft or drag without dropping it; a Space *tap* is still repeat-last
  (P0.4) — only Space+movement pans.
- **P0.6 Feel constants are tokens.** Every tolerance, radius, threshold,
  alpha, and step curve is a named constant (`ui-tokens.toml` or a `mod
  consts` block) referenced by the contract — never an inline magic number.
- **P0.7 Arming routes through the registry.** Hotkey, palette entry
  (typed name + aliases), and tools-rail icon all dispatch the same
  `CommandId`; the armed tool is visible in the rail and via the cursor.
- **P0.8 Availability gating.** Commands declare availability
  (board tab, selection present, …) and are inert — not error-prone —
  when gated.
- **P0.9 Canvas-space scale.** Every object that lives on a Slate board
  or a File Atlas canvas — shapes, strokes, fills, borders, fillets,
  authored text, derived labels, icons, badges, and node-local tabs or
  sub-tabs — scales with the camera exactly as a shape does. Screen size
  is `world × zoom`. Never hold typeface size, icon size, tab height, or
  stroke width constant in screen pixels so the object "stays readable"
  while the host shrinks. A clamp with a ceiling (`(10.0 * z).clamp(8.0,
  13.0)`) is the same defect the moment the ceiling bites. When type is
  too small to read, drop it (LOD); do not freeze it. The only exceptions
  are those a contract **names**: window chrome in `atlas-shell` (top bar,
  tools rail, readouts, Advanced), pointer-attached chrome such as
  **P2.GhostFollow**, and path-edit handles — anchor squares, Bézier
  handle lines and knobs, and the draft rubber band's markers while a
  path is being drawn or directly edited. Those follow the pointer's
  editing, not the path, so they stay screen-constant (Illustrator
  convention; user decision 25 September 2026). The path they edit still
  scales. A tab on a portal frame is a canvas object, not
  window chrome — maximized, that same tab may stay screen-sized because
  it has become window chrome. Linear size is `atlas-shell::canvas_scale::px`
  (fillets, strokes, insets, handles). Type is painted through
  `atlas-shell::canvas_text`. Cover Flow is `atlas-shell::home` (one
  implementation; embed it).   Type on a yawed or perspective host (album
  faces) is laid out **once** at a fixed size, then each glyph is
  projected as a short column strip. A full-face title texture through
  `paint_artwork` foreshortens but ripples stems (affine UVs). Never
  `Painter::text` / `galley` at a projected midpoint. Hit slop may stay
  screen-constant so a tiny object remains clickable; the painted
  graphic still tracks the camera.
- **P0.10 Navigable menus own the wheel.** Any open list, dropdown,
  scrolling popup, or menu consumes wheel and pinch input while hovered.
  The canvas under it never zooms or pans (user decision 26 September
  2026). The owner is `atlas_shell::menu_wheel`: a menu calls `claim` with
  its screen rect every pass it is open; egui's own popups count without
  one. A canvas camera checks `wheel_owned` before zooming. A per-menu flag
  read by one canvas is the defect this replaces.

## L1 — Object-class

### P1.node — every board node

- **P1.node.hover-preference** passive hover highlights can be disabled per
  primary node kind in Preferences → Advanced settings → Board hover
  highlights. Defaults preserve existing highlighting. This is a local
  preference, never document data; selection/focus indicators, hit-testing,
  and editing handles remain unchanged.

- **P1.node.flags** lock / hide / group semantics (Ctrl+L/H/G family);
  locked nodes still feed smart guides.
- **P1.node.select** click select, Shift+click add/toggle, Shift+marquee
  adds; hover-resize on an unselected node yields to Shift/Ctrl pick.
  Group click selects the group, Ctrl+Shift+click a member. A board sweep
  is Window when the pointer's screen x is at or right of the press
  (fully enclosed pick geometry only) and Crossing when it is to the left
  (anything touched). Ctrl during the sweep adds, same as Shift.
- **P1.node.move** drag with smart guides; ortho (F8, Shift inverts),
  grid snap (F9), and the persistent object-snap set (**P1.node.osnap**)
  apply; arrows nudge. Smart guides (InDesign / tldraw / Keynote) only
  consider objects in the current view, in the same row or column, and
  not behind a closer neighbor. Reach (Tight / Nearby / Wide) is a
  Document Settings session preference. The forcefield pulse is the
  guide. Curve endpoints use the same point snap as drafts. A corner
  scale snaps its scaled corner to a neighbour edge (nearer axis when
  aspect is locked, both when free). Alt suspends.
- **P1.node.osnap** every live board point — hover, press, drag end, grip,
  corner handle, GhostFollow hotspot — goes through `resolve_point_snap`
  (or `resolve_draw_rect` for DragScale). Preview and commit consume that
  result. Never paint a rubber-band or ghost from the raw cursor while a
  snap is live. Point picks and bbox moves consult
  `ObjectSnapSet` (Document Settings → Object snaps, with Snap to grid).
  Applicability is
  `SnapKind::accepts(node_facets, from)` — a kind that needs a facet the
  node does not advertise is rejected (Tangent on a rect or bezier path,
  Quadrant on a polygon, End on an ellipse, Intersection on an ellipse).
  Tan / Perp require a prior point (Rhino:
  not effective for the first pick). Alt suspends. Ortho and Tab lock
  suspend point snaps (DominantOrtho). Locked nodes remain targets; hidden
  nodes do not. Connector AABBs are not snap geometry — the derived curve
  is. Priority: End > Int > Cen > Mid > Quad > Perp > Tan > Near. Any
  object snap overrides grid. Implementation: `slate-doc::osnap` (syntax)
  + `apps/slate/src/app/board_osnap.rs` (picker).
- **P1.node.transform** Select-tool bounding-box chrome is live on hover —
  no prior selection. Edges show Windows bidirectional resize arrows and
  change the cursor only — they do not paint selection handles or light the
  identity tab as a selection would. Body hover eases a soft outline
  (`board_preview` tokens). Corners show 45° arrows — the two adjacent
  edge normals, so a wide or tall box (including a 2+ group AABB) still
  gets a diagonal, not an axis arrow. Corner drag scales
  proportionally by default;
  `Shift` frees aspect; edge+`Shift` locks aspect; `Ctrl` resizes about
  center. A corner scale snaps that corner to a neighbouring edge within
  the smart-guide threshold: aspect lock keeps the nearer axis and derives
  the other; free aspect snaps both. Guide lines show while snapped. Alt
  suspends, and preview and commit use that rect. The opposite handle is
  the scale origin for every corner and edge — all four corners are the
  same rule, not four special cases.
  A rotated resize pins that origin in world space so the grabbed edge
  is the one that moves (local AABB math alone walks the far edge once
  rotation is about the live center; most visible at 180°).
  Dragging a mirrorable picture's edge or corner past its opposite mirrors
  it on that local axis (`board_snap::resize_from_handle_mirroring`,
  `slate-doc::mirror`): the rect stays positive and the flip is authored
  state in the same undo step. Other kinds clamp at the minimum size.
  Modifier matrix (Select-tool bbox):

  | Mods | Corner | Edge |
  |------|--------|------|
  | none | uniform from opposite corner | 1-axis from opposite edge |
  | Shift | free (non-uniform) from opposite corner | uniform from opposite edge |
  | Ctrl | uniform from center | 1-axis from center |
  | Ctrl+Shift | free from center | uniform from center |
  | Alt (held at press) | duplicate, then the scale above; snaps stay off while Alt is held | same |
  | Ctrl+Alt+Shift (2+ group only) | layout scale from opposite corner; member size unchanged; union opposite corner pinned. Does not duplicate. | layout 1-axis; sizes unchanged; union opposite edge pinned |

  Ctrl+Alt+Shift without the union pin walks the supposed-fixed corner
  on Nw / Ne / Sw (member extents do not scale, so remapping origins
  alone translates the stack). Se happened to look right because the
  remapped origins' min is the group origin. Implementation:
  `board_snap::apply_group_box_scale` remaps then `pin_group_union`.
  Rotate is a zone just *outside* a corner. Over it and through the
  whole rotate drag, the OS cursor hides under a circular-arrow pointer
  in the Windows cursor scheme: white, rimmed in black, OS cursor size,
  no tooltip (stated 2026-09-26). Windows ships no rotate cursor and
  egui has no `CursorIcon` for one, so this is pointer-attached chrome
  (`board_handles::paint_rotate_cursor`). Only for kinds that rotate (shapes, frames, text, images). Portals, connectors,
  dock strips, and simple lines stay axis-aligned. Rotation is rigid:
  textured content maps onto the node's unrotated local rect and the
  whole quad turns (`board::node_texture_vertices`), never re-fit into
  the rotated bounding box. A press on a wire-grip midpoint
  starts a connector and suppresses edge resize at that point (hit-test
  the press origin, not the live pointer). The rest of the edge is
  resize.   Selection chrome is a silhouette of each selected shape (fillet,
  ellipse, path stroke) — a faint fill plus outline, never a painted
  union bounding box. Resize hover-hit still uses the group AABB
  (corners included, same 45° cursor as a single node). Shift+click on
  an unselected node's hover-resize band adds it to the selection
  instead of starting a resize.
- **P1.node.corner-grip** (Select tool; Miro-style, **stated** 25 September
  2026): a **square** grip on every node that `supports_corners` —
  rectangle, regular polygon, line polyline, frame, placed linked-media
  card, and portal frame — so the person can set a **custom corner amount**
  by dragging (**stated** user scope). The selection-strip **Corners**
  squircle edits the same authored `Corner` field (**stated**), and it
  appears on portal strips too (user decision 25 September 2026).
  **Placement (stated):** the grip sits **on the border** at the
  treatment's tangent point — the fillet's tangent point, or the chamfer's
  cut — measured along one edge from one corner. Box hosts use the **top
  edge from the top-left corner**, moving right (Miro's documented surface
  is a slider / number field; a community report shows the rounded-rectangle
  handle but does not name the edge, so the edge is our choice). Regular
  polygons use the side from the first vertex (the top one until a vertex
  +/− turns the polygon, `polygon` D14) toward the next vertex
  clockwise; line polylines use the leaving side of the first turning
  vertex. A fillet amount is the arc **radius**, so on a polygon or
  polyline the grip's travel is `radius × tan(turn / 2)`; a chamfer amount
  is the cut along each edge. At amount 0 (and below it) the grip rests at
  the small fixed inset `FILLET_GRIP_MIN_INSET_WORLD` along that edge.
  **Drag (stated):** away from the corner increases the amount, toward it
  decreases. The amount changes continuously from its press-time value —
  never a jump on grab: travel along the edge is `clamp(start_travel +
  (projected − press_projection), 0, max)`, so the first 1 px move changes a
  box's amount by about 1 unit. The gesture starts on press, not on egui's
  drag threshold. While held, the grip is drawn at the pointer's projection
  onto the edge, clamped to `[0, max travel]` — exactly under the cursor for
  the whole drag, whatever the amount; dragging back past the corner clamps
  the amount to square and the grip to the edge start. Idle (hover, rest,
  and immediately after release) the grip is drawn — and hit-tested — at
  `max(travel, inset)`, so a release settles it by at most the inset, never
  during the drag. Chamfer and percent modes are kept (`edit_corner`).
  **Click (stated):** a press and release within
  `place_tokens::DRAG_THRESHOLD` opens an inline numeric field beside the
  grip, inside the edge, using the stringers' `selection_tools::inline_number`;
  Enter dispatches **one** journaled `board.shape.fillet` patch (clamped to
  the host's largest amount), Esc cancels without touching the selection.
  **Multi-selection (stated 26 September 2026):** every selected
  corner-capable host shows its grip; grabbing any one and dragging sets
  every selected host from the dragged amount, each clamped to its own
  largest amount, live, as **one** journal group on release; Esc restores
  them all; the click-to-type field sends every selected host (one
  `board.shape.fillet`, clamped per host). Proposals: locked hosts and hosts
  whose portal chrome hides the cast are skipped; the topmost grip wins
  where two overlap; a quick second click on a grip is not a canvas
  double-click, so it neither collapses the selection nor opens text.
  **Polyline corners (stated 26 September 2026):** a single selected line
  polyline shows a grip near **each** turning vertex instead of the one
  shared grip. Each rides its vertex's **incoming** segment, back from the
  vertex, at that corner's tangent distance (same edge-riding rule), and
  its drag or typed amount sets that corner alone, stored as a per-vertex
  override (`PathData.corner_amounts`, SVG-expressible, one geometry owner
  for board and export). The Corners value still sets every corner and
  clears the overrides. Proposals: only a single selection offers
  per-vertex grips (a multi-selection shows the shared grip, which also
  clears overrides); only the hovered or held grip lights; per-vertex
  edits never reach peers.
  **Outline (stated):** the selection outline and contents-focus highlight
  of a corner-capable node follow its authored corner (fillet or chamfer),
  including 3D viewports and portals (user, 25 September 2026). Both stroke
  `SlateApp::node_screen_outline` (`corner_outline` of the resolved corner);
  a 3D viewport's live ring is its contents-focus highlight.
  **Proposals** (implementation detail, not re-litigated per contract):
  painted half-size `FILLET_GRIP_PX` via
  `canvas_scale` (P0.9) — 1.6, **stated** 26 September 2026 as 40% of the
  former 4; the hit box keeps the former half-size `FILLET_GRIP_HIT_PX` plus
  slop and never falls under `HIT_SLOP_PX` on screen, and the grip drops
  only when that hit size is illegible (proposal: the shrink does not change
  the zoom at which the grip hides); hidden in image crop mode and when portal chrome
  suppresses the ordinary selection cast; cursor is the two-headed resize
  arrow along the edge; clamp `[0, host maximum]` (half the short side for
  boxes, half the shorter adjacent side for vertices); live preview,
  **one** journal group on release (`board.shape.fillet`);
  Esc mid-drag restores press-time radius (ActiveOperation); Shift →
  integer world units; radius readout at the pointer during drag
  (`canvas_text`, **P2.GhostFollow** — screen-constant offset, not
  multiplied by zoom). **Frame:** members are not
  clipped by the frame fillet on the board; exported slides clip deck
  contents with `overflow:hidden` on the slide rect (existing frame/slide rule).
  **Pick:** click and marquee still use the node AABB. **Portal
  `Corner::Square`:** resolves through
  `slate_doc::media::portal_frame_corner` to the same model-owned default on
  board and export; a grip drag journals an explicit radius. Owner: edge
  geometry `slate_doc::geom::corner_grip_edge` and
  `polyline_vertex_grip_edges`; vertex construction
  `slate_doc::wire::filleted_vertex_path_each` (board and export); app side
  `board_handles` + `board_transform`.
  Where its hit box overlaps a resize edge band, the visible grip wins both
  hover and press; the NW corner point itself remains the NW resize target.
- **P1.node.zorder / clipboard** PageUp/PageDown/Ctrl+B; Ctrl+C/X/V,
  Ctrl+Shift+V in place.

### P1.shape — closed shapes (rect, ellipse, frame)

- **P1.shape.style** fill + stroke; new **closed** shapes (rect, ellipse,
  regular polygon, closed path with fill, …) consume the last closed-form
  memory (`CreateStyleMemory.closed`, mirrored in `BoardLastStyle.memory`)
  when the kit recipe is inherit. A stroke-only create does not wipe the
  remembered fill.
- **P1.shape.aspect** Shift during creation locks aspect (square/circle).
- **P1.shape.vertex-style** vertex picks and per-vertex style on closed
  forms (user, 28 September 2026: "alow for vrtex selection on closed forms
  like sqares and polygons" and "for squrcle menue allow for vertex by
  vertex adjustment of forms properties"). A single selected rectangle or
  regular polygon exposes its vertices to the same picked-point set as
  P1.curve.grips: a click without travel on a vertex picks it, Shift+click
  toggles, and Direct Selection (A) picks them as anchors. The picks feed
  P1.curve.vertex-style unchanged: the tip HUD (Alt width, Ctrl color,
  Shift opacity) and the strip's stroke width, color, opacity and Corners
  amount act at the picks and blend around the closed loop. Closed
  polylines and paths are already curves and follow P1.curve.grips. The
  form keeps its kind: nothing converts a rectangle or polygon to a path,
  and a vertex never moves on its own (that would be another kind);
  resize, rotate and polygon sides stay. **Model:** the per-vertex arrays
  (`tips`, `corner_amounts`, one per vertex, rectangle order top-left,
  top-right, bottom-right, bottom-left; polygon order from
  `regular_polygon_vertices`) ride in `ShapeNode.path` with no segments,
  are `None` when every entry is empty, and serialize only when present.
  A styled form paints as its derived closed outline on the board and in
  the artifact (`slate_doc::vertex_style::closed_form_paint_shape`), so
  both interpreters draw one shape; an unstyled form paints exactly as
  before. **Proposals:** vertex hit is 7 screen px, screen-constant like
  path-edit handles; under Select only the picked vertices (filled) and
  the hovered one (hollow) paint; a quick second click on a vertex is a
  pick, not a canvas double-click; a polygon side glyph wins on its own
  disc; the corner grip and a vertex resolve to the nearer, a tie to the
  vertex; a side step carries each vertex's style to the nearest new
  vertex by angle; Delete with closed-form picks does nothing (removing
  a vertex would change the kind) and arrow keys nudge the whole node.
  **Not in scope:** per-vertex fill blended across the interior is a mesh
  gradient, which SVG cannot express (Art. IV). The conforming
  alternative, a linear or radial gradient anchored to picked vertices,
  is an open question in `rectangle.md` and `polygon.md`.

### P1.curve — open curves (line, arc, polyline, bezier span, pen, brush ink)

- **P1.curve.render** dashes follow the complete source curve between dash
  boundaries. Round caps/joins have no overlapping internal facets; tapered
  strokes retain the source's adaptive samples. Curve detail refines with zoom
  in both fills and strokes; changing view scale does not change authored data.
  Shared ownership: `vector-ink` dash/mesh/outline, with screen-error budgets and
  cached tessellation in `board_path` (also consumed by wires).

- **P1.curve.style** stroke only, no fill; stroke width/cap/dash editable
  after the fact; Ctrl+J joins endpoints.
- **P1.curve.create-style** **per tool** (stated 2026-09-25; supersedes the
  shared open-curve memory). Each drawing tool — brush, pen, line, arc,
  polyline, Bézier — remembers its **own** last stroke color and width and
  never inherits from another tool. The brush keeps its settings
  (`brush.md` D16). Each stroke tool has its own slot
  (`CreateStyleMemory::tool(StrokeTool)`): the tool's own commit and a
  **single-node** edit to a stroke it drew this session update that slot,
  stroke + opacity only. Remembered width is never 0 (minimum 2 world units).
  A tool that has not drawn yet uses `default_curve_stroke` in the theme ink,
  not the brush foreground — **Square** end caps, Miter joins (distinct from
  expressive ink's round caps). Brush/Pen kit ink keeps its own round defaults
  when pinned (`P2.StickyInk`). Workbooks saved with the shared `open` slot
  seed every tool from it once. Closed shapes keep one shared memory
  (P1.shape.style). Implementation: `board_style::BoardLastStyle`, updated from
  `patch_nodes` (single target), grip commits, and tool commits; persisted on
  `ViewState.create_style`. A curve tool's color wheel writes that tool's
  slot only and never the brush foreground (review r7, 27 September 2026:
  no user statement asks for a shared color). Vector curve tools
  (pen, line, arc, polyline, Bézier) always commit a hard vector stroke
  (`Stroke::hard_vector`): edge softness, stamp, and Gaussian blur are never
  inherited, not even from an edited brush stroke, and those tools offer no
  softness or blur control. Existing documents are not rewritten on load.
- **P1.curve.tip-chord** (stated 2026-09-25 as the width chord, widened
  2026-09-27) Alt+right-drag with pen, line, arc, polyline, or Bézier armed
  runs the Brush size HUD (`board_color::drive_brush_hud`, no copy) on that
  tool's own width: horizontal scrub, no softness, Esc restores, release
  saves to the tool's memory. The color and opacity HUDs reach the same
  tools (user, 27 September 2026). The chords take the right button from pan
  and the context menu like the brush chords. A chord released without
  travel opens numeric entry for that HUD's quantity instead, through the
  same setters and commit (user, 28 September 2026; `brush` D04, D08).
  **Mid-draw tips** (user,
  27 September 2026: "earlier versions had the ability for the user to
  dynamically change brush properties part way through the drawing process
  and for the drawing preview to update to show the interpolation between
  those two states and for the committed drawing to retain them
  parametrically"): every placed point or sample records the tool's whole
  tip — width, color, and opacity (`vertex_style::PlacedTip`). On a line,
  arc, polyline or Bézier a chord sets the tip of the point being placed
  and of the points after it; points already placed keep theirs. The draft
  preview paints the tips it will commit (`board_path::draft_stroke_ink`,
  real colors, not the accent), and commit writes them as grip tips
  (`vertex_style::set_grip_placed_tips`). The preview mesh is rebuilt only
  when the draft, its tips or the zoom change, and a live Pen stroke
  rebuilds only its last piece of 64 samples (`board_path::DraftInkCache`,
  Art. II). Equal tips commit a plain stroke.
  Curve opacity is node-level, so the node takes the most opaque tip and
  each vertex's alpha carries its share of it. When every tip was placed
  at 0 %, the node is 0 % and each vertex keeps its full alpha (user,
  27 September 2026: opacity reaches 0 % on every tool, and a 0 % stroke is
  still picked by its geometry). The tool keeps the tip it
  finished with (`board_style::keep_tool_tip`). **Easing** (user,
  26–27 September 2026: "linear tweening of curve width only on linear
  shapes. For curved, filleted, pen drawn shapes sigmoid or appropriate
  interpolation should be used"): straight by arc length on lines and
  sharp or chamfered polylines, straight along the sweep on a circular
  arc, smoothstep on Bézier spans, Pen curves and filleted polylines
  (`slate_doc::geom::tip_ease` on the path as it paints). A stamped stroke
  eases per segment: straight on a line segment, smoothstep on a curve
  (`vector_ink::tipped_contours`). **Freehand:** mid-stroke the Pen and the
  Brush stop sampling while a HUD is up, so the scrub draws nothing. When
  the HUD closes, the pointer is warped back onto the last drawn sample
  (the swatch warp's path) and the closing frame draws nothing, so the
  stroke resumes where it paused instead of jumping across the scrub
  (review r7, 27 September 2026). The samples after it blend from the last drawn tip to the new one by
  smoothstep over 24 screen px, or the wider of the two tips if that is
  longer (`board_path::FreehandTips`); the live preview draws each sample
  at its own tip. The fit splits at the blend's ends, and each fitted
  vertex takes the tip drawn at its spot, so `PathData::tips` stores one tip
  per vertex (a Brush stroke's tips are absolute stamped tips). Tips on a
  hard vector stroke are relative (`PathData::vector_widths`): the widest
  vertex paints at `Stroke::width`, so a later width edit scales the whole
  stroke. Both interpreters stroke them through `slate_doc::geom::tipped_stroke`
  and `vector_ink::stroke_mesh_tipped` / `stroke_mesh_tinted` /
  `stroke_outline_tipped`; the artifact writes the filled outline, graded
  by `linearGradient` stops whose `stop-opacity` carries each vertex's
  alpha (P1.curve.vertex-style). Curve tools never gain softness.
  Formerly P1.curve.width-chord.
- **P1.curve.grips** selected open curves expose their defining points as
  gripable handles (endpoints, on-curve anchors) — **not** a resize bbox.
  Applies to **every** selected simple line in the selection, not only when
  one line is selected; homogeneous multi-line selections skip group bbox
  handles. Direct Selection (A) additionally exposes tangent handles and
  segments. **Parametric editing** (user, 26 September 2026: "reselecting
  the element after creation should expose its control handles"): a
  **single** selected single-contour path shows its grips with the Select
  tool. A polyline, open or closed, shows every corner vertex and end
  point; a circular arc shows start, end and through point; a Bézier span
  or fitted pen stroke shows every anchor and every non-zero tangent
  handle. Lines keep their endpoint grips. Every grip is painted and picked
  by the shared path-edit overlay: only painted grips, nearest within 7
  screen px, an anchor wins a tie; a painted grip (a curve anchor, a
  handle knob, or a line end point) under the selection strip still takes
  the press, with or without a strip panel open, and a press on one is
  never a click-away; it commits the panel's pending edits at any zoom,
  and the panel closes only when the new vertex pick leaves its squircle
  off the strip. A rectangle or regular-polygon vertex
  (P1.shape.vertex-style) under a strip button leaves the button its click
  (shape-selection-toolbar D13; `board_direct::curve_knob_under`). One drag moves one point and is one
  journaled Patch; Esc mid-drag restores. A handle drag takes the Bézier
  keys (bezier-span D05 / D07): Alt moves only that handle (user pass,
  28 September 2026, pm3), Shift keeps its direction and changes only its
  length, Ctrl scales both handles of its anchor by the same ratio (user,
  28 September 2026). A dragged grip keeps its grab offset. The carried
  point snaps (user pass, 28 September 2026, pm2) through
  `resolve_point_snap` (P1.node.osnap). An arc is rebuilt through
  its three points. A filleted or chamfered polyline keeps its authored
  radius and re-applies it to the new corners, clamped per corner by the
  adjacent edges, never by the bounding box (user, 26 September 2026).
  Arcs are recognized by geometry, not tool provenance: an open path of
  cubic spans that stays on one circle within the Arc tool's fitting
  tolerance. The through grip is the middle of the sweep, so it
  re-centers after a drag (the AutoCAD arc midpoint grip; user pass,
  28 September 2026, pm6).
  Grips win over resize at a bounding-box corner; the visible fillet grip
  wins over a vertex grip. A click or press on a grip picks that point
  into a per-curve picked-point set, painted filled and never journaled,
  which later per-vertex properties will read; a handle knob picks its
  anchor. **Shift: click vs drag.** The user flagged Shift+click picking as
  conflicting with the Shift handle lock (28 September 2026, pp2:
  "conflicts with above request for shift click function"). Travel
  resolves it (proposal, awaiting the user's confirmation): a Shift click
  without travel on an anchor or handle knob adds or removes that pick
  and moves nothing; a Shift drag on a handle knob locks its direction
  and picks nothing. Two quick clicks on grips are two picks, never the canvas
  palette. Index order: path vertex order; start, through, end for an arc. **Edges**
  (user, 28 September 2026, "dig selection"): Ctrl+Shift+click on a
  segment, away from the grips, picks that edge and its two end vertices
  (Rhino sub-object selection; the group-member and locked meanings stay).
  More clicks add edges or toggle one off. A picked edge paints a line
  in the selection color that scales with the canvas (P0.9:
  `canvas_scale::px(3, z)`, dropped under one screen pixel; it is not a
  path-edit handle), flattened once per zoom bucket
  (`board_path::curve_tolerance`) and cached until the curve, the zoom
  bucket or the picked edges change (review round 8); dragging it moves every
  picked vertex as one snapped Patch; Delete removes the picked segments
  through the trim owner (`commit_open_pieces`), splitting an open curve
  and opening a closed one. With points picked the property strip shows
  only per-vertex squircles (shape-selection-toolbar D13).
  Implementation: `board_direct::curve_grip_target`,
  `board_path::arc_grip_points`, `path_edit_overlay::path_edit_hit`,
  `slate_doc::scene::Corner::vertex_effective`.
- **P1.curve.vertex-style** per-vertex stroke style (user, 26 September
  2026: "if a user selects a vertex of a polyline and then opens the stroke
  width stringer and adjusts the stroke width, adjust just that vertex's
  stroke width, creating a taper between that vertex and its adjacent
  neighbors"). With grips picked (P1.curve.grips) on a line, arc, polyline,
  Bézier span or pen stroke, the Stroke stringer's width edits only those
  vertices, and the stringer reads the first picked vertex. With no grip
  picked, the edit applies to the whole curve as before: a width edit scales
  every vertex, keeping the taper. A click off the grips clears the pick.
  Each edit is one journaled Patch through `board.shape.edit`, whose request
  carries the picked grips (`PropertyRequest::points`), so agents drive the
  same edit. Between vertices the width blends straight for sharp or
  chamfered polylines and lines, straight along the sweep for arcs, and
  with a smoothstep for Bézier spans, Pen curves and filleted polylines
  (zero slope at every vertex, so the stroke has no chines;
  P1.curve.tip-chord, 27 September 2026). A filleted polyline eases from
  polyline vertex to polyline vertex by vertex parameter, not between the
  fillet ends it paints through, and a cut on it takes that eased value
  (`split_tips_at` reads the shape's corner).
  Color follows the same model (user, 26 September 2026: "sub-select
  individual vertices to create a blended color between that vertex and its
  neighbors"): the Stroke color field and opacity rail edit the picked
  vertices and read the first of them, colors blend between vertices by the
  same rule as widths, and a Stroke-strip color edit with no grip picked
  sets every vertex. The property strip stays absolute. The tip HUD's
  whole-curve edits instead shift the curve (user, 28 September 2026, ed1:
  "i dont wat it to be a whole scale overwrite of the curves properties but
  rate a shifting of them"): with no grip picked and no point hovered, the
  HUD edits the whole selected curve under the Select tool or Direct
  Select; width scales every tip by one factor, the wheel shifts every tip
  color and the stroke color by the change from the curve's first color to
  the pick (hue rotates, saturation and value shift;
  `board_color::shift_hsv`), so a gradient stays a gradient, and opacity
  scales the node's, so each vertex keeps its share.
  A vertex's opacity is the opacity it paints at (node opacity ×
  its alpha); an edit writes it back through the share rule
  (`vertex_style::set_grip_opacity` over `placed_spans`), so the others
  keep painting as before. **Painted strokes** (review r7, 27 September
  2026): a brush stroke's tips are absolute stamped tips, one per anchor,
  and take the same edits. Picked anchors edit only their tips (softness
  too); with none picked, size and softness scale every tip in
  proportion from the tips at the edit's start
  (`vertex_style::scale_stamped_tips`), color (strip: set; HUD: shifted)
  and texture reach every tip (`vertex_style::edit_every_tip`), and the
  stroke's width stays its
  widest tip so the tile padding covers it. Uniform tips collapse onto the
  stroke. The painter (`board_path::stamped_contours`) and the artifact
  (`brush_stamp`) both read `PathData::paint_tips`.
  **Proposal:** the widths are the existing `PathData::tips`, one per
  vertex, not a second per-vertex list. The blend is read from the geometry
  (`slate_doc::geom::tip_ease`: any curve that is not a circular arc is
  smooth), like the grips (user pass, 28 September 2026, tn3, ph1). An arc
  stores a tip at every span joint, derived from its three grip values; an
  arc with an odd span count gains a joint at its through point (user
  pass, 28 September 2026, ph1). A filleted polyline keeps its vertex
  widths, and each fillet's middle takes its corner's width (user pass,
  28 September 2026, tp4). The width is C1-continuous through the fillet,
  with no kink at its middle or its tangent points (user, 28 September
  2026, tp4): a point on a fillet takes the vertex parameter where the ray
  from the fillet's center meets its half's edge, and each half is cut
  into pieces whose middle joint sits at the corner's own parameter
  (`slate_doc::wire::filleted_vertex_path_params_each`, the one owner;
  `geom::FILLETED_TIP_STEPS`). Grip drags keep the tips when the grips
  still fit (user pass, 28 September 2026, pm5). Edits that change the vertex count carry them
  (user, 26 September 2026: "per-vertex properties survive editing"):
  Trim and Split pieces keep the tips and corner overrides of the source
  vertices they keep, and a cut vertex takes the stroke's width and color
  at the cut by this blend, with no corner override
  (`slate_doc::vertex_style::split_tips_at`, the one owner); a tipped open
  curve is cut in curve parameter space and keeps its curves
  (`vertex_style::cut_curve`). A Direct
  Selection edit that drops a vertex drops its entry; an added closing
  copy of the start takes the start's tip; a nudge keeps every vertex's
  style. An object-level Join concatenates its sources' vertex styles in
  joined order (`vector_ink::join_endpoints_traced`,
  `vertex_style::vertex_styles`). A smoothing pass resamples the
  tips onto its refit vertices at the same fraction of each contour's
  length (`vertex_style::arc_length_params`). Both interpreters paint through
  `slate_doc::geom::tipped_stroke` and `vector_ink::stroke_mesh_tipped` /
  `stroke_outline_tipped`; the artifact writes the variable-width outline as
  a filled path. Colors are the tips' `color`, and a uniform stroke keeps
  one `<path>`. A stroke whose vertex colors differ exports as the
  quads between its stroke sections (`vector_ink::stroke_pieces_tinted`,
  the triangles the board mesh paints). A quad with two different end
  colors fills with a two-stop `userSpaceOnUse` `linearGradient` from one
  section center to the next, which is the board's own per-vertex color
  interpolation. Short filled segments of one color each were the
  alternative, but they would step the blend. An opaque stroke fills its
  whole outline in the mean color under the pieces, so browser
  antialiasing seams between quads do not show the background. While
  drawing, the tip chords set the point being placed (P1.curve.tip-chord).
  Drafts record the tool's tip per placed point (`BoardPathDraft::tips`,
  `LineDraft::start_tip`) and commit them as grip tips
  (`vertex_style::set_grip_placed_tips`). The draft preview paints through the
  same call (`board_path::draft_stroke_ink`).
  **End conditions** (user, 28 September 2026, tl5: "should be able to
  spesify end condition on a per vertex bass"): the row edits the whole
  curve, except the end conditions: with an end grip picked, it sets that
  end only (user, 28 September 2026, tl5). The Alt+right-drag style row
  (flat, round, arrow at the end, narrow at the start, narrow at both ends)
  restyles the whole curve when nothing is picked or only interior points
  are (es1, user pass, 28 September 2026: "Releasing on one restyles the
  whole curve, even with points picked"). With an end grip or anchor of an
  open curve picked (or hovered, as for the HUD's other edits) the row
  offers that end's choices (flat here, round here, arrow here, narrow
  here; with both ends picked, at both ends) and sets only the picked
  ends; the other end keeps its own. One release is one journaled Patch,
  and Esc before the release restores. The model is `Stroke::cap_start` /
  `cap_end` (only where the cap differs from `cap`; `set_end` never
  rewrites `cap`, so giving both ends the same cap leaves the dash caps
  alone, review round 8), `arrow_start` /
  `arrow_end`, and narrowing in `WidthProfile` (`Taper` narrows one end,
  `Ends` both), read and written through `Stroke::end` / `set_end`. The
  row's current whole-curve choice reads the effective end caps
  (`Stroke::end_caps`): two equal end caps count as the curve's end cap,
  so Round set on both ends of a Butt curve reads as Round and narrowing
  both ends reads as narrow at both ends; the reader never rewrites
  `cap` (`board_tip_hud::curve_style_of`, second review of round 8). Both
  interpreters cap the two ends through `vector_ink::stroke_mesh_ends` /
  `stroke_outline_ends` (dash ends in between keep `cap`), trim under each
  head with `slate_doc::geom::trim_arrow_ends`, and draw each head with
  `slate_doc::geom::arrow_head`; the artifact writes mixed caps as the
  filled outline. A Trim or Split piece keeps the condition of each source
  end it still owns and a cut end takes the plain base cap
  (`Stroke::keep_ends`); a Join drops the conditions at the joined ends and
  keeps the free ends'. **Open question:** an end condition on an interior
  vertex (perhaps a per-vertex join style) is not built.
  Implementation: `slate_doc::vertex_style`,
  `board_properties::Property::apply_at`, `board_tip_hud::apply_tip_choice`.
- **P1.curve.pick** click and marquee selection hit the **stroke** (via
  `vector_ink::hit_stroke` + `pick.slop` ≈ 4 screen px), never the node's
  axis-aligned rect alone. Closed unfilled paths included — each contour
  is tested on its own (no ghost segment between `ClosePath` and the next
  `MoveTo`, and no infinite-line extension past a vertex). Legacy
  `ShapeKind::Line` included. Marquee: the stroke centerline intersects
  the marquee, or a stroke hit at the marquee center. Unfilled paths do
  not grow bounding-box resize chrome. Implementation:
  `board_path::hit_shape_stroke`, `board_path::marquee_hits_node`.

### P1.wire — connector ports (spawn handles)

- **P1.wire.ports** wire spawn handles sit on **consistent object
  features**, never the world AABB of a rotated host. Area objects
  (rect, ellipse, text, image, frame, portal, dock strip)
  expose the four local-edge midpoints of `node.rect`, then rotate
  those points about the node center — so a rotated rectangle's ports
  travel with the rectangle, and an ellipse's ports are the local-axis
  extrema (which lie on the ellipse). Closed strokes (closed polyline,
  closed path) expose arclength `t = 0`, `0.5`, `1` on the path itself,
  including the closing seam — never the path's AABB. **Open shapes**
  (lines, open arcs, polylines, Bézier spans, unclosed pen strokes) offer
  no ports and take no new wire ends; wires saved earlier still resolve.
  One predicate at the port owner decides: `slate_doc::is_open_shape`.
  Connectors have no ports. New geometry declares a
  class on `slate_doc::WireHost` (oriented box, open stroke, or a
  future silhouette / vertex facet) — it does not special-case a file
  format. Geometry is derived at resolve time (Art. VI.3); the journal
  stores only `Side` + `t` + node id. Both interpreters call
  `connector_route` with that host pose (Art. IV).   Implementation:
  `crates/slate-doc/src/wire_host.rs`.
- **P1.wire.hit** The port press target is a pointer hit, not painted
  geometry, so P0.9 does not apply; painted discs still go through
  `canvas_scale::px`. At zoom ≥ 1 the inner disk is 8 screen px and the
  outward half-plane is six times that. Below zoom 1 the inner disk
  grows as `8 * zoom^-0.35`, capped at 14 screen px, and the outward
  reach stays six times the inner disk. A port whose painted radius is
  below 1.5 screen px is not drawn and is not hittable. A press inside
  any node's body does not start a wire from another node's port, unless
  that body also holds the port (a slide frame around the card). A
  press inside a multi-selection's bounding box moves that selection.
- **P1.wire.drag** While a wire is dragged from a port, the free end's
  tangent stays on the source side's axis, reversed (horizontal for
  left/right, vertical for top/bottom), matching a wire into a facing
  port. The leader drawn to the flow action menu uses that same curve.
- **P1.wire.color** A connector's color is optional (`ConnectorNode::color`).
  `None` paints `Palette::wire`, a medium gray with a light-mode and a
  dark-mode value, read from the active theme each frame, on the board and
  in the export. A picked color is `Some` and is kept, whatever it is.
  Files saved before the field read a stroke equal to the old board ink
  (light `#1b1e22` or dark `#dde2e8`) as `None`; that load migration is
  the only place the ink is compared.
- **P1.wire.rails** orthogonal wires that share a source or destination
  **fan along the port** and run on **parallel mid-span rails** (File
  Atlas nested-rail / PCB-trace treatment) so they do not stack. The
  default route is a **50/50 three-leg** (horizontal or vertical trunk
  at the midpoint). An L, a wrap, a stair, or a 45° cut is used only
  when that corridor hits a host — stairs never compete with a legal
  three-leg on cost (they share Manhattan length and will flicker).
  Collision uses each host's **oriented silhouette** (box, or the oval
  for an ellipse), not the world AABB. Sibling rails take the fan side
  their dest already sits on (no-crossover); a dead zone around the
  port centre plus connector id as the remaining tie-break stops the
  sign from flipping while a dest is dragged across. Per side the
  farthest wire exits outermost and turns nearest the port (the File
  Atlas leader rule), so converging wires arrive sorted. **User, 28
  September 2026:** "colission avoidence should be best efort. not
  absolute. alocate a zone for conection where bundeling wires can
  spred out but once that space is filled up the packing and bundeling
  simply densifeies with optimaly sorted incoming wires." Each
  connection has a bounded zone; lanes sit at the preferred spacing
  while they fit and pack evenly denser inside the same zone down to a
  floor; crossings and node bodies are avoided where a route exists,
  never by growing the zone or rerouting far away. Normal connectors
  and crosstalk wires share it; File Atlas passes its own spacing
  (no floor) and looks as before. Geometry stays derived (Art. VI.3).
  Implementation: `vector_ink::rails` (`nested_rails`, `LaneSpacing`,
  `ConnectionZone`), `slate_doc::scene_ortho_lanes`, tokens in
  `slate_doc::CONNECTION_ZONE`.

### P1.text / P1.image

Exist (edit-in-place, crop/adjust); patterns promote here when a second
text- or image-producing tool appears.

### P1.portal — portal nodes (generated / document / host)

Promoted when `portal-web-embed` joined the portal contracts. Repository
Lens and Status Board, the generated subtypes that first extended these
rows, were later removed from the product; the pattern stays for the host
portals. Older contracts may still state some of these rules inline —
rewriting an already-approved matrix cell is a worse cost than the
duplication. New portal contracts reference these and add only deviations.

- **P1.portal.frame** the frame — rect, class, kind, title, source,
  query/parameters, fill — is journaled authored data; contents come from
  elsewhere and are never journaled (Art. V.1, V.3, VI.3). Journaled acts
  are frame-only: place, move, resize, rebind, re-query, delete, bake.
- **P1.portal.place** `Armed → Dragging(rect) → Committed(unbound)`.
  Press-drag-release defines the frame; travel below `draft.drag_threshold`
  (4 px) places the subtype's `default_size` centred on the click
  (same click-place rule as **P2.DragShape**). Unmodified
  drag is free-aspect; Shift locks 16:9 (**deviates P1.shape.aspect**).
  Binding is a separate, non-modal step — no file dialog opens inside a
  draw gesture. One-shot: commit returns to Select (P0.4). Placement
  details also live in **P2.PortalPlace**.
- **P1.portal.folder-drop** Dropping a **folder** on the board opens a
  chooser, not an implicit bind. The default option is the File Atlas
  lens. Every other lens that can honestly read that folder is listed
  (Repository Lens when `.git` is present, Status Board when
  `project-state.json` is present, Web when an HTML entry file is
  present). One extra option is **not a lens**: place the folder's files
  on the board as images. Alt keeps today's ordinary drop (files become
  items; no chooser). HTML *files* still become web portals without a
  chooser. Promoted from `portal-atlas-lens` (2026-08-21) so later
  lenses append a row instead of inventing a second drop path.
- **P1.portal.bind** One `SourceUri` stored relative-first (Art. IX.2).
  Rebinding is a journaled `Patch` and discards cached contents. Generated
  portals refuse remote URLs and hosted APIs (Art. I.4); host web portals
  accept `http(s)` per their own contract.
- **P1.portal.health** source health is the tri-state `Ok` / `Unknown` /
  `Missing`, resolved without blocking any user-facing operation, and every
  unresolved or missing state **names the locator it tried** so it does not read
  as a bug (Art. IX.3).
- **P1.portal.enter** click selects the frame; double-click (or Enter on the
  selection) enters the contents when the subtype has enterable contents;
  Esc leaves. Generated instruments (Repository Lens, Status Board) have no
  contents-focus mode — double-click stays on the frame. No board tool
  reaches the contents from outside the frame. Interactive host portals
  implement this as **P1.portal.contents-focus**.
- **P1.portal.contents-focus** Selection is not contents focus. A click on
  an interactive host portal (web, agent, File Atlas, and any future inner
  surface) selects the frame; the board keeps the wheel, pan, and tool
  (P0.5). Double-click or Enter (`portal.<kind>.focus`) takes contents
  focus for that frame only. Only then do contents receive pointer and
  wheel. Esc peels contents focus without tearing contents down (P0.1).
  While a host portal has contents focus, the board suppresses that
  frame's selection cast, selection outline, dimension stringers, and
  the contents-focus highlight stroke. The frame stays selected. The
  File Atlas theme hairline and any authored stroke stay. A primary
  click **outside** the focused portal body — including a click
  on another node — also peels; that click then belongs to the board.
  Wheel and pan never reach an unfocused portal, even if the pointer is
  still over its frame after focus has been peeled. Entering one host
  portal peels any other. Maximize is a separate layer and peels first.
  Chrome (identity tab, maximize hit, border band) stays Slate's while
  focused. Implement it the same way every time: one logical
  `focused: Option<NodeId>` (do not leave web / agent / atlas / repo
  slots independently live), capture input only when focused (or
  maximized, when the inner surface *is* the window), and paint inner
  widgets with `interactive: false` until then. Do not treat "selected"
  as "the inner surface owns navigation." Cover Flow embeds use
  `HomeModel.interactive`. The File Atlas inner map is
  `atlas-shell::folder_map` — the same camera, leaders, collapse grips,
  and cards as the standalone File Atlas window. Slate must not import
  `apps/file-atlas` or instantiate `AtlasApp`. Host-kind *code* reuse
  (locators, empty CTA, bake, focus prelude, paint shell) is
  **P2.PortalHost** — do not paste `board_web.rs` to start a fourth host.
- **P1.portal.determinism** determinism is required of **generated** portals
  only (Art. V.3); Art. IV.2 governs extracted graphs. Host and document
  portals answer D28 with *provenance* — what is being shown and when it was
  obtained — and do not claim reproducibility.
- **P1.portal.style** portals paint from `Palette::portal` and the portal token
  block; they never consume `BoardLastStyle` and never become the last
  single-node edit. **Deviates P1.shape.style** — analysis and host surfaces
  stay identical between boards and between the two apps (Art. X). File Atlas
  unauthored fill follows `Palette::card` (slightly lighter than the board);
  unauthored File Atlas stroke paints a 1-unit `Palette::border_strong` hairline.
  Fill/Stroke squircles author `portal.fill` / `portal.stroke`. Other portals
  do not paint an outline unless authored; a minimalist stroke appears only
  for edge hover. Contents focus suppresses that stroke
  (P1.portal.contents-focus).
- **P1.portal.empty-ui** Generated unbound / loading / error copy shares
  one painter in `board_portal.rs`. Host unbound CTA is
  **P2.PortalHost.empty**.
- **P1.portal.pick** The frame picks on its rect, including marquee.
  Contents expose no grips and are not selectable as board nodes. Resize
  re-lays-out a generated portal; it does not scale a picture. Portals
  stay axis-aligned (no rotate chrome). Resize handles follow
  **P1.node.transform** — live on hover, no prior selection.
- **P1.portal.export** Both interpreters of a generated portal consume the
  same layout function (Art. IV). No script, no fetch on that path.
- **P1.portal.export-honesty** whatever the artifact writer emits carries a
  caption saying what it is and when it was obtained; an unbound or `Missing`
  portal exports its state card, never an empty rectangle (Art. IV.1).
- **P1.portal.bake** An explicit bake command copies authored nodes
  matching the current contents and leaves the portal live (Art. VI.3).
  Bake copies; it does not convert.
- **P1.portal.sync** Frame, source, and query sync as journal deltas.
  Contents are per-peer and never transmitted. Focus and hover are
  presence (Art. VIII.5).
- **P1.portal.agent** Placement and bind/refresh/bake commands are
  registry SPECs. Agent-issued frame/source/query mutations stage for
  acceptance (Art. VII.6). Agents never write the source.
- **P1.portal.elevated-red** Deep red (`palette.danger`) on an agent card
  means elevated privilege, never decoration (user, 27 September 2026:
  "red is for elevated privileges, a visual cue that this is not a typical
  chat train"). Same red, different places, so hue alone never carries the
  meaning: a red **label** (the model name) marks a conversation with Full
  access; red **text** with a red sender label marks a message another agent
  wrote; a red **wire** is a crosstalk. Nothing else on a chat card is red
  except the Delete card menu row. Promoted from `portal-agent-link` D32 and
  `portal-agent-crosstalk` F1; the older D32 cell keeps its inline wording.
- **P1.portal.clip** Contents paint *inside* the frame fillet. Order:
  fill (rounded rect) → contents (textured or vector, clipped to the rounded
  outline) → identity tab (when the kind has one) → stroke last. No kind
  paints the square leftover corners in the canvas fill: that mask covers a
  frame the portal overlaps, including one with its own fill. Board and export
  share the one `slate-doc` corner model (`portal_frame_corner`,
  `PORTAL_FRAME_DEFAULT_FILLET`); on the canvas, effective radius and stroke
  follow **P0.9** (authored corner × zoom). Host bodies that cannot
  polygon-clip inset their axis-aligned clip by the painted radius
  (`portal_body_content_clip`). Maximized, radius is 0. Web portal pixels are visually full-bleed to the
  frame/body outline; the invisible focused-page border hit band is
  input-only, never a bezel. A square `clip_rect` / `painter.image` of the
  AABB still **deviates** where a body has not yet been meshed to the outline.
- **P1.portal.chrome** Identity chrome is **web-only**, owned by
  `atlas-shell::tabs::portal_tab_bar_corner`: a plain bar with centered page name/URL,
  no blister. It overlays the full-bleed page and retracts after 1.2 seconds
  idle or pointer departure; interaction or its top edge reveals it. Native
  page scrollbars share that visibility, retaining transparent gutters to
  prevent reflow. Maximized bars use the Slate index top-bar height and type;
  scrollbar thickness follows that same scale. Canvas chrome follows P0.9.
  Maximize is the only bar button. Explicit folding remains a context-menu /
  `portal.chrome.toggle` action, recovered from the top-interior reveal strip.
  Other portal kinds have no identity bar.
- **P1.portal.maximize** Every portal carries the four-corner maximize
  square (`atlas-shell::tabs::paint_maximize_glyph`), the same graphic
  as the Slate / File Atlas caption control. On a web portal it sits in
  the tab bar's window-control slot (far right). On every other portal
  — and on a folded web tab — it floats in the node's upper-right.
  Clicking it (`portal.maximize`) makes that portal the sole interface:
  it fills the window at the screen's aspect, Slate chrome is not
  registered, and a web portal keeps its tab at the top of the screen.
  The node rect is not mutated (derived view-state, Art. VI.3). Esc
  peels maximize first (P0.1) and leaves contents focus intact. Restore
  returns the portal to its authored frame.
- **P1.portal.local-ui** Source-specific controls live on that portal —
  Selection inspector, portal chrome, or a Set portal command — never on
  Document Settings or any other board-wide panel.   Document Settings is
  canvas-scoped (grid visibility, board object snaps, …). A sophisticated
  host (Rhino view, web embed, repo lens query, …) that needs a unique
  interface adds it to its own contract and inspector. Board-wide chrome
  must not grow a row per portal kind or node kind. Exception only when
  the user explicitly states a control is canvas-wide. New portal
  contracts answer **D35**.

### P1.dock-strip — canvas-embedded dock copies

A `NodeKind::DockStrip` is a journaled poster of a palette. Chrome lives
in `atlas-shell::dock`; the node lives in `slate-doc`; paint / hit /
gestures live in `apps/slate/src/app/board_dock_embed.rs`. File Atlas
has no board, so it ignores Drop to canvas.

- **P1.dock-strip.frame** the rect, `palette_id`, and `visible` tool-id
  snapshot are journaled; contents are derived from the dock recipe plus
  that snapshot (Art. VI.3). Copies never pin, unpin, or hide the
  baseline dock. Unlimited placements.
- **P1.dock-strip.arm** a click (no drag) on an icon **arms** the same
  command the baseline dock would — create tools do not place on that
  click. Instant actions (join, grid, snaps, color swap) still fire.
  A click on padding / border selects the node.
- **P1.dock-strip.move** click-hold-drag anywhere on the node, including
  an icon, moves the whole strip. Edge resize still wins
  (`P1.node.transform`). Create tools stay armed; they do not start a
  draw from the toolbar.
- **P1.dock-strip.chrome** docked flyouts and canvas copies share one
  painter: `atlas_shell::dock::measure_icon_strip` +
  `paint_icon_strip_card` / the flyout strip. Fieldset groups of
  secondary circular icons; tertiary toggles stack two-high on that
  icon datum. No second card around a canvas copy — it is the same
  strip the dock paints. Window-chrome dots (Minimize, Drop) stay on
  the docked popover only. Canvas copies are a poster:
  measure the docked intrinsic size, then **contain-scale** into the
  node rect — extra bounds are margin, never a reflow (P0.9).   When the pinned band no longer fits the canvas, icons wrap into
  extra rows **inside** each palette — groups stay in one row.
  Wrap is an accordion: fill a sideways row, then step overflow up
  (or out on a left dock). Every category peels one column in the
  same round so a neighbor cannot overlap while another is still a
  single row. Pallets sit on the category rule (the basedatum).
  Never hex-stagger, never fair-share every box to a one-icon tower.
  Stacked captions on the dock use a horizontal ellipsis; the strip
  uses a vertical two-dot column. Hover labels appear in one place,
  centered above the dots.
- **P1.dock-strip.select** selection and hover rings use
  `node_screen_outline` → `rounded_rect_outline` of the painted card
  at `icon_strip_card_radius` (the same fillet paint uses) and
  `board_preview.select_line_weight` / `hover_line_weight`. No AABB
  box. Axis-aligned: no rotate chrome.

## L2 — Tool-family archetypes

### P2.RhinoDraft — precision draft tools (line, arc, polyline, bezier span)

State machine: `Armed → Placing(point k) → … → Commit`.

- **P2.RhinoDraft.gesture** both grammars commit identically:
  click-move-click **and** press-drag-release. Disambiguation: movement
  beyond `draft.drag_threshold` px before release = drag grammar; release
  within threshold = click grammar.
- **P2.RhinoDraft.rubber** live rubber-band preview from the last placed
  point to the (constraint-resolved) cursor.
- **P2.RhinoDraft.ortho** held Shift inverts the F8 ortho state for the
  pending segment (45° steps from the last placed point, board convention).
  Every drawn segment takes it: Line, Polyline, Arc, Bézier span, and the
  Brush / Eraser Shift line (stated 2026-09-27), and the Pen Shift line
  (user, 28 September 2026: "shift for strate line", "at 45 dgree
  intervals"; pen D07). Where Shift already means
  something else (rectangle and ellipse square / circle) that meaning holds.
- **P2.RhinoDraft.tab** Tab locks the pending segment's *direction* from
  the last placed point toward the pointer; movement then only changes
  length; Tab again unlocks; placing the point, Esc, or a tool change ends
  the lock. Point snaps still apply, projected onto the locked ray, so the
  preview and the commit read one resolved point. One owner:
  `SlateApp::toggle_segment_lock` / `resolve_segment_point`
  (`board_osnap.rs`); the tool's pending origin comes from
  `pending_segment_origin`. The board keeps the Tab key, so egui focus
  navigation does not steal it (stated 2026-09-27).
- **P2.RhinoDraft.numeric** after the first point, typed digits build a
  length readout; Enter (or the committing click) places the next point at
  that distance along the current direction. Backspace edits; Esc clears the
  entry before it cancels anything else.
- **P2.RhinoDraft.esc** Esc backs out one placed point per press; with no
  points placed it disarms to Select (P0.1 layering). Deviation: the
  Bézier span commits on Esc with two or more anchors, and backs out
  anchors with Ctrl+Z instead (`bezier-span.md` D12).
- **P2.RhinoDraft.oneshot** commit returns to Select; Space/Enter re-arms
  (P0.4).

### P2.GhostFollow — armed create-tool chrome

The confirmation that a create command is live after menu / palette / dock /
hotkey arming, before the first press. Placement itself stays with
P2.DragShape, P2.PortalPlace, or P2.PlaceOnce.

- **P2.GhostFollow.cursor** while armed and the pointer is over the board,
  hide the OS cursor and paint a pointer in `place.cursor_tint`
  (`palette.accent`). Same mechanism as the rotate cursor
  (`CursorIcon::None` + glyph). Every armed drawing tool shows a crosshair
  or a tip circle instead (stated 2026-09-25): Line, Arc, Polyline, Bézier,
  Polygon, Rectangle, and Ellipse show the OS crosshair (Rectangle and
  Ellipse keep the silhouette); Brush / Eraser / Smooth / Pen hide the OS
  cursor under a circle sized to the tip; Select / Pan are unchanged.
  `board_place::armed_cursor` is the one table.
- **P2.GhostFollow.glyph** a small **screen-space** silhouette of the armed
  result follows the pointer: size `place.ghost_size` (22 px), offset
  `place.ghost_offset` (14, 14) from the hotspot, alpha `place.ghost_alpha`
  (0.55). Glyphs: rounded rect (Frame / Rect), ellipse (Ellipse), regular
  polygon at the default side count (Polygon), portal frame (rounded rect +
  title bar in `Palette::portal`), text box, sticky. DragScale tools whose
  result is not a box preview their real outline, not the bounding box.
  Not the default world size. Named exception to **P0.9**:
  pointer-attached chrome; no zoom coupling.
- **P2.GhostFollow.drag** on press, the silhouette is replaced by the live
  rubber-band (constraint-resolved). The tinted pointer stays. PlacePoint
  tools have no rubber-band: the glyph remains until click-place.
- **P2.GhostFollow.place** DragScale rects — preview and commit — go through
  one `board_place::place_rect` / `PlaceConstraint` table, not per-tool
  copies. Rect / Ellipse: Shift → square (P1.shape.aspect). Frame: default
  = preset aspect, Shift → square. Portals: default = free, Shift → 16:9
  (P2.PortalPlace.aspect). The glyph stays screen-space (P0.9); the
  **point** it is attached to is the snapped world point (osnap +
  forcefield). DragScale snaps the live rect's moving edges
  (`resolve_draw_rect`) so the second corner is not a naked cursor.
- **P2.GhostFollow.tokens** feel constants live in
  `board_place::place_tokens` (P0.6).

### P2.DragShape — area tools (rect, ellipse, frame)

- press-drag-release sizes the node; travel under `draft.drag_threshold`
  (4 **screen** px from the press) places the tool's `default_size`
  centred on the click (rect / ellipse from the kit recipe; frame from
  the live preset). The live path starts on press — an egui click is
  not a drag, so `drag_started` never sees ClickPlace. A drag that
  stays under `MIN_DRAW` still discards. Shift = aspect (P1.shape.aspect).
  Rectangle and ellipse: Ctrl draws from the press point as the center
  (Shift still squares). Other area tools ignore Ctrl. Commit returns
  to Select.

### P2.PortalPlace — area placement for portal frames

The placement grammar every portal subtype has arrived at, promoted from
`portal-agent-link`, `portal-atlas-lens`, and `portal-web-embed`.

- **P2.PortalPlace.gesture** `Armed → Dragging(rect) → Committed(unbound)`:
  press-drag-release defines the frame and the release commits an **unbound**
  portal painting its own empty state. Binding never happens inside a draw
  gesture — no file dialog, no network fetch.
- **P2.PortalPlace.click** travel under `draft.drag_threshold` (4 px) places
  `<portal>.default_size` (960×540 world units) centred on the click.
  Same click-place rule as **P2.DragShape**.
- **P2.PortalPlace.aspect** held Shift locks 16:9; unmodified drags are
  free-aspect. **Deviates P1.shape.aspect** (square).
- **P2.PortalPlace.snap** grid snap and smart guides apply to the frame rect
  (P1.node.move); contents never snap. No direction lock, no numeric entry.
- **P2.PortalPlace.oneshot** commit returns to Select; Space/Enter re-arms
  (P0.4).

### P2.PortalHost — shared host-portal mechanism

Interaction is **P1.portal** (especially contents-focus, bind, export).
This pattern is the *code* rule so a fourth host is not a paste of
`board_web.rs` (Constitution Art. XII). Applies to web, agent, File Atlas,
and any later inner surface. Generated portals stay on `board_portal.rs`
and **P1.portal.empty-ui**.

- **P2.PortalHost.locators** One `resolve_source` / `source_locator` pair
  (Art. IX.2). No per-kind copies (`resolve_web_source`).
- **P2.PortalHost.empty** Unbound CTA (prompt + Browse) is one helper in
  `board_portal_chrome`. Sizes go through `canvas_scale::px` (P0.9). A
  `.max(n)` / `.clamp` floor on that path is a defect.
- **P2.PortalHost.bake** Write PNG beside the workbook and journal Image +
  provenance through one helper. Kind supplies pixels and the note string;
  it does not fork `write_*_poster_png`.
- **P2.PortalHost.focus** One logical `focused: Option<NodeId>` (see
  P1.portal.contents-focus). Entering a host peels every other host
  through one function — not `web_blur(); agent_blur(); atlas_blur();`
  inlined in each kind. Use `peel_contents_focus_if_clicked_outside`.
- **P2.PortalHost.shell** Paint sequence is shared: layout → corner-shaped
  fill → clipped body hook → identity chrome → corner-shaped stroke.
  Kind-specific work is the body hook and a palette border, not a copied
  wrapper. Call `paint_portal_shell_finish`; no canvas-colour corner mask exists.
- **P2.PortalHost.hit** `border_hit_px` comes only from
  `portal_frame_tokens()` (P0.6). Local `BORDER_HIT_PX` constants are
  forbidden.
- **P2.PortalHost.tree** A folder map inside a portal is
  `atlas-shell::folder_map`. Scan/watcher session logic belongs in a
  shared session type, not a second copy of File Atlas's pump.

### P2.StickyInk — expressive stroke tools (brush, eraser)

- sticky (stays armed until Esc/tool change); every stroke commits its own
  undo step; `[`/`]` step width (Photoshop tiers); Alt+right-drag scrubs that
  width, and Brush also scrubs softness on the vertical axis; width-circle
  cursor; brush spring-loads the eyedropper on Alt+left.

### P2.RhinoJoin — selection join / region union

- **P2.RhinoJoin.oneshot** preselect operands, run Join, done. One journal
  group. First selected operand's style wins.
- **P2.RhinoJoin.open** open+open (no closed in the set) joins nearest
  endpoints; one open closes (merge within snap, else a seam).
- **P2.RhinoJoin.region** any closed operand switches to boolean union
  of *connected* regions (union is one piece). Open operands in that
  set become stroke-weight ribbons, then union. Disjoint operands stay
  put — Join does not invent a compound of islands (Rhino). Group
  (Ctrl+G) is the grouping command.
- **P2.RhinoJoin.skip** frames, portals, text, images, connectors, locked,
  and hidden are never operands.

### P2.RhinoTrim — pick cutters, click the dying piece

- **P2.RhinoTrim.phases** `PickCutters → TrimParts`. Preselect on arm skips
  to TrimParts; those objects are cutters and may trim each other.
- **P2.RhinoTrim.click** each TrimParts click deletes one span (open) or
  one arrangement face (closed) and journals one undo group.
- **P2.RhinoTrim.extend** Shift+click near an open end extends that end to
  the cutter. Line cutters are infinite (`ExtendCuttingLines` on).
- **P2.RhinoTrim.cutters** an open cutter divides a closed target along its
  whole path; a closed cutter divides an open target (user, 28 September
  2026). Only two-point line cutters extend; a longer open cutter cuts
  where it runs. A cut closed target stays a closed, filled region.
- **P2.RhinoTrim.esc** Esc peels TrimParts → PickCutters → Select.
- **P2.RhinoTrim.enter** Enter advances PickCutters → TrimParts, or exits
  TrimParts to Select. Already-committed clicks stay.

### P2.RhinoSplit — pick cutters, click to keep every piece

Same phase machine, Esc, Enter, infinite line cutters, and preselect as
**P2.RhinoTrim**. The click is the opposite of Trim:

- **P2.RhinoSplit.keep** the hit object is replaced by every arrangement
  piece (open spans or closed faces). One journal group. Nothing is
  deleted. A click that would not divide is a no-op.
- **P2.RhinoSplit.targets** shapes only — text/images use Trim's clip, not
  a multi-node split. Frames and portals never pick.
- **P2.RhinoSplit.extend** no Shift+extend (Trim-only).

### P2.PlaceOnce — click-to-place (text, sticky note)

- click places and enters edit-in-place; Esc/blur commits text. A sticky
  note is one-shot: the place ghost leaves and Select returns. Tab while
  editing still spawns the next note.

## L3 — Tool-specific

Only in `contracts/<tool>.md`. If you're about to write the same L3 rule in
a second contract — stop and promote it.


### P1.shape.properties — selection properties and dimensions

Scene capabilities select one shared squircle strip above the selection: Stroke for shapes, images, wires, text boxes and sticky notes, slide frames, and portals; Fill for closed shapes, text sticky-note backgrounds, frames and portals; Corners for rectangles, images, and slide frames; photo filters for images (not 3D model viewports); Viewport display and Measure for a single 3D model viewport (media D13); routing/weight/dash/arrows for wire-only selections. A File Atlas portal adds a Formatting squircle (search, type radios, Ghost/Hide, Zoom to matches, Zoom to fit) owned by `selection_tools::atlas_format_editor`. Frame-only actions (deck order, present) join that same strip. Images nest by drag and drop. Mixed selections expose common controls. Width/height/length belong to separate exterior dimension stringers. Rectangle axes follow rotation; circles use diameter; closed paths use tight local bounds. Open curves (line, arc, polyline, Bézier, pen) and brush strokes show no dimension stringers (user, 26 September 2026), so a selection made only of them shows none. Proposal: a mixed selection that includes a closed shape keeps its union W/H. Groups without wires use union XY dimensions and uniform centroid scaling. Wire-containing selections omit box-dimension edits because attached endpoints follow their hosts. Portal source UI stays on the portal.

The strip icons expand on selection and collapse when it clears. Palette edits are transient previews until icon change or outside click commits one journal group. A press on empty canvas also deselects. Esc, tool changes, and target changes discard pending previews. Numeric dimensions commit on Enter/focus loss, scale about the measured center, preserve stroke width, and reject invalid values. Locked/read-only selections cannot be mutated. Chrome takes precedence over canvas gestures and follows P0.9. Popups draw above all canvas chrome (`selection_tools::POPUP_ORDER`). The fillet capsule is 30% taller than the 17-unit wire capsule; the photo-filter capsule is twice that fillet height. None clears the adjustment; the other radios are low-resolution filtered thumbnails, and the intensity track is as thick as the radio radius. A chips-only variant (`FilterCapsuleStyle::ChipsOnly`) keeps the same circle chips without the intensity track; width follows the chip count. Heights live in `selection_tools`.

Opening an adjustment icon fades selection tint, outlines and endpoint grips out while keeping the objects selected and property controls/stringers visible. Named exception: the Text editor on text nodes keeps the outline (text D14). Switching editors keeps the decoration hidden; closing or cancelling restores it. Hover decoration yields too, so it cannot obscure the authored color or stroke. The shared dynamic-panel style guide owns the transition; this is presentation state, never a document-opacity mutation.

RGB and opacity are independent edits. One shared desktop sampler serves every eyedropper; a result is tied to the captured workbook, selection and property and preserves alpha. Percentage corners retain relative intent; absolute corners retain authored distance with an effective clamp. The model owns both modes; board and export interpret them identically. See specs/shape-property-editing.md and specs/desktop-color-sampling.md.

[Dynamic object-property panels](../../../crates/atlas-shell/DYNAMIC_PANELS.md) owns the approved visual composition, inline RGB treatment, transient cursor metrics, short-stringer layout, and light/dark styling. Implement through `atlas-shell::selection_tools`; the whole assembly follows P0.9. Recent colors are bounded, deduplicated ViewState usage metadata, updated at successful journal boundaries; previews/cancel and geometry-only commits do not add colors. History is seeded once for legacy files, persists per document and is not authored scene data or part of undo/export.
