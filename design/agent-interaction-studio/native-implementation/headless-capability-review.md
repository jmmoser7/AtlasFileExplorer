# Headless and durable-source capability review

Date: 30 October 2026. Capability moderator. **Verdict: revise.**

Scope: read-only source review of `tools/headless-rhino`, the runtime's
`headless_rhino` module, Slate's headless worker/publication/catalog adapters,
the three packaging-script deltas, and immediate durable-source consumers.
No code changes, builds, SDK runs or tests were performed by this reviewer.
This document is the only review artifact written. This is a moderate targeted
review within the difficult native integration, not a whole-repository audit.

Comparison uses the protected dirty baseline
`target/studio-native-baseline/20261030-204228/{manifest.json,tracked.patch,untracked}`,
whose HEAD is `a75990653b5cf97484d03c2c36d34920f615a831`, rather than assuming
HEAD contains all pre-existing capabilities. The new generator fields and
headless module/helper are absent from that baseline; the existing locator and
asset planner owners below have no working-tree delta and were not extended
with those new fields/types. Governing references read: capability-baseline,
shared-owner-plan, design-intent, headless-runtime-handoff and ACCEPTANCE.

## Actionable findings

### P1 — Save As and recovery omit the new linked SourceUri fields

Owner: `crates/slate-doc/src/locators/linked.rs:61–82` (`rebase_node`).
Added fields: `scene/agent.rs:63–74`.

The existing traversal rebases portal.source, artifact.history_root/workspace,
agent.bundle and agent.seed. It never visits
`generator.archived_history[*]`, `generator.shown_text.source`, or
`generator.model_outputs[*].source`. Consequently a relative reference that
resolved under the old workbook directory resolves against the new directory
after Save As. This affects both current nodes and undo/redo node snapshots:
`rebase_linked_command` delegates to the same incomplete traversal. Recovery
`absolutize_document` also delegates to it. An archived transcript, selected text
reply or model revision may become unavailable or refer to a different file.

Minimal fix: extend this existing traversal once, including nested paint nodes,
and keep scheme/absolute-path behavior in `rebase_locator`. Rebase locators only;
do not read or copy transcript/history bytes. Artifact `active_memory` is an
`ArtifactMemoryPin { revision, through }`, **not a SourceUri**: it needs no extra
path walker; its containing history_root remains the source-family owner.

Required tests: extend `workbook_assets/source_rebase_tests.rs` and slate-doc
recovery tests with all three new relative reference kinds. Assert the same
resolved targets after cross-directory Save As, reopen, undo/redo and recovery;
failed Save As preserves all spellings and journal snapshots. Include a nested
paint node and a decoy at the destination to detect wrong-file resolution.
Confirm private history/session directories were not copied.

### P1 — Generated native model files do not travel with saved workbooks

Owners: `crates/atlas-core/src/workbook_assets.rs:381–403`
(`plan_data_dir_asset`), `crates/slate-doc/src/locators.rs:101–119`
(`rewrite_output_sources`), and
`apps/slate/src/app/application_connector/headless.rs:244–286`.

Both blank creation and accepted results write `.3dm` under machine-local
`headless-rhino/models`. They add ordinary items and durable model-output
references. Save/Collect reaches those items through
`workbook_assets.rs:385`, but the existing core planner explicitly refuses
anything except raster images and owned capture videos. Thus these generated
model artifacts remain linked to one machine's app-data directory after Save
and Collect. Moving the saved workbook to another machine loses the model and
its history. This is generated output, so the linked-original exception does
not supply delivery.

There is a second necessary half of the fix: `RewriteLocators` updates items
and agent.seed only. If collection is enabled without extending that owner,
the viewport item can point at a portable copy while the same output's
`ModelOutputRef.source` still points at app data. Choosing its preview then
switches back to the old locator.

Minimal fix: teach the existing asset planner a narrowly scoped generated-model
case and extend the existing reversible locator rewrite to all matching model
catalog sources, including initial empty models. Preserve ordinary linked
originals. Collect only accepted immutable model files, never the helper's
`jobs` tree, source snapshots, source-root metadata, private transcripts or
grants. Preserve the existing cloud guard and no-overwrite behavior.

Required tests: Save/Collect an empty builder and two accepted models, remove
the original app-data copies, reopen from a moved workbook directory and select
each preview; both item and catalog resolve the copied bytes. Undo/redo the
locator rewrite and test Save As copying already-owned assets. Assert linked
originals and private sibling job/session files were not copied. Existing
selected-model HTML export should link/copy only the displayed model and retain
the existing native model/poster serializer; add a regression confirming that
none of the new transcript references is recursively collected/exported.

### P2 — Ordinary Send cannot continue from a selected earlier model revision

Owners: `application_connector/headless/catalog.rs:61–91`,
`application_connector/native.rs:15–29`,
`agent_presence/portal_ops/native.rs:37`, and
`application_connector/headless.rs:24–44`.

History selection journals the item/shown_model and deliberately does not
change the kernel binding. The existing native-binding check then correctly
refuses a tool call whenever the displayed path differs from session.path.
Ordinary `portal.agent.send` does not prepare/rebind that selected source;
only explicit `board.model.headless.activate` or the separate typed human
generate command does. Therefore selecting an earlier owned result and using
the builder's ordinary Send leads to an activation error instead of continuing
from the selected result. It does **not** silently mutate the latest kernel
revision; retain that safety check.

Minimal fix: on explicit Send, prepare/rebind the selected in-scope generated
revision through the existing worker/source owner before provider tools can
invoke it. Do not bind merely on preview selection. Do not mint or widen an
agent grant, revive revoked membership or weaken exact source validation.
External-source or scope changes keep existing authorization semantics.

Required test: generate A/B, select A, verify viewing alone changes no binding
or authority, Send explicitly, and verify the next operation sees A's bytes,
revision and units; B stays immutable. Cover read-only, revoked grant, a new
external source, and selection change during preparation. Reopening and Undo
to a historical output must get the same safe continuation behavior.

### P2 — Missing selected text can be serialized as an error placeholder

Owners: `apps/slate/src/app/board_flow.rs:613–630` and
`apps/slate/src/app/artifact_export.rs:28`.

The new selected-output resolver correctly distinguishes unavailable source
from a different output. However `agent_note_reply` converts its error into
`Some("Selected text output unavailable — waiting for its source.")`.
`export_agent_replies` calls that display helper, so exporting before that
source loads, or after it disappears, serializes the status message as the
note's answer. This is particularly reachable with the Save As omission above.

Minimal fix: keep the display status local to the viewport and have export
resolve the exact selected checkpoint or return an actionable error/wait state,
as the artifact-source exporter already does. Do not fall back to newest text
or copy its private session bundle into export.

Required test: select a prior reply, export after reopen while source is pending
and with source missing; export waits/fails explicitly. Once loaded, exported
text equals the selected reply rather than the newest reply or status string.

## Preserved behavior evidenced in source

- The helper uses `RhinoCore` plus `RhinoDoc.CreateHeadless` and writes real
  File3dm output with cached meshes; the worker starts it with CREATE_NO_WINDOW.
  No visible connector surface is requested in the headless adapter. The real
  SDK proof was reported passed by the implementer; this review did not rerun
  or independently certify it.
- Plans are closed typed operations with deny_unknown_fields and numeric,
  operation-count and source-ID checks. There is no arbitrary generated-script
  execution path in this adapter. Unsupported geometry reports failure.
- Existing Session::begin, DraftPermit, caller identity, Lease admission and
  publication checks remain in the path. Original-source write-back is absent.
  Model acceptance checks read-only/locked/deleted node, exact binding/key,
  headless descriptor and output root, then uses patch_nodes_as for one scene
  transaction. A cancelled/stale result cannot simply overwrite the viewport.
- Source bytes are bounded and cloud-guarded. Imported File3dm retains unrelated
  objects, tables and attributes. Explicit solid references preserve target
  attributes; source units are retained and incompatible conversion refused.
  Relative material textures are made absolute against the original source
  directory without reading their bytes. Disposable SDK proof covers target ID,
  unrelated object, units, layer/material and texture locator preservation.
- Acceptance queues an explicit worker unbind/rebind, verifies the accepted
  file hash and refreshes source snapshot/origin. Existing interactive connector
  code remains a separate optional path. Ordinary Slate viewport/camera/render
  owners are reused.
- Blank creation uses a worker, link_ready_media and normal provider/program
  binding. The shared media owner checks read-only again at completion.
- Installed helper discovery checks beside the executable, then a test/development
  fallback. Release/package scripts build/copy the optional helper and README;
  commercial Rhino binaries are not copied by these additions. Source inspection
  alone is not a packaged-install smoke test.
- `active_memory` stores only revision/checkpoint metadata. No new path walker
  is necessary for it, and no export traversal of archived_history or active
  memory was found. Avoid solving persistence by exporting private bundles.

## Evidence still required before clearance

The existing `real_headless_generation_transfers_and_loads_native_preview`
ignored Rust test covers actual transfer/production loader, accepted-source
rebind, a second generation and original-file immutability. It was **not run
by this reviewer** and its result was pending at review time. Acceptance/stale
unit tests are source evidence, not execution evidence here. Obtain the
validator's exact results for final code; do not infer end-to-end success from
the separate SDK proof. Add the targeted persistence/continuation/export tests
above, then retain source-matched native visual/gesture evidence for A12/A13.
No UI/pixel certification is granted by this report.
