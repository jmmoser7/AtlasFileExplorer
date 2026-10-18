# Slate interaction inventory

Baseline for a shared interface language across infinite canvas applications.

Snapshot date: 14 October 2026. Platform: Windows, mouse and keyboard. Scope: the current Slate working tree, including local changes present on this date. This is a descriptive inventory, not a change to Slate's existing interaction contracts.

Slate's present workspace is the Board. Home, presentation playback, embedded portals, and specialized content editors surround that board. Grid, Venn, standalone Lens, Repository Lens portals, and Status Board portals are retired; older references to them are not current interface requirements.

Read this file for behavior and categories. The companion [registered command index](02-registered-command-index.md) preserves every command description in the current registry, including specialized actions and compatibility entries. That index is a generated snapshot; the application's command registry remains the owner of command identities and bindings.

## 1 Vocabulary and interaction states

- **Canvas or board:** the unbounded world containing authored objects. The camera determines which part of the world is visible.
- **Object or node:** a frame, shape, path, text, linked media card, connector, portal, or embedded toolbar.
- **Selection:** the current set of objects being acted on. A selected object can expose controls that an ordinary hovered object does not.
- **Hover:** the pointer is over a target without pressing. Hover can reveal an outline, a grip, a tooltip, a media preview, or a source-specific control.
- **Armed tool:** the mode that will interpret the next canvas gesture. Arming and creating are separate actions.
- **Draft:** an unfinished creation, such as a line's first point or a polyline's placed vertices.
- **Active operation:** a manipulation in progress, such as dragging, scaling, rotating, cropping, or editing a tangent handle.
- **Grip:** a control with a specific action. A resize region, corner-radius square, path anchor, tangent knob, and wire port are different grips even when they overlap visually.
- **Contents focus:** input has entered a portal or content editor. The surrounding object's position on the board is separate from the content's own navigation or editing.
- **Transient state:** selection, hover, camera navigation, drafts, and focus. **Authored state:** objects, their properties, relationships, and saved content changes.
- **Commit:** finish an authored change. Ordinary board edits enter the undo journal. A continuous gesture or property scrub normally becomes one undo step.

Notation: primary means left mouse button; secondary means right mouse button; middle means the wheel button. A click is a press and release without sufficient travel to become a drag. Modifiers are held during the relevant gesture unless explicitly described as toggles. Screen pixels measure pointer tolerance; world units measure authored geometry.

## 2 Input ownership and precedence

The same physical input can mean different things according to the target, active tool, editing state, and held modifiers. A shared interface must preserve those conditions alongside the gesture.

- Window menus, dialogs, popups, and focused fields claim their own input. Hovering a scrollable menu gives it wheel ownership, so the canvas beneath it does not zoom or pan.
- Typing in a text field, inline text editor, spreadsheet editor, web page, or agent composer suppresses conflicting bare-letter tool shortcuts and canvas command entry. A page receiving input also owns ordinary digits, Tab, and arrows. Slate still reserves some command chords and Escape; ownership is not a blanket claim to every key combination.
- An explicit editing grip takes priority over a nearby general resize region or connector port. For example, Ctrl-drag on a Bézier tangent knob edits handles; it does not resize from the center or redraw a wire.
- A content interaction can take priority over moving its outer object: a live model navigates, a focused spreadsheet scrolls, and a live web page handles pointer input inside its body.
- A plain right-button click opens a context menu only when the gesture has not become a right-drag or a tool-specific right-button HUD gesture.
- Armed tools, incomplete drafts, crop sessions, and specialized editors can consume Enter or Tab before global repeat or object cycling sees them.
- Commands have availability conditions such as a workbook, a board, a selection, a compatible content kind, or an unlocked object. A shortcut's existence does not make it valid in every context.

Sources: [command dispatch](../../apps/slate/src/app/dispatch.rs), [cancellation](../../apps/slate/src/app/dispatch/cancel.rs), [universal patterns](../keymap/contracts/PATTERNS.md).

## 3 Camera navigation and infinite zoom

### 3.1 Pan

- Right-drag pans the board. Middle-drag and Space plus primary-drag are temporary pan gestures. The Hand tool provides primary-drag panning as a persistent mode.
- The combined Select and Pan dock button arms that mode; pressing it while active toggles Select and Pan. The corresponding bare-letter shortcuts use Slate's delayed letter handling described in section 20.
- Shift plus wheel pans horizontally instead of zooming. With nothing selected, arrow keys pan by 60 screen pixels, or 240 with Shift. With a selection, the same arrows nudge the selected objects.
- Ctrl plus right-drag is **turbo pan** when no tool-specific HUD owns it: pulling away from the press point controls speed, returning to that point stops movement, and the dominant horizontal or vertical axis locks the direction. Engagement starts after four screen pixels and axis locking after six. An engaged pan suppresses the release context menu.
- During a compatible draft, temporary pan and wheel zoom preserve the draft so the user can reach a distant point without abandoning the operation.
- Dragging content and dragging background are distinct. A background press can start a selection sweep in Select mode; it is not an unconditional primary-button pan.

### 3.2 Zoom and fitting

- The wheel zooms around the pointer. The pointer's world location acts as the zoom anchor. The canvas is unbounded spatially, but the current camera scale is bounded from 0.05 to 3.5.
- Home fits board content. The minimap provides an overview; click or drag within it centers the camera at that world position, and its pin state persists.
- **Advertised but unverified on the outer Board:** the registry lists trackpad pinch and plus or minus zoom. The current board audit found no corresponding pinch or plus/minus input consumer. Embedded File Atlas does consume pinch; the tool catalog has its own keys. Do not infer outer-board support from those other surfaces.
- The Zoom tool supports click to multiply zoom by 1.5, Alt-click to divide by 1.5, and primary-drag to fit a drawn window when both dimensions exceed eight screen pixels. It temporarily overlays the previous tool. Escape first cancels a live zoom window; a later Escape or invoking the Zoom tool again disarms it and restores the previous tool.
- Search results can move the camera to their objects. Cycling objects makes a minimal camera adjustment when needed to bring the target into view.
- Portal cameras and 3D cameras are separate from the board camera. A wheel gesture inside focused content can navigate that content instead.

### 3.3 What scales with zoom

Authored geometry, strokes, text, object-local labels, portal tabs, and embedded toolbars scale with the board camera. Content that becomes too small can disappear through level of detail rather than holding a readable fixed size. Window chrome stays at screen scale. Pointer-attached tool ghosts and path-edit anchors, handle lines, and knobs are named exceptions that remain usable at screen scale. Hit tolerance can remain in screen pixels even when the graphic shrinks.

Sources: [camera helpers](../../apps/slate/src/app/canvas.rs), [command descriptions](../../apps/slate/src/app/commands.rs), [canvas scale pattern](../keymap/contracts/PATTERNS.md).

## 4 Hover and passive feedback

- Eligible objects show a passive outline on hover. Preferences can enable or disable passive highlighting independently for frames, images, shapes, text, connectors, portals, and embedded toolbars.
- Hover eligibility follows the actual shape or stroke where supported. An open curve's large rectangular bounds are not meant to behave as a filled card.
- Selection has its own visual treatment. The selected silhouette follows curves, ellipse edges, and rounded corners; it is not simply a row of visible square bounding-box handles.
- Moving near an eligible edge, corner, or rotation region changes the cursor and reveals the relevant manipulation opportunity. Screen hit regions can be larger than the painted grip.
- Wire ports are disclosed near the relevant side midpoint or object feature. Hovering one port previews that port, rather than showing every possible port permanently.
- Path anchors and tangent controls have a separate editing presentation. Hover and picked-vertex state can make a curve the target of a tip HUD without changing the whole-object selection.
- Armed creation tools tint the pointer and show a small ghost of their result before a press. A ghost is feedback for an armed tool, not an already-created object.
- Snapping provides a target marker or label and transient alignment feedback. A Tab-locked direction adds a padlock beside the pointer.
- Video hover scrubs the clip. Album hover previews pages or images. These content previews are distinct from object selection.
- Menus and toolbar controls can open on hover as well as click. Their open surfaces then own pointer and wheel input within their bounds.

Sources: [picking](../../apps/slate/src/app/board/board_pick.rs), [selection handles](../../apps/slate/src/app/board_handles.rs), [tool ghosts](../../apps/slate/src/app/board_place.rs), [hover preferences](../../apps/slate/src/app/ui/advanced.rs).

## 5 Picking and selection

### 5.1 Object selection

- A plain primary-click replaces the selected set with the target. Clicking an already selected object preserves the set so a subsequent drag can move it.
- Shift-click or Ctrl-click adds a target, or toggles an already selected target out. A modified click on empty canvas preserves the existing set.
- Select all selects eligible visible objects. Hidden and ordinarily locked nodes do not participate in normal picking.
- Picking uses object geometry and stacking. Ordinary objects are considered before connector strokes, and frames last; reverse scene order selects the topmost eligible hit within each pass. Explicit controls win over object bodies; path stroke geometry wins over an inflated empty rectangular interior.
- Flat groups select as a unit. Ctrl+Shift-click bypasses group expansion and can select an individual group member. It can also force-select a locked node for a one-off edit.
- Ctrl+Shift-click on a curve segment has the more specific meaning of picking that segment and its endpoint vertices. It must not be described only as a universal group-member shortcut.
- Tab and Shift+Tab cycle eligible objects in reading order while idle, skipping connectors and treating a group as one stop. They do not cycle objects while a draft owns direction locking or a field owns keyboard focus.

### 5.2 Selection sweeps

- In Select mode, dragging on empty board starts a rectangular sweep. Left-to-right is a containing window; right-to-left is a crossing window. The current pointer's horizontal position relative to the start determines the mode, so changing direction can change the preview. Window uses a solid border; Crossing uses a dashed green border. Shift or Ctrl held at release adds to the existing set.
- Open curves are tested against their actual stroke geometry. A sweep inside a curve's empty bounds does not select it merely because the rectangles overlap.
- **Current connector exception:** connectors use crossing behavior in both sweep directions. Strict containing-window behavior for connectors remains a proposed refinement.
- Frames themselves are excluded from the completed sweep result, as are hidden and locked objects.
- A sweep can start on a frame's body only when the frame covers at least the full visible canvas width and height. It then selects members of that frame. Otherwise a frame-body drag moves the frame.
- A sweep that starts outside such a frame currently has no general restriction to one frame's members.
- Direct Selection also supports sweeping vertices. Ctrl+Shift-drag on the active curve instead sweeps segments: Window encloses complete segments, Crossing intersects segments, and repeated sweeps add. These are separate selection sets from whole objects.
- An empty gap inside a multi-selection's union rectangle belongs to moving that selection, rather than starting a new sweep or wire.

### 5.3 Selection scope

Whole objects, group members, curve vertices, curve segments, paint layers, raster selections, spreadsheet cells, chat text, and portal content each have a meaningful selection scope. A shared system should identify that scope before applying Delete, copy, drag, properties, or Escape.

Sources: [selection sweep contract](../keymap/contracts/select-sweep.md), [pick guards](../../apps/slate/src/app/board/board_pick/input_guards.rs), [gesture release](../../apps/slate/src/app/board/board_gesture_end.rs), [curve picking](../../apps/slate/src/app/board_path.rs).

## 6 Move scale rotate and arrange

### 6.1 Move and duplicate

- Primary-drag an eligible object or selected set to move it. A group moves together; moving a frame also carries its geometrically contained members.
- Alt held when a move or scale begins duplicates instead of changing the originals. Duplication is latched at the start of the operation; Ctrl+Alt+Shift group layout scaling is the explicit exception. A cancelled drag removes any provisional duplicates.
- In an Alt-duplicate **move**, snaps remain suspended for the whole duplicate gesture, including after Alt is released. In an Alt-scale gesture, snap suspension follows whether Alt is currently held.
- Arrow keys nudge selected objects by one world unit; Shift uses ten. In Direct Selection, selected anchors take priority. The same arrows pan when there is no selection.

### 6.2 Scale and rotation

- Edge and corner regions resize eligible objects even on hover before selection. A corner preserves aspect by default and Shift frees it; an edge changes one axis by default and Shift preserves aspect. Ctrl resizes from the center. Otherwise the opposite handle remains fixed, including on rotated objects.
- A rotation region outside an eligible corner or vertex supports rotation, with a circular two-headed cursor. Rotation softly snaps within four degrees of 45-degree multiples; Alt disables that snap. On regular polygons, Shift places the grabbed vertex on a cardinal axis. Portals, connectors, embedded toolbars, and simple curve-grip objects do not expose this ordinary rotation control.
- Shift or Ctrl on an unselected object's hover resize band first means add-selection. On a selected object it means the transform modifier. Tiny object bodies retain a move region even when resize hit regions overlap most of the graphic.
- Scaling a multi-object selection normally scales the arrangement and its members. Ctrl+Alt+Shift on a group grip instead repositions members without scaling their individual sizes; the opposite union handle remains pinned.
- An eligible picture dragged past its opposite edge or corner mirrors. Explicit horizontal and vertical mirror commands also exist for compatible pictures, paths, and lines.
- Path edits can expose endpoints and tangent knobs instead of ordinary card-style resize behavior. Their modifiers are documented in section 10.
- Escape during a live manipulation restores the press-time state and keeps selection. A release outside the window with no pointer position also restores the operation. No abandoned partial manipulation should become an undo entry.

### 6.3 Arrange and object flags

- A multi-selection exposes alignment controls in idle Select mode with crop off. Two objects permit alignment; three or more permit distribution. Left, center, right, top, middle, bottom, horizontal distribution, and vertical distribution operate on the selected set. These act on press, consume the rest of that press, and ignore modifiers and snapping. Hidden nodes and connectors are omitted; locked members contribute to bounds but remain fixed. Arrangement uses authored rectangles rather than rotated visual outlines.
- Bring to front and send to back change stacking. Group and Ungroup change selection grouping. Hide removes objects from ordinary paint and picking; Show all restores visibility. Lock retains visible objects and snap targets but prevents ordinary selection and manipulation; Unlock all restores editability.
- Object-local property controls and dimension stringers edit the selection. Clicking a dimension accepts typed values; Enter applies and Escape cancels. Dimension edits use a defined anchor such as the object center.
- Corner-radius grips sit on the actual border at the corner tangent. Drag changes the radius without jumping on grab; Shift uses whole units. Clicking permits numeric entry. Multiple selected objects receive the shared amount clamped to each object's valid geometry. A single polyline exposes per-corner editing.
- A selected regular polygon reveals plus and minus controls at a hovered vertex. They add or remove a side while retaining that vertex's location; the supported range is three to twelve sides.

### 6.4 Optional bumper interaction

Bumper cars is disabled by default. When enabled, a node with its Bumper property on pushes other bumper nodes during a drag; friction governs the glide after release. Alt passes through rather than pushing. Escape restores the whole in-progress shove, and undo reverses the gesture as a unit. Buffer and friction are object properties, not universal camera settings.

Sources: [transform patterns](../keymap/contracts/PATTERNS.md), [transform implementation](../../apps/slate/src/app/board_transform.rs), [handles](../../apps/slate/src/app/board_handles.rs), [alignment](../keymap/contracts/align-widget.md), [bumper cars](../keymap/contracts/bumper-cars.md).

## 7 Snaps constraints and precision

### 7.1 Three different aids

- **Grid visibility**, G or F7, draws the reference grid. **Snap to grid**, F9, attracts authored points to a 20-world-unit grid. They are independent toggles; grid snap defaults off.
- **Smart guides** align moving geometry with relevant neighboring edges, centers, rows, and columns. Reach has Tight, Nearby, and Wide choices; Nearby is the default. Candidate reach is measured against screen distance and visible geometry, not a whole-board search regardless of zoom.
- **Object snaps** target geometric features: End, Mid, Center, Near, Intersection, Quadrant, Perpendicular, and Tangent. They have a master switch plus individual feature switches. The master and End, Mid, Center default on; the other kinds default off. The set is a session preference rather than authored scene content.
- Locked objects can remain snap references even though they are excluded from ordinary selection. Hidden objects do not behave as ordinary visible targets.

### 7.2 Live constraint behavior

- F8 toggles persistent ortho, initially off. During eligible segment gestures, held Shift inverts that setting, yielding 45-degree direction steps when the resulting constraint is enabled. Area tools use Shift for aspect instead and do not apply F8 to their rectangle.
- Tab during a segment locks the current direction from the previous point. Pointer movement then changes length. Tab again unlocks; placing the point, cancelling, or changing tool clears the lock.
- **Current implementation:** object snaps resolve and are projected onto the locked direction. An older universal-pattern passage says point snaps are suspended under a direction lock; that passage does not match the current resolver.
- Eligible tools accept numeric entry for their parameter, such as length, dimensions, or radius. Numeric entry belongs to the active tool and can take precedence over type-to-command.
- Point resolution first checks Alt suspension, then applicable ortho, object snaps, smart guides, grid, and finally the raw point. Object snaps use a twelve-screen-pixel acquisition radius and prioritize End, Intersection, Center, Mid, Quadrant, Perpendicular, Tangent, then Near; distance breaks ties within a kind. Smart guides acquire within nine screen pixels. The displayed target and the committed point use the same resolved geometry.
- Snap kinds have geometric limits: Tangent and Quadrant need circle or ellipse-like geometry; Perpendicular and Tangent need a prior point. Arbitrary curve-to-curve intersections and Bézier tangent snapping are not shipped board snaps.
- Area tools snap the moving rectangle edges and both drag corners rather than pretending the whole gesture is a single cursor point.
- Moving with ortho retains object-snap and guide corrections projected onto the constrained axis, while grid is suspended. This differs from the precedence used for individual point picks.
- Temporary overrides are context-specific. Alt can bypass snapping during moves and edits, yet Alt means background sampling for the Eyedropper and tangent-handle independence on a curve. There is no single global Alt meaning.

### 7.3 Snap feedback

Object-snap symbols and labels identify the acquired feature. Smart-guide acquisition fires a brief tapered ribbon that expands and fades instead of leaving persistent alignment leaders everywhere. The same feedback applies to compatible tool ghosts, drag corners, endpoints, and scale handles. Releasing a constraint must also release its feedback.

Sources: [object snap contract](../keymap/contracts/object-snap.md), [snap resolver](../../apps/slate/src/app/board_osnap.rs), [smart guide candidates](../../apps/slate/src/app/board_snap.rs), [snap feedback](../../apps/slate/src/app/board_forcefield.rs).

## 8 Tool arming placement and closed shapes

### 8.1 Common lifecycle

- A command, dock icon, palette result, or assigned shortcut arms a tool. The active tool is visible through dock state and its cursor. Opening a tool family and arming a particular tool are separate actions.
- Rectangle, Ellipse, Polygon, Frame, and portals support click for a default-sized object or press-drag-release for an authored size. More than four screen pixels of raw pointer travel chooses drag-size; snapped movement and zoom do not change that classification.
- Creation ghosts follow the resolved point. A press replaces the area ghost with a live size preview. Preview and commit share the same snapped geometry. Degenerate results can be discarded.
- Most creation tools are one-shot and return to Select after creating. Brush, Eraser, Smooth, Trim, Split, and Deck remain armed. Text remains in composition until finished; Sticky returns to Select after placement while its caret is active.
- Rectangle, Ellipse, Polygon, Line, Arc, Polyline, and Bézier use a crosshair. Pen, Brush, Eraser, and Smooth hide the ordinary pointer beneath a tip-size circle. Frame and portal placement use tinted pointer chrome and a small result ghost.
- Repeat-last can re-arm a repeatable tool. Escape can peel a live gesture, draft, and then tool; switching tool also ends the previous tool's transient state according to its owner.

### 8.2 Shape placement and style memory

- Rectangle and Ellipse draw freely from a corner; Shift produces a square or circle. Polygon uses a regular polygon inside its construction box, initially a hexagon; Shift gives it equal aspect. Ctrl places Rectangle, Ellipse, or Polygon around the press point as center and combines with Shift.
- Frame uses the current preset aspect by default; Shift squares it. Portals use free aspect by default and Shift constrains to 16:9. Frame and portal placement do not use Ctrl-from-center.
- Area placement ignores F8 ortho and Tab direction locking. Alt temporarily suspends its snap aids.
- Closed primitives consume the shared remembered closed-shape fill and stroke. Open-curve edits do not overwrite that closed-shape memory. Closed Polyline and Bézier results also consume it in the current implementation.
- New shapes become selected. Rectangle dimensions are local width and height; ellipse dimensions are diameters; a circle's diameter edit preserves circularity. Polygon dimensions use its tight vertex bounds, including its current phase and side count.
- Fillet or chamfer can be absolute or percentage based. Absolute values preserve distance through later resize; percentage values preserve proportion.
- Closed shapes can host editable words and image content. Their text, pixels, geometry, and style retain distinct editing scopes.

Sources: [tool arming](../keymap/contracts/tool-arming.md), [placement rules](../../apps/slate/src/app/board_place.rs), [creation release](../../apps/slate/src/app/board/board_gesture_end.rs), [tool model](../../apps/slate/src/app/board/board_tools.rs).

## 9 Precision curve drafting

### 9.1 Line

Line accepts click-move-click or press-drag-release. A short first release leaves the first point live; a longer initial drag commits on release. In second-point mode the next press commits the resolved endpoint once, and its later release cannot move it.

After the first point, the live preview shows the actual stroke and a length/angle readout. Shift inverts F8 ortho, Tab locks the current direction, and object snaps project onto that locked ray. Digits and one decimal point enter a positive exact length; Backspace edits it. Enter commits that length along the current or locked direction, or commits the resolved endpoint without a number. Escape clears numeric entry first, then removes the first point, then disarms.

### 9.2 Polyline

Each primary press places one vertex. Release adds none. The pending segment can snap to earlier draft vertices and completed segments, excluding itself. Shift/F8 and Tab constrain the pending segment, and placing a vertex releases its direction lock.

Enter finishes open. Double-click finishes without a duplicate final vertex. After three vertices, snapping back to the first closes without duplicating the start. Escape finishes two or more already placed points; fewer points are discarded. Finishing never appends an unplaced hover endpoint. Closed results use shared closed-fill memory.

### 9.3 Arc

Pick start, end, then a through-point. While choosing the third point, the bulge changes while the endpoints stay fixed; the third pick commits. Partial arcs cancel rather than finishing on Escape. Point snaps, Shift/F8, and Tab apply from the prior picked point. Selected arcs expose start, end, and through grips. Moving one reconstructs the circular arc; the through grip recenters on the resulting sweep.

### 9.4 Bézier span

- Click places a corner anchor without handles. Press-drag places an anchor with symmetric tangents. Four screen pixels separates click from handle drag. Adjacent anchors without handles connect straight.
- Hover on the start anchor for 350 milliseconds, after at least two anchors and with no active drag, previews closure. Click while that preview is present closes and finishes. An earlier click edits the start anchor instead; leaving resets the dwell.
- Enter or Escape with at least two anchors finishes open. Escape with fewer discards. Double-click whose second press lands on an existing anchor finishes; two fast clicks in different places remain two anchors.
- Escape during an active press drops the anchor being placed or restores the anchor being edited.
- Draft undo removes the last anchor; draft redo restores it. This local history precedes document undo. Undo at zero anchors exits drawing.
- Draft anchors and handles remain editable before completion. Other draft anchors can be snap targets; the pending segment uses the shared direction and object-snap constraints.
- Arc, Polyline, and Bézier have commands but no dedicated default keyboard chord. Their tool-family label L must not be mistaken for three additional Line shortcuts.

Open curves use geometry grips rather than permanent box dimension stringers. Closed paths can expose bounds dimensions.

Sources: [line](../keymap/contracts/line.md), [polyline](../keymap/contracts/polyline.md), [arc](../keymap/contracts/arc.md), [Bézier](../keymap/contracts/bezier-span.md), [draft implementation](../../apps/slate/src/app/board_path.rs), [multi-point input](../../apps/slate/src/app/board_path/multi_point.rs).

## 10 Curve refinement trim split and join

### 10.1 Anchors segments and tangent handles

- Direct Selection targets an editable curve, including one member of a group. Click picks an anchor; Shift-click toggles anchor picks. Drag moves the picked anchors. A segment press picks its neighboring anchors so dragging reshapes that segment. Empty-space drag can sweep anchors; an empty click clears anchor picks before clearing the curve target.
- Ordinary Select also exposes appropriate grips: Line endpoints, Arc defining points, Polyline vertices, and Bézier or Pen anchors and nonzero handles. Ctrl+Shift adds the more specific segment-picking route described in section 5.
- Double-clicking a Path anchor toggles corner and smooth. Rectangle or polygon vertices remain pick-only until a geometric edit promotes the primitive to an editable Path.
- Moving or deleting a primitive rectangle/polygon vertex promotes it to a closed Path while preserving appearance and style. A style-only per-vertex edit preserves the primitive.
- Dragging a Bézier tangent normally preserves its coupled relationship. Alt moves only the grabbed tangent; Shift locks its direction and changes length; Ctrl scales both tangent lengths by the dragged ratio while retaining their directions; Ctrl+Shift combines those constraints. Alt takes precedence over Ctrl.
- A Shift-click with no travel can toggle a grip pick even though Shift-drag locks tangent direction. The click and drag phases have different semantics.
- Selected vertices can receive their own width, color, opacity, and corner treatment. Compatible values blend toward neighboring vertices. A whole-curve tip edit preserves taper through relative scaling.
- Delete on picked segments removes those segments, splitting an open curve or opening a closed curve as appropriate. This is distinct from deleting the entire selected object.

### 10.2 Trim and Split

Trim and Split have a cutter-picking phase and a target-picking phase. Preselection supplies cutters and enters target picking directly; otherwise click adds cutters, Ctrl-click removes them, and Enter advances. Hover previews the relevant cutter boundaries and affected span, face, or resulting pieces. These clicks pick regions, so grid, ortho, and point snaps do not relocate them.

Trim removes the clicked part. Shift-click near an open endpoint extends it to the nearest cutter. Split keeps all resulting pieces and does not use Shift-extension. Two-point line cutters extend as cutting lines; longer open cutters only cut where they actually run. True treated and rotated boundaries matter, including fillets, chamfers, and ellipse outlines.

Each target click is one undo group and leaves the tool armed. Enter finishes. Escape returns from target picking to cutter picking, then to Select; previously committed cuts stay and can be undone. Closed trimmed forms become editable Paths with appropriate holes and preserved styling. Trim can clip text or images without changing their source. Split excludes text and images. Both exclude frames and portals. There is no separate Untrim action.

### 10.3 Join

Join acts immediately on selection. Open paths join at nearest ends, merging ends within tolerance or adding a bridge otherwise. Connected closed regions union; mixed open operands contribute their stroke-weight region. Disjoint operands remain separate. The resulting selection and replacement enter one undo group. With the two ends of one open path picked, Direct Selection's Join can close that path.

Sources: [direct selection](../keymap/specs/direct-selection.md), [handle implementation](../../apps/slate/src/app/board_direct.rs), [trim](../keymap/contracts/trim.md), [split](../keymap/contracts/split.md), [join](../keymap/contracts/join.md).

## 11 Ink brush tips smoothing and color

### 11.1 Pen Brush Eraser and Smooth

- **Pen:** freehand input becomes a fitted editable vector curve. It is one-shot. Freehand points are not quantized to object or grid snaps; later anchors can be snapped. Shift-drag draws from the newest surviving Pen endpoint, or the current press when none exists. Shift-click connects at the exact click angle, or seeds a start. Tab can lock a straight segment. Compatible extensions append to the prior Pen path as a new undo step.
- **Brush:** primary-drag paints; click makes a dab. It stays armed. Shift-drag continues from the previous brush endpoint in 45-degree steps; Shift-click connects at any angle or seeds the endpoint. Tab locks the Shift segment. These Brush/Eraser Shift constraints do not use the ordinary F8 inversion rule. Strokes are not quantized to point/grid snaps.
- **Brush compositing:** opacity applies to the whole stroke, so normal self-overlap does not repeatedly darken that stroke; separate strokes composite. Available textures include Smooth, Graphite, Pencil, Ink, and Watercolor. Watercolor intentionally builds up at distant self-crossings.
- **Eraser:** click makes a dab and drag sweeps. Painted ink erases locally; crossed vector strokes are removed whole on release and dim during preview. Ordinary images, text, frames, and connectors are not vector-eraser targets. Inside an image painting session, erasure is limited to the active paint layer. One pass does not repeatedly compound at its own crossings; later passes can compound.
- **Eraser straight pass:** Shift uses the previous pass endpoint with 45-degree steps and Tab direction lock. Escape restores a live pass; a later idle Escape disarms.
- **Smooth:** drag sweeps a circular influence over existing strokes with interpolation between pointer samples. It refits vectors and authors blur on painted strokes. Empty canvas creates nothing. Shift makes a straight smoothing pass from the previous endpoint. Smooth stays armed; Escape discards its live preview. It does not inherit Brush's Tab lock or 45-degree rule.

### 11.2 Right-button tip HUD

- A tip HUD can target an armed ink/vector tool, picked curve vertices, a hovered eligible vertex, or one selected curve. Its target freezes for the session. A held modifier plus right-drag claims input before board pan, turbo pan, or a context menu.
- Ctrl-right opens the color wheel for a color-capable target. Alt-right adjusts size. Shift-right adjusts opacity or strength. Color-capable Ctrl takes priority over Alt; Alt takes priority over Shift. Eraser and Smooth have no color wheel.
- In the size HUD, moving right enlarges the diameter by two screen pixels per pointer pixel. Moving up increases softness for painted or sweeping tips. Vector curves have no softness. A nearby style row chooses brush textures or vector styles; entering the brush-texture band hardens softness to zero while retaining size.
- In the opacity/strength HUD, upward travel increases and downward travel decreases the value; 100 screen pixels spans the full range.
- The color wheel has a saturation/value disk, a separating gap, a hue ring, and 24 recent-color dots. Entering a recent dot adopts its color and centers the pointer on it. Crossing outside the wheel reaches desktop sampling. Recent colors reorder after release.
- A right chord released within four screen pixels and 0.8 seconds opens numeric entry, except while pausing an in-progress freehand stroke. Size uses board units; softness, opacity, and strength use percentages; color uses hue, saturation, and value. Tab/Shift+Tab accepts a row and moves; Enter or outside click accepts and closes, consuming that outside press; Escape restores the opening values.
- Opening a HUD during Pen or Brush pauses ink sampling. Closing returns to the last ink point and blends toward the new tip over the transition distance rather than leaving a jump.
- Accepted edits to an existing curve commit as one patch; Escape restores the original. Creation-tip settings affect subsequent output rather than adding a standalone geometry node.

### 11.3 Color and keyboard adjustments

Bracket keys change diameter; Shift-brackets change softness in quarter-range steps. Foreground/background color controls include reset and swap. Eyedropper sampling can reach desktop pixels outside Slate. Alt while Brush temporarily samples foreground while preserving alpha. The dedicated Eyedropper's Alt sample targets background. Color editors also offer a desktop sampling button: click then click a pixel, or press-drag-release to sample; Escape cancels.

Sources: [Pen](../keymap/contracts/pen.md), [Brush](../keymap/contracts/brush.md), [Eraser](../keymap/contracts/eraser.md), [Smooth](../keymap/contracts/smooth.md), [tip HUD](../../apps/slate/src/app/board_tip_hud.rs), [color input](../../apps/slate/src/app/board_color.rs).

## 12 Text editing shape words and sticky notes

- Text creation by click auto-sizes width to content. Drag fixes the wrap width; Shift-drag constrains the initial box to its default 280:48 aspect. The scene receives no new text object until nonempty composition finishes. Escape or click-away commits and selects nonempty text in one add; an empty draft disappears.
- Sticky currently click-places a 200-by-200 white note with centered dark text. Placement is journaled before typing, the caret opens immediately, and the tool returns to Select. Drag-size described in an older text contract is not current Sticky behavior.
- While editing a sticky, Tab commits its words and creates an adjacent sticky on the right; Shift+Tab creates one on the left. The gap is 24 world units and caret focus transfers to the new note.
- Double-click re-enters text or sticky editing, with the style palette initially closed. A text field owns typing, character selection, clipboard shortcuts, and newline entry, rather than allowing those inputs to trigger board commands.
- Ordinary text keeps its wrap width and grows downward. Sticky paint can shrink type to fit, retaining its authored type size as the ceiling. Resizing a text host reflows the frame rather than automatically changing authored type size.
- Double-click an eligible unlocked closed shape to edit words inside it, including an otherwise unfilled interior. New shape words are centered. The host's geometry, text alignment, clipping, font, and style remain authored properties.
- Object-local controls edit typeface, size, justification, font color, and supported bold, italic, or underline properties. Host fill and stroke remain separate from letter color.
- Linked plain-text and source cards accept an authored canvas override; Enter inserts a newline and Escape or click-away commits the words and grown card bounds as one scene edit. The linked source text file stays intact.
- A quote-started new text document uses the distinct linked-card route described in section 20, rather than the ordinary Text object tool.

Sources: [text contract](../keymap/contracts/text.md), [text placement](../../apps/slate/src/app/board/board_text_place.rs), [text overlay](../../apps/slate/src/app/board/board_text_overlay.rs), [sticky layout](../../apps/slate/src/app/board/board_sticky.rs), [shape property controls](../../apps/slate/src/app/board_properties.rs).

## 13 Pictures crop paint layers and raster selections

### 13.1 Crop and content within a mask

- Crop command, an eligible picture's double-click, or its crop control enters content editing. Edge/corner drags change the crop window; an interior drag pans the source pixels. Overhanging pixels dim, crop brackets appear, and ordinary resize controls hide.
- A picture filled into a closed shape retains that shape as a fixed mask; content grips pan or scale the pixels behind it. Dragging a picture into a shape can transfer it; Alt retains its source.
- Crop uses its own content coordinate system and ignores board F8, F9, and object snaps. Selection and the relevant palette remain available.
- Escape during a crop drag restores that drag's start and retains crop mode. Idle Escape or Enter exits retaining the committed crop. An outside empty-canvas click exits and can deselect. Multiple selected images can receive the shared crop in one undo group while preserving their separate picture placements.
- Image adjustments are non-destructive authored properties. Their controls include opacity, brightness/contrast, color/filter adjustments, and invert, with continuous scrubs coalesced into one undo step. Whole-picture transform, crop, filter, and source replacement are separate actions.

### 13.2 Paint layers

- Image Brush and Lasso are mutually exclusive local tools; invoking the active one turns it off. Hosted drawing uses existing gesture grammar, clipped to the visible image and any pinned raster selection. Palette controls own their presses before any stroke begins.
- Layer circles select the active layer; plus adds. Leftmost is topmost. Drag reorders, Alt-drag duplicates, and dragging out makes an independent canvas copy.
- Layer right-click offers rename, visibility, duplicate, delete, and adding another image as a layer where eligible. Enter or blur accepts the inline name. Opacity scrubs live, release commits once, and Escape restores.
- Dropping pixels into the active image and choosing Add as layer differs from the layer menu's asynchronous Add image action, which explicitly creates a separate layer.
- Idle Escape disarms the local image tool while retaining the selected image and its palette. Paint erasure remains on the active layer.

### 13.3 Lasso and subject selection

- Polygonal Lasso places vertices on press. Shift constrains to 45 degrees in image-local coordinates; Delete removes the latest vertex. Enter, double-click, or returning near the start finishes a selection of at least three points. Escape cancels.
- In Select mode, hovering an eligible image for 400 milliseconds starts asynchronous subject highlighting. Movement within eight screen pixels preserves dwell. Preview follows the silhouette rather than painting an extra rectangular region box or floating Pin button.
- First click pins the subject selection; further clicks add include prompts and Alt-click adds an exclude prompt. Right-click the region offers creating a layer from the selection.
- Drag highlighted geometry out and release on board to create an independent cutout. Escape cancels the pending drag or selection. Replacing the source or switching tabs invalidates pending results so an old asynchronous reply cannot revive dismissed state.
- Raster selection, layer selection, and whole-picture selection remain separate. Delete and copy need to respect which scope owns input.

Sources: [image crop](../keymap/contracts/image-crop.md), [image workflow](../keymap/contracts/image-workflow.md), [paint layers](../keymap/contracts/image-paint-layers.md), [layer implementation](../../apps/slate/src/app/board_image_layers.rs), [subject selection](../../apps/slate/src/app/board_image_segment/input.rs), [lasso](../../apps/slate/src/app/board_image_segment/lasso.rs).

## 20 Command discovery search and repeat

### 20.1 Ways to invoke an action

Hotkeys, menus, dock controls, object-local controls, palette search, and history refer to registered command identities. Some gestures are handled locally and have descriptive registry records rather than machine keyboard chords. The full [command index](02-registered-command-index.md) contains 289 records across nine registry categories, plus four secondary keyboard aliases.

- Double-click empty board in Select or Hand opens a command palette at that world point. Creation actions use that placement context.
- Palette search matches command names and aliases, allowing language such as box, circle, note, slide, or a portal name. Enter runs the chosen available result; Escape dismisses the palette.
- Type-to-command opens the palette when typing starts on an idle board. A bare letter that is already a shortcut waits approximately 700 milliseconds. A second printable character within that interval promotes the letters into a palette query; otherwise the shortcut arms its tool. A pointer press or another chord can flush the pending shortcut early. Auto-repeat of the held letter does not repeatedly open command entry.
- A leading single or double quote opens the palette with Media new text document as its result. Enter creates a blank linked text card at the pointer with the caret ready. Escape creates nothing.
- F1 opens Commands and shortcuts in Advanced. F2 opens command history. Advanced also contains settings, connection health, update controls, preview preferences, and diagnostic information.

### 20.2 Search objects rather than commands

Canvas search finds linked items, authored text, frame titles, and tags. Enter and Shift+Enter cycle matches and move the camera; nonmatches dim to 35 percent. Escape closes search. Search navigation, palette action execution, and ordinary object cycling are separate operations even though each can use Enter or Tab in its own context.

### 20.3 Repeat and history

Space tap or idle Enter repeats the most recent repeatable command, which can re-arm a one-shot tool. Space fires on release only when held for less than 250 milliseconds with no pointer-button use during the hold. Space-drag stays pan. Undo, redo, clipboard operations, destructive actions, and other Never entries are excluded according to registry policy. Active editors, drafts, crop, and Trim or Split phases receive Enter first. History is an action record; camera and selection are not ordinary scene undo entries.

Sources: [dispatch](../../apps/slate/src/app/dispatch.rs), [overlays](../../apps/slate/src/app/overlays.rs), [command registry](../../apps/slate/src/app/commands.rs), [registry design](../keymap/specs/command-registry.md).

## 21 Context menus and secondary button behavior

- Plain right-click opens the object's contextual menu, or the empty-board menu on a miss. Right-drag, engaged turbo pan, a tip HUD, or a page-owned secondary click suppresses the ordinary board menu.
- Right-click does not immediately replace selection. If the clicked object is already selected, actions target that selection; otherwise actions target the clicked object alone. Choosing the action establishes the target set before dispatch.
- Recipes differ for ordinary images, pinned raster selections, layer icons, PDF and presentation media, video, documents, models, text files, spreadsheets, rectangles, ellipses, polygons, lines, paths, text and stickies, frames, toolbars, connectors, multi-selection, grouped selections, and each portal's bound or empty state.
- Common action families include stack order, properties, source opening, copy, duplicate, delete, lock, hide, group, crop, layers, tags, wire presentation, and portal source or focus controls. Unsupported actions are omitted. The grouped-selection recipe takes priority over a generic multi-selection recipe.
- Empty canvas offers settings for all unlocked wires, plus Show all hidden and Unlock all when applicable. Changing canvas wire settings applies color, width, or routing in one undo group.
- Tags opens a submenu by hover or click. A checked tag means every target item has that assignment. Clicking it removes that group's assignment; clicking an unchecked tag assigns it. Tags within one group are mutually exclusive; assignments across groups combine. Tag rows can remain open for further changes.
- Pressing outside the menu or submenu dismisses it. Selecting an ordinary action closes it. Menus own scrolling while hovered.
- A live web body's right-click belongs to the page. The portal border and chrome retain Slate's portal menu.
- Layer-icon right-click opens layer-specific actions; it does not use the whole-picture menu. A toolbar's own configuration menu similarly differs from a board object's generic menu.

Sources: [context menu recipes](../../apps/slate/assets/context-menus.json), [menu target and dispatch](../../apps/slate/src/app/board/board_menu.rs), [tag submenu](../../apps/slate/src/app/board/board_menu/tags.rs), [menu contract](../../crates/atlas-shell/MENUS.md).

## 22 Commit cancel undo and recovery

### 22.1 The cancellation stack

The general stack is active operation, draft, armed mode, selection, then chrome. One Escape peels one owned layer. Local owners can run earlier: cross-window carry, paint or result drags, portal maximize, portal contents focus, preview panels, or live model navigation. The shared interface should represent that ownership explicitly rather than broadcasting Escape to every component.

- Live move, resize, rotation, fillet, or handle manipulation restores the press-time objects; provisional duplicates disappear; selection remains; the aborted operation adds no undo entry.
- A live Zoom window cancels before the Zoom tool disarms. A portal restores from maximized view before releasing its contents focus.
- Crop Escape finishes the crop session and keeps selection. Inline text Escape commits the editing session. Polyline Escape can finish two or more placed points. These are finish behaviors, not universal discard behaviors.
- Direct Selection peels point picks, then its target, then the tool. Trim and Split peel their part-picking and cutter-picking phases separately.
- Escape from presentation exits playback. Escape in a focused menu or palette dismisses that surface according to its local owner.
- Releasing an ordinary node drag with no pointer location restores the operation. Redirecting a dragged object into a specialized drop action restores its temporary movement before committing the drop, so undo does not also expose an accidental preliminary move.

### 22.2 Undo grouping

Completed ordinary board mutations are named and invertible. One continuous move, draw, scale, rotation, property scrub, wire edit, crop session, or multi-object arrangement normally produces one undo step. Text editing commits a session. Trim or Split can commit a separate change per target click. Undo and redo operate on authored state; selection, hover, focus, temporary tool ghosts, and camera exploration are not authored content.

Explicit writes to linked source files, such as a spreadsheet's Save action, are a different boundary from changing a linked object's canvas presentation. The interface names that source-file action; leaving an editor with unsaved source edits has its own save or discard flow. Do not infer that every external application action or source-file write is reversed by Slate's board undo.

Sources: [cancel owner](../../apps/slate/src/app/dispatch/cancel.rs), [gesture finalization](../../apps/slate/src/app/board/board_gesture_end.rs), [universal cancellation contract](../keymap/contracts/PATTERNS.md), [journal model](../../crates/slate-doc/src/scene.rs).

## 23 Status feedback preferences and failure states

- The readout bar displays workbook counts, tag coverage, link health, zoom, and active tool values such as line length and angle, brush state, or slide count. Its gear chooses visible readouts.
- Hidden and locked counts are actionable readout links: click to show all or unlock all. Read-only workbook status identifies that another session holds the write lease and directs editing through a saved copy.
- F11 and the lower-left chevron hide or show the bottom readout bar. This is different from entering presentation or maximizing the operating-system window; the top bar and tools remain.
- F4 bookmarks the current moment in the session activity log. Advanced exposes the log folder and last recorded stall.
- Feedback offers bug and feature forms, attachments, and reproduction-step recording. During recording, visible chrome marks the mode. Idle Enter ends recording; Enter consumed by a text field or editor retains that editor's meaning. Reports are saved locally with app context and any recorded steps.
- Background decoding, scans, source resolution, poster capture, and model or agent work show available cached content or progress while work runs. The canvas remains interactive; completion should not silently redirect the user's current input.
- Missing, unknown, unsupported, locked, loading, and unbound states are different. An unsupported model names the reader gap. An unbound portal offers a source choice. A cloud-only or undecodable video can remain a poster. A missing link remains a link that can be identified or repaired.
- Local preferences include dock placement, panel pinning, hover highlighting, snap aids, and preview budgets. Authored object properties and source locators belong to the workbook. Browser sign-in and API secrets belong to the machine, outside workbook content and export.
- This inventory covers mouse and keyboard behavior plus wheel and trackpad pinch where implemented. The Pen tool's name does not establish stylus pressure, tilt, touch gestures, or accessibility behavior; those require a separate modality audit before becoming shared promises.

Sources: [readouts](../../apps/slate/src/app/ui/readouts.rs), [Advanced](../../apps/slate/src/app/ui/advanced.rs), [feedback input](../../crates/atlas-shell/src/feedback/ui.rs), [workbook lifecycle](../../apps/slate/src/app/ARCHITECTURE.md).

## 24 Foundation for a shared interface language

The reusable foundation is a common set of interaction records with explicit exceptions. Sharing a shortcut alone is insufficient: the trigger, target, ownership, live feedback, commit, and cancellation need to agree.

### 24.1 Categories to share across applications

- Camera navigation and world-to-screen scale.
- Pointer targeting, hover disclosure, stacking, and hit tolerance.
- Whole-object and sub-object selection, groups, and window or crossing sweeps.
- Move, copy, resize, rotate, align, and property editing.
- Snap acquisition, constraint priority, direction locks, and numeric input.
- Tool arming, drafts, sticky tools, one-shot tools, finish, cancel, and repeat.
- Context menus, command search, object search, and command history.
- Text fields, nested editors, portal focus, and input transfer.
- Drop payloads, source linking, clipboard formats, and external-file boundaries.
- Undo grouping, asynchronous feedback, empty states, and failure recovery.
- Window chrome, tabs, dock placement, pinning, menus, and readouts.

Specialized layers then supply curve handles, brush tips, raster selections, spreadsheet cells, video playheads, model cameras, chat history rails, and foreign web input. Each specialized layer declares which common interaction it replaces while focused.

### 24.2 Fields for each future interaction record

For each behavior, capture a stable name and category; applicable surface and target; required selection or focus; initiation; click versus drag threshold; each modifier in each phase; state transitions; snap and numeric-entry behavior; cursor and preview; commit and undo unit; Escape or outside-click behavior; conflicts and winning owner; defaults and measurable feel constants; post-edit entry; failure states; and an implementation reference.

For portals also capture source binding, content authority, input entry and exit, view-state ownership, maximize, source-specific controls, stale or missing state, trust and consent, and export form. For media include playback, crop or camera state, screenshot behavior, and supported versus fallback content.

The existing [dimension registry](../keymap/contracts/DIMENSIONS.md) already defines D01 through D17 for tool interactions, D18 through D35 for portals, and D36 through D40 for media camera exports and saved views. The [pattern hierarchy](../keymap/contracts/PATTERNS.md) supplies inherited universal, object-class, and tool-family rules. Extend or translate those owners when creating a cross-application standard; do not quietly establish this descriptive snapshot as a competing contract database.

### 24.3 Decisions still required before standardization

Slate's overloaded modifiers, delayed letter shortcuts, different Escape finish behaviors, bounded zoom range, source-file editing, and current connector sweep exception are choices to evaluate with the other applications. The inventory preserves them so a later design can accept, revise, or deliberately specialize each choice. It does not declare every current Slate exception to be the future universal standard.

## 25 Coverage and maintenance

The inventory spans all seven current primary node classes: Frame, Image, Shape, Text, Connector, Portal, and DockStrip. It spans all 25 BoardTool variants: Select, Pan, Frame, Rectangle, Ellipse, Polygon, Line, Arc, Polyline, Bézier span, Pen, Text, Brush, Eraser, Smooth, Eyedropper, Sticky, Direct Selection, Agent portal, Web portal, File Atlas portal, Slate portal, Trim, Split, and Deck. Content editors and command-only actions extend that tool list.

The command appendix accounts for every registry record, including commands that are exposed only in specialized content or preferences. Source links in each category identify the owning implementation or contract. This is a static code and contract audit, not a claim that every gesture was exercised in a running Windows session.

When Slate changes, update the relevant owning contract and implementation first, refresh the corresponding inventory category, and regenerate the command appendix from `commands.rs`. Preserve the distinction between implemented behavior, an accepted contract, a proposal, and a retired entry. For future cross-application work, record deliberate differences rather than silently normalizing them away.
