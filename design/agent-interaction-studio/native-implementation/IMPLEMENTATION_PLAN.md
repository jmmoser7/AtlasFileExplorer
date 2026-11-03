# Slate agent interfaces — native implementation plan

Owner: architect in this chat. Authorized 30 October 2026 (America/New_York).
Reference: Studio checkpoint a7599065, latest owner corrections, and the five finalized debriefs.

## Outcome

Bring Chat Train, Images, Dashboard Builder, Text Agent and 3D Agent Builder to the reviewed Studio behavior using the existing Rust owners. Chat Train and Images govern ambiguity. Preserve existing capabilities, document compatibility, undo, permission boundaries and native wire/style palettes. The 3D builder runs Rhino headlessly and publishes real model artifacts into Slate; it does not require a visible Rhino document/window.

## Review team

- Capability moderator: capability-baseline.md. Inventories pre-change behavior and rejects functionality, serialization, safety or undo regressions.
- DRY reviewer: shared-owner-plan.md. Approves common APIs and rejects copied per-modality UI/logic.
- Design Intent reviewer: design-intent.md and ACCEPTANCE.md. Judges user flow, geometry, look and feel hover/scroll/selection, exact control placement and documented necessary deviations.
- Architect: owns central schema/commands, integration, build serialization, test evidence and final delivery. Reviewers are independent gates, not substitutes for tests.

## Build order

1. Preserve baseline and finalize contracts. Existing uncommitted native work is protected in target/studio-native-baseline/20261030-204228; never reset or absorb unrelated changes. Run pre-change checks and record failures separately.
2. Implement common Rust generator chrome once: compact provider/model/effort/speed picker and explicit key action, header controls, actual-thumbnail history, continuous dialogue, transient scrolling, return grace and shared preview selection. Extend atlas-shell owners and connect existing app adapters.
3. Implement typed graph/history behavior once: graft matching/capped input capsules, explicit preservation, blank downstream creation, terminal cuts, independent result snapshots and durable access preference integration. Preserve other native input kinds and immutable Chat Train checkpoint semantics.
4. Integrate per-host content: Dashboard uses the shared generator and undoable native acceptance transaction without the removed menu labels; Text adds exact answered-range links/hover viewer; headless Rhino creates and returns real .3dm/model results through the existing connector and preview pipeline.
5. Run focused regression suites after each coherent integration, then native build, compatibility/contract checks and matched Studio/native interaction captures. Correct rejected submissions before delivery.

## Builder boundaries

The shared chrome builder owns generator/image-editor shell modules only. The text builder owns the pure text-answer helper and its native note integration; central model declarations are applied by the architect. Later image/history and headless-runtime builders receive distinct files after their APIs are agreed. Central app declarations, scene schema, command registry, dependency manifests and lockfiles stay architect-owned. Only one Cargo build runs at a time.

## Acceptance gates

Each submission reports changed owners, mapped design-intent IDs, preserved capabilities, tests and evidence. Passing builds alone are insufficient. Compare blank/small/medium/large/connected/series states; long model names, unsupported settings, expired-key replacement, repeated requests, 1500ms return, resize/fold retention, hover bridges, thumbnail multiselect, graft jobs, history cutting, cancellation and reload. Preserve permission/error handling and responsive public progress for long runs.

Headless Rhino must prove real geometry/output with installed licensed Rhino and no UI window. If installation/runtime/admission prevents that, report the concrete failure and do not label a mock or generated script as a working 3D result. xFigura is inspiration only; undocumented internals are not implementation facts.

## Validation snapshot — 1 November 2026 (R3 observed; R4 pending)

This additive snapshot supersedes earlier current-status wording only; prior chronology, source/binary identities and receipt scopes remain intact. It does not change [the state ledger](JS-UI-STATES.json), grant a native Pass, or certify parity from shared tests.

- **Observed R3:** source inventory (1,791 files) SHA `79d0b4ce177d29a667723d35df174901b34ba9d5041a9109003bdb77f0aab365`; native EXE SHA `ff958096de7f4356c42db33723c5524ff2c8182ac450f2df076e10061fb47710`. Actual ordinary Chat, Image, Dashboard, native Text and valid Model cards/roots, plus linked Text, were observed. Detached card repairs are visible; 3D choices are actual Codex-only availability. Explicit Add/Cancel key flow was exercised without Save. These are observed cases, not all variant/state acceptance.
- Evidence: [first-five-coarse.json](../../../target/studio-native-validation/ordinary-entry-r3-case2/receipts/first-five-coarse.json), [deep-r3.json](../../../target/studio-native-validation/ordinary-entry-r3-case2/receipts/deep-r3.json), and [speed-linked-addendum.json](../../../target/studio-native-validation/ordinary-entry-r3-case2/receipts/speed-linked-addendum.json). The 26 deep R3 captures cover actual blank-output drag with no session/run, media graft, result selection/main-item behavior, and Unicode Answer on the same host with Escape and unchanged source. Native event PPP is unresolved; fresh matched-PPP evidence remains pending. These receipts retain their exact R3 attribution.
- **Four confirmed R3 defects:** credential modal below tooltip/editor layers; Ultrafast missing its two bolts; no return to the optional parameter's provider default; focused plain-prompt wheel leaking to canvas zoom on both blank and bound Images. Unfocused plain-prompt wheel still belongs to the canvas; visible dialogue owns it independently of focus.
- **R4 candidate, not verified behavior:** independently approved [Speed/default packet](../../../target/studio-native-validation/speed-default-repair-20261031/receipt.json) (10 changed paths; separate test-only private-call correction) and [focused-wheel4 packet](../../../target/studio-native-validation/focused-prompt-wheel-20261031/receipt.json) are frozen. Their new tests remain UNEXECUTED. Modal revision 1 addresses the previously identified production/nested-top gaps; revision 2 test-modifier/idle-contract corrections are finishing. No R4 native result is promoted from source review.
- **Validation at this snapshot:** 317-file Shell closure cheap testcfg passed and its component link is active; this is not a component-test result. Prior R3 44 Shell tests, App testcfg and contract checks passed in their recorded scopes. The global size check has 17 existing debt findings, with no new R3 size violation. All 166 App cases and new R4 tests still require execution on the consolidated source. The older roughly 80-minute pre-presentation unit link was CANCELLED/UNEXECUTED; no tests were waived or credited. No shared-test rollup substitutes for per-element/variant native evidence.
- **API boundary:** the planned local isolation probe was rejected before CreateProcess by automatic policy: `blocked by policy`, with no finer reason. No probe/action occurred, and no retry or alternative route is authorized. Key Save/Change remains unverified. Existing canonical Core/component synthetic-store fixtures remain distinct from native UI evidence; API validation, secrets, Text tools, registered commands and capability/authority owners stay intact.
- Quiet-performance target remains **UNMET**; profiler/latency work has separate source and evidence. Last reported account meter was 74% used / 26% remaining ACCOUNT-WIDE; it cannot be attributed to this chat, treated as completion evidence or assigned reset credit.

Next gate: finish and independently review the coherent R4 overlays, then serialized checks and actual test execution under the sole build lease; identify the submitted binary and collect fresh per-element/variant native interactions at known PPP/zoom. Remaining native states and acceptance are pending; no blanket parity claim follows from these observations.

## Current checkpoint — 31 October, source repair before freeze

Native ordinary first-use/visual acceptance remains **REJECTED/PENDING** across every element. The 117 JS source recipes and historical capture receipts remain the reference; use [the per-element/entry ledger](JS-UI-STATES.json) and [evidence instructions](JS-UI-REFERENCE.md#per-element-and-entry-evidence-ids) to prevent shared-component evidence from certifying other modalities or entry paths.

- `/root/picker_first_use_repair` owns source repairs A/B/C; `/root/menu_keyboard_results` owns D. Both are preparing a coherent source freeze. Root coordinates integration; no overlapping source edits are assigned here.
- Shared-shell component verification reports **36 distinct tests PASS / current315 closure**, SHA `65a2032f82b8516bfdeadc695af96d7fb476ad31bcd64dde86b0229598ddb584`; receipt: [shell-triage2-final-receipt.json](../../../target/performance-recovery/20261031/shell-triage2-final-receipt.json). This is component-only evidence, not native visual or ordinary-entry acceptance.
- Review order: finish A/B/C and D source repairs; freeze exact changed owners/source identity; independently review source against JS behavior and native capability/authority; run relevant serialized integration checks on that coherent snapshot; identify the submitted binary; then exercise matched native states through each real entry path. Repair failures before reporting acceptance.
- Keep per-element/entry states Missing until that evidence exists. Preserve historical receipts with their original source/binary scope; no new capture is claimed by this documentation update. Existing API/model eligibility, native authority and necessary bounded exceptions still apply.

No production completion is claimed until the required reviews and native checks pass.


## Historical integration checkpoint — 30 October, orchestration handoff

Historical handoff below retains its original scope; current owners and review order are above.

Latest owner steering at that handoff: root orchestrates only after the validated headless startup. New implementation builders use GPT-6.1 Sol, medium reasoning, Standard speed verified in the native UI. The last observed baseline allowance moved from 61% to 60%; this is budget evidence, not a completion measure.

Current exclusive owners: native integration worker owns shared-shell follow-up, provider/key/dialogue/focus adapters, 1500 ms panel return and artifact thumbnail integration; graft/history worker owns pure input planning, history semantics, wire/port algorithms and bounded execution against the compatible GeneratorSettings schema; headless runtime worker owns existing connector runtime/native admission/receipt integration; validation worker serializes Cargo with --jobs 2. Text worker's completed passage annotation integration is handed off; root makes no code edits.

Landed, awaiting compiler/native acceptance: app literals for revised shell options; actual available provider list and session dialogue data; compatible AgentPortalRef.generator settings (optional count/preserve history and stable grafted slot IDs); one-shot exact response focus through shared activity UI; Text participation in existing panel return memory; provider/key intents routed to existing setters and masked machine-secret editors; headless connector module/pump/mode guards and registered human commands. First Cargo check failed three shared-shell visibility errors; those errors have targeted fixes. A second app check is coordinated against a frozen snapshot.

Remaining gates are explicit: per-revision actual output thumbnail cache/capture and Shift selection; effective preserve-history and count execution; terminal cuts/blank downstream and graft behavior (graft owner); artifact-spawned Chat Train picker parity and durable access behavior review; width/fold/draft retention regression checks; current unavailable/mismatched text-answer navigation falls back to its saved passage answer rather than selecting a different response. Headless generation currently requires a placed model; blank 3D creation remains unmet DI23. Native binary identity, matched interaction captures, save/reload/undo, real licensed headless runtime evidence and final contracts check remain pending. Passing compilation alone is not acceptance.
