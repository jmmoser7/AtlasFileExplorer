# Generator command parity — current handoff

31 October 2026. Parity source applied; compiled validation pending. No native acceptance or current-source test claim. No commit created.

## Current source and validation boundary

- Production contract reconciliation is updated in `docs/keymap/contracts/artifact-generation.md` (Command, introduction and D08) and the matching approved D08 behavior in `decisions.json`. It records the already-reviewed Count-per-graft-branch, shared-bundle, longest-list matching and bounded artifact-job behavior; it introduces no new design decision.
- The validation worker explicitly released its Rust source freeze after building the pre-parity app-test executable. All 13 original Rust files then matched their recorded SHA256 values; the exact staged patch and one new test module were applied once to the actual repository, preserving original newline conventions. No Cargo command has been run by this worker. The pre-parity executable's results remain attributed to its earlier source snapshot: its initial generator chrome group had eight passes and two failures (access UI enabled state and text-output undo depth), assigned separately to the native integration worker.
- The reviewable implementation/baseline remains at `C:/Users/jeffr/source/repos/FileAtlas/.codex-command-parity-stage-20261031/{baseline,staged}`, with `command-parity-draft.diff`, original `source-hashes.json` and `applied-source-hashes.json`. The independent regression reviewer cleared the draft adapters after bounded identity/slot metadata and human-versus-agent model cleanup corrections. The final model-host guard only narrows model selection to image hosts, ensuring linked-item/source-grant checks apply; the final additional test checks pre-inspection endpoint denial. This is source review, not compiled/tested evidence.
- Applied source exposes `portal.agent.generator_settings`, `board.input.graft`, `portal.agent.text_output` and `portal.agent.model_output` through enumeration, typed targeted schemas and ordinary witnessed adapters under Constitution VII.1; human SPECS/dispatch already contain these IDs.
- The pre-parity model Collect test also failed its complete catalog equality assertion after Undo because the fixture's raw Windows locator used backslashes while the existing rewrite journal returned forward slashes. The fixture now uses the production `source_locator` owner plus canonical forward slashes when constructing expected `SourceUri`s. The full IDs/source/size/mtime equality and actual Collect/move/reopen checks remain; no production locator behavior changed. This root-authorized correction was applied/formatted after the initial parity manifest and communicated to the validator before a new source snapshot.
- The documented numeric bounds are artifact-specific: `board_agent/artifact/generation.rs:4` owns `MAX_IN_FLIGHT = 2`, lines 50–53 reject a planned submission exceeding eight expanded jobs, and `board_agent/artifact/pump.rs:70` caps preparing/running admission at two. Its publishing worker checks at lines 312 and 534 also cap active work at two. Image/Text generation retains its separate existing scheduler and is not assigned these numbers by this contract update.

## Planned production owners and safeguards

Use existing generator/graft/text/model mutation owners to prepare ordinary scene patches; use targeted preparation, real scoped grants, witnessed execution, staging/autonomy, journal authorship and stale-target checks to apply them. No raw dispatch bypass, new authority controller, headless lifecycle command, provider secret or ephemeral answer-navigation command is in scope.

Model selection must resolve an existing immutable catalog identity to its registered workbook item without filesystem reads or item registration during preparation. Normal publication already registers output links. The human selection path retains its existing missing-link recovery; targeted selection will report a missing registration explicitly instead of choosing a latest output or arbitrary path. Validate persistence through Collect/save/reopen before accepting this boundary.

Text checkpoint identity metadata and selection must honor the existing conversation endpoint grant. Descriptors may expose bounded opaque identities and current generator metadata, never transcript bodies, source locators or secrets. Model display changes retain the existing linked-input permission.

## Exact applied source files

All paths below are relative to `apps/slate/src/app/` and exist in both scratch trees except the new test module:

- `agent_presence.rs`
- `agent_presence/calls.rs`, `agent_presence/proposals.rs`, `agent_presence/read_descriptors.rs`, `agent_presence/tests.rs`
- `dispatch/targeted.rs`, `dispatch/targeted/prepare.rs`, `dispatch/targeted/schema.rs`
- `board_flow/ports.rs`
- `board_agent/generator_chrome.rs`, `board_agent/generator_chrome/text_outputs.rs`
- `application_connector/headless/catalog.rs`
- `workbook_assets/model_collection_tests.rs`
- New: `agent_presence/generator_command_tests.rs`

The shared helpers prepare scene patches without document mutation. Model preparation uses a previously registered catalog item; missing registration returns an actionable human-preview recovery error. Accepted targeted model selection invalidates the preview cache without invoking `unlock_model`, which can enter native/application contents focus. The human adapter retains that pre-existing UI behavior. Text output endpoint permission is checked before checkpoint inspection and again during witnessed/proposal precommit validation. Model selection retains the existing linked-input grant and linked-item stale check. Descriptors expose bounded opaque output/checkpoint IDs and offered graft slots; overlong exact IDs are skipped, never truncated.

## Pending exact checks

Six new tests under the filter `app::agent_presence::tests::generator_commands`:

- `generator_commands_share_human_patches_with_witnessed_approved_agent_journals`
- `generator_command_schema_and_target_validation_fail_without_mutation`
- `generator_command_grant_revocation_blocks_the_accepted_patch`
- `text_output_scope_is_checked_before_private_checkpoint_validation`
- `generator_command_descriptors_preserve_conversation_and_source_boundaries`
- `missing_model_registration_and_stale_preparation_never_fallback_or_mutate`

Extended existing test filter: `generated_model_catalog_collects_rewrites_undoes_and_moves_without_private_files` verifies typed immutable model selection after Collect, save, move and reopen. Also retain immediate existing generator chrome, graft/history, artifact graft, targeted-command and presence proposal regressions as selected by the sole validation worker.

No parity Rust test has yet been compiled or executed. The validation worker is the sole Cargo owner. `rustfmt --check --edition 2021 --config skip_children=true` passed on all fourteen applied source files; narrow `git diff --check` passed. Contract JSON parsing and its documentation diff check passed. Update this source-versus-tested boundary after validator results; native acceptance remains pending.
