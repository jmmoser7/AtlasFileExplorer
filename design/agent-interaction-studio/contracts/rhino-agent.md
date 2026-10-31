# Rhino 3D interaction contract

Status: draft for owner review; v0.2 supersedes the rejected v0.1 interface.
Family: native canvas element / agent.
Scope: HTML/CSS/JS reference only. No Rust or live provider changes.
Inherits: Studio S01–S09 where relevant; [native chrome audit](../research/native-agent-chrome.md).

## Shared presentation

A content surface sits on the infinite canvas, with Slate property icons at its lower edge. The existing ImageStudioLayout owns attached/detached sizing, rounded corners, blister placement and notches. Attached editor overlays its top backing under the host; narrow hosts use the same 420-unit detached card as Images. There is no separate application title bar, revision footer, permanent conversation rail or Progress/Script tab bar.

The agent's top datum contains the compact model selector, adjacent three dots, circular revisions and right-aligned Play/Stop. Provider, model, effort and speed use the existing picker and actual declared options. Three dots open native Settings plus the reviewed Preserve model history and Full access controls. Checkbox actions stay open for visual feedback. Prompt text wraps by authored width, not camera zoom.

Activity is a 24-unit summary above the prompt, with single/double-chevron folding, a 96-unit partial view and 480-unit full limit. It contains public commentary/tool output, including generated source where relevant. No private reasoning is exposed. There is no empty activity panel when no activity exists. Transient scrollbars overlay content without displacing text.

Preserve model history is off in the initial native-style view. Enabling it adds the requested back-and-forth dialogue within the generator; the same snapshot can travel downstream. Existing transcripts/drafts are retained. Cut history establishes a fresh future context boundary without deleting historical content.

## Preserved native baseline

The content surface is the 3D viewport, with axes and selectable model geometry. The invented filename/view/format header and footer are removed. Native model functions belong in the property strip: display, measurement, screenshot, maximize, filter and connector, alongside ordinary style and Agent controls. See `board_properties/property_strip.rs:65–92`.

Display choices are **Shaded, Arctic, Hidden line, Material mask, Z-buffer**, from `scene/model_camera.rs`. The Rhino glyph and measurement glyph are exported from the native icon registry; no replacement icon vocabulary is invented. A native-connected host has compact lower Slate 3D / Grasshopper controls (`application_connector/presentation.rs`), not a new file/application header; actual host switching remains a future integration.

The generator uses the same compact model/revision/Play arrangement and Settings as the artifact pattern. Public activity can show a generated Rhino Python script in its folded stream; Open source / View generated script exposes the full text. No source is executed by the Studio.

## Golden paths

1. Select model → use native property display icon → choose a native display mode.
2. Focus the viewport → orbit; Shift-drag pans; ordinary wheel changes model zoom, Ctrl-wheel retains canvas zoom.
3. Agent → enter a geometry change → Play → see public operation status and script output in the same generator.
4. Preserve model history → refine in place or Fork downstream with independent context.
5. Stop → retain the last completed model and public output.

## Native versus Studio refinements

The source audit records native measurements and the Image-session changes separately. Eighteen-unit revision circles and compact eight-unit prompt inset are inherited from reviewed Images; native history is 26 and native generator inset 12. Extra content-specific workflows are not silently promoted to native baseline. The illustrative content is not a prescribed application design.

## Verification and limits

Browser checks cover selection, generator opening/closing, compact model/settings, optional history, and public activity. Shared layout tests cover native strip counts and attachment/fallback. Source references were inspected directly; no native app was running, so this remains a source-informed best guess rather than a certified screenshot match. Live generation, native permission changes, file export and host operations remain outside the Studio.

The current contract and screenshots supersede first-pass presentation claims; old screenshots are historical evidence only.
