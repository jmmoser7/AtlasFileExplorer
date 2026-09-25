# Regular polygon — interaction contract

Status: draft
Family: tool
Reference: [Shapes research](../research/shapes-selection-2026-09-17.md), 2026-09-25
Command: board.tool.polygon · Key: Y · Palette: Shapes flyout (polygon)
Inherits: P0.*, P1.node, P1.shape, P2.DragShape, P2.GhostFollow

Implementation authorized by the user on 2026-09-25. Shared behavior follows P1.shape.properties. Model: `ShapeKind::RegularPolygon` with `sides` (3–12, default 6) inscribed in the drag box; fillet uses the shared Corners stringer.

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
| D08 | Numeric / manual entry | Local width and height stringers on the bounding box; **Sides** (3–12) in the Corners panel when a regular polygon is selected. No dimensions in the toolbar. | stated | 100 |
| D09 | Preview & readouts | P1.shape.properties: preview resolved creation geometry and show the shared geometry-appropriate selection strip. | pattern | 85 |
| D10 | Cursor | Retain existing armed-tool cursor; controls use shell hover and focus feedback. | pattern | 85 |
| D11 | Commit | P1.shape.properties: one accepted property editor or dimension value creates one invertible journal group; no-op edits add no history. | pattern | 85 |
| D12 | Cancel | P1.shape.properties: Esc cancels the pending property edit and preserves selection. Existing creation cancellation remains unchanged. | pattern | 85 |
| D13 | Selected presentation | P1.shape.properties: squircle Fill / Stroke / Corners controls above the selection, gated by geometry; dimensions use separate exterior stringers. | pattern | 85 |
| D14 | Post-edit | Fill, Stroke, Corners (fillet/chamfer like rectangles). Corners panel also exposes **Sides** for regular polygons only. | stated | 100 |
| D15 | Non-goals | P1.shape.properties: no independent per-vertex editing in the strip; no star/irregular polygon in this tool. | stated | 100 |
| D16 | Create-style inheritance | **Closed-form** create-style memory (P1.shape.style): inherit last closed-shape stroke/fill; open curves do not overwrite polygon defaults. | stated | 100 |
| D17 | Hit-testing & pick | Fill uses the filleted polygon outline; stroke uses shared path pick where applicable. Controls consume input before canvas gestures. | pattern | 85 |

## Geometry capabilities

Fill, Stroke, Corners (fillet/chamfer), and Sides (3–12). Board painter and HTML artifact both derive outline from `regular_polygon_vertices` + shared fillet path (`slate-doc::geom`).

## Feel constants

- Existing `draft.drag_threshold`: 4 screen px.
- `selection_toolbar.gap`: 14 world units, scaled via canvas_scale (P0.9).
- Default kit size: 160×160 world units; default sides: 6.

## Golden paths

- **GP1:** Y → drag a box → hexagon inscribed; Shift → equal aspect box.
- **GP2:** Select polygon → Corners → fillet 12 → board and export match.
- **GP3:** Select polygon → Corners → Sides 8 → geometry updates; undo restores.

## Implementation notes

Kit: `core.slatekit` tool `polygon`, grammar `drag_rect`, node `regular_polygon`. Registry command `board.tool.polygon`. Icon: `Icon::Polygon` in atlas-shell tool set.

## Open questions

None for v1.
