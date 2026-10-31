# Text Agent interaction contract

Status: draft for owner review; v0.2 supersedes the rejected v0.1 interface.
Family: native canvas element / agent.
Scope: HTML/CSS/JS reference only. No Rust or live provider changes.
Inherits: Studio S01–S09 where relevant; [native chrome audit](../research/native-agent-chrome.md).

## Shared presentation

A content surface sits on the infinite canvas, with Slate property icons at its lower edge. The existing ImageStudioLayout owns attached/detached sizing, rounded corners, blister placement and notches. Attached editor overlays its top backing under the host; narrow hosts use the same 420-unit detached card as Images. There is no separate application title bar, revision footer, permanent conversation rail or Progress/Script tab bar.

The agent's top datum contains the compact model selector, adjacent three dots, circular revisions and right-aligned Play/Stop. Provider, model, effort and speed use the existing picker and actual declared options. Three dots open native Settings plus the reviewed Preserve model history and Full access controls. Checkbox actions stay open for visual feedback. Prompt text wraps by authored width, not camera zoom.

Activity is a 24-unit summary above the prompt, with single/double-chevron folding, a 96-unit partial view and 480-unit full limit. It contains public commentary/tool output, including generated source where relevant. No private reasoning is exposed. There is no empty activity panel when no activity exists. Transient scrollbars overlay content without displacing text.

Preserve model history is off in the initial native-style view. Enabling it adds the requested back-and-forth dialogue within the generator; the same snapshot can travel downstream. Existing transcripts/drafts are retained. Cut history establishes a fresh future context boundary without deleting historical content.

## Preserved native baseline and requested addition

The content is the note itself. `board_flow.rs:571–579` delegates to `paint_agent_note`; `board_flow/text_note.rs` owns latest reply and bottom prompt. The prototype removes the invented Courtyard brief / Editable text application header. The agent-media property strip keeps Fill, Stroke and Agent. Text uses 14-unit native note typography with a bottom reserve so the property strip does not cover words.

Highlight-to-query is the explicit new capability: select text in the note, then type into the generator. A compact excerpt shows what is targeted and survives moving focus. The selection records text, offsets and source revision. A proposed replacement changes only that matching range after Apply; stale or modified ranges refuse replacement.

**Create chat train** is available in the three-dot menu for the requested text workflow. In-window conversation uses Preserve model history. Neither action adds a permanent second application toolbar.

## Golden paths

1. Select a passage → query below → excerpt remains tied to the exact selection.
2. Ask for a rewrite → inspect the proposal → Replace highlighted text → only that range changes.
3. Change the source while a proposal is pending → applying it refuses the stale range.
4. Enable history → have successive turns in this generator; Create chat train continues from the note with inherited context.
5. Deselect/reselect → preserve the note, draft, width and short return grace.

## Native versus Studio refinements

The source audit records native measurements and the Image-session changes separately. Eighteen-unit revision circles and compact eight-unit prompt inset are inherited from reviewed Images; native history is 26 and native generator inset 12. Extra content-specific workflows are not silently promoted to native baseline. The illustrative content is not a prescribed application design.

## Verification and limits

Browser checks cover selection, generator opening/closing, compact model/settings, optional history, and public activity. Shared layout tests cover native strip counts and attachment/fallback. Source references were inspected directly; no native app was running, so this remains a source-informed best guess rather than a certified screenshot match. Live generation, native permission changes, file export and host operations remain outside the Studio.

The current contract and screenshots supersede first-pass presentation claims; old screenshots are historical evidence only.
