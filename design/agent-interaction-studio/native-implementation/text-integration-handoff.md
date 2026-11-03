# Text request/response integration handoff

30 October 2026. Coherent implementation patch; not compiled or visually certified. No commit.

## Changed owners

- `apps/slate/src/app/board_flow.rs`: narrow `queue_text_block` hook snapshots selected authored text, UTF-8 range, question and session under `(workbook, node, request_id)` before enqueue (which can dispatch immediately).
- `apps/slate/src/app/board_flow/state.rs`: ephemeral selection/request/navigation fields and `TextAnswerRequest`.
- `apps/slate/src/app/board_flow/text_note/answers.rs`: mirrors selection into FlowUi so queue can read it without egui Context; records answers using provider-authored `patch_nodes_as` transaction. Existing tooltip, galley cache and hit behavior retained.
- `apps/slate/src/app/board_agent/generator_pump.rs`: selected quote and immutable submitted question reach the actual request prompt; changed source/conversation refuses dispatch; discarded requests clear their answer snapshot.
- `apps/slate/src/app/board_agent/session.rs`: binds only matching current request/session successful Idle response to captured source/range; cancellation, edited source and failed/obsolete requests cannot create links. Queued changed snapshots remain until dispatch rejects them, preventing accidental fallback to an ordinary unscoped request. Pumps native navigation into the shared app adapter handoff.
- `apps/slate/src/app/board_agent/session/text_answer_tests.rs`: native request lifecycle regressions.
- `crates/slate-artifact/src/render.rs`: existing HTML text owner delegates inline answer serialization; local answer interaction consumes clicks without navigating slides.
- `crates/slate-artifact/src/render/text_answers.rs`: valid-range links and escaped public Q/A dialogs; no session identities or stored HTML in document nodes.

All paths above retain pre-existing working tree changes. No shared shell/headless connector files changed.

## Navigation adapter contract

`agents.flow.text_answer_navigation: Option<(u64, NodeId, slate_doc::text_answers::TextAnswerRef)>` carries `(workbook, text node, { session, through })`. The common agent pump consumes the existing `take_text_answer_navigation(ctx)` intent into this field. Native integration worker was notified directly. Its existing generator dialogue must take the intent, open that view and scroll to response index `through - 1`. Viewing never resumes inference, restores memory or changes provider/session identity. Missing/mismatched sessions should use the durable public `TextAnswer.answer` snapshot rather than another response. Exact scrolling remains the common app adapter's responsibility and is not certified here.

## Tests added

Native `board_agent::session::text_answer_tests`:

- `selected_passage_uses_submitted_question_and_links_only_matching_completed_response`
- `changed_text_before_dispatch_never_sends_selected_query`
- `cancelled_or_edited_requests_cannot_link_late_success`

Artifact `render::text_answers::tests`:

- `unicode_ranges_escape_public_content_without_exporting_session_identity`
- `edited_passages_export_as_plain_text`

Targeted standalone rustfmt succeeded on all eight Rust files (parser check). `git diff --check` passed for modified tracked files. Cargo was deliberately not run under coordinator instruction. Added tests still require execution. Native selection/query/completion/hover/navigation/undo and exported-dialog visuals require integration acceptance. Existing pure annotation/rebase/serialization tests from the earlier worker remain relevant.

## Explicit remaining extent

Durable annotations currently target authored `TextNode` notes only. Linked-file Text Agent notes use `ImageNode.text_override`/external file output and retain existing behavior; they have no annotation storage or selection-query binding. Do not claim linked-file text parity. Smallest extension proposal: optional defaulted annotation metadata alongside `ImageNode.text_override`, using the same UI-free `TextAnswer` validation/rebase owner and the existing authored inline-edit command. Source refresh must validate the exact previous excerpt before rebasing or invalidate stale ranges. Do not add a competing rebaser, composer, HTML document field, or unrelated sidecar store.

HTML text serialization has local hover titles and accessible modal Q/A snapshots. Native dialogue destination navigation is an app concern; exports intentionally carry public answer content without private agent session locators. SVG text-only paths have not gained interactive hyperlink/Q&A presentation.

Rollback is a narrow removal of the described queue, session and render hooks plus new fields/helpers/tests, preserving the earlier pure `TextNode.answers` model and other workers' dirty changes. No whole-file reset/staging was performed.
