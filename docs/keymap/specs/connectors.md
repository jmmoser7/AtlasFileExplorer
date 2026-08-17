# Spec — connectors (wires) on the board

Stage-2 spec. Research inputs: `../research/grasshopper.md` (all), `../research/miro.md`.
Constitution: Art. IV (SVG ceiling — `<path>` + `marker`), Art. VI (journaled),
Art. VIII (connectors are machine-readable relations for the context beacon).

## Model (`crates/slate-doc/src/scene.rs`)

```rust
pub enum ConnectorEnd {
    /// Anchored to a host feature (P1.wire.ports): a local box edge
    /// (`Top`…`Left`, `t` along that edge) or an open-stroke site
    /// (`Start` / `Mid` / `End`, `t` = arclength on `Mid`).
    Anchored { node: NodeId, side: Side, t: f32 },
    /// Dangling end at a world point (FigJam "let it vibe" — legal state).
    Free { point: [f32; 2] },
}

pub struct ConnectorNode {
    pub a: ConnectorEnd,
    pub b: ConnectorEnd,
    pub stroke: Stroke,               // existing SVG-ceiling stroke type
    pub arrow_a: bool, pub arrow_b: bool,  // default: none (moodboard), 
    pub label: Option<String>,        // optional text at path midpoint
    pub display: WireDisplay,         // Default | Faint  (Hidden deferred)
}

pub enum WireDisplay { Default, Faint }

// NodeKind gains:  Connector(ConnectorNode)
```

Rules:

- **Geometry is derived, never stored.** The curve between endpoints is
  computed at paint/export time from the current [`WireHost`] pose of
  anchored nodes (local features, not the world AABB — **P1.wire.ports**).
  Document Settings → Wires chooses the session display (`WireRouting`, not
  journaled — same class as the grid and object snaps):
  - **Bezier** (default): a cubic leaving each anchored end along the
    host's outward (rotated local-edge normal, or the stroke tangent at
    start/end / left-normal at mid). Handle length
    `clamp(0.35 * distance, 24.0, 160.0)`.
  - **Orthogonal** (**P1.wire.rails**): leaves each end along the host's
    true outward (so a rotated edge does not immediately re-enter), then
    takes a **50/50 three-leg** through the midpoint when that corridor
    is clear. An L, a wrap, a stair, or a 45° cut runs only if the
    three-leg hits a host — never as a same-length alternative (that
    flicker is a spaz). Sibling wires that share a source or destination
    fan along the port and take parallel rails (`ORTHO_EXIT_GAP` /
    `ORTHO_RAIL_GAP`) so they do not stack. Each dest claims the fan
    side it already sits on (no-crossover); a dead zone around the
    port centre keeps that sign from flipping during a drag. Collision uses each host's
    oriented silhouette (box, or the oval for an ellipse), not the
    world AABB. Equal-length ties prefer the right-hand path, then the
    bottom path. Corner fillets match File Atlas PCB-trace wires
    (`ORTHO_CORNER_RADIUS` world units, P0.9).
  `Node.rect` for a connector is its recomputed AABB (kept fresh whenever an
  endpoint node patches, or the routing toggle changes) so marquee/hit
  systems keep working.
- Connectors ignore frame membership and never become slides.
- Deleting a node deletes connectors anchored *only* to it? **No** — the
  anchored end degrades to `Free` at its last world position (journaled as
  part of the same command group, so undo restores the anchor). This keeps
  delete invertible and simple.
- `SceneCmd` is unchanged — Add/Remove/Patch cover everything.

## Grips (the interaction affordance)

Ports sit on **object features**, not the world AABB (**P1.wire.ports**):

- **Area objects** (rect, ellipse, text, image, frame, portal, dock strip,
  closed path): four ports at the midpoints of the **local** edges, then
  rotated about the node center. An ellipse's local-axis extrema lie on
  the curve; rotating the ellipse moves those ports with it.
- **Open strokes** (line, arc, polyline, bezier, pen): three ports at
  arclength `t = 0`, `0.5`, `1` (start / mid / end). Old files that stored
  a box side on a line snap that AABB point onto the nearest of those
  three.
- Connectors themselves have no ports.

With the **Select tool**, a grip previews only when the pointer is
within ~8 px of **that port**. An edge or stroke between ports is
inert for preview — it does not reveal the others. A press on a port
starts a wire and **beats** the edge-resize band (hit-test the press
origin, not the live hover cache). Snap radius while dragging a wire:
**14 px screen space** to a port; anywhere on a local edge or open
stroke snaps to the projected site (`t` along the edge, or arclength
on `Mid`).
- A dedicated **Connector tool** is *not* added in P1 — grips-from-Select
  matches Miro and avoids another mode. (Palette entry "Connect…" can arm a
  one-shot wire from the selected node's nearest port.)

## Wire gestures (the Grasshopper grammar)

| Gesture | Behavior |
|---------|----------|
| **Drag from grip** | Rubber-band bezier preview from the grip. Near a valid target grip/edge: preview snaps and renders solid. Release on target → `Add` connector. Release on empty canvas → **open the canvas palette at that point, pre-filtered to placeables** (Blueprint pattern, recommended by research); placing an item auto-connects to its nearest side. Esc during drag cancels. |
| **Plain drag** (no modifier) | Adds; whiteboard default is additive (research §9: "add default"). Existing connectors on the grip are untouched. |
| **Shift+drag** | Identical to plain add in P1 (kept so Grasshopper muscle memory does nothing surprising). Cursor shows a small `+`. |
| **Ctrl+drag from a grip with wires** | **Detach**: grabs the nearest existing connector end off the grip; it follows the cursor. Release on another grip/edge → `Patch` (rewired). Release on empty → the end becomes `Free` there (`Patch`). Cursor shows `−`. |
| **Ctrl+Shift+drag from a grip** | **Move all**: every connector end on that grip follows; release on target grip re-anchors all of them (one journal group of `Patch`es). Release on empty cancels. |
| **Drag a connector endpoint dot** (connector selected) | Same as Ctrl+drag detach — the discoverable path (FigJam style). |
| **Click a connector** | Selects it (stroke hit-test: 8 px pick width + existing `vector-ink::hit_stroke`). Delete/Backspace removes. Right-click → arrowheads, faint/default, label, delete. |
| **Double-click a connector** | Edit its label (text entry at midpoint). |

One gesture = one undo step (existing board convention): live drags mutate,
release journals net Add/Patch/Remove via `record`.

## Painting (`apps/slate/src/app/board.rs`)

- Tessellate the bezier via the existing path pipeline (`board_path.rs`
  `PathMeshCache`) — cache key includes both endpoint rects' relevant
  geometry (or invalidate on endpoint patch). No per-frame tessellation
  (Art. II).
- Faint = 40% stroke opacity, thinner. Selected connector: endpoint dots
  visible + standard selection tint. Arrowheads are small filled triangles
  oriented to the curve tangent at the end.
- Z-rule: connectors paint **above frames, below every other node** so a
  wire appears to connect from underneath its hosts and never laps their
  graphics. Selection/hover chrome for a selected wire still paints after
  the hosts. Click pick matches: a host under the pointer beats the wire.

## Artifact + beacon parity

- `crates/slate-artifact`: serialize as SVG `<path d=…>` with
  `marker-end`/`marker-start` triangles and `<text>` on the label midpoint.
  Faint = opacity attribute. Same derived-geometry function must be shared
  (put the bezier derivation in `slate-doc` so both interpreters call it).
- `crates/atlas-ai` context beacon: in-view connector relations
  (`from_node`, `to_node`, `label`) join the board context JSON — the canvas
  is the prompt (Art. VIII).

## Tests

- slate-doc: connector serde roundtrip; degrade-to-Free on node delete
  (command group inverts cleanly); bezier derivation is deterministic and
  side-perpendicular at both ends on unrotated boxes; `WireHost` ports
  follow a rotated rect/ellipse and an open stroke's start/mid/end.
- slate-artifact: golden SVG snippet for an anchored + a free-ended
  connector with arrowhead + label.
