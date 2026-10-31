# Dashboard Builder interaction contract

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

Dashboard is a generic Web artifact, not a fixed business-dashboard application. Property order is Fill, Stroke, Agent, Capture (`board_properties/property_strip.rs:23–54,191–195`). The sample chart is plain output; there are no invented reporting-period controls, dashboard navigation tabs or selectable-widget subsystem.

Prompt: **Describe the dashboard or the next change**. Native Settings: **Fork, Fresh eyes, Inspect memory, Open source**. Pending output adds **Apply preview** and **Discard preview** there, never inside the rendered page. Reference: `board_agent/artifact/editor.rs:209` onward. Full access remains a persistent preview preference, not real authorization.

## Golden paths

1. Select artifact → Agent → compact generator opens using the same anatomy as Images.
2. Enter a change → Play → inspect public progress and staged output → Settings → Apply preview or Discard preview.
3. Select another circular revision → that output becomes visible.
4. Enable Preserve model history → continued dialogue stays in the same generator; Fork creates an independent downstream continuation.
5. Narrow the host → property controls and generator follow the shared detached layout.

## Native versus Studio refinements

The source audit records native measurements and the Image-session changes separately. Eighteen-unit revision circles and compact eight-unit prompt inset are inherited from reviewed Images; native history is 26 and native generator inset 12. Extra content-specific workflows are not silently promoted to native baseline. The illustrative content is not a prescribed application design.

## Verification and limits

Browser checks cover selection, generator opening/closing, compact model/settings, optional history, and public activity. Shared layout tests cover native strip counts and attachment/fallback. Source references were inspected directly; no native app was running, so this remains a source-informed best guess rather than a certified screenshot match. Live generation, native permission changes, file export and host operations remain outside the Studio.

The current contract and screenshots supersede first-pass presentation claims; old screenshots are historical evidence only.
