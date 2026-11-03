# Element Studio debrief contract

Status: working specification; existing Chat Train agreement retained, Images and three new element layouts remain under review.
Family: design-authoring tool. Date: 30 October 2026.
Command: static HTML entry point; no production command added.

## Purpose and scope

One persistent tab and independent canvas per element. The HTML, screenshots and per-element debrief together define the proposed implementation. Production Rust is changed only in a later explicitly requested phase. This commit captures the accumulated Studio and adds Dashboard Builder, Text Agent and Rhino 3D for further iteration.

## Reviewed baseline

Retain Chat Train as first tab, Images as second. Preserve their conversations, settings and user-authored context menus. Shared menu rows remain compact, with detail on hover. Provider, model, effort and speed remain separate; model capability metadata controls available values. API keys use the dedicated masked entry dialogue; this Studio discards key values. Canvas navigation remains owned by canvas-navigation.js. Scroll indicators overlay text, appear while scrolling and fade without a text gutter.

## New draft canvases

| Tab | Main specimen | Interaction to review |
|---|---|---|
| Dashboard Builder | DOM/SVG dashboard with metrics, chart, milestones and team views | Select a widget, request a change, inspect staged output, Apply or Discard |
| Text Agent | Editable courtyard brief | Highlight text, query the exact excerpt, inspect a scoped replacement, Apply or Dismiss; stale ranges refuse replacement |
| Rhino 3D | Projected pavilion with named model parts | Orbit/pan/zoom, switch view/display, run refinement, inspect public progress and Rhino script |

## Shared agent contract

The shared owner is elements/runtime.js with pure state in elements/session-model.js. Each element only supplies specimen content, deterministic sample transformations, public progress stages and its own tools. All use the existing ChatAgentPicker catalogue and explicit key-entry path. Preview model/access preferences persist across the three new tabs and sessions. These local preferences grant no real filesystem or application permission. Production permission scope must be reconciled with its existing owners; this phase does not alter them.

Single dialogue retains successive message pairs in its window. Chat train sends the next iteration into a separate connected window; Continue in new window also forks explicitly. Preserved context is copied, so child edits cannot change the parent. When preservation is off, a continuation carries no inherited conversation and shows no history wire. A terminal history wire may be selected and deleted; earlier transcript remains visible, but a new context boundary prevents it from reappearing in future inherited history. Re-enabling preservation starts from that boundary.

Text selection is recorded as exact text, offsets and source revision. The quoted excerpt persists when focus moves into the composer. Query submission and replacement validate the source revision. Source text changes only after Apply. The selection does not authorize editing unrelated text.

## Feedback and run lifecycle

The visible activity contains public progress summaries, observable tool actions and generated script output, never private reasoning. Progress and Script are separate views. A run exposes Stop immediately and retains completed output if interrupted. Script content arrives with the relevant sample stage. The composer remains usable and retains its draft/caret during stage updates. A duplicate submission while running is refused. Page reload interrupts an unfinished sample run rather than silently restarting it.

The native counterpart of a long-running task must report meaningful milestones and tool results as they occur; elapsed state must stay honest even during a quiet tool call. It may not manufacture progress or claim that code ran when it only generated it. Studio stage timing is deliberately short and simulated; no model, shell or Rhino API is invoked.

## Geometry and persistence

Use shared 232-unit compact menus and Chat Train typography. Conversation text is 14/20 with a 28-unit user indent. Each node stores its width, location, draft, preview state, inherited snapshot and visible transcript. Camera is independent per canvas. Moving, resizing or switching tabs must not erase a draft. Tiny history handles carry no text label on the canvas; the description is hover/accessibility text. Growing content is fitted after an explicit run; ordinary reading is not repeatedly recentered.

## Known limits

The new layouts are first-pass proposals, not certified native pixel matches. The Rhino viewport is projected SVG, not a NURBS renderer. Generated code is display-only. Native tool registration, context protocols, permission enforcement, real stream transport, undo journal and provider job scheduling remain owned by production. Earlier Images/Chat Train setting scopes are unchanged; global permission integration remains future native work. Persisted data lives in this browser's localStorage, not shared across devices.

## Verification

Pure state checks cover branch isolation, selected-excerpt snapshots, duplicate-run guarding, and fresh history boundaries. Browser checks cover all three canvases, Builder staging/apply, text selection/query/apply and train creation, Rhino view/display controls and progress/script output. Shared navigation and model capabilities use their existing checks. Screenshots and detailed results are in validation/new-agent-studios.md.

## Promotion

Shared principles already approved remain constraints. The three new specimen arrangements and sample outcomes need owner review before lock-in. Their contracts are available from each tab's Debrief button. This design checkpoint does not imply production deployment.


## v0.2 — Native anatomy correction (supersedes v0.1 presentation)

The first-pass header/toolbars were rejected. Current frame and generator controls now reuse ImageStudioLayout and native property-tool ordering. Permanent titles, revision footer, mode label, ready row and Progress/Script tabs were removed. A bare output surface leads into native property icons and a compact attached or detached generator. Three dots contain Settings and optional preserved history. Native Settings actions and conditional Apply/Discard are retained; public activity is a folded summary above the prompt. No native styling palette or tool implementation is changed.

See [the native source audit](../research/native-agent-chrome.md) for measurements, exact owners, preserved behavior and the explicit Image Studio adaptations. Current layout remains a source-informed draft; earlier v0.1 imagery and layout descriptions are superseded. No claim of native pixel certification is made.

## v0.3 — Images is the shared implementation precedent

This amendment supersedes earlier descriptions of Settings, Continue/Fork commands, separate Progress/Script views, native activity-summary rows and widget selection. The current owner-requested controls are documented in the v0.3 Dashboard Builder, Text Agent and Rhino contracts. The actual Images dialogue and count/model-label components now live in generator-ui.js and are called by all four generators. Input source creation, wire attachment, graft recipes and four-slot capsules are available in each new canvas. Output drag creates closed blank downstream surfaces; one Preserve model history checkbox controls context. Header spinner and hover chevrons replace the separate activity row. The three host layouts remain under visual review; these changes do not authorize Rust implementation.

## v0.4 — Output previews and linked answers

The new generators use actual saved output thumbnails in the header and Images-style rounded-square preview strips on hover. Selecting a preview keeps the strip visible and does not open the toolbar. The 1500 ms rebound applies to the dismissed agent and its model/options menu. Dashboard's three-dot menu now contains only Preserve model history. Answered text selections link to their response and show a question/answer mini viewer on hover/focus. See validation/preview-links-v4.md and the v0.4 element contracts. Native code remains unchanged.
