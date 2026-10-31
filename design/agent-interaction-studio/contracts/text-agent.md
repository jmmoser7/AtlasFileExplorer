# Text Agent interaction contract

Status: draft for owner review — 30 October 2026
Family: portal
Command: Studio tab only; production commands remain with the existing text-language-model window
Inherits: Studio S01–S08; shared generator chrome and Chat Train conversation styles
Owner: native `apps/slate/src/app/board_flow.rs` (`paint_agent_text_window`), `board_agent/generator_chrome.rs`, `board_agent/focus.rs`; browser `elements/text-agent.js` with shared element Studio runtime
Scope: a linked text-document-style card, selection-aware queries and revisions. Plain HTML/JS/CSS; no native changes or provider calls.

## What it is

A text box with a small shared agent dock. The document remains directly editable. Highlighting a passage carries that exact excerpt into the dialogue below, where the user can ask a question or request a revision. The shared dock supports an ongoing conversation in one window or a downstream Chat Train.

## Preserved baseline and explicit additions

Native `paint_agent_text_window` delegates to the existing agent-note presentation. Model choices, capability availability, access and activity are owned by shared generator chrome. This Studio retains that separation: the text preview owns text, ranges and proposed edits; the runtime owns agent configuration, conversations and history.

The owner explicitly requested highlighting text and querying that selection, plus single-window and Chat Train continuation. The quiet document layout, selection chip, proposed-edit strip and 650×390 initial size are first-pass interpretations awaiting review. They are not newly locked production requirements.

## Behavior matrix

| ID | Dimension | Draft outcome |
|---|---|---|
| D01 | Initiation & arming | Open Text Agent tab; select the text document and open the shared agent dock. |
| D02 | Stickiness & repeat | Document and dialogue persist across requests; no new card is required for each reply. |
| D03 | Gesture grammar | Select passage → ask in dock → response/proposed edit → optional Apply. Train creation remains an explicit shared action. |
| D04 | Click vs drag | Shared S05: canvas dragging remains separate from selection inside the active editable document. |
| D05 | Modifiers | Native text-selection keys and clipboard shortcuts; shared dialogue Enter/Shift+Enter behavior. |
| D06 | Constraints & snapping | n/a to passage selection; inherited canvas layout behavior. |
| D07 | Direction/value locks | n/a. |
| D08 | Manual entry | The document supports plain-text editing and paste; pasted markup is discarded. |
| D09 | Preview/readouts | The exact passage is highlighted and quoted in the dock; revision proposal is shown before applying. |
| D10 | Cursor | I-beam over text; shared drag affordance elsewhere. |
| D11 | Commit | Asking never silently rewrites the document. Replace highlighted text or Replace document applies the proposal. |
| D12 | Cancel | Dismiss discards a proposal; shared Escape closes transient UI. |
| D13 | Selected presentation | Selection has a subtle highlight. Shared node grips and compact agent controls remain outside the prose. |
| D14 | Post-edit | Edit text directly or ask again; Undo text edit restores the previous applied wording. |
| D15 | Non-goals | No rich-text formatting system, file writes, real LLM invocation or private reasoning display. |
| D16 | Create-style inheritance | Shared quiet card surface and typography; text style controls remain native owners. |
| D17 | Hit-testing & pick | DOM text ranges inside the document; canvas/card hit handling belongs to the shared runtime. |
| D18 | Class/authority | Generated review preview. Eventual native mutation must use the existing authored-text journal. |
| D19 | Source binding | This prototype starts from a local sample brief. Native linked-file binding remains unchanged. |
| D20 | Query/parameters | Shared provider/model/effort/speed/access; selection text and range revision accompany a scoped query. |
| D21 | Regeneration/staleness | Each edit increments textRevision. A stale range is rejected when applying; it cannot replace unrelated wording. |
| D22 | Contents interaction | Select/read/edit text. Clicking the dock preserves the excerpt used for the next query. |
| D23 | Level of detail | Shared canvas zoom; no separate low-zoom text policy introduced. |
| D24 | Export | n/a in prototype; native linked-text export remains its existing owner's responsibility. |
| D25 | Bake | Apply changes current preview text. Native authored-text commit remains a future integration. |
| D26 | Collaboration | Browser-local persisted draft only; no new remote peer protocol. |
| D27 | Agent surface | Uses shared dock. No provider/network tool runs from the document preview. |
| D28 | Provenance | Captured excerpt, start/end offsets and textRevision; proposal retains its target until applied/dismissed. |
| D29 | Performance | Input saves without full preview rerender, preserving caret/selection; bounded preview scrolls. |
| D30 | Failure/honesty | Simulation is identified by shared runtime; stale selected ranges cannot apply; original text remains until acceptance. |
| D31 | View-state ownership | Draft text/proposals are local Studio state; transient DOM selection is restored via a saved range. |
| D32 | Trust/consent | Plain text only, no pasted executable markup, no native filesystem mutation. |
| D33 | Chrome | Shared model dropdown, robot/dock, menu sections and fold/history controls. |
| D34 | Maximize | Shared runtime handling; no text-specific full-screen policy. |
| D35 | Local UI | Only editable document and proposed revision belong to the text preview. Global settings stay outside it. |

## Feel constants

- Initial document 650×390 world units; 28-unit prose inset.
- Body 14px / 1.6, quiet 17px document title and 10px secondary label.
- One subtle range highlight; no permanent role labels or bubbles in the dialogue.
- Shared transient 3-unit scrollbar: no text gutter, fades after idle.
- Proposed revision is bounded at 165 units and does not cover the source text.

## Golden paths

1. Select a sentence, click into the dock and ask what it means. The excerpt stays quoted/highlighted; a response does not alter the source.
2. Request clearer wording. Inspect the proposal, then Replace highlighted text. Only the exact selected range changes.
3. Change the source text after a proposal. Applying the stale proposal reports the mismatch rather than replacing a shifted range.
4. Dismiss a proposal. Original text and dialogue remain.
5. Continue in the single window, then create a downstream train. The shared model choice and conversation carry over without duplicating document contents.
6. Type and paste multiline text. The caret remains stable and pasted HTML does not become executable content.

## Verification and limits

JavaScript syntax and pure proposal checks passed: selected-range edits preserve the source until Apply, target the captured text, and reject stale revisions. The parent Studio integration is responsible for browser verification of selection capture, scoped application, train continuation and persisted reloads. Replies and transformations are deterministic simulation: they demonstrate flow, not real language-model quality. Custom Highlight is used where supported; the dock's quoted excerpt remains the fallback if custom highlights are unavailable.

Version 0.1. Shared principles are inherited; this element's new arrangement remains open for iterative review.
