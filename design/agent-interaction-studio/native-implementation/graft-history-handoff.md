# Grafted inputs and optional generator history — implementation handoff

30 October 2026. Native acceptance remains pending. No commit was created.

## Existing owners extended

- `slate-doc::agent_inputs::plan_grafted_inputs[_with_shared]` clones the existing `InputSnapshot` into stable longest-matched branches. Short channels repeat their last reference; ungrafted channels remain bundles. Declared Table/Control/View sources remain shared. An empty grafted channel rejects submission. Deferred web-image references remain one reference until capture completes.
- `board_agent/generator.rs` uses the existing bounded generation queue. Image Count remains the provider's count on every branch; Text Count creates independent queued requests for every branch/result. Branch request IDs, captured input revisions, selected sources, model parameters, seed and existing output/request provenance retain identities independently of completion order.
- `board_agent/input_prep.rs` shares one `GeneratorInputCapture` among a submission's branches. The existing web/model/video capture methods populate each dynamic source once, retaining completed captures while another source loads. Each branch receives those frozen values. A subsequent submission gets a new capture state. This freezes values at the initial successful capture, not necessarily at the pointer's exact submission instant.
- Artifact plans use the existing preparing/running/publishing lifecycle, bounded at two active workers. Count expands each graft branch; the batch is capped at eight jobs. Plans retain original content and memory references and independent execution sessions. Stop removes pending work, and completed accepted revisions remain. A source, prompt, model or settings change invalidates the remainder rather than silently changing its snapshot. Successful publication uses the existing journaled acceptance transaction.
- `wire_host` owns the 48/90/132 input datums and four-terminal capsule math. `wire_scene_route` derives stable capped terminal offsets for both native/export consumers without changing saved slot/t identities. Ordinary wire colors remain native.
- `spawn_flow` creates blank downstream generators with menus closed and no unsolicited prompt focus or generation. Newly independent generators explicitly start with preservation disabled. Missing settings retain legacy behavior.
- `generator_history_is_terminal` checks outgoing data/history consumers. Generator-memory cuts preserve data wires and completed output references. Chat Train rails remain immutable. The cut journals the new session, bundle reference, empty provider channel and preservation=false in one transaction; Undo restores the previous document state.
- A fresh session manifest is seeded with retained image output records only. Sources are resolved through the existing workbook source resolver and retained as absolute links. No media bytes, transcript turns, grants or provider conversation ID are copied. Existing transcript manifests are retained in `GeneratorSettings.archived_history`; `history_cut` prevents archived artifact checkpoints from becoming active again on re-enable.

## UI adapter APIs

- `SlateApp::set_input_grafted(Some(detail)) -> bool`, detail `{ "node": u64, "slot": "media"|"prompt"|"style", "grafted": bool }`; registered command `board.input.graft`.
- `SlateApp::graft_input_count(node, slot) -> Option<usize>` distinguishes a circle from a grafted capsule.
- `wire_host::graft_capsule_height(incoming)` returns 12–24 world units; `graft_terminal_offset(index,incoming)` uses 4-unit pitch and caps subsequent wires at the fourth terminal.
- `agent_inputs::archived_generator_history(node) -> &[SourceUri]` supplies existing manifest references for inspection through the existing source/transcript reader. These references are **stored and exposed, but this worker has not added an archived-transcript display**. They must never be fed into inference.

The UI integration worker owns Graft inputs menu placement and shared host capsule cutouts. The growing capsule painter and routing math are present; final cutout/hover parity needs their integration and native visual review.

## Tests and evidence

Added pure matching/shared-bundle/isolation/empty-channel, capsule-cap and terminal graph tests. Added synthetic artifact lifecycle tests for Count/independent sessions, Stop, stale source changes and cut→re-enable→fresh request with Undo. Existing artifact lineage fixtures explicitly select legacy retention semantics so compatibility remains separately exercised.

Rustfmt completed with exit 0. Cargo was deliberately serialized by the coordinator. The current app check and focused test results must be read from the validation worker's retained logs; no passing build, native visual parity or live provider claim is made here.

## Remaining restrictions and required review

1. Cross-modality history remains restricted: artifact planning accepts artifact checkpoint memory pins; a generic image/text memory pin targeting an artifact is not adapted by that planner. Artifact→image/text inheritance currently depends on the source's loaded session turns rather than loading the exact immutable artifact revision checkpoint. Do not describe either path as full cross-modality history parity.
2. Archived transcript display and result-level provenance inspection remain UI integration work. Branch provenance currently uses existing request/output records and stable branch IDs/revisions rather than a new output-specific branch metadata structure.
3. Dynamic capture is asynchronous and uses existing availability/error handling. Native/provider tests must establish real capture fidelity, independent request execution, partial failures and retained historical output selection. Synthetic tests do not establish those properties against a real provider.
4. Matched native captures, reload/export/undo checks and the acceptance gates in `ACCEPTANCE.md` remain required. No completion claim is warranted until these restrictions and validation findings are reconciled.

## Capability review corrections — GR1–GR4

The canonical `graft-capability-review.md` was present and read. The following source corrections and synthetic regressions are landed, formatted, and **not yet tested on final integrated source**. The coordinator reported check4 exit 0 before these corrections; that result does not validate this packet.

- **GR1:** `Plan.authored_prompt` captures the exact editor/authored value before trimming/composition. Provider `request.prompt` remains the immutable composed/grafted value. Start journals only the authored value, preserving the semantic guard. Added `gr1_wired_only_grafted_count_survives_prepare_start_and_publish_without_authoring_prompt` and `gr1_actual_running_prompt_edit_rejects_publication`. The first exercises four Count/graft jobs through real prepare/start/publication adapters using explicitly synthetic output. Updated the older feedback-during-preparation test to require stale-request rejection while preserving the newer draft.
- **GR2:** `agent_input_refs` calls the UI owner's `resolved_selected_text_output -> Result<Option<String>,String>`. Only Ok(None) uses legacy behavior; an explicit missing/invalid source/checkpoint errors when that source is actually consumed. Native Text precedence and the live edit override honor explicit selection. Added `selected_historical_text_overrides_authored_source_for_plain_and_grafted_inputs`, `missing_or_invalid_explicit_text_selection_errors_instead_of_using_latest`, and `archived_selected_text_reloads_through_existing_source_cache`. **Dependent UI correction remains:** while explicitly selected, display must match this selected text; beginning/authored editing must clear `shown_text` through the existing edit/journal owner. The UI integration worker owns that correction and its edit-after-select test. Do not waive GR2 while display/edit precedence disagrees with wire precedence.
- **GR3:** added optional, serde-compatible `ArtifactGeneratorRef.active_memory: Option<ArtifactMemoryPin>`, a reference to existing immutable history, independent of the shown content revision/checkpoint. Only successful fresh generation publication updates it and clears `history_cut`; revision browsing preserves both. Explicit Preserve planning and downstream memory wires use the active checkpoint; legacy None retention keeps legacy behavior. A cut clears active memory and leaves retained content accessible. Added `gr3_browsing_old_content_never_reactivates_cut_memory_and_fresh_memory_stays_separate`, including fresh generation, old output browsing and Undo/redo.
- **GR4:** a cut refuses while provider work is dispatched and instructs Stop first, preventing an in-flight old execution from rebinding. A successful cut invalidates this target's queued image/text Count/graft snapshots, shared pending capture, artifact preparation/backlog, request identity and passage snapshots through existing lifecycle helpers. Undo restores document references without restarting canceled work. Added `gr4_cut_invalidates_pending_count_capture_without_removing_outputs_or_restarting_on_undo` and `gr4_cut_invalidates_artifact_preparation_and_ignores_its_late_completion`. Other targets and upstream sessions remain untouched.

Additional follow-up tests cover Text Count execution, shared frozen dynamic input values, semantic movement/camera/preview immunity, actual prompt edits, and retained output after a failed artifact branch. All are synthetic lifecycle tests; real provider/capture/native evidence remains required.

## Exact implementation file inventory

Relative to the repository root, this worker changed or added:

- `crates/slate-doc/src/agent_inputs.rs`
- `crates/slate-doc/src/agent_inputs/graft.rs`
- `crates/slate-doc/src/agent_inputs/generator_history.rs`
- `crates/slate-doc/src/agent_inputs/snapshot.rs`
- `crates/slate-doc/src/scene/agent.rs` (history settings, archive references, shown-text field; later model catalog edits belong to the headless worker)
- `crates/slate-doc/src/scene/artifact.rs` (active-memory reference)
- `crates/slate-doc/src/wire_host.rs`
- `crates/slate-doc/src/wire_host/rail_tests.rs`
- `crates/slate-doc/src/wire_scene_route.rs`
- `apps/slate/src/app/board_agent/generator.rs`
- `apps/slate/src/app/board_agent/generator_pump.rs`
- `apps/slate/src/app/board_agent/input_prep.rs`
- `apps/slate/src/app/board_agent/inputs.rs`
- `apps/slate/src/app/board_agent/input_prep_tests.rs`
- `apps/slate/src/app/board_agent/input_selection_tests.rs`
- `apps/slate/src/app/board_agent/relay_draft_tests.rs` (new preparation field in existing fixtures)
- `apps/slate/src/app/board_agent/artifact.rs`
- `apps/slate/src/app/board_agent/artifact/generation.rs`
- `apps/slate/src/app/board_agent/artifact/pump.rs`
- `apps/slate/src/app/board_agent/artifact/spawn.rs`
- `apps/slate/src/app/board_agent/artifact/tests.rs`
- `apps/slate/src/app/board_agent/artifact/tests/grafts.rs`
- `apps/slate/src/app/board_agent/artifact/tests/graft_review.rs`
- `apps/slate/src/app/board_agent/artifact/tests/feedback.rs`
- `apps/slate/src/app/board_flow/ports.rs`
- `apps/slate/src/app/board_flow/spawn_flow.rs`
- `apps/slate/src/app/board_wire.rs`
- `apps/slate/src/app/board_wire/drag.rs`
- `design/agent-interaction-studio/native-implementation/graft-history-handoff.md`

Other workers' shared-file edits were retained. No Cargo command, commit or staging was performed by this worker.

## DI17 active terminal cut correction — 31 October 2026

This section supersedes the earlier GR4 Stop-first behavior, which Design Intent rejected. A terminal cut now performs cancellation itself; no separate user Stop action is required. Interior checkpoints and immutable Chat Train rails retain their refusal semantics.

The existing stop owner has the narrow `cancel_generator_history_work(node)` adapter. The cut validates/prepares its journal commands, then calls request-scoped Stop while the original session and source directory are still installed. It revokes existing headless native-ticket leases and retires all this target's queued/capture/preparation/request snapshots before committing the single cut journal transaction. `live=false` belongs to that same patch, avoiding a second live-mode undo action. Accepted model/image/artifact outputs and ordinary data wires remain linked.

Late artifact worker results are rejected by existing preparing/running request ownership after cancellation. The existing session-source installation path additionally checks `generator_session_is_current` before applying channel, transcript, image or output-cache state. It refuses canceled/stale generator requests, including after document Undo, while permitting an independent sibling's current request. Undo restores document references only; canceled inference and queued siblings stay canceled.

A legacy artifact can share a provider runtime/session with an upstream host. If another host currently owns active work on that runtime, this cut does not send a runtime-wide Stop that would cancel the sibling. It revokes/invalidates this target's invocation/publication ownership and switches only this target to a fresh execution source. The detached old runtime may finish its already-dispatched computation; its response cannot publish into the cut target or restore active history. This is the concrete request-scoping limit of the existing runtime-wide stop signal, not a requirement for another user action or a new controller.

Exact Rust files changed in this correction:

- `apps/slate/src/app/board/board_mutate.rs`
- `apps/slate/src/app/board_agent/results.rs`
- `apps/slate/src/app/board_agent/artifact/spawn.rs`
- `apps/slate/src/app/board_agent/session.rs`
- `apps/slate/src/app/board_agent/input_selection_tests.rs`
- `apps/slate/src/app/board_agent/artifact/tests/graft_review.rs`

Generic `board.delete` now shares `prepare_generator_history_cut` with the snipping/explicit cut owner. Selected generator-memory wires whose target is also being deleted retain normal target-deletion behavior. Otherwise the shared terminal validator supplies the cancellation/context patch. Additional incoming memory-wire removals are expanded through the existing deletion planner so index ordering, ordinary node deletion and mixed multi-selection remain one journal transaction. An interior wire refuses the deletion before cancellation or scene mutation.

Meaningful synthetic regressions added/updated:

- `gr4_cut_invalidates_pending_count_capture_without_removing_outputs_or_restarting_on_undo` now also exercises active Images and Text cancellation, delayed response admission refusal, live-mode reset in the cut transaction and Undo without restart.
- The same test selects the actual memory line plus an ordinary node and dispatches `board.delete`, requiring one undo step and restoration of both selections without restarting work.
- `di17_cut_does_not_cancel_a_shared_runtime_siblings_current_request` checks sibling request/awaiting state survives, its response remains admissible, and target responses remain refused after Undo.
- `di17_cut_revokes_headless_ticket_before_delayed_model_completion_can_publish` uses the existing in-memory native peer, a dedicated headless surface and real ticket/lease admission; completion arrives after the cut and its publication lease remains revoked. No Rhino/provider execution is claimed.
- `di17_active_artifact_cut_cancels_count_queue_and_rejects_delayed_publication` exercises a running artifact/count batch, preserves the accepted revision, then delivers delayed publication and verifies no source/history restoration; Undo does not restart the batch.
- `di17_delete_refuses_interior_history_but_deleting_its_target_keeps_normal_semantics` dispatches the actual Delete command for an interior edge and then for the edge plus its target, verifying refusal versus unchanged normal deletion/Undo behavior.

Rustfmt completed with exit 0. No Cargo or test command ran in this correction; validator-owned execution and independent source review remain pending. No schema, provider protocol, connector runtime or UI painter/controller changes were introduced by this correction.

DI17 P2 lock follow-up: the shared preparation owner now refuses a locked surviving target (and a read-only workbook) before preparing a fresh manifest, cancellation or document mutation. Added actual-command regression `di17_delete_refuses_to_cut_a_locked_surviving_generator_before_canceling_work`: an unlocked history line cannot alter its locked target or cancel that target's active request; normal explicit whole-target deletion remains on the unchanged deletion planner. Only `artifact/spawn.rs` and `artifact/tests/graft_review.rs` changed for this follow-up; rustfmt completed successfully, execution remains pending.
