# Application connector on a model portal - interaction contract

Status: agreed
Family: portal
Reference: User-directed Rhino.Inside build and autonomous design, 25 October 2026. Explicit requested behaviors are pre-accepted; remaining behavior inherits the existing host, media and source contracts under that implementation authorization.
Command: `board.model.connector.activate`, `board.model.connector.deactivate`, `board.model.connector.capture`, `board.model.connector.restore_view`, `board.model.connector.invoke` - Key: none - Palette: Rhino.Inside
Inherits: P0.*, P1.node, P1.portal, P2.PortalHost.

Native acceptance is pending. This contract is not marked shipped by compilation alone.

## Behavior matrix

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-----------------|--------|------|
| D01 | Initiation & arming | Explicit Rhino blister activation on an eligible linked 3dm starts the selected registered connector; ordinary loading retains the existing Slate model preview. | stated | 100 |
| D02 | Stickiness & repeat | Activation is session-local for this model. Reusing the same blister returns to Slate preview while retaining native documents and unsaved changes. | stated | 100 |
| D03 | Gesture grammar | P2.PortalHost lifecycle and contents-focus govern entry and exit; runtime states distinguish opening, active, parked, unavailable, and failed. | pattern | 90 |
| D04 | Click vs drag rule | P1.node select/transform and P1.portal contents interaction retain the existing press and drag thresholds. | pattern | 90 |
| D05 | Modifiers | P0 modifiers remain unchanged; Rhino receives native modifiers only while its contents own interaction. | pattern | 90 |
| D06 | Constraints & snapping | Board placement uses the shared snap owner. Rhino modeling constraints and view snaps belong to Rhino inside its portal. | pattern | 90 |
| D07 | Direction / value locks | Native Rhino direction and value locks are handled by the foreign application while contents are focused. | pattern | 90 |
| D08 | Numeric / manual entry | Native Rhino command entry remains inside its native UI; the board adds no duplicate Rhino numeric editor. | pattern | 90 |
| D09 | Preview & readouts | Retain the existing preview while opening; display the actual registration, connection, runtime, or source failure. Never paint a mock native editing session. | pattern | 90 |
| D10 | Cursor | P1.portal contents-focus owns input handoff; outside contents use ordinary Slate cursors and hit rules. | pattern | 90 |
| D11 | Commit | Linked captures and board geometry changes use named SceneCmd transactions. Rhino owns native geometry edits and native undo; connection state is ephemeral. | pattern | 90 |
| D12 | Cancel | P0 Esc layering releases foreign contents focus before leaving maximize. Detach preserves the native document; disconnection never grants save, discard, or process termination. | pattern | 90 |
| D13 | Selected presentation | P1.node selection and existing image-backed model properties remain available around the hosted contents. | pattern | 90 |
| D14 | Post-edit | Return to either preview or native mode from the same source. Park other model sessions before attaching the selected native surface. | pattern | 90 |
| D15 | Non-goals | No arbitrary-code endpoint, workbook executable paths, automatic original-file write-back, or claim that Rhino runs inside HTML export. | pattern | 90 |
| D16 | Create-style inheritance | Retain the existing image/model authored styling owners. Native capture records what Rhino reports with its own view schema and consistency. | pattern | 90 |
| D17 | Hit-testing & pick | P1.node and P1.portal shared body/border hit ownership; reserve the Slate control apron outside the native surface. | pattern | 90 |
| D18 | Portal class & authority | Host-class surface attached to an existing image-backed model node. Rhino owns native edits; Slate owns the frame and linked output journal. | pattern | 90 |
| D19 | Source binding | Resolve the linked model through board_portal source resolution and bind its stable source/revision only after explicit activation. Launch roots come from machine registration. | pattern | 90 |
| D20 | Query & parameters | Slate preview camera remains in its existing model. Native view data is namespaced and versioned; native handles, PIDs and process state never enter the workbook. | pattern | 90 |
| D21 | Regeneration & staleness | Source rebind, deletion, closure and generation change invalidate old replies and captures. Preserve dirty native work rather than silently rebinding it. | pattern | 90 |
| D22 | Contents interaction | The Rhino blister changes the current model from Slate preview to the actual declared native Rhino surface and back. Native modeling works inside the focused contents. | stated | 100 |
| D23 | Level of detail | Use the shared host shell and existing model LOD. A parked or unavailable native surface leaves a useful preview/poster. | pattern | 90 |
| D24 | Export serialization | Export the ordinary poster and source pointer. A native foreign editor is unavailable in exported HTML and is identified honestly. | pattern | 90 |
| D25 | Bake | Capture uses the shared output/publication path to add one linked still beside the source without replacing the model. | pattern | 90 |
| D26 | Collaboration & per-peer | Board frame and authored linked assets follow existing collaboration. Each machine resolves its own connector, native process and license; live foreign contents are per-peer. | pattern | 90 |
| D27 | Agent surface | Registered activation, deactivation, capture, view restore and invocation share human/agent dispatch. v0.2 agent draft edits require an explicit source/document/command grant; linked originals remain human-only. | pattern | 90 |
| D28 | Determinism & provenance | Capture retains source revision, native schema, projection/camera state, dimensions, timestamp and actual consistency. Foreign app state is not claimed deterministic. | pattern | 90 |
| D29 | Performance envelope | Launch, IPC, hashing, artifact I/O and native readback run on bounded workers; stale generations are discarded. Native window geometry stays on its required UI thread. | pattern | 90 |
| D30 | Failure & honesty states | Missing connector/runtime/license, failed authentication, wrong source/revision, unsupported schema, dirty rebind, lost window and uncertain outcomes are surfaced distinctly without fabricating success. | pattern | 90 |
| D31 | View-state ownership | The source locator and board frame are durable authored intent. Native runtime and per-peer view are ephemeral; captured view provenance is durable linked-asset metadata. | pattern | 90 |
| D32 | Trust, sandbox & consent | Machine registration selects the executable and approved roots. Authenticated same-user pipes pin PID/session/creation time; no workbook or connector message grants launch or mutation authority. | pattern | 90 |
| D33 | Portal chrome | Retain the bottom blister and relevant Slate meta controls outside the native body; use atlas-shell layout, icons and scale owners. | stated | 100 |
| D34 | Portal maximize | Use the existing shared portal maximize shell extended to eligible model images. Preserve the native surface and blister, resize with DPI-aware physical geometry, and use existing Esc peeling. | stated | 100 |
| D35 | Portal-local UI | Rhino-specific controls stay on this model portal; no per-kind controls are added to board-wide Document Settings. | stated | 100 |

## Golden paths

1. GP1 - Load a 3dm: ordinary Slate preview works without launching Rhino. Activate Rhino from the model blister: bind the selected model and enter its declared surface.
2. GP2 - Maximize and restore: shared geometry resizes the native window and retains the Slate control apron; Esc releases focus and then maximize.
3. GP3 - Capture and drop back: one linked still carries native view metadata; dropping it on the matching model restores that view. Wrong source, stale revision and unsupported schema are refused explicitly.
4. GP4 - Switch models and return: the prior document remains available with unsaved changes; only the chosen model owns the active embedded native surface.
5. GP5 - Missing or lost connector: retain the useful preview/poster and source link; report the failure and never launch on workbook open/export.
6. GP6 - Agent modeling: deny linked-original mutation; deny an isolated-draft edit without a host grant; allow only the granted source/document/command set and reject after revocation.

## Implementation reuse

Owner: atlas-connector wire/policy/native view metadata; atlas-connector-runtime transport/registration/NativeSurface; atlas-packs installation offers; board_portal and board_portal_chrome focus/fullscreen/source shell; board_portal/capture output; atlas-shell selection controls.

Required extraction: shared host focus and maximize eligibility support the existing image-backed model before adding the connector adapter. Foreign still completion enters shared capture output. Rhino SDK code stays in the independent sibling project.

Forbidden forks: no copied web/Atlas portal, second locator resolver, bespoke screenshot writer, parallel focus prelude or second fullscreen interpreter. DRY gate verdict: extract-first, with the listed owner extensions preceding integration.

## Open questions

None for implementation under the stated autonomous build. Native embedding, focus/dialog behavior, multi-model switching and film capture remain validation gates, not assumed results.
