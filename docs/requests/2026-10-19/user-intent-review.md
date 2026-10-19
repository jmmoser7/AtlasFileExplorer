# Independent intent review — Slate research presentation workflow — 19 October 2026

## Scope and evidence

This is an intent checklist for the reported end-to-end task: produce an eight-slide research presentation using drawing, Web and File Atlas portals, image generation, and chat. It does not claim that the delivered build was inspected or that any case passed.

The collaborator-local source of current user evidence is `demo-ai-workspace/.atlas-ai/agent/req-46804-1792418532597602800-3/feedback-log.json`. Although described as a four-note file, it contains **five** notes:

- `req-46804-1792419408467877000-28` — web focus/read is blocked by unapproved origin; needs a visible consent request and a denial that names the exact origin/control.
- `req-46804-1792419520366428200-114` — no board/frame preview pixels; asks for read-only render at stable frame ID and slide order/basic styles in board reads.
- `req-46804-1792419667438959600-162` — confirmed on Google and EPA portals after provider Full access; no visible web-origin control. Asks for per-origin consent, request action/link, and distinct capability state for board autonomy, provider access and origin permission.
- `req-46804-1792419696775440800-184` — generator command lacks aspect/count; requested landscape 4:3 produced square output, while reads report `aspect: source` and `count: 0` despite a placed result. Asks to expose settings and report job status, output count and dimensions.
- `req-46804-1792420061098710000-417` — helper outcome is truncated near 500 characters, with `next_offset: null`; asks for paginated/cursor-based full participant result text.

The October 17 Suggestion box reports are historical predecessors, not this run's four-note set. Their IDs are `2026-10-17_111213-bug-ok-the-play-button-on-image-gene` (later image runs lose play/progress/count affordance), `2026-10-17_111940-bug-there-should-be-some-directional` (image text-input handle dropped on blank canvas should create focused editable text), and `2026-10-17_112811-bug-paste-that-screenshots-seem-to-i` (pasted screenshots lose detail/resolution). Keep their own disposition/evidence separate.

## Acceptance clauses for the requested task

A fresh native run must start each case Untested and record the exact executable identity, source/workbook, and evidence. Exercise the complete task at ordinary UI input, including the committed board and presentation at rest. Required outcomes:

1. **Complete eight-slide deliverable.** Create and inspect eight ordered slides that combine authored drawing, linked Web and File Atlas content, generated images, and text. Reopen or present the result and confirm ordering, visual coherence, readable content, and that edits remain inspectable/reversible.
2. **Origin consent is discoverable.** Start from an untrusted/new origin and find the consent decision through the ordinary workflow before access. Verify the user can understand the origin, grant or deny, and continue or recover without guessing or being silently granted access.
3. **Board inspection is actionable.** From the agent's normal interface, read slide/frame order and relevant authored styles; use a preview/tool that makes the current board or slide result inspectable before completion. Verify the human can compare the readout/preview with the actual board.
4. **Generators are controllable and observable.** Expose aspect ratio and image count at the point of use; repeated runs keep a visible action, show in-progress status, retain job status/results, and permit the user to inspect the generated result. Include the October 17 regression: the action remains available for later generations and indicates progress while running; count changes reliably.
5. **Chat is usable as text.** Select and copy a portion of a conversation; Ctrl+A selects the complete chat text, including the intended history, without selecting unrelated canvas content. Verify copied text pastes in order and remains available after interacting with the board.
6. **Complete helper findings are readable.** Retrieve the finished helper outcome through the collaborator command surface, including content beyond the first ~500 characters. Verify pagination/cursor metadata advances and permits the full result to be read without confusing it with participant-list pagination.
7. **Action budget and observable work are clear.** Make a hard action limit, remaining budget and exhaustion behavior visible before interruption; verify the task resumes without losing board/chat state. Show only observable actions and brief visible work notes. Do not present invented internal thought as evidence.
8. **Persistent show-your-work canvas.** Keep source/search portals, notes, generator nodes and iterations on the board by default so the research path and alternatives remain visible. Arrange portal sets and results in readable grids with recognizable labels and unobscured content; reject piles/stacks that hide evidence. Verify the committed board at rest and after reopen/presentation.
9. **Preserve accepted interaction.** Recheck the approved marble avatar/hats and compact persistent chat in the same full workflow. They must remain recognizable and usable without taking over board evidence or covering relevant content.
10. **Drawing/text regression.** Create a blank text object by dragging from the image text-input handle onto empty canvas; it opens focused and ready to type. Edit, move, undo/redo, save/reopen, and confirm the surrounding presentation remains intact.
11. **Image fidelity regression.** Paste a screenshot and compare its source resolution and visible detail with the placed result at useful zoom. Confirm scaling/layout choices do not silently discard source detail.

## Governing constraints and review boundary

Articles VII.1–2 require the agent and human to share the registered command surface and require agent work to be visible, inspectable, reversible, and journaled. Article VIII.1 keeps context explicit for ordinary chat; VIII.2 routes input modalities through commands; VIII.3 protects direct manipulation and asynchronous work. Article IX.1–3 requires linked sources, relative-first locators, and nonblocking source operations; IX.5 keeps source write-back explicit and human-directed. These guide the cases above; no user request here authorizes weakening them.

The repo feedback rule requires evidence from the exact native build, separate agent-observed results from user acceptance, and leaves missing or blocked cases open. The root agent owns all native UI inputs and evidence collection; this review makes no interaction or pass claim.

## Evidence boundary and implementation review

The collaborator feedback log provides exact reproduction reports and desired missing capabilities, but not native-build acceptance. Root owns native UI input and will supply later evidence for independent review. The planned renderer reuse has a sound ownership direction: extract `FrameRaster` from the test helper and reuse the existing `paint_board_node` path rather than create a second board painter. Review its resulting pixels for parity with the visible board, stable frame targeting, and readable content; source ownership alone does not pass the preview case.

The current implementation status below is reported by the root implementer and remains **Untested** in the delivered native app. I have inspected only the protocol/default prompt and the documented command contract; I did not independently inspect the changed implementation or run the workflow.

| Area | Reported source state | Native acceptance |
|---|---|---|
| Web origin | Human Webtab Allow/Revoke controls | Untested: locate and use consent on a new origin; verify denial and later resume; confirm provider Full access and board autonomy do not grant it. |
| Task budget | 4096-call task budget, visible counters, Continue for a fresh budget | Untested: discover remaining budget, reach exhaustion safely, continue, and retain board/chat/work. |
| Generator | Explicit aspect and count 1–8; actual import dimensions and job status | Untested: request landscape 4:3 with multiple outputs, inspect truthful status/count/dimensions and placed results, then run again. |
| Helper outcomes | Full paginated retained results, bounded to 64 KiB | Untested: retrieve a result beyond its first page and verify the cursor/generation plus completeness. |
| Preview/read | Production painter reused through promoted `FrameRaster` isolation | Untested: compare preview pixels, stable frame ID, frame/slide order and styles against the visible saved board; verify incomplete previews are reported as incomplete. |
| Chat text | Select-copy and Ctrl+A with bounded rendering | Untested: select a range, copy/paste it, Ctrl+A the intended full history, and verify canvas focus does not steal or broaden selection. |
| Tags | Existing reversible create/assign/unassign owners | Untested: use authorized scopes, witness/target rules, undo, and linked-image assignment behavior. |

The `board-link-v1.md` default prompt is strong on source-linked notes, persistent evidence, distinct permission classes, truthful observations, portal grids, generator settings, previews and action-budget continuation. It explicitly prohibits claiming research from portal creation and disallows hidden shell/browser/file substitutes. Two clarifications would make the contract harder to misread: before reading a File Atlas item, identify it from the granted portal enumeration and use its exact locator; if scope is unavailable, report the precise blocked item and request the human grant. Also say that `app.feedback.report` creates an agent-authored local note and must be used only when the user asks to record feedback; it is neither a Suggestion box submission nor an externally filed report. The command schema/`board-link.md` does state local-only semantics, but the task prompt itself omits when an agent should invoke it. The note shape has no explicit author/time fields, so any UI that displays this log should identify the collaborator as its author instead of presenting it as a human report.

File privacy remains bounded by the runtime's enumerated portal/readable-source checks, not by prompt wording alone. Do not treat provider Full access as expanded board scope, origin consent or source permission. Source-linked board text should cite the minimum necessary locator/URL and omit credentials or unrelated local path details.

## Suggested native task prompt

> Create exactly eight ordered slides answering **[the research question supplied by the user]**. On the board, keep three titled areas: Research, Drafts, and Presentation. In Research, retain the Web and File Atlas source portals you actually use. Read only content visible through authorized portals; if an origin or file scope is denied, name the exact origin/item and stop that read until the human grants access. Put concise notes beside their cited sources, label observed facts separately from proposals, and never claim a source was read just because a portal exists. In Drafts, keep sketches, generator briefs, meaningful iterations, and a readable grid of alternative outputs. Generate landscape 4:3 images with an explicit requested count, then report the actual job status, output count, and imported dimensions; do not discard alternatives. Use one bounded helper to cross-check a specific claim if the grant allows it, then read its complete retained outcome page by page. Use available reversible tag commands only within the granted scope. In Presentation, build eight clearly titled slide frames in deck order, using authored drawing, text, cited Web/File Atlas evidence, and inspected generated imagery. Leave the Research and Drafts evidence visible after the deck is complete. Show only actions actually performed and brief visible work notes; do not expose invented internal reasoning. Inspect board reads and a complete production-painter preview, then save only through the authorized workbook workflow. If the action budget runs out, preserve the work and ask the human to Continue.

This is a task prompt suggestion, not proof of success. If the actual user supplied a more specific research question, use that exact question. Do not fill the placeholder with an invented topic.

Do not archive any Suggestion box report based on this review. The feedback log is not itself an archived/closed report bundle.
