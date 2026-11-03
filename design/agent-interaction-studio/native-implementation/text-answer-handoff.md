# Text answer builder — coherent patch handoff

Date: 30 October 2026. Native implementation is partial, not shipped or visually certified.

## Files owned and changed

- `crates/slate-doc/src/text_answers.rs` (new): exact UTF-8 range + quote, question/answer snapshot, session/checkpoint reference, overlap replacement, safe rebase and ordered valid ranges.
- `crates/slate-doc/src/authored_text.rs`: the existing pure `apply_inline_edit` owner now rebases answers with text in the same node mutation. Journal undo/redo consequently retains the before/after links with the text.
- `apps/slate/src/app/board_flow/text_note.rs`: declares the answer helper.
- `apps/slate/src/app/board_flow/text_note/answers.rs` (new): selection capture, validated journaled answer insertion, cached hit rectangles derived from the existing galley, underlined native link affordance, retained target highlight, hover/focus answer tooltip, and pure navigation intent.
- `apps/slate/src/app/board/paint_node/text.rs`: calls that helper over the existing text galley and authored rotation.
- `apps/slate/src/app/board/board_text_overlay.rs`: obtains TextEditOutput and captures a non-empty character selection without consuming it on focus change.

Root separately added `pub mod text_answers`, TextNode.answers with serde defaults, and constructor defaults. Do not reapply those changes blindly.

## Stable APIs

`TextAnswerRef { session, through }` identifies a completed response checkpoint; no provider key or grant is stored. `TextAnswer { start, end, quote, question, answer, response }` stores UTF-8 byte offsets and the intended visible snapshot.

`TextAnswer::new`, `is_valid_for`, `char_range`; `text_answers::{byte_range,upsert,rebase,valid_answers}`. Export should iterate valid_answers and escape ordinary text, quote, question and answer rather than embed provider/session JSON in HTML.

Native methods: `capture_text_answer_selection(ctx,id,text,character_range)`, `text_answer_selection(ctx,id)`, `record_text_answer(id,source,range,response,question,answer)`, `take_text_answer_navigation(ctx)`. The record method refuses a changed source or read-only edit and uses patch_nodes.

## Remaining integration — required before feature completion

1. Actual text requests bypass chat_send.rs: `prompt.rs::send_agent_prompt_from` routes `is_text_block` to `board_flow.rs::queue_text_block`, using text_block_prompt and the queued generator path. That prompt is rebuilt in `generator_pump.rs::capture_generator_inputs`. Do not attach selected text only to chat_send, where it would miss Text Agent runs.
2. Selection currently lives in egui Context temporary data so it survives focus changes. queue_text_block has no Context parameter. The economical continuation should move requestable selection into the existing FlowUi ephemeral owner (key by workbook/node), or explicitly snapshot it from the UI before submission. Keep request metadata keyed by exact request ID plus captured source/range/question, never only by node.
3. Snapshot selected context at queue time, include the quote in the actual text request, and revalidate at dispatch/completion. On matching successful reply, call record_text_answer with session + through index. A stale/cancelled/other-request completion must not link or overwrite document content. `board_agent/session.rs::pump_agent_sessions` is the current real result pump (it calls sync_agent_text_output); results.rs is mainly placement/Stop/menu helpers.
4. Consume take_text_answer_navigation in the common generator view path: open the existing agent panel and scroll to the exact checkpoint/response without resuming or restoring active memory. No new composer. This hook is not yet wired.
5. Export consumes the new durable ranges through the existing artifact text branch. Root owns render.rs.
6. Existing linked-file text windows use ImageNode/text_override and document editor; they have no TextNode.answers storage. The first patch targets authored TextNode agent notes. Decide one shared annotation owner for linked-file text before claiming parity for that second path; do not silently duplicate fields and rebase logic.

## Validation performed and outstanding

Standalone rustfmt completed on owned Rust files, and diff whitespace checks passed. Cargo was deliberately not run while the integrator coordinated resource-heavy baseline builds. Pure tests were added for Unicode cursor conversion, outside-edit rebase, invalidating edited passages, overlap/repeat queries, serialization and the authored_text mutation hook. They still need execution. Native selection/query/completion/navigation/undo/export golden paths and visuals are not yet certified.
