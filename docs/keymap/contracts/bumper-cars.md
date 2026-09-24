# Bumper cars — interaction contract

Status: shipped
Family: tool
Reference: air hockey; opt-in collision between board objects
Command: app.optional.bumper_cars, board.bumper.push, board.shape.edit · Key: none · Palette: Bumper cars (aliases: collide, collision, puck)
Inherits: P0.* (all), P1.node.move, P1.curve.pick, P1.shape.properties — deviations flagged below.

An opt-in node property. Shapes and sticky notes that carry it push each other
when one of them is dragged, and glide after release according to their
friction with the canvas. It is off for everyone until turned on under
Preferences → Configure → Tools.

## Behavior matrix

D01–D17 are every tool-scoped dimension. D18–D35 are portal-only and do not apply.

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-----------------|--------|------|
| D01 | Initiation & arming | Not an armed tool. With the preference on, a Bumper squircle joins the shared selection strip when every selected node is compatible: shapes (rect, ellipse, polygon, closed and open paths, lines, arcs, pen and brush ink) and sticky notes (text with a fill). Images, frames, portals, wires, plain text boxes and dock strips do not show it. With the preference off the squircle never appears. | stated | 90 |
| D02 | Stickiness & repeat | The Bumper property stays on a node until turned off in its dashboard. Turning the preference off hides the squircle and suspends every collision; nodes keep their settings in the file and wake up when the preference is back on. The preference is not in repeat-last. | stated | 100 |
| D03 | Gesture grammar | Rides the existing move drag (P1.node.move); no new grammar. Each frame of a drag, contacts are solved between bumper bodies only. A node without the property passes through everything. The dragged set follows the pointer; bodies it touches are pushed out of contact and can push the next body in a chain. On release, bodies keep the speed they had and glide until friction stops them (Physics model). | stated | 85 |
| D04 | Click vs drag rule | Unchanged. `draft.drag_threshold` (4 screen px) still separates a click from a move. A click selects and never pushes anything. | pattern | 90 |
| D05 | Modifiers | Alt held mid-drag suspends collision for that frame, the same way Alt suspends snaps. Alt at press still duplicates; the copy collides once Alt is released. Shift and Ctrl keep their selection meaning. | pattern | 65 |
| D06 | Constraints & snapping | Snaps resolve first on the dragged set (grid, object snaps, smart guides). Collision resolves second: an anchor can stop the dragged set short of a snap target. Pushed and gliding bodies never snap. | guess | 65 |
| D07 | Direction / value locks | Ortho (F8) constrains the dragged set as it does today. Pushed bodies move along the contact normal, not along ortho. A locked node (Ctrl+L) with the property is always an anchor, whatever its friction. | guess | 70 |
| D08 | Numeric / manual entry | Dashboard values are edited in place like the fillet capsule: click the number, type, Enter or click away to commit, Esc to cancel. Buffer is world units, 0 to 200. Friction is 0% to 100%. | pattern | 85 |
| D09 | Preview & readouts | No buffer outline is ever drawn; the buffer is felt, not seen. The scrub readout next to the cursor follows P1.shape.properties and vanishes on release. | stated | 100 |
| D10 | Cursor | Unchanged move cursor. The squircle's tooltip reads "Bumper cars". | pattern | 85 |
| D11 | Commit | One gesture, one undo (P0.2). Release commits the dragged set and every body the collision moved, including where gliding bodies come to rest, as one journal group of rect patches authored by the human. Dashboard edits commit one patch group for the selection, as other property edits do. Changing the preference writes no journal entry. | pattern | 80 |
| D12 | Cancel | Esc mid-drag puts the dragged and pushed bodies back where they were and writes nothing. Esc in the dashboard discards its preview. Once released, Ctrl+Z is the way back. | pattern | 85 |
| D13 | Selected presentation | One Bumper squircle in the shared strip (`atlas_shell::selection_tools`), drawn filled when the property is on. It opens one fillet-height capsule holding an On / Off radio, a Buffer slider with its value, and a Friction slider labelled Puck at 0% and Anchor at 100%. A mixed selection reads Mixed until a value is set. | stated | 90 |
| D14 | Post-edit | Reopen the squircle to change values or turn it off. Copy, paste and Alt-duplicate carry the property. Members of one group move as one set and do not collide with each other. | guess | 60 |
| D15 | Non-goals | Cut from the first version: spin from impact, mass or density (every body weighs the same except anchors), gravity, attraction, motion while nobody is dragging, images / frames / portals / wires as bodies, anything in the HTML export, and File Atlas. | guess | 55 |
| D16 | Create-style inheritance | New shapes never inherit the Bumper property from the last edited style. Each node opts in. | guess | 70 |
| D17 | Hit-testing & pick | Bodies come from pick geometry, not the bounding box. A closed shape with a visible fill is a solid. A closed shape with no fill is a ring: only its stroke band collides, so a body inside it stays inside and a body outside stays out. An open curve is a wall along its stroke. A sticky note is its rotated rectangle. Every pairing collides: solid, ring and wall with each other. | stated | 90 |

## Menus

| ID | Behavior | Source | Conf |
|----|----------|--------|------|
| M1 | Preferences → Configure → Tools → Bumper cars, a checked row. "Tools" is the home for opt-in tools with a narrow audience. Off on a fresh install. A local preference in Slate's `settings.json`, not workbook data. Registered as `app.optional.bumper_cars`; no default hotkey. | stated | 90 |
| M2 | Every snap row moves into Preferences → Snaps: Grid, Snap to grid (F9), Object snaps, Smart guides, Reach Tight / Nearby / Wide, then the eight object-snap kinds End through Tangent. Same command ids (`board.grid`, `board.snap_grid`, `board.osnap.*`). Preferences keeps the two dock rows and Advanced settings. The dock's Document settings group is unchanged. | stated | 95 |
| M3 | Nested menus are a shared-chrome capability (Art. X): `atlas_shell::menubar::MenuItem` may hold child items and opens a further flyout at `submenu_width` / `submenu_gap`. File Atlas inherits it; its menus do not change. | pattern | 90 |

## Physics model

| ID | Behavior | Source | Conf |
|----|----------|--------|------|
| B1 | Effective fill = fill alpha × node opacity. At or above `bumper.solid_alpha` (5%) a closed shape is a solid; below it, a ring. A ring or wall band is half the stroke width on each side of the centerline. | stated | 80 |
| B2 | Buffer is world units added outward from the body's boundary; a ring's band grows on both sides. Contact happens when two inflated boundaries touch, so a buffer of 20 keeps bodies 20 apart even when only one has it. Default 0. | stated | 85 |
| B3 | Friction 0% is an air-hockey puck; 100% is an anchor that is never pushed and stops whatever hits it, though it can still be dragged directly. Between, friction sets how fast a moving body slows. Default 60%. A gliding body may cross the edge of the view captured at release; once its box is fully outside that rect it brakes at `bumper.escape_decel` and stops within `bumper.escape_distance` of the edge. The rect is frozen at release, so panning or zooming never causes a collision. There is no rail at the view edge. | stated | 100 |
| B4 | Throw speed is the pointer's velocity over the last `bumper.release_window`, capped at `bumper.max_speed`. At release the solver runs the glide to rest in fixed steps (`bumper.step`, at most `bumper.max_steps`) and commits the resting positions with the drag. The glide on screen replays that result and adds nothing to the file (Art. VI.3). Grabbing a body mid-replay ends the replay; the new drag starts from the committed resting position. | stated | 100 |
| B5 | One fixed restitution, `bumper.bounce`, for every contact. No per-node bounce control. | guess | 55 |
| B6 | Body extraction and the solver are the pure module `slate_doc::bumper`, with no egui (Art. I.1). Node outlines come from `slate_doc::geom`, the one owner that pick (`board_path`) and trim (`board_trim`) also call. Contact geometry between bodies is `vector_ink::collide`. The model gains `Node.bumper: Option<Bumper { buffer, friction }>`, default `None`. The artifact writer carries no output for it, like `locked`: it is behavior, not style (Art. IV). The app side (`board_bumper.rs`) feeds drags in and replays the glide. | pattern | 85 |
| B7 | A bounding-box broadphase finds pairs first. Flattened bodies are cached per node and rebuilt only when that node changes. Contact solving stops at `bumper.iterations` per frame. The release solve is budgeted at 2 ms for 200 bodies; it stops at the step cap. | pattern | 80 |

## Feel constants

| Token | Meaning | Initial value |
|-------|---------|---------------|
| `bumper.solid_alpha` | Effective fill alpha at which a closed shape is solid | 0.05 |
| `bumper.default_friction` | Friction for a node when Bumper is first turned on | 0.6 |
| `bumper.max_buffer` | Buffer slider range, world units | 200 |
| `bumper.release_window` | Pointer history used for throw speed | 80 ms |
| `bumper.max_speed` | Throw speed cap | 6000 world units / s |
| `bumper.decel` | Deceleration at 100% friction below anchor; scales linearly with friction | 12000 world units / s² |
| `bumper.escape_decel` | Deceleration once a body has left the release view | 60000 world units / s² |
| `bumper.escape_distance` | Distance past the view edge a body may travel | 40 world units |
| `bumper.step` | Fixed release-solve step | 1/120 s |
| `bumper.max_steps` | Release-solve step cap | 480 |
| `bumper.iterations` | Contact passes per frame / step | 8 |
| `bumper.bounce` | Restitution at every contact | 0.5 |

## Golden paths

1. **GP1 (push):** two filled rects with Bumper on, side by side with a gap. Drag the left one right through the right one's position. The right one moves ahead of it and they never overlap. Release; one Ctrl+Z restores both.
2. **GP2 (pass-through):** the same, with Bumper off on the right rect. The dragged rect overlaps it and nothing else moves.
3. **GP3 (ring holds):** an unfilled circle with Bumper on, a small filled rect with Bumper on inside it. Drag the rect toward the ring's stroke; it stops at the stroke and stays inside.
4. **GP4 (anchor):** a rect at 100% friction. Drag another bumper rect into it; the anchor does not move and the dragged rect stops against it.
5. **GP5 (glide):** a 0% friction rect thrown at release. Its committed position is further along the throw than the release point, and replaying the solve with the same inputs lands on the same position.
6. **GP6 (escape brake):** a 0% friction rect thrown toward the view edge stops within `bumper.escape_distance` past the edge captured at release.
7. **GP7 (dormant):** with the preference off, GP1 behaves as GP2 and the squircle is absent; the nodes' Bumper settings survive save and reopen.
8. **GP8 (Esc):** mid-drag in GP1, Esc restores both rects and adds no undo entry.

## Implementation notes

Owners: `slate_doc::geom` (node world outlines, shared with pick and trim),
`slate_doc::bumper` (property, body classes, solver, tokens),
`vector_ink::collide` (contact between two bodies; separating axes for convex
solid pairs, deepest point otherwise, bands pushed back against their motion
so a fast puck cannot tunnel a thin ring), `atlas_shell::selection_tools::bumper_editor`
(capsule), `atlas_shell::menubar` nested flyouts, `apps/slate/src/app/board_bumper.rs`
(drag hook, replay, journal). Dry-review verdict extract-first was satisfied by
moving `node_open_polyline` / `node_closed_poly` / `path_data_to_world_bez`
into `slate_doc::geom` before the bumper code consumed them.

Golden-path coverage: GP1 `a_push_never_overlaps_and_one_undo_restores_both`,
GP2 `a_node_without_bumper_is_passed_through`, GP7
`with_the_preference_off_bumpers_are_dormant` +
`the_property_survives_save_and_is_absent_when_off`, GP8
`esc_mid_push_restores_everything_and_writes_nothing` (app, headless);
GP3 `a_ring_keeps_a_puck_inside`, GP4 `an_anchor_stops_the_drag_and_does_not_move`,
GP5 `a_throw_glides_and_replays_identically`, GP6
`an_escaping_puck_stops_just_past_the_view` (slate-doc). Strip gating and
journaled edits: `bumper_squircle_needs_the_preference_and_a_compatible_selection`.
Native light/dark review of the capsule and the glide feel is still to do on
the desktop.

## Open questions

None. Momentum is solved at release. A visible fill of 5% or more is solid. With the preference off, bumper nodes are dormant. Grid moves into Snaps; the dock's Document settings group is unchanged.
