# Native capability and regression baseline

Audit role: capability moderator. Date: 30 October 2026.
Reference checkout: `a75990653b5cf97484d03c2c36d34920f615a831` plus the existing working tree.
Scope: read-only native source/contract/test inspection. No Rust code was changed and no build or live provider operation was run by this audit.

## Evidence status and governing rule

The checkout contains many modified native files and untracked native agent/connector files. This is not a clean released snapshot. “Existing” below means a capability evidenced in the current checkout, not a claim it shipped or passed on this machine. Builders must preserve both HEAD behavior and already-authored working-tree changes; the parent must snapshot those changes before integration. Tests listed below are existing source evidence, not execution results.

The latest user authorizes native implementation. Chat Train and Images are the visual/interaction precedents. Their approved Studio behavior controls presentation changes. Existing capabilities omitted from the prototype stay supported unless the user explicitly removed them. The latest explicit Dashboard removal is the Apply Preview/Discard Preview **menu UI**, not authority to erase revision safety, undo, acceptance provenance or provider permission requests.

## Critical findings before builders start

1. **Dashboard acceptance is a real transaction.** `board_agent/artifact.rs` owns separate preparing/running/pending state. `artifact/editor.rs` currently exposes Apply/Discard; export materializes the accepted, exact hashed revision. Removing those labels must not publish unverified, stale, cancelled or read-only output. Recommended replacement for design review: valid completed generation automatically selects/accepts via the existing acceptance transaction, retaining one-step undo; reject stale/cancelled/read-only completion and retain the preceding accepted revision. Provider tool approval remains a separate control and must not be auto-answered.
2. **Headless Rhino is not the current worker.** The protocol supports `SurfaceMode::Headless`, but `application_connector/worker.rs` currently binds a local `.3dm`, requests surfaces and insists on `NativeFullUi`. The Rhino.Inside pack is a probe/registration descriptor for an independently installed connector, Rhino 8 and a license. No `RhinoDoc.CreateHeadless` or headless model-generation service was found in this repository. Headless generation needs a real compatible adapter path, not a hidden claim that the full-UI worker is headless. Preserve interactive Rhino as optional.
3. **Do not replace the existing input graph with prototype arrays.** `slate_doc::agent_inputs` already owns typed, ordered, pinned, consumed bindings; table/control/view/artifact inputs exist in addition to image/text/style. Shared grafting must extend this owner and preserve all existing kinds and explicit input authority. No native graft field/implementation was found in the inspected scene/request owners.
4. **Model capabilities already come from providers.** Native model parameters retain opaque IDs, defaults and advertised values. Do not ship the Studio fixture model list or fabricate Ultra fast support for a provider.
5. **There is already one return-grace owner.** `board_properties/selection.rs` owns `OBJECT_PANEL_RETURN_GRACE = 1500 ms` and remembers `GeneratorPresentation`. Extend its eligibility for Text/3D; do not create independent timers per modality.
6. **Native previews already have scalable owners.** Use `GeneratorHistory` optional thumbnails, shared image presentation/blister geometry, native preview/cache/capture paths and the current 3D renderer. Rendering all full-size outputs on every frame would regress the lazy/cached design.

## Capability inventory

### Shared agent execution, configuration and authority

| Capability to preserve | Current owner / evidence | Required regression gate |
|---|---|---|
| Provider selection, discovery and startup | `atlas-packs`, `atlas-ai/src/packs.rs`, `board_agent/programs.rs`, `board_agent/models.rs`, `portal_picker.rs` | Installed/missing/loading/error states remain distinct; no automatic executable launch merely because a model row is shown. |
| Exact model IDs and provider-advertised options | `scene/agent.rs::AgentPortalRef.model_parameters`, `atlas-ai/src/agent/models.rs`, `selection_tools/generator_picker.rs`, `board_agent/generator_chrome.rs` | Preserve unknown authored model IDs during catalog refresh; reset stale parameters only on explicit model change; snapshot settings on submit. Unsupported controls cannot be offered. |
| Explicit key entry and replacement | `atlas-ai/src/ui/pack_ui.rs::openai_key_window`, `app.packs.openai_key`, `atlas-core/src/secrets.rs`, `atlas-ai/src/runtime.rs` | Route new Add/Change chevrons to the existing machine-secret flow. Never put keys in prompts, URLs, workbook JSON, logs, exports or shared agent directories. Invalid/expired key remains recoverable. |
| Persisted user access choices | `atlas-ai/src/access.rs` per-user `agent-access.json`; artifact default + explicit session denials + separate relay grant | Global preference UX must keep explicit denial, read-only policy and relay authority distinctions. Workbook import never imports trust. |
| Exact-identity consent rebind | `agent_presence/consent_policy.rs` (working-tree changes) | Identity/provider/scope mismatch cannot resurrect authority; persisted preferences are not live membership grants. |
| Provider approval + failures | `board_agent/approval.rs`, artifact approval tests, runtime session approval state | Permission requests remain actionable; stale approval IDs cannot answer a new request. Delivery failure must stay retryable and visible. |
| Stop, queue, steering, progress | `board_agent/life*`, `generator_pump.rs`, `results.rs`, `atlas-ai/src/runtime.rs`; current-request activity adapter | Stop before/after startup; do not launch duplicate queued runtime; late results cannot overwrite new requests. Display public commentary/tool evidence only, scoped to current request. |
| Durable transcript and output locators | `atlas-agent` session bundles; `agent_chat` references; relative `SourceUri`; session image import | Reopen/move workbook without copying secret/session internals into scene. Unavailable sources report errors rather than substitute another result. |

### Chat Train

- Existing presentations: individual train cards, message pairs, single conversation window, full transcript, collapsed/summary state, authored widths and fit-to-text. `board_agent/train.rs` re-chunks all branches, retargets wires and applies one undo transaction.
- Drafts are real unsent state. Presentation changes absorb/restore prompt text and caret; undo/redo restores drafts and wires without replaying inference (`draft_tests.rs`).
- Forks retain a pinned checkpoint, use their own linear provider session, and leave the unbranched tail in its provider session (`fork_tests.rs`, `train_setup.rs`, `chat_edge.rs`).
- Existing actions include conversation rename/switch, program/model selection, full-access preference, schedules, bundle/unbundle, image bundle output, artifact opening/spawning, evolution history, and bounded crosstalk/relay. The Studio's cleaner menus may regroup these, but omission is not removal authorization.
- A streaming output port doubles as Stop; position and scale stay fixed while the card grows. User-authored fold and width decisions must not reset with streamed updates.
- Working-tree `agent_presence/chat_train.rs` binds real provider sessions to narrow wired source reads/native tools. Creating or sending a chat cannot create/resume membership or grant tools; explicit Board access/Resume is required. Read-only reference images/CSV are not native mutation targets.

### Images

- Providers: ComfyUI, ChatGPT through Codex image mode, and OpenAI API are supervised image engines; there are separate text providers. Preserve those distinctions and actual model catalogs.
- Existing image settings include per-run count, source aspect, seed locking, local Live mode and Keep current frame. Live changes include wired model camera, prompt and checkpoint. Unsupported provider modes remain unavailable.
- Inputs: current visible picture, explicit upstream primary image overriding current frame, separate style references, wired prompts followed by authored text, generated output bundles, model/web/video captures at run start, and input preflight errors.
- Results: latest/shown selection, persistent picked output, immutable ID/source pins, per-result crops and paint stacks, multiple outputs, album browsing, ordinary image editing, bundle/unbundle and downstream generation. A pinned output must not silently become the latest output.
- Native image controls already share attached/detached blister, prompt and model UI. Preserve crop, fill, stroke, filters, paint/layers and image clipboard/export behavior while changing only the reviewed agent chrome.
- Active-result copy/duplicate and sequential/batch saves are explicit reviewed changes; do not accidentally copy full history or strip source/crop/paint provenance from genuine linked originals.
- Studio-only additions needing native work include terminal-cut optional preserved model history, shared graft job matching/capsules, precise running-bond result previews, 18-unit header thumbnails, and the revised compact option names/grouping.

### Dashboard Builder / artifacts

- Host is an existing Web portal with arbitrary HTML content, not a hardcoded dashboard app. It has source loading, live page interaction, capture, table links and local control inputs.
- `board_agent/artifact` owns asynchronous preparation/publication, immutable family/revision/checkpoint records, exact content parent versus memory parent, model/provider pinning, preview/accepted separation and stale-result rejection.
- `ArtifactGeneratorRef` stores relative source family, shown revision, all revisions, checkpoint, workspace and optional packaged source hash; private history stays outside exported workbooks.
- Table and Control inputs are real capabilities. `window.slateLink` data and explicit control bindings must remain working even though the Studio used image/text/style examples.
- Existing artifact fork and Fresh-eyes actions express separate content/memory lineage. New Preserve model history and output-drag behavior must map onto that distinction rather than dropping content pins.
- Output revision browsing is separate from inference. Selecting an earlier result must not mutate later history or its provider checkpoint.
- Exports/package materialize the **exact accepted revision**, verify hashes, and refuse wrong/missing source; browser content, not a screenshot substitute, remains interactive in exported artifacts where supported.
- The user's one-option Dashboard menu replaces the old menu presentation. Preserve acceptance integrity and source/history operations at their correct internal owners.

### Text Agent

- Native text-media notes show generated reply plus prompt. Human-owned text retains authored size; automatic fit only applies when that ownership permits it (`board_flow/text_note.rs`, generator/text ownership tests).
- Existing text and image inputs, generated text/source overrides, direct run/stop, in-frame model choice, linked history, export/readable content and ordinary text editing must remain available.
- The new selected-passage query, exact-range replacement, local answer hyperlinks, question/answer hover viewer and response-destination scrolling exist in the Studio, not yet in the inspected native text model.
- Implement durable anchor IDs and range/revision validation in a UI-free text/annotation owner. Editing the queried range must invalidate its answer binding; unrelated edits should rebase it. A stale answer must not edit another passage.
- The answer hover viewer is public response content, not model reasoning. It must not change model-history preservation or grant authority.

### Rhino / 3D builder

- Slate already renders 3D files through `model-preview`/`rhino-mesh` and `model3d`; known formats include Rhino `.3dm`, OBJ, STL and glTF. Preserve loader errors, poster cache, world units, camera, display mode, orbit/pan/zoom, picking and authored frame bounds.
- Native property tools include Shaded/Arctic/Hidden line/Material mask/Z-buffer, measurement, capture, maximize, filters, and the Rhino connector. Ordinary fill/stroke/crop presentation cannot be replaced by prototype toolbar implementations.
- Model captures carry source/view provenance. Native capture must use actual native pixels, not silently fall back to cached Slate geometry when native capture fails. Restoring views must validate source scope/hash and units.
- Connector lifecycle uses `atlas-packs` discovery, `atlas-connector` negotiation/types and `atlas-connector-runtime` transport/surfaces; Slate-specific adapters live in `application_connector`. The full application can remain open while its Slate card is parked; returning to Slate does not kill the external document.
- Current native invocation is closed-descriptor based, bounded, source-scoped, generation-checked and receipt/ticket tracked. Read, isolated-draft mutation and original-source write-back have distinct authority. Agents cannot mutate linked originals; isolated mutation requires both negotiated v0.2 capability and explicit draft autonomy.
- **Latest direction:** builder defaults headless and produces model artifacts without opening a Rhino document/window. The optional existing interactive connector remains supported. Do not claim this direction is implemented by `NativeFullUi` attachment; treat real headless adapter capability/installation as a prerequisite.

## Cross-cutting rendering, data and lifecycle requirements

- All board mutations remain inverse `SceneCmd` transactions with author identity; UI refactoring must not bypass the journal. Inspecting or selecting history is not a new inference request.
- Old `.slate` documents retain legacy bindings, model parameters, sources and port semantics. New optional fields need deserialize defaults and export parity.
- `slate_doc::agent_inputs::WireBinding` currently owns direction, kind, output pin, stable order, bundle semantics, consumed state and slot. Current kinds include Text, Images, Table, Artifact, Memory and Control; model View is a typed slot. Preserve all, including hidden wires and source-remap behavior.
- Native generator port positions are fractional (`0.25/0.5/0.75` for images, distinct text/artifact/view ports). Studio geometry is a deliberate new presentation. Semantic bindings must survive the visual relocation; do not infer old wire meaning solely from its newly drawn height.
- Existing Chat Train history rails are protected presentation links; Artifact memory is independently removable. New terminal-only history cuts must distinguish memory links from ordinary image/content/table/control wires, and retain generated records.
- Native output thumbnails/capture must be lazy, cached and bounded. Activity belongs to the current request; stale output and old approval events cannot repaint the current request as successful.
- Preserve focused-editor versus canvas wheel ownership, pointer-based zoom, ordinary right/middle/Space pan, selection then text highlighting, and one-gesture undo. Extend the shared 1500 ms selection-memory owner to eligible new hosts.
- Capture and export remain shared: still PNG/JPEG/WebP; silent MP4 capture with pause/resume/save/cancel; view metadata for 3D; relative source references and private-data exclusion.

## Existing test gates to retain or adapt deliberately

| Area | Existing tests / representative names |
|---|---|
| Shared model picker | `generator_picker/tests.rs`: `root_summarizes_the_catalog_without_expanding_all_choices`, `pages_keep_every_provider_choice_and_the_exact_opaque_parameter_id`, restored-page validation. |
| Native app chrome/authority | `board_agent/generator_chrome/tests.rs`: settings snapshots, current-request-only activity, machine-local Dashboard default with explicit denials, no provider-kind/default cross-contamination. |
| Chat presentation and draft | `draft_tests.rs`: two drafts preserve text through switches/undo/redo; wire on merged message follows the correct card; menu parity across presentations. |
| Chat geometry/stop | `fork_tests.rs`: one-undo fork, saved width/fold/tail composer, header model eligibility, wire color preservation. `stop_tests.rs`: menu wheel ownership, stop before link publication, fixed stop position. |
| Session/authority | `agent_presence/chat_train/tests.rs`: no automatic permissions, explicit autonomy retained, deleted wire revokes request, send cannot resume revoked membership, no duplicate relayed runtime. |
| Image input/result provenance | `generator_tests.rs` and `generator_tests/daisychain.rs`: authored model params, upstream primary precedence, style separation, picked crop/paint stack retention, pinned output never substituted; `image_album_tests.rs` persistent picks and closed-blister squares. |
| Artifact integrity | `artifact/tests/lineage.rs`: exact pinned prefix, same-session checkpoint reuse, stale/cancelled/read-only result retention, explicit child model precedence. `artifact/tests/approval.rs`: both provider approval choices, retryable delivery failures, stale IDs. `artifact/export/tests.rs`: exact source/hash/package behavior. |
| Backward compatibility | `slate-doc/agent_inputs/compatibility_tests.rs`, `scene/agent.rs` old-program round trips, typed port/slot tests and `wire_host` eligibility/rail tests. |
| 3D / connector | `application_connector/tests.rs`: activation without installed offer, parked session retention, actual-pixel capture, wrong-source rebind, Escape/restore, real caller authority. Protocol `draft_authority.rs`, `session_policy.rs`, `lifecycle.rs`, `surface_discovery.rs` include headless offers and stale/denied operations. |
| Text | Existing generator ownership/crop/text override tests plus new native annotation range/edit/reopen/export tests required. Studio `text-links.test.cjs` is a semantic reference, not sufficient native validation. |

Run affected crate/test groups against a preserved current-tree snapshot before integration, then native build and interactive screenshot/gesture review. Never remove a test solely because the new UI no longer paints its old label; map the protected capability to its replacement. Report pre-existing failures separately. A successful compile is not evidence of pixel or gesture parity.

## Moderator acceptance process

For each builder submission require: changed-owner list; native command and persistence deltas; comparison to the latest Studio rule IDs/captures; before/after capability table; relevant test results; and explicit rollback. Reject duplicated provider catalogs, duplicated image/menu geometry, blanket permission widening, synthetic headless-success claims, or any destructive result substitution.

The design-intent reviewer decides placement and presentation ambiguity. The moderator blocks lost capability, stale-authority/data corruption and unsupported success claims. The DRY reviewer blocks competing owners. The parent resolves any conflict explicitly in the integration record before merging.

## Open prerequisites / decisions

- Obtain the installed independent Rhino connector's real headless capability and delivery contract; current source evidence alone cannot certify it.
- Confirm automatic validated artifact acceptance through the existing transaction as the replacement for removed Apply/Discard UI, preserving provider permissions and Undo.
- Define one native optional conversation/history schema shared by generator hosts, without merging data inputs and memory inputs.
- Define grafted data-tree jobs alongside existing typed graph/consumed/pinned semantics and bounded execution; retain table/control/view support.
- Define native text answer anchor persistence/export ownership and the cache/source owner for actual Dashboard/Text/3D revision thumbnails.

This audit is ready for builder-review follow-ups. It is not a native build pass or a claim of zero regressions.
