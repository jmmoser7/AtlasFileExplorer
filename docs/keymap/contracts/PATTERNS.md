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
  (`atlas-commands::CancelLayer`.)
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
  tools rail, readouts, Advanced) and pointer-attached chrome such as
  **P2.GhostFollow**. A tab on a portal frame is a canvas object, not
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

## L1 — Object-class

### P1.node — every board node

- **P1.node.flags** lock / hide / group semantics (Ctrl+L/H/G family);
  locked nodes still feed smart guides.
- **P1.node.select** click select, Shift+click add/toggle, marquee;
  group click selects the group, Ctrl+Shift+click a member.
- **P1.node.move** drag with smart guides; ortho (F8, Shift inverts),
  grid snap (F9), and the persistent object-snap set (**P1.node.osnap**)
  apply; arrows nudge.
- **P1.node.osnap** point picks (draft, grips, place) and bbox moves consult
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
  center. A rotated resize pins the opposite handle in world space so the
  grabbed edge is the one that moves (local AABB math alone walks the far
  edge once rotation is about the live center; most visible at 180°).
  Rotate is a 90° arc cursor just *outside* a corner, only for
  kinds that rotate (shapes, frames, text, images). Portals, connectors,
  and simple lines stay axis-aligned. A press on a wire-grip midpoint
  starts a connector and suppresses edge resize at that point (hit-test
  the press origin, not the live pointer). The rest of the edge is
  resize. Selection chrome is a silhouette outline that follows the
  painted geometry (fillet, ellipse, AABB) — no corner or midspan
  squares on a single node or a 2+ group box. Resize hover-hit on that
  outline includes the corners (same 45° cursor as a single node).
  Ctrl+Alt+Shift on a group
  grip remaps member positions through the box scale and leaves each
  member's size unchanged.
- **P1.node.zorder / clipboard** PageUp/PageDown/Ctrl+B; Ctrl+C/X/V,
  Ctrl+Shift+V in place.

### P1.shape — closed shapes (rect, ellipse, frame)

- **P1.shape.style** fill + stroke; new shapes consume the current style
  defaults; color applies via fg/bg state and inspector.
- **P1.shape.aspect** Shift during creation locks aspect (square/circle).

### P1.curve — open curves (line, arc, polyline, bezier span, pen, brush ink)

- **P1.curve.style** stroke only, no fill; stroke width/cap/dash editable
  after the fact; Ctrl+J joins endpoints.
- **P1.curve.create-style** the last **single-node** edit (inspector patch,
  grip edit, or prior create) seeds stroke + opacity on the next compatible
  create (draft curves: Line, arc, polyline, …). When nothing was edited yet,
  draft curves use `default_curve_stroke` at the current fg color — **Square**
  end caps, Miter joins (distinct from expressive ink's round caps). Brush/Pen
  ink keeps its own round defaults (`P2.StickyInk`). Implementation:
  `board_style::BoardLastStyle`, updated from `patch_nodes` (single target) and
  grip commits.
- **P1.curve.grips** selected open curves expose their defining points as
  gripable handles (endpoints, on-curve anchors) — **not** a resize bbox.
  Applies to **every** selected simple line in the selection, not only when
  one line is selected; homogeneous multi-line selections skip group bbox
  handles. Direct Selection (A) additionally exposes tangent handles and
  segments.
- **P1.curve.pick** click and marquee selection hit the **stroke** (via
  `vector_ink::hit_stroke` + `pick.slop` ≈ 4 screen px), never the node's
  axis-aligned rect alone. Legacy `ShapeKind::Line` included. Marquee: the
  stroke centerline intersects the marquee, or a stroke hit at the marquee
  center. Implementation: `board_path::hit_shape_stroke`,
  `board_path::marquee_hits_node`.

### P1.text / P1.image

Exist (edit-in-place, crop/adjust); patterns promote here when a second
text- or image-producing tool appears.

### P1.portal — portal nodes (generated / document / host)

Promoted when `portal-web-embed` became the third portal contract, then
extended when `portal-status-board` became the second generated subtype
(2026-08-16). Older contracts may still state some of these rules inline —
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
  an interactive host portal (web, agent, and any future inner surface)
  selects the frame; the board keeps the wheel, pan, and tool (P0.5).
  Double-click or Enter (`portal.<kind>.focus`) takes contents focus for
  that frame only. Only then do contents receive pointer and wheel. Esc
  peels contents focus without tearing contents down (P0.1). Maximize is
  a separate layer and peels first. Chrome (identity tab, maximize hit,
  border band) stays Slate's while focused. Implement it the same way
  every time: a derived `focused: Option<NodeId>`, capture input only
  when focused (or maximized, when the inner surface *is* the window),
  and paint inner widgets with `interactive: false` until then. Do not
  treat "selected" as "the inner surface owns navigation." Cover Flow
  embeds use `HomeModel.interactive`.
- **P1.portal.determinism** determinism is required of **generated** portals
  only (Art. V.3); Art. IV.2 governs extracted graphs. Host and document
  portals answer D28 with *provenance* — what is being shown and when it was
  obtained — and do not claim reproducibility.
- **P1.portal.style** portals paint from `Palette::portal` and the portal token
  block; they never consume `BoardLastStyle` and never become the last
  single-node edit. **Deviates P1.shape.style** — analysis and host surfaces
  stay identical between boards and between the two apps (Art. X).
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
- **P1.portal.clip** Contents paint *inside* the frame fillet. Order:
  fill → contents (textured or vector, clipped to the rounded outline) →
  identity tab (when the kind has one) → stroke last, so the page cannot
  oversail the corners. On the canvas, radius and stroke follow **P0.9**
  (`portal_frame.corner_radius × zoom`). Maximized, radius is 0.
  **Deviates** a square `clip_rect` / `painter.image` of the AABB.
- **P1.portal.chrome** Identity tab is **web-only**. Painted by
  `atlas-shell::tabs::portal_tab_bar` (the workbook tab language: one
  active tab, no `+`). The tab shows the locator. The strip is slimmer
  than the Slate / File Atlas top bar (`portal_frame.tab_height_scale`,
  40%). Maximize is the only chrome button on that strip — hover
  brightens the glyph, not a fill. Folding is a context-menu / command
  action (`portal.chrome.toggle`), not a second toolbar icon; a folded
  tab is recovered from a reveal strip on the top interior
  (`portal_reveal_hint`). Right-click on the tab or the frame opens
  portal-specific actions (plus Maximize / Hide tab). On the canvas the
  tab is a node-local object (**P0.9**). Maximized, it is window chrome
  and may stay screen-sized. This is Slate chrome, not browser chrome —
  D15's cut of a browser tab strip still holds. Other portal kinds have
  no tab.
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
  canvas-scoped (grid visibility, board object snaps, wire routing, …). A sophisticated
  host (Rhino view, web embed, repo lens query, …) that needs a unique
  interface adds it to its own contract and inspector. Board-wide chrome
  must not grow a row per portal kind or node kind. Exception only when
  the user explicitly states a control is canvas-wide. New portal
  contracts answer **D35**.

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
  pending segment (45° steps, board convention).
- **P2.RhinoDraft.tab** Tab locks the pending segment's *direction* at its
  current angle; movement then only changes length; Tab again unlocks.
- **P2.RhinoDraft.numeric** after the first point, typed digits build a
  length readout; Enter (or the committing click) places the next point at
  that distance along the current direction. Backspace edits; Esc clears the
  entry before it cancels anything else.
- **P2.RhinoDraft.esc** Esc backs out one placed point per press; with no
  points placed it disarms to Select (P0.1 layering).
- **P2.RhinoDraft.oneshot** commit returns to Select; Space/Enter re-arms
  (P0.4).

### P2.GhostFollow — armed create-tool chrome

The confirmation that a create command is live after menu / palette / dock /
hotkey arming, before the first press. Placement itself stays with
P2.DragShape, P2.PortalPlace, or P2.PlaceOnce.

- **P2.GhostFollow.cursor** while armed and the pointer is over the board,
  hide the OS cursor and paint a pointer in `place.cursor_tint`
  (`palette.accent`). Same paint path as the rotate cursor
  (`CursorIcon::None` + glyph). Line keeps its crosshair; Brush / Eraser
  keep the width circle; Select / Pan are unchanged.
- **P2.GhostFollow.glyph** a small **screen-space** silhouette of the armed
  result follows the pointer: size `place.ghost_size` (22 px), offset
  `place.ghost_offset` (14, 14) from the hotspot, alpha `place.ghost_alpha`
  (0.55). Glyphs: rounded rect (Frame / Rect), ellipse (Ellipse), portal
  frame (rounded rect + title bar in `Palette::portal`), text box, sticky.
  Not the default world size. Named exception to **P0.9**:
  pointer-attached chrome; no zoom coupling.
- **P2.GhostFollow.drag** on press, the silhouette is replaced by the live
  rubber-band (constraint-resolved). The tinted pointer stays. PlacePoint
  tools have no rubber-band: the glyph remains until click-place.
- **P2.GhostFollow.place** DragScale rects — preview and commit — go through
  one `board_place::place_rect` / `PlaceConstraint` table, not per-tool
  copies. Rect / Ellipse: Shift → square (P1.shape.aspect). Frame: default
  = preset aspect, Shift → square. Portals: default = free, Shift → 16:9
  (P2.PortalPlace.aspect).
- **P2.GhostFollow.tokens** feel constants live in
  `board_place::place_tokens` (P0.6).

### P2.DragShape — area tools (rect, ellipse, frame)

- press-drag-release sizes the node; travel under `draft.drag_threshold`
  (4 px) places the tool's `default_size` centred on the click (rect /
  ellipse from the kit recipe; frame from the live preset). A drag that
  stays under `MIN_DRAW` still discards. Shift = aspect (P1.shape.aspect).
  Commit returns to Select.

### P2.PortalPlace — area placement for portal frames

The placement grammar every portal subtype has arrived at, promoted from
`portal-lens-repository`, `portal-agent-link`, and `portal-web-embed`.

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

### P2.StickyInk — expressive stroke tools (brush, eraser)

- sticky (stays armed until Esc/tool change); every stroke commits its own
  undo step; `[`/`]` step width (Photoshop tiers); width-circle cursor;
  brush spring-loads eyedropper on Alt.

### P2.RhinoJoin — selection join / region union

- **P2.RhinoJoin.oneshot** preselect operands, run Join, done. One journal
  group. First selected operand's style wins.
- **P2.RhinoJoin.open** open+open (no closed in the set) joins nearest
  endpoints; one open closes (merge within snap, else a seam).
- **P2.RhinoJoin.region** any closed operand switches to boolean union.
  Open operands in that set become stroke-weight ribbons, then union.
- **P2.RhinoJoin.skip** frames, portals, text, images, connectors, locked,
  and hidden are never operands.

### P2.RhinoTrim — pick cutters, click the dying piece

- **P2.RhinoTrim.phases** `PickCutters → TrimParts`. Preselect on arm skips
  to TrimParts; those objects are cutters and may trim each other.
- **P2.RhinoTrim.click** each TrimParts click deletes one span (open) or
  one arrangement face (closed) and journals one undo group.
- **P2.RhinoTrim.extend** Shift+click near an open end extends that end to
  the cutter. Line cutters are infinite (`ExtendCuttingLines` on).
- **P2.RhinoTrim.esc** Esc peels TrimParts → PickCutters → Select.
- **P2.RhinoTrim.enter** Enter advances PickCutters → TrimParts, or exits
  TrimParts to Select. Already-committed clicks stay.

### P2.PlaceOnce — click-to-place (text, sticky note)

- click places and enters edit-in-place; Esc/blur commits text; sticky note
  chains the next placement on Tab.

## L3 — Tool-specific

Only in `contracts/<tool>.md`. If you're about to write the same L3 rule in
a second contract — stop and promote it.
