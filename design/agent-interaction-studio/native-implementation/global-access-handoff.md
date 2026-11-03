# A04 / DI-05 global access — current handoff

## Authoritative follow-up: admission coherence and independent relay bootstrap

Both confirmed P2 follow-up gaps have source fixes. `spawn_chrome::refresh_agent_access` reloads the existing small machine-local preference snapshot outside paint, restoring full/session/global caches and the independently enumerated saved relay set. Startup and the test restart seam use that same owner. Ordinary Send refreshes before binding; actual shared `submit_prepared_agent_request` refreshes again at admission after deferred preparation. Crosstalk Start refreshes before its existing full/read-only/relay refusal checks.

Cursor admission builds its SidecarPolicy from this observed snapshot and retires changed idle/booting sidecars through the existing owner. Codex admission sends the same snapshot through its existing in-process runtime channel; it is never added to AgentRequest/request.json, workbook, journal or synced sources. `runtime::request_full_access` enforces the existing read-only/relay restrictions against the observed snapshot; callers without a host observation retain machine-store resolution. AccessView uses the admitted existing policy cache while a request is running, so later preference edits cannot falsely recolor its actual authority. An idle different process refreshes at its next admission rather than probing the filesystem while painting. This fixes the concrete hidden-authority mismatch at invocation; continuous idle UI synchronization is not claimed.

Relay restoration now uses `Snapshot::relay_sessions`, independently of `full_sessions`. A global-only Cursor full grant plus a separately saved relay grant survives startup. Global access never creates relay authorization; absent/revoked relay still refuses unrestricted Cursor crosstalk Start, and explicit full-access denial still leaves the builder restricted.

Additional changed files: `board_agent/submission.rs`, `prompt.rs`, `crosstalk.rs`; `atlas-ai/src/runtime.rs`, `runtime/tests.rs`, `sidecar.rs`. New exact filters:

- `cargo test -p slate admission_refreshes_other_instance_choices_and_pins_the_submitted_policy --lib`
- `cargo test -p slate restart_retains_independent_relay_grant_for_a_globally_allowed_cursor --lib`
- `cargo test -p atlas-ai admitted_snapshot_retains_codex_policy_and_never_overrides_turn_restrictions --lib`

Rustfmt parse and targeted diff whitespace checks passed. No Cargo, provider execution or native UI certification by this worker; compiler, focused execution and independent source review remain pending. The historical cache limitation paragraph below describes the first checkpoint and is superseded by this follow-up at turn admission.

Source checkpoint: 30 October 2026. Moderate surgical extension of the existing permission preference and shared Access UI owners. Rustfmt parsed the affected Rust sources; no Cargo, app execution, commit, staging, reset or native UI certification was performed by this worker. Protected dirty baseline: `target/studio-native-baseline/20261030-204228`.

## Behavior and compatibility

`atlas-ai::access::Snapshot` owns the new optional `global_default_full` value in the existing per-user `agent-access.json` (version 3). An absent value preserves the previous artifact-only `artifact_default_full` behavior: an existing dashboard preference cannot silently grant ordinary Chat Train, Image, Text or headless-model provider authority. The shared model-menu row now explicitly says **Full access · all agents**. Its first explicit choice persists the new preference. A false global choice supersedes the legacy dashboard default; explicit conversation grants remain supported. Explicit session false overrides always win. Existing per-conversation Chat Train controls remain per conversation.

Global default and effective grant remain separate in AccessView. Existing artifact-only authority can show effective full access with the global checkbox off; read-only/crosstalk restrictions keep effective access off even while the global default is on. The existing command ID `portal.artifact.full_access` is retained for compatibility; its registered description and behavior now apply across hosts. No additional permission controller, storage file or workbook field was introduced.

`Snapshot::granted` and `granted_for_request` apply the global choice to fresh sessions across host request shapes. Artifact requests additionally retain the legacy behavior until a global choice exists. Existing runtime and Cursor sidecar next-turn code use those owners; repeated covered requests retain the same preference. The ordinary Slate `agent_full_access` adapter uses the effective snapshot rather than only the enumerated session set. UI snapshots reload at startup and after local preference edits, as before; provider runtime rereads the machine-local store for subsequent turns. Already running turns retain their original policy. Simultaneously open app UI caches do not yet live-refresh a setting changed by another process.

Native connector admission, live membership, provider/source identity, scope, source writeback and read-only ownership checks are unchanged. This preference is not a connector permit. Relay remains a separate explicit grant, and `runtime::turn_full_access` still refuses read-only turns and relayed requests lacking relay authority. The new headless UI fixture verifies its connector binding is unchanged by the preference.

## Changed owners

- `crates/atlas-ai/src/access.rs`: versioned preference, shared effective resolution, migration/roundtrip/request-boundary regressions.
- `apps/slate/src/app/board_agent/spawn_chrome.rs`: ordinary agent effective grant adapter.
- `apps/slate/src/app/board_agent/generator_chrome.rs`: shared AccessView for all agent-bound hosts and existing preference command; unrelated exact-origin hunk belongs to integration worker.
- `apps/slate/src/app/board_agent/generator_chrome/tests.rs`: preference and all-host UI regressions; other unrelated tests belong to integration worker.
- `crates/atlas-shell/src/selection_tools/generator_access.rs`: AccessView contract comment.
- `crates/atlas-shell/src/selection_tools/generator_picker.rs` and `menu_selector/generator_tests/picker.rs`: compact global label and matching real-menu tests only.
- `apps/slate/src/app/commands.rs`, `COMMANDS.md`: existing command description.

## Exact focused validation (validator owns Cargo)

- `cargo test -p atlas-ai access:: --lib`: includes `global_access_roundtrip_covers_new_hosts_repeated_requests_and_keeps_turn_boundaries`, `legacy_dashboard_default_does_not_expand_until_an_explicit_global_choice`, and existing per-session/legacy/relay tests.
- `cargo test -p atlas-ai relayed_turns_need_the_relay_grant_and_read_only_never_gets_full_access --lib`.
- `cargo test -p slate global_default_is_machine_local_and_keeps_explicit_denials --lib`.
- `cargo test -p slate global_access_ui_covers_all_hosts_and_survives_reload_without_connector_authority_changes --lib` (Image, ordinary Chat Train, Text, Dashboard, attached headless 3D; scene/journal and connector binding unchanged; fresh independent app snapshot).
- `cargo test -p slate full_access_is_a_local_grant_per_conversation_and_never_journaled --lib`.
- Existing `atlas-shell` picker/menu tests should run with their updated all-agent label.
- Preserve existing identity/provider/scope and revoked-membership regressions in `agent_presence` and native connector tests; no new authority bypass was introduced here.

Pending: compiler/test execution and independent source gate. Native permission interactions, restart behavior, simultaneous-process UI cache behavior and sustained provider operations are not certified. The repeated prompt loop was not reproduced; the source-only global preference gap was confirmed and repaired. Roll back only the named A04 hunks against the protected dirty baseline if required; never restore whole shared files.
