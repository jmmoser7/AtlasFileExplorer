# Regular polygon — interaction contract

Status: **agreed** (user approved D07 on 2026-09-25; every row decided)
Family: tool
Reference: [Shapes research](../research/shapes-selection-2026-09-17.md), 2026-09-25
Command: board.tool.polygon · Key: Y · Palette: Shapes flyout (polygon)
Inherits: P0.*, P1.node, P1.shape, P2.DragShape, P2.GhostFollow

Implementation authorized by the user on 2026-09-25. Shared behavior follows P1.shape.properties. Model: `ShapeKind::RegularPolygon` with `sides` (3–12, default 6) inscribed in the drag box, turned by `phase_deg` (0 = first vertex at top center; serialized only when turned); fillet uses the shared Corners stringer.

## Behavior matrix

D01–D17 are every tool-scoped dimension. D18–D35 are portal-only and do not apply. Shared toolbar rules have one owner: P1.shape.properties in PATTERNS.md. Per-geometry capabilities remain in this contract.

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-------------------|--------|------|
| D01 | Initiation & arming | Arm through `board.tool.polygon` or the Shapes flyout; **Y** is the primary chord. | stated | 100 |
| D02 | Stickiness & repeat | One-shot creation; return to Select. Repeat follows P0.4. | pattern | 85 |
| D03 | Gesture grammar | P2.DragShape: click places the kit default hexagon; press-drag-release sizes the inscribed polygon. | stated | 100 |
| D04 | Click vs drag rule | P2.DragShape: use raw press-to-release travel in screen pixels, independent of snap displacement. | pattern | 85 |
| D05 | Modifiers | Shift locks equal aspect (regular polygon in a square box, like square/circle). Ctrl during DragScale draws from the press point as center. Later resize is P1.node.transform, including Alt-at-press scale copies. | stated | 100 |
| D06 | Constraints & snapping | P1.node.osnap: one resolved point feeds preview and commit; Alt suspends, ortho/direction constraints retain priority. | pattern | 85 |
| D07 | Direction / value locks | Percentage corner intent persists through resize; absolute values retain the authored distance and clamp only their effective value. Mode conversion is per host and preserves the visible amount. | stated | 100 |
| D08 | Numeric / manual entry | Local width and height stringers on the bounding box. No dimensions in the toolbar. Side count is not typed: it changes through the vertex +/− (D13, D14), which replaces the Corners-panel **Sides** integer (stated 26 September 2026; the integer was removed, not repaired — it painted below the panel's reserved height, so it could not be clicked). | stated | 100 |
| D09 | Preview & readouts | Armed: a small polygon GhostFollow glyph beside the pointer (P2.GhostFollow.glyph). Drag: the preview is the regular polygon itself (first vertex top center), not its bounding box. Then the shared P1.shape.properties selection strip. | stated | 100 |
| D10 | Cursor | Crosshair while armed over the board (stated 2026-09-25). Controls use shell hover and focus feedback. | stated | 100 |
| D11 | Commit | P1.shape.properties: one accepted property editor or dimension value creates one invertible journal group; no-op edits add no history. | pattern | 85 |
| D12 | Cancel | P1.shape.properties: Esc cancels the pending property edit and preserves selection. Existing creation cancellation remains unchanged. | pattern | 85 |
| D13 | Selected presentation | P1.shape.properties: squircle Fill / Stroke / Corners controls above the selection, gated by geometry; dimensions use separate exterior stringers. **P1.node.corner-grip** on every selected polygon (multi-selection: one drag or typed amount sets every selected host, clamped per host — stated 26 September 2026): the grip rides the side from the first vertex toward the next vertex clockwise, at the treatment's tangent point; drag along that side, click to type. **Vertex +/− (stated 26 September 2026):** hovering a vertex of a selected polygon shows a **+** outside it and a **−** inside it, on that vertex's bisector, at the hovered vertex only; both glyphs scale with the canvas (P0.9: `canvas_scale::px` radius 6 and offset 14 world units). Proposals: offered on every selected, unlocked polygon (multi-selection included) but drawn only at the one hovered vertex; hidden during any drag, in crop mode, and where portal chrome suppresses the cast; dropped (LOD) when the glyph radius is under 4 screen px; a glyph at its limit is omitted (no + at 12, no − at 3); the vertex hover zone is `hit_px(10)` and each glyph `hit_px(6)`; the corner grip wins where they overlap, and the glyph wins over the resize band only on the glyph itself, where the cursor is a pointing hand; a quick second click on a glyph is not a canvas double-click. **Vertex picks (user, 28 September 2026: "alow for vrtex selection on closed forms like sqares and polygons"):** a click without travel on a vertex of the one selected polygon picks it (P1.shape.vertex-style); Shift+click adds or removes a pick; under Direct Select (A) the vertices are anchors to pick, never to move. Proposals: the vertex hit is the nearest vertex within 7 screen px (screen-constant like path-edit handles); a click elsewhere clears the picks; a quick second click on a vertex is a pick, not a canvas double-click; the vertex +/− and the pick share the vertex, so each glyph wins on its own hit disc (its press is taken at press time, so it never picks) and a click on the vertex outside both discs picks it; the picked vertices paint filled and the vertex under the pointer hollow, with the glyphs beside it. | stated | 100 |
| D14 | Post-edit | Fill, Stroke, Corners (fillet/chamfer like rectangles). A fillet amount is the arc radius at every vertex and a chamfer is a straight cut, both clamped per vertex so the outline stays inside the drag box with no spikes on the board or in the export. **Sides (stated 26 September 2026; + amended 8 October 2026, user-ratified):** clicking **+** adds a side (max 12) and keeps the clicked vertex at its angle while the other vertices re-space around it; clicking **−** removes a side (min 3) and keeps a vertex at the removed vertex's position; each is **one** journaled `board.shape.sides` patch (undo restores both count and turn). Proposals: the turn is stored as `ShapeNode.phase_deg`, so the drag box, fillet, and stroke are unchanged and vertices stay on the box's inscribed ellipse; vertex 0 remains the corner-grip anchor, so the grip may move to another side after a step; `Property::RegularSides` stays as the command-parity setter for agents. **Per-vertex properties (user, 28 September 2026: "for squrcle menue allow for vertex by vertex adjustment of forms properties"):** with vertices picked, the tip HUD (Alt width, Ctrl color, Shift opacity) and the strip's stroke width, stroke color, opacity and Corners amount apply to the picked vertices only, blending around the closed loop (P1.curve.vertex-style); each edit is one journaled patch. The polygon stays regular: moving one vertex would make it another kind, so vertex moves are not offered. Proposal: a side step carries each vertex's style to the nearest new vertex by angle. | stated | 100 |
| D15 | Non-goals | No star or irregular polygon in this tool. Picked vertices take per-vertex stroke and corner edits (D14; user, 28 September 2026: "appy similar tooling for closed form vertex selectin", superseding the v1 exclusion of per-vertex strip editing). No per-vertex fill: a fill blended from vertex to vertex across the interior (user, 28 September 2026: "including fill which would create gradients acrost he forms interiors") is not SVG-expressible (Art. IV); see Open questions. | stated | 100 |
| D16 | Create-style inheritance | **Closed-form** create-style memory (P1.shape.style): inherit last closed-shape stroke/fill; open curves do not overwrite polygon defaults. | stated | 100 |
| D17 | Hit-testing & pick | Fill uses the filleted polygon outline; stroke uses shared path pick where applicable. Controls consume input before canvas gestures. | pattern | 85 |

## Geometry capabilities

Fill, Stroke, Corners (fillet/chamfer), and Sides (3–12, stepped at a vertex by `slate_doc::scene::regular_polygon_resided`). Board painter and HTML artifact both derive outline from `regular_polygon_vertices(rect, sides, phase_deg)` + shared vertex-corner path (`slate-doc::geom`, `slate_doc::wire::filleted_vertex_path`). A fillet is a true circular arc of the authored radius, tangent at `radius × tan(turn / 2)` from each vertex; a chamfer cuts the authored distance along each edge. Both clamp to half the shorter adjacent side, and the outline never revisits a point, so a stroke cannot miter-spike.

## Feel constants

- Existing `draft.drag_threshold`: 4 screen px.
- `selection_toolbar.gap`: 14 world units, scaled via canvas_scale (P0.9).
- Default kit size: 160×160 world units; default sides: 6.

## Golden paths

- **GP1:** Y → drag a box → hexagon inscribed; Shift → equal aspect box.
- **GP2:** Select polygon → Corners → fillet 12 → board and export match.
- **GP3:** Select a hexagon → hover a vertex → + outside, − inside → click + → a heptagon whose clicked vertex stays where it was, the new edge on the far side; undo → the hexagon returns. Click − at a vertex → a pentagon with a vertex still there. At 12 sides only − shows; at 3 only +.
- **GP4:** Arm Y → a small polygon follows the pointer → drag → the preview is the polygon, not its box.
- **GP5:** Select polygon → drag the corner grip along the upper-right side → every vertex rounds live; release is one undo step. Click the grip → type 20 → Enter.
- **GP6:** Triangle → fillet or chamfer 10 000 → the outline stays inside the box with no spikes, on the board and in the export.

## Implementation notes

Kit: `core.slatekit` tool `polygon`, grammar `drag_rect`, node `regular_polygon`. Registry command `board.tool.polygon`. Icon: `Icon::Polygon` in atlas-shell tool set.

## Open questions

None blocking the agreed rows. One question awaits the user outside the matrix:

- **Per-vertex fill (Art. IV conflict, 28 September 2026).** The user asked for a fill "which would create gradients acrost he forms interiors". A fill blended from every vertex across the interior is a mesh gradient. SVG has no mesh gradient (SVG 2 dropped `meshgradient` and browsers never shipped it), so the HTML artifact could not paint what the board paints, breaking the SVG ceiling both interpreters hold (Art. IV). Approximating it with many tinted triangles would make the export an imitation, not a serialization. Not implemented. **Conforming alternative (proposal, awaiting the user):** a linear or radial gradient fill whose stops are anchored to vertices the user picks. Two picked vertices set a `linearGradient` axis from one to the other; one picked vertex and the centroid set a `radialGradient`. The export writes it with `gradientUnits="userSpaceOnUse"` and the board paints the same stop math. It needs a new fill style on the model, which lands in both interpreters or not at all. The other route is a ratified amendment to Article IV that admits mesh fills with a named export fallback.
