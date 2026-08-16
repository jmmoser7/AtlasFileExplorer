# Tool arming preview — interaction contract

Status: **shipped** (matrix accepted 2026-08-15 via the
`tool-arming-preview-contract` canvas; golden paths `arming_gp1`–`arming_gp6`
and `board_place` unit tests pass)
Family: tool
Reference: armed-state chrome for DragRect + PlacePoint create tools
Command: `board.tool.frame` / `board.tool.rect` / `board.tool.ellipse` /
`board.tool.text` / `board.tool.sticky` / `board.portal.repo_lens` / `board.portal.status_board` /
`board.portal.agent` / `board.portal.web` · Keys: F / R / O / T / N ·
Palette: frame, rectangle, ellipse, text, sticky, web portal, …
Inherits: P0.* (all), P1.shape.aspect, **P2.GhostFollow**, P2.DragShape,
P2.PortalPlace — deviations flagged below.

> This contract does **not** replace the placement grammars. It adds the
> missing confirmation that a create command is live after menu / palette /
> dock / hotkey arming. Click-to-place defaults, drag-to-size, and Shift
> aspect stay with the existing archetypes; they are routed through one
> shared `board_place::place_rect` so future DragRect commands reuse them.
>
> Implementation: `apps/slate/src/app/board_place.rs`. Preview and commit
> both call `place_rect`. Golden paths: `arming_gp1`–`arming_gp6` in
> `apps/slate/src/app/tests.rs`.

## Behavior matrix

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-----------------|--------|------|
| D01 | Initiation & arming | Menu, palette (typed name + aliases), dock icon, and hotkey all dispatch the same CommandId (P0.7). The instant the tool is armed — before any click — GhostFollow starts (P2.GhostFollow). The rail highlight already exists; this is the missing canvas-side confirmation that the command is live. | stated | 100 |
| D02 | Stickiness & repeat | Unchanged. DragRect tools stay one-shot (P2.DragShape.oneshot / P2.PortalPlace.oneshot). Space/Enter re-arms (P0.4). This contract does not change stickiness. | pattern | 88 |
| D03 | Gesture grammar | Armed → GhostFollow (small cursor-locked silhouette) → Press → (ClickPlace \| DragScale) → Commit. GhostFollow is chrome only and never creates a node. Click/drag commit rules stay with the existing archetype. Shared `board_place::place_rect` computes the DragScale rect for both the live rubber-band and the journaled commit. | stated | 100 |
| D04 | Click vs drag rule | Cursor travel > `draft.drag_threshold` (4 screen px) before release = DragScale; otherwise ClickPlace at the tool's default size (rect / ellipse from the kit recipe, frame from the live preset, portals from `<kind>.default_size`). A DragScale release under `MIN_DRAW` still discards. | stated | 100 |
| D05 | Modifiers | P2.GhostFollow.place. During DragScale only. GhostFollow ignores modifiers. No new Alt/Ctrl create modifiers. | stated | 100 |
| D06 | Constraints & snapping | GhostFollow is screen-space chrome and does not snap. DragScale keeps today's F9 grid + smart guides on the live rect. F8 ortho does not apply to area placement. | pattern | 80 |
| D07 | Direction / value locks | n/a for area placement. Tab does not lock during GhostFollow or DragScale. | pattern | 85 |
| D08 | Numeric / manual entry | n/a during place (Art. III). Size after commit via bbox grips / inspector only. | pattern | 80 |
| D09 | Preview & readouts | P2.GhostFollow.glyph. Frame / Rect / Ellipse / portals / Text / Sticky each have a silhouette. During DragScale the silhouette is replaced by the existing live rubber-band. No new dock readout. The glyph is pointer-attached chrome (P0.9): screen-space, no zoom coupling, no authored text. | stated | 100 |
| D10 | Cursor | P2.GhostFollow.cursor. Text / Sticky get the tint **and** a small glyph (OQ4, 2026-08-15). | stated | 100 |
| D11 | Commit | Unchanged. Existing `finish_draw` / `place_*_at` / journal cmds. The ghost never commits. One gesture = one undo (P0.2 / P0.3). | pattern | 90 |
| D12 | Cancel | Esc during GhostFollow or mid-DragScale disarms to Select, no node (P0.1 Mode layer). | pattern | 88 |
| D13 | Selected presentation | n/a — this contract does not change post-commit handles (P1.node bbox / P1.curve.grips). | pattern | 85 |
| D14 | Post-edit | n/a — existing bbox / inspector / portal bind paths unchanged. | pattern | 85 |
| D15 | Non-goals | Cut (Art. III): full-size world-space ghost at default portal / frame size; OS `.cur` files; per-tool unique OS cursor shapes; a Triangle tool (not shipped); numeric sizing during GhostFollow. | stated | 100 |
| D16 | Create-style inheritance | Unchanged. Shapes still consume `BoardLastStyle`; portals still do not (P1.portal.style). Ghost paint uses accent / portal tokens, not the last style — the silhouette is chrome. | pattern | 85 |
| D17 | Hit-testing & pick | n/a — the ghost is not hittable. Existing node pick unchanged. | pattern | 85 |

## Feel constants

| Token | Meaning | Initial value |
|-------|---------|---------------|
| `place.ghost_size` | Armed silhouette long edge (screen px) | 22 |
| `place.ghost_offset` | Silhouette offset from pointer hotspot (screen px) | 14, 14 |
| `place.ghost_alpha` | Silhouette opacity | 0.55 |
| `place.cursor_tint` | Painted pointer color | `palette.accent` |
| `draft.drag_threshold` | Click vs drag split (existing) | 4 screen px |
| `MIN_DRAW` | Discard threshold for a DragScale that never grew | 8 world |
| `rect.default_size` | Click-place rectangle | 180 × 120 world |
| `ellipse.default_size` | Click-place ellipse (circle) | 160 × 160 world |

Pinned as the named-constants block `board_place::place_tokens` (P0.6).

## Golden paths

1. GP1: Board view · type "frame" in the palette (or F, or dock Frame) → pointer tints, 22 px rounded-rect follows the cursor. No node yet.
2. GP2: Arm Web portal · move · click-release under threshold → 960×540 unbound portal centred on the click, tool = Select, ghost gone.
3. GP3: Arm Rect · press · drag past threshold · hold Shift → live rubber-band is square via `PlaceConstraint` · release → Rect node matches that rect, tool = Select.
4. GP4: Arm Ellipse · press-drag-release → ellipse sized to the constraint-resolved drag rect; ghost replaced by the rubber-band for the whole drag.
5. GP5: Arm Frame · Esc → no node, tool = Select, OS cursor restored.
6. GP6: Arm Rect (rect silhouette) · arm Ellipse without clicking → silhouette swaps to ellipse, still no node.

## Open questions

None
