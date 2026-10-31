# Rhino 3D agent interaction contract

Status: v0.1 draft for review, 30 October 2026
Family: model viewport and agent portal
Command: Studio tab only; no production command or Rhino request is executed
Inherits: Studio S01–S08 where applicable; Dashboard Builder activity pattern
Owners: shared Studio runtime for agent chrome and history; Rhino preview for model content

## What it is

A single 3D model canvas with the same agent picker, dialogue, history and activity controls as the Dashboard Builder. The user asks for geometry, inspects a model, and follows public execution progress and the generated Rhino Python script. The first specimen is a 12 × 8 meter timber pavilion with columns, beams and a slatted roof.

This is an HTML/CSS/JavaScript design specimen. The displayed model is projected SVG geometry. It neither launches Rhino nor claims an active Rhino connection. Running the agent animates a preview revision only.

## Preserved references

- `apps/slate/src/app/model3d.rs`: perspective navigation, left-drag orbit while editing a viewport, Shift-drag pan, model display modes, fit and model camera ownership. The native orbit sensitivity is 0.008 radians per screen pixel. The specimen mirrors that direction and sensitivity but uses orthographic projected geometry.
- `crates/atlas-packs/packs/rhino-inside.json`: the independently installed Rhino.Inside connector exposes lifecycle, sources, surfaces, commands, views and capture. This prototype does not change its installation, license or authorization requirements.
- `apps/slate/src/app/board_agent/header_menu.rs`: agent access, layout, history and scheduling remain agent concerns, not local model viewport widgets.
- `contracts/PATTERNS.md`: shared compact controls, model capability choices, zero-gutter scrolling, active feedback and one element per Studio tab.

## Requested principles and proposed defaults

| ID | Requirement | First-pass behavior | Status |
|---|---|---|---|
| R01 | Separate Rhino Studio | Rhino 3D has its own canvas and draft debrief | Requested |
| R02 | Dashboard parity | Shared runtime owns picker, conversation, tools, progress, scripts and history | Requested |
| R03 | Visible work | Inspect model → Prepare geometry → Write Rhino script → Check geometry → Update preview | Requested principle; phase labels proposed |
| R04 | Rhino API modeling | Generated Python uses RhinoScriptSyntax and the document undo API | Requested direction; execution deferred |
| R05 | Model navigation | Focus viewport; left-drag orbit, Shift-drag pan, wheel zoom | Native precedent |
| R06 | Display controls | Perspective, Top and Front; Shaded or Wireframe; Fit | First-pass proposal |
| R07 | Selection | Click a solid to highlight it and name it in the viewport status | First-pass proposal |
| R08 | Canvas navigation | Right-drag remains owned by the outer Slate canvas; Ctrl+wheel remains canvas zoom | Shared requirement |

No first-pass appearance is declared pixel-perfect or locked. Only the user-approved shared principles are inherited as approved; this new model surface is open for review.

## Behavior matrix

| Dimension | Behavior | Source |
|---|---|---|
| Initiation | Open Rhino 3D tab; existing specimen appears | Requested |
| Configuration | Provider, model, effort, speed, credentials and access use shared Studio controls | S01, S02, S06 |
| Selection | Local model object name is retained in `node.data.selected` and rendered with a teal edge | Proposed |
| Navigation | Viewport takes primary pointer interaction; outer canvas keeps right/middle/Space navigation | Native precedent, S08 |
| Generation | Shared Run/Stop sequence emits public status and script text; last phase updates the specimen revision | Requested, S03 |
| Cancellation | Shared Stop cancels the preview run; no partial native objects exist because no external API ran | Prototype limit |
| Dialogue | One-window and history-train behavior is supplied by shared runtime; model content owns no separate chat implementation | Requested parity |
| Persistence | Camera, display, selected solid, view and model variant are plain JSON under `node.data` | Proposed |
| Errors | Future Rhino errors belong in public tool output; preserve last good model and expose the failed step | Native integration requirement |
| Access | Studio controls are illustrative; no native permissions are granted here | Prototype limit |
| Dark mode | Model stage, grid and local chrome inherit the Studio theme | Shared precedent |

## Feel constants

| Token | Value | Meaning |
|---|---|---|
| Specimen frame | 760 × 420 world units | Default card size |
| Preview header | 32 units | File and display controls |
| Preview footer | 26 units | Selection and units |
| Model base size | 12 × 8 meters | Pavilion plan |
| Roof datum | 4.05 meters | Roof slat base height |
| Orbit rate | 0.008 rad/screen pixel | Read-only native precedent |
| Zoom range | 0.35–3 preview scale | Specimen-only camera limit |
| Initial roof | 13 slats | Baseline view |
| First refinement | 17 slats | Demonstrates visible geometric change |

## Golden paths

1. Open Rhino 3D and inspect pavilion. Toggle Wireframe, cycle Top/Front/Perspective and Fit; geometry remains the same.
2. Focus the viewport and drag to orbit. Shift-drag pans. Wheel changes model scale only after viewport selection. Right-drag still moves the Studio canvas.
3. Click a column or roof slat. Its edges highlight and its object name appears beneath the viewport.
4. Run the initial roof-spacing request. Public status exposes the current step and script content. At completion the preview gains closer-spaced slats.
5. Stop during a preview run. The last completed model remains available. A later run can start normally.
6. Carry model discussion downstream through the shared history mechanism. Each branch keeps its own preview data and conversation snapshot.

## Generated script and native handoff

The script creates only new objects. Each box uses eight ordered corner points, stores returned IDs, and raises an error if creation fails. A single undo record groups the operation. If a step fails, only IDs created by that run are deleted. Redraw state is restored in `finally`. The geometry outline follows the [official RhinoScriptSyntax reference](https://developer.rhino3d.com/api/RhinoScriptSyntax/).

The eventual native agent must inspect model units and source revision before applying a script; route execution through the existing registered connector and authorization surface; marshal Rhino document operations to the proper host thread; validate tool arguments; and expose output, errors, cancellation and undo through established owners. A model update must preserve unrelated objects and return created object IDs, measurements and a fresh viewport capture. No private reasoning is shown: activity consists of public step summaries, tool outcomes and generated scripts.

## Verification and limits

`node --check elements/rhino-agent.js` passes. Parent integration verifies the assembled tab, persistence, activity sequence and shared controls in the browser. There is no Rust build, Rhino runtime, network model generation, exact NURBS geometry, live mesh inspection or production permission grant. SVG orbit is a presentation surrogate, not the native renderer. Selection is model-object naming in the specimen; it is not yet a native component-selection protocol.

## Revision control

v0.1: first-pass Rhino specimen and shared-runtime configuration. Local viewport controls, sample geometry, progress step names and colors remain review proposals. Future revisions must retain shared control parity and reconcile approved changes with the native contract registry before production implementation.
