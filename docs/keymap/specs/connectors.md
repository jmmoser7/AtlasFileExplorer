# Spec — connectors (wires) on the board

Stage-2 spec. Research inputs: `../research/grasshopper.md` (all), `../research/miro.md`.
Constitution: Art. IV (SVG ceiling — `<path>` + `marker`), Art. VI (journaled),
Art. VIII (connectors are machine-readable relations for the context beacon).

## Model (`crates/slate-doc/src/scene.rs`)

```rust
pub enum ConnectorEnd {
    /// Anchored to a node's edge: `side` ∈ {Top,Right,Bottom,Left},
    /// `t` ∈ 0..=1 along that side (0.5 = midpoint default).
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
  computed at paint/export time from the *current* rects of anchored nodes:
  a cubic bezier leaving each anchored end perpendicular to its side, with
  handle length `clamp(0.35 * distance, 24.0, 160.0)` world units (tune).
  `Node.rect` for a connector is its recomputed AABB (kept fresh whenever an
  endpoint node patches) so marquee/hit systems keep working.
- Connectors ignore frame membership and never become slides.
- Deleting a node deletes connectors anchored *only* to it? **No** — the
  anchored end degrades to `Free` at its last world position (journaled as
  part of the same command group, so undo restores the anchor). This keeps
  delete invertible and simple.
- `SceneCmd` is unchanged — Add/Remove/Patch cover everything.

## Grips (the interaction affordance)

- With the **Select tool**, hovering a non-connector node within ~8 px of
  its edge reveals **4 side grips** (small circles at side midpoints).
  Hover a grip: enlarge + highlight. Snap radius while dragging a wire:
  **14 px screen space** to a grip; anywhere on a node's edge snaps to the
  nearest side with `t` = projected fraction.
- A dedicated **Connector tool** is *not* added in P1 — grips-from-Select
  matches Miro and avoids another mode. (Palette entry "Connect…" can arm a
  one-shot wire from the selected node's nearest side.)

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
- Z-rule (one global rule per research §5): connectors paint **above frames
  and fills, below selected-node handles**; within nodes they respect vec
  z-order like everything else.

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
  side-perpendicular at both ends.
- slate-artifact: golden SVG snippet for an anchored + a free-ended
  connector with arrowhead + label.
