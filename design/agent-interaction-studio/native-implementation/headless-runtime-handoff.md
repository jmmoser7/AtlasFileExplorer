# Headless Rhino runtime handoff

## Actual blank-model GPU diagnosis (31 October, read-only)

`target/review/media-refinement/actual-headless-new.slate` retains a real 7377-byte
`.3dm`, nonempty cache key and Codex/Chat AgentPortalRef. Model classification
does not require drawable geometry. The wrong image editor comes from
`board_flow/editor_helpers.rs:71`: its `is_flow_node` gate uses the no-item-path
port resolver, which excludes Chat media, then falls into the default image
starter. Preserve the valid saved Model/Chat binding when fixing this UI route.

Blank completion already explicitly activates the isolated headless worker and
source binding. However ordinary attached-agent Send currently gains native
tools only when `agent_presence/chat_train.rs::chat_train_wires` finds a native
input wire and explicit Board access/Resume authorizes that source. The model's
own AgentPortalRef is not such a wire, so a new unwired builder still needs a
scoped host-context integration; an editor-routing fix alone is insufficient.
Activation does not grant authority. DraftPermit creation remains gated by the
existing actor/autonomy checks and isolated-draft Session admission. This
diagnosis changed no Rust sources and ran no Cargo.

## Capability-review corrections — authoritative current status

The three assigned findings are implemented in source, with regression tests
submitted to the serialized validator. **Compile/test clearance is pending**;
this worker ran no Cargo. Earlier sections below are historical checkpoints.

| Finding | Current change | Verification pending |
|---|---|---|
| P1 linked SourceUri omissions | `slate-doc::locators::linked` now owns one recursive visitor used by Save As, recovery and reversible relocation. It visits generator archived history, selected text, model outputs, Text/Image answer origins, and nested paint nodes while retaining existing scheme/absolute-path rules. It reads/copies no referenced bytes. | Extended Save As/reopen/failed-save/undo-redo fixtures, nested recovery fixture, and text-origin/scheme test. |
| P1 generated models not portable | Existing asset planner admits only explicitly linked `.3dm` files directly under app-data `headless-rhino/models` with matching public provenance. Existing reversible relocation rewrites matching catalog sources through that same visitor. Arbitrary/unmarked models, job snapshots and transcript siblings remain excluded; cloud/no-overwrite guards remain. | Core empty/A/B Collect + owned Save As test; app item/catalog relocation, undo/redo, removal of original copies and moved-workbook reopen test. |
| P2 historical selection then Send | Registered human Send queues leased preparation only for the selected catalog output in the existing isolated headless scope. Worker checks old binding/revision, snapshots the selected bytes, and uses the same explicit source unbind/rebind owner as accepted output. Provider Send waits for its matching binding event. Node/prompt changes, Stop, read-only, external source or stale key suppress continuation. | Registered-Send/preview-only/history-cut/no-membership test and read-only/external refusal tests. Full real A/B historical Send and revoked-membership continuation still require integration coverage. |

Source preparation does **not** create/resume membership, grant mutation
authority, restore cut conversation memory or alter any provider checkpoint.
Unbind clears native draft grants; ordinary Send/native.invoke subsequently use
their existing provider, actor, membership and scoped permit checks. The selected
geometry source is independent from conversation memory. Cancelled preparation
checks its lease again after file I/O before rebinding.

Provenance is the existing scrubbed `GeneratedSidecar`: empty prompt,
`model: "Rhino 8"`, no seed, `params.output_kind` equal to
`slate.headless-rhino.model.v1`, creation timestamp and leaf output run ID matching
the `.3dm` stem. No transcript, source snapshot/path, grant or secret is included.
It is written by blank/model output workers and copied only with that linked
public model; collection never recursively walks the helper/private folders.

Files touched for this correction: `crates/slate-doc/src/locators.rs`,
`locators/linked.rs`, new `locators/linked/tests.rs`, `recovery/tests.rs`;
`crates/atlas-core/src/workbook_assets.rs`, new
`workbook_assets/headless_models.rs` and `headless_models/tests.rs`;
`apps/slate/src/app/workbook_assets.rs`, new
`workbook_assets/model_collection_tests.rs`, `source_rebase_tests.rs`;
`application_connector/{headless.rs,headless/send.rs,worker.rs,
worker/headless.rs,worker/headless/rebind.rs,worker/headless/blank.rs,
native/test_support.rs}` and `board_agent/awaits.rs`. Existing helper discovery
remains `<slate.exe directory>/HeadlessRhino.exe` with repository-only dev/test
fallback; no packaging or geometry operations changed in this correction.

Exact test filters handed to validation:

- `text_answer_origins_rebase_but_remote_and_absolute_history_keep_identity`
- `save_base_and_recursive_sources_survive_without_opening_sources`
- `collect_only_provenanced_linked_models_and_save_as_owned_copies`
- `generated_model_catalog_collects_rewrites_undoes_and_moves_without_private_files`
- `save_as_and_reopen_preserve_artifact_targets_without_collecting_history_or_sessions`
- `failed_save_as_preserves_relative_sources_and_redo_snapshots`
- `historical_preview_only_then_explicit_send_prepares_without_memory_or_authority_changes`
- `read_only_or_external_selected_model_cannot_prepare_send`

The separate missing-selected-text export-placeholder finding belongs to the
native UI worker. Real SDK proof below remains valid geometry evidence, not
persistence/delivery or final native UI acceptance.

## HR1/HR2 follow-up — latest checkpoint

This supersedes the original limitations below. Source edits are formatted and
submitted for the next compile/review. Root check4 app lib passed before this
follow-up; no Cargo was run by this worker.

- HR1 now loads the trusted bound `.3dm` snapshot into isolated File3dm data,
  preserves unrelated geometry/tables/attributes, and offers explicit
  `reference {id,object_id}` targets for solid operations. `units:source` keeps
  existing units; incompatible explicit units are refused. Source reads are
  bounded and reject cloud placeholders. Relative material texture FileReferences
  become absolute against the original origin without opening/copying textures.
- Accepted outputs explicitly unbind/rebind through the existing Session and
  update the immutable source snapshot/origin. History selection does not rebind
  the kernel; explicit activation can branch a selected older revision.
- `board.model.headless.new` creates a real empty `.3dm` on a worker, links it
  via `link_ready_media`, and attaches ordinary AgentPortalRef using
  program_binding/bind_program. It inherits `default_model(false)` provider/model
  settings, with Codex only as fallback. Existing permissions remain required.
  The shared UI worker routed model Send through ordinary portal.agent.send.
- Defaulted `GeneratorSettings.model_outputs/shown_model` retain immutable
  `ModelOutputRef` SourceUri/metadata. APIs: `model_generator_outputs(id)`,
  `model_generator_preview_info(id,identity)` (existing poster/cache, no stat in
  paint), and `select_model_generator_output(id,identity)` (journaled selection).
  Shared history UI hookup belongs to the UI worker and was in progress.
- HR2 edits `tools/headless-rhino/build.ps1`, `scripts/build-release.ps1`,
  `scripts/package-windows.ps1`, and `scripts/verify-windows-package.ps1`.
  Optional SDK build produces `target/release/HeadlessRhino.exe`; packaging
  allowlists the helper and README beside Slate. Installed discovery uses
  `<slate.exe directory>/HeadlessRhino.exe`, with separate repository-only test
  fallback. Missing SDK/compiler skips this optional capability; ordinary Slate
  remains buildable. No Rhino DLL is copied; package checks reject those binaries.

Actual disposable metadata proof passed: `target/headless-rhino/isolated-proof`
contains `original.3dm`, successful `fresh/model.3dm` and `repeated/model.3dm`.
Fresh targeted edit moved the stable target GUID by 5; repeat used that accepted
output and moved it by 2, verifying the final target at 7. Both SDK runs and
independent `IsolatedEditingProof.exe` verifications exited 0. Unrelated sphere
GUID/geometry, millimeters, named layer/material, object attributes and rebased
missing relative texture locator survived. Each output has two objects, one
preserved object and seven cached face meshes (9919 vertices, 10358 faces).
Original SHA-256 stayed
`AAD6A4565263462422D9DE098CEBBF603D6C8E5DF327038A93A5B4863DE12B4C`.
Earlier failed `job1` attempts remain diagnostic evidence; `fresh`/`repeated`
are the final successful proofs. No user model was modified.

Helper build at the packaged location and PowerShell parsing of all four scripts
passed. Optional missing-SDK invocation exited 0 with an unavailable message.
The ignored Rust worker test now creates a real blank fixture, generates,
transfers/parses, accepts/rebinds and generates again, asserting preserved prior
geometry and unchanged original/first-output bytes; it still needs execution.

Remaining: next compile/focused tests, shared UI catalog hookup, actual native
blank/Stop/provider/permission/history/save-reload tests and captures. Review the
new catalog SourceUri fields in existing Save As/source-rebase and workbook
asset/export walkers for portability; those central owners were not edited here.
No complete 3D builder acceptance is claimed from SDK proof alone.

## Original checkpoint (implementation history)

30 October 2026. Implementation submitted for integration checks; native acceptance
is pending. Maps to DI-23 / A07 / A12. No commits or dependency/Cargo setting
changes were made by this worker.

## Owners and integration

- `tools/headless-rhino/{HeadlessRhino.cs,build.ps1,README.md}`: installed Rhino 8
  SDK bridge, x64 STA/headless isolated document, closed operation dispatch,
  original Breps with cached render meshes, saved-output re-read verification.
- `crates/atlas-connector-runtime/src/headless_rhino.rs` and
  `headless_rhino/{plan,worker,session}.rs`: strict typed plans, serialized bounded
  owned helper lifetime, cancellation/progress, existing v0.2 `Session` admission,
  isolated draft grants, revisions and authenticated artifact publication.
- `apps/slate/src/app/application_connector/{headless.rs,worker/headless.rs}`:
  existing connector worker/session and native ticket integration; native preview
  parsing and journaled model-item acceptance.
- Narrow changes to existing `application_connector/{worker.rs,actions.rs,
  native/types.rs,native/retention.rs,worker/native.rs}` and
  `agent_presence/portal_ops/native.rs`: typed `NativeResult::Model`, cleanup,
  invocation and publication after current actor validation and `Lease::publish`.
- Native integration worker applied runtime module declaration, app headless
  module/state/pump, exclusion from full-UI presentation/focus, and three command
  registrations. This worker added the exact `Event::Progress` match arm.

The initial separate pending-controller proposal was replaced by the existing
`application_connector` Session/native ticket/DraftPermit/Lease/native.invoke
owners. Only a small human-command pending map waits for that same ticket path.
There is no new agent protocol, viewport, mesh parser or competing authority.

## Native callable path

Explicit human `board.model.headless.activate` binds a placed model as a new
isolated headless computation target. The linked source is not imported or
overwritten. `board.model.headless.generate` accepts detail
`{"node":123,"parameters":{"plan":"<GeometryPlan JSON>"}}`; `.stop` revokes
owned pending leases. An agent discovers the command through the existing native
inspect/invoke surface after explicit headless activation and must retain its
existing source scope, autonomy and matching isolated-draft permit.

The closed operation set is box, sphere, cylinder, planar extrusion, move,
rotation, uniform scale, Boolean union and difference. References name earlier
IDs. Transforms/Booleans consume input outputs; terminal Breps retain operation
IDs and cached face meshes. The existing scalar-only connector schema admits a
strict `plan` string of at most 4096 characters; detailed typed validation occurs
before process launch. Unsupported operations/unknown fields are errors.

Completed bytes pass the existing ArtifactStore/PublicationResult,
ArtifactSender/receive_artifact checksum and receipt contract, then the production
`model-preview` loader. The app rechecks source/key/caller and publication lease,
then journals only the model node's item change. Frame/style/authored camera and
the previous source survive; Undo restores the previous item. Headless sessions
do not attach/focus Rhino surfaces and the Slate model renderer remains active.

## Build and checks

SDK helper build succeeded, exit 0, with no installed packages:

```powershell
./tools/headless-rhino/build.ps1
```

Uses installed `C:/Program Files/Rhino 8/System/RhinoCommon.dll` and .NET Framework
x64 `csc.exe`; commercial binaries stay at the installed location. Runtime
default deadline is 180 seconds (observed cold startup approximately 35 seconds).
Stop cooperates at SDK boundaries; after five seconds it can terminate only its
owned helper and reports an uncertain isolated result with no publication.

Rust formatting ran using `C:/Users/jeffr/.cargo/bin/rustfmt.exe`. No Cargo
process was started by this worker; root owns serialized validation. The first
root app check reported unrelated atlas-shell visibility failures and no runtime
headless diagnostics; it is not a successful headless compile claim.

Ready focused tests:

```powershell
cargo test --locked --release -p atlas-connector-runtime headless_rhino
cargo test --locked --release -p slate headless
cargo test --locked --release -p slate real_headless_generation_transfers_and_loads_native_preview -- --ignored --nocapture
```

Runtime tests cover strict scalar descriptor, invalid/unknown/path-bearing plans,
nonfinite/degenerate/unbounded dimensions, forward references/duplicate IDs,
valid transforms and agent admission without a draft grant. App tests cover
frame/camera preservation with one Undo and stale-key acceptance refusal. The
ignored real worker test exercises actual SDK generation, verified transfer and
the production native preview loader against a temporary fixture (no user model
is replaced). Root/validator must report the actual results.

## Retained actual SDK proof

- `target/headless-rhino/proof`: box minus sphere, then moved [1,2,0]. Helper
  exited 0. Real .3dm re-read: meters, one terminal Brep, seven cached meshes,
  1187 vertices, 1092 faces, bounds [1,2,0]..[13,10,3].
- `target/headless-rhino/proof-all`: all nine operations, including union,
  difference, all transforms and extrusion. Saved .3dm re-read verified original
  Breps and persisted cached meshes: millimeters, three terminal Breps, 16 cached
  meshes, 1458 vertices, 1272 faces. Progress/result files retained. Owned helper
  PID 22460 had MainWindowHandle 0 during startup and subsequently exited.
- Existing baseline STA probe is retained under
  `target/studio-native-baseline/20261030-204228`.

## Gaps and review requirements

The submitted Rust/app integration still needs root's compile and real ignored
test, native matched-binary gesture/screenshots, cancellation/missing-license/
timeout checks and independent DRY/capability review. SDK proof alone does not
certify native UI acceptance. There is no new natural-language geometry planner
or blank 3D host: this submission exposes bounded real generation through an
existing placed model's native command surface. A provider must produce the
closed plan through the existing tool adapter. Rhino installation/license
availability is reported honestly; there is no interactive fallback or canned
geometry. Broader Rhino API operations, imported sources, parallel licensed
kernels and arbitrary script execution are intentionally unavailable.

Rollback: remove these new helper/runtime/app child modules and narrow hooks;
leave the existing interactive connector and native renderer intact. Retain
generated evidence under ignored target. Do not reset unrelated working changes.
