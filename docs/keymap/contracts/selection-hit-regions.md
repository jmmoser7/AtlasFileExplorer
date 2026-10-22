# Selection hit regions — interaction contract

Status: agreed
Family: tool
Reference: Selection hit-zone studio, Adaptive regions / foreground-priority
iteration approved by the user on 20 October 2026: “i think you got it lock
this in and apply to the full project then build and push to github”.
Command: existing `board.tool.select` and registered move, resize, rotate,
corner, crop, and wire commands · Key: existing Select binding · Palette:
existing Select / editing entries; no new command or binding.
Inherits: P0.* (all), P1.node, P1.wire, P1.curve.grips, P1.curve.pick,
P1.shape.properties — deviations flagged below.

The user accepted the interactive sandbox as the design decision. A second
proposal matrix is unnecessary. Approval authorizes native implementation;
it is not evidence that the native implementation has compiled or passed UI
acceptance. The shared policy and feel constants live once in
[P1.node.hit-regions](PATTERNS.md), not in per-kind contracts.

## Behavior matrix

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-----------------|--------|------|
| D01 | Initiation & arming | Existing Select activation and P0.7/P0.8. P1.node.transform retains hover entry; this contract adds no tool, binding, or palette alias. | pattern | 90 |
| D02 | Stickiness & repeat | Existing Select/crop modes and P0.4. Region relevance is derived per view and never re-arms or disarms an editing tool. | pattern | 90 |
| D03 | Gesture grammar | P1.node.hit-regions supplies the hover/press resolver; P0.1–P0.3 and the existing operation own press → drag → release/cancel. Crossing fields, layers, or relevance thresholds cannot retarget an active gesture. | stated | 100 |
| D04 | Click vs drag rule | Keep each existing editing command's travel threshold and click meaning. The resolver uses the original press; a later drag-start event cannot pick a different region. | precedent | 95 |
| D05 | Modifiers | Inherit P1.node.select, P1.node.transform, P1.curve.grips, and the active crop/wire command. No new modifier; Shift selection and Shift-drag must keep their existing distinct meanings. | pattern | 90 |
| D06 | Constraints & snapping | Inherit P1.node.move/osnap/transform and the active command's contract. Region expansion chooses a gesture owner only; resolved authored points still pass through the existing snap owner for preview and commit. | pattern | 90 |
| D07 | Direction / value locks | Inherit the active editing command. Region arbitration introduces no Tab lock or parameter lock. | pattern | 90 |
| D08 | Numeric / manual entry | Inherit P1.shape.properties and existing corner/dimension entry. The compact frame adds no numeric-entry surface and cannot take typing from a focused editor. | pattern | 90 |
| D09 | Preview & readouts | P1.node.hit-regions. Any region visualization and hover cursor consume the same winner as press. deviates P0.9: only the ephemeral editing frame and pointer hit regions use the named selection-control exception; authored paint and property/stringer layout retain P0.9. | stated | 100 |
| D10 | Cursor | Inherit P1.node.transform and the active grip/crop/wire cursor. Cursor ownership follows the resolved region, including host layer and adjacency arbitration in P1.node.hit-regions. | stated | 100 |
| D11 | Commit | Inherit P0.2/P0.3 and the original editing command's journal path. Hit-region layout changes no scene data; one completed manipulation is one undo step, and no-op gestures add none. | pattern | 90 |
| D12 | Cancel | Inherit P0.1. Esc restores all press-time authored state without a commit, retains the selection/mode as the existing command specifies, and clears the held-region capture. | pattern | 90 |
| D13 | Selected presentation | P1.node.hit-regions owns relevance and compact/buried control frames. P1.curve.grips/pick, explicit vertex/side controls, P1.shape.properties, and portal nonrotation/capability gating remain authoritative. | stated | 100 |
| D14 | Post-edit | Each later gesture resolves from the current camera, geometry, layer order, and pointer under P1.node.hit-regions. Existing authored edit and undo/redo paths remain the sole mutation owners. | stated | 100 |
| D15 | Non-goals | No reserve-point grid, new rotation capability, curve bounding-box editing, crop-mask redesign, changed property-panel layout, authored-scene scaling exception, or portal-internal input redesign. Native and browser evidence remain distinct. | stated | 100 |
| D16 | Create-style inheritance | n/a. Selection-region derivation neither consumes nor changes creation-style memory. The chosen editing command retains its existing property behavior. | pattern | 90 |
| D17 | Hit-testing & pick | P1.node.hit-regions is the one shared policy for continuous fields, projected-size relevance, cursor expansion, overlap priority, and adjacent-corner ties. P1.node.select and P1.curve.pick retain actual shape/stroke picking; eligible wire geometry/identity remains P1.wire.ports/flow-datum. | stated | 100 |

## Feel constants

The named values are owned by **P1.node.hit-regions** and implemented in
`atlas-shell::selection_tools::hit_regions`. Consumers reference that owner;
this contract does not maintain a second set of numbers. Existing per-command
drag thresholds, snapping tolerances, crop limits, and direct-edit pick radii
remain with their original contracts.

The named P0.9 exception includes transient wire-port dots, their matching host
cutouts, and port-role labels. They follow the Wire domain's projected-size
prominence and bounded screen presentation. Their native `WireHost` sites and
identities are unchanged. Authored labels, content paint, and tool blisters
retain their existing camera scaling; these editing overlays are not exported.
Transient fillet-grip squares likewise use Resize prominence and bounded screen
paint; authored corner radii continue to follow the camera.

## Golden paths

These are required native acceptance paths. Prototype results are retained
as design evidence only. Exercise press ownership, cancellation, and one undo
for every mutating path below.

1. **GP1 (scale ladder):** place 240×112, 96×48, 24×18, and 1×1 rectangular
   samples. Zoom out and back in around each. Compare relevance against
   P1.node.hit-regions at domain boundaries and intermediate sizes; equivalent
   projected sizes must have equivalent fields. At the far end, move remains
   available and dormant fine regions take no input. Zoom the 1×1 host in
   until eligible wire/resize/rotation actions return. Reach the full approved
   range through wheel and camera commands, rather than forcing a test camera;
   save/reopen must retain point-detail zoom. A stationary first resize press
   cannot enlarge an existing object to the creation minimum. At overview zoom,
   stroke/connector picks and smart-guide distances retain screen-pixel reach.
2. **GP2 (continuous fields):** on a large selected rectangle, press an edge
   away from its midpoint and drag. Then rotate from several positions along
   the exterior corner arc. Hover/press must agree, without isolated
   midpoint-only hit spots. Approach a field from outside and check the
   cursor-proximity reach defined by the shared policy.
3. **GP3 (foreground invariance):** capture a front rectangle's move, resize,
   rotation, and fillet winners alone. Place another host behind it with
   positive-area overlap, including under its exterior corner controls. The
   same probes must retain the front winner and the same front field geometry.
   Reverse front order and repeat. Dragging over a covered rear corner must
   not steal the foreground operation.
4. **GP4 (overlap wire interior):** overlap two wire-capable hosts. Approach a
   front port from just outside and just inside its actual edge. The outside
   probe must not start its wire; the inside probe can. Repeat on a compact
   front host and at a rounded/rotated edge. Existing wire identities and
   normal-host flow datum must survive routing, undo, save/reopen, and export.
5. **GP5 (adjacent corner):** make two host corners exactly coincide without
   positive-area overlap. Probe on both sides of their shared corner, then at
   exact equal silhouette distance. Check the policy's host choice, reverse
   front order, and repeat at multiple zooms with rotated and rounded hosts.
   A barely overlapping pair must follow the overlap rule instead.
6. **GP6 (buried selection):** fully cover a rear object. It must not expose
   floating reserve dots over the front object. Explicitly select the rear
   object and move it using its outside frame's clear move affordance.
   Foreground transforms retain priority. Report any genuinely covered
   regions as covered; do not convert them to a reachability pass.
7. **GP7 (held owner):** start a resize/rotate/fillet/wire gesture, then cross
   other fields, another host, and a relevance boundary. The original press
   owner remains stable. Escape restores the start; a completed gesture
   undoes in one step. Shift-click toggles selection and Shift-drag preserves
   the intended selected group.
8. **GP8 (native boundaries):** exercise Select and Direct Select curve grips,
   Ctrl+Shift segment picking, rectangle/polygon vertex controls, crop-body
   pan and crop edges, a property popup, grouped movement, and a nonrotatable
   portal. Their existing contracts must still pass; empty shape-AABB corners
   cannot gain unrelated body picks, and portals cannot gain rotation.
9. **GP9 (paint and persistence):** zoom each sample with wheel and camera
   commands. Authored shapes, text, labels, tabs, strokes, and image paint
   continue to scale under P0.9. Save/reopen and export contain no compact
   control frame or hit-region state. Wheel ownership over menus and
   portal-local surfaces remains P0.5/P0.10 and the portal contract.

## Implementation reuse

Owner: `atlas-shell::selection_tools::hit_regions` for the shared policy;
`apps/slate/src/app/board_handles` adapts eligible native controls;
`apps/slate/src/app/board_transform/hover.rs` resolves competing hosts using
the cached document-aware `slate_doc::geom::OutlineQueries` geometry owner.
Port resolution stays in `slate-doc::WireHost`, shared by native paint,
picking, routing, and artifact serialization.

Forbidden forks: importing the JavaScript prototype into native Slate,
per-portal copies of the hit policy, independent hover/press policies, or
changing authored geometry to make an editing control easier to hit.

## Validation state

Design approval is complete. The approved sandbox's recorded 58 checks and
browser interactions establish design evidence, not native resolution.
This contract remains **agreed** until the final native build compiles,
golden-path tests pass, existing registry command coverage is verified, and
computer-use evaluation demonstrates the final build's look and behavior.
No original native report is closed by this document alone.

## Open questions

None. Implementation/validation work is required; the agreed design does not
await another user approval.
