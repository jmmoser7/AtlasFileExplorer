# Native agent build — acceptance gate

Status: implementation authorized; native acceptance pending. Date: 30 October 2026.
Applies to Chat Train, Images, Dashboard Builder, Text Agent and headless 3D Agent Builder.
Reference: reviewed Studio `a7599065`, current sections of the five debriefs and subsequent owner instructions. [Design intent](design-intent.md) records detailed rationale and known prototype omissions.

## Mandatory ordinary-entry evidence gate

Use the [operational JS UI reference](JS-UI-REFERENCE.md), including its explicit owner overrides and source/fixture limits, for state-by-state comparison. Before reporting native parity complete, each row below requires evidence from the current submitted binary reached through the real ordinary user flow. An already-bound/generated fixture, direct test helper, synthetic transcript or passing component suite cannot substitute for first-use entry.

| Element | Ordinary entry that must be exercised | Additional path kept distinct |
|---|---|---|
| Chat Train | Place/open an ordinary unbound Chat source and configure it through its actual controls. | Bound/reopened and artifact-spawned Chat Train. |
| Images | Open Agent on an ordinary imported/blank image before it has an agent binding. | Generated, connected, result-selected and reopened image. |
| Dashboard Builder | Reach Dashboard creation through its ordinary registered flow; also inspect an ordinary imported Web source's entry. | Artifact-bound/generated/reopened Web host; do not substitute it for a plain Web source. |
| Text Agent | Open Agent on ordinary native text and an imported linked text source; submit on that same source host, repeat with Preserve off/on, select response options and separately exercise explicit Replace. | Bound/generated/reopened Text and exact recorded-answer navigation. A legacy downstream-spawn command does not satisfy this Agent Send flow. |
| 3D Agent Builder | Import an ordinary valid model with no agent and open its actual Agent tool. | Existing bound/headless/generated/reopened model; explicit setup/access/Resume remains separate. |

For each row, attach matched JS/native state screenshots and interaction assertions for the applicable closed/open Agent, picker root/expanded/long/unsupported choices, credential Add/Change and cancel flow, hover/preview bridge, rebound, scrolling, Escape/outside dismissal, and first explicit submission/result states. Record the source/node state, provider/model IDs, viewport/theme/DPI/display scale/canvas zoom, binary/source identity and entry actions. Keep private credentials out of captures; key saves and paid/live execution use only existing authorized validation, never fabricated evidence. A JS sample without an importer or real runtime specifies its appearance/interaction only; native ordinary-source lifecycle still needs independent proof.

Maintain an explicit status for every required state: **Pass**, **Missing**, **Failed**, or **documented native-only exception**. Missing or Failed cannot be called parity complete or silently covered by another modality/path. Record each native-only capability or necessary departure with its source/command owner, reason, preserved authority/capability and reviewer disposition; fixture omissions do not authorize deleting native functionality. Earlier tests and captures retain their original scope and binary attribution. This enforces the user's existing design and acceptance requirements; it does not invent another user approval step or replace the gates below.

Evidence IDs and independent statuses are defined in [Per-element and entry evidence IDs](JS-UI-REFERENCE.md#per-element-and-entry-evidence-ids) and [the state ledger](JS-UI-STATES.json). Record `state/element/entry[/image-variant]` with current source/binary, ordinary entry actions/node state, matched JS/native captures and interaction outcome. All added native statuses remain Missing. Shared geometry evidence needs distinct real entry proof for each applicable modality; Images must account for small/medium/large/connected/series/blank coverage. API key cases require actual API capability; attached Rhino is Codex-only. Prototype authoring controls and native Fill/Stroke/Corners placeholders add no product requirements.

For automated ordinary-entry fixtures, wait for the actual production popup's measurement/readiness frame before resolving its controls. Composer input must target the currently enabled, visible, clipped TextEdit in its real transform and layer; retain exact widget identity and focus diagnostics. A stale rectangle or a synthetic ready flag cannot replace that path. These lessons were verified by the R10/R11 unit repairs and the current 220-case run; they grant no native state Pass and do not replace the ordinary-entry evidence above.

## Mobile layout approval request ledger

Shared across all agents; the [last-resort escalation policy](../../../docs/dev-loop.md#last-resort-mobile-layout-escalation)
permits at most four approval screenshot views in any rolling 24 hours.
Initialized 1 November 2026 with **zero requests issued under this policy**.
Before sending, the coordinator serializes reservations here and counts all
reserved/sent views within the preceding 24 hours, including each layout view
in a compound image. A cancelled unsent reservation may be released explicitly;
sent/rejected views still consume budget. Keep timestamps and reply status current.

| Requested/reserved at (UTC) | Owner | Screenshot IDs / view count | One decision; why intent is insufficient; recommendation | Reply status / timestamp |
|---|---|---|---|---|

## Authority

Latest explicit owner instruction wins; then the current finalized debrief/Studio behavior. Chat Train and Images resolve common-interface ambiguity. Existing native capability/transaction/permission owners survive UI refinement. Historical Studio text is evidence, not a competing current specification. Native implementation authorization is not native acceptance.

## Quick review checklist

| Gate | Pass condition |
|---|---|
| A01 Shared implementation | Common picker/menu/dialogue/layout/preview/scroll/history use common native owners; no parallel modality controller or application wrapper. |
| A02 Capability preservation | Moderator inventory reconciled: editing, bindings, host, capture/export/source, clipboard, journal and recovery are retained. Removing a menu row does not delete the capability. |
| A03 Configuration | Provider/model/effort/speed distinct; actual capabilities disable unsupported choices; long names remain usable. Artifact-spawned Chat Train has the same picker. |
| A04 Credentials/access | Dedicated masked Add/Change key handles expiry. Global user preference survives agents/restarts and honors actual granted scope without prompt loops; secrets never enter documents. |
| A05 Menus/rebound | Compact stationary submenus, hover help, outside-click/Escape dismissal; <=1500 ms return restores and later return does not. Dashboard menu is Preserve model history only. |
| A06 Dialogue/geometry | Continuous reviewed text treatment and one plain composer; header spinner/hover folds; no Complete/progress wrapper. Width/complete text survive resize/fold/reload. |
| A07 Long work | Real public work/script/tool progress plus Stop; preserve reading position/results. No private reasoning, fake stages, spinner-only minutes or code-only false completion. |
| A08 Previews | Actual saved-output circles and exclusive rounded-square running bond; persistent selection/bridged hover, no border/ring, no toolbar on preview click; applicable Shift multi-selection retained. |
| A09 Wires/history | Blank closed child on output drag; independent context and separate gray wire; terminal-only cut retains data/output and prevents context resurrection. |
| A10 Graft/jobs | Graphable applicable inputs; shared bundles versus longest split; Count per branch; four-slot capsule; bounded execution, per-job provenance and partial-failure recovery. |
| A11 Text answers | Unicode-safe target persists, local response link/hover viewer/exact navigation works, unrelated edits rebase and changed passage invalidates; replacements explicit and undoable. |
| A12 Headless 3D | Real model artifacts generated in Slate with no visible Rhino document/window; retain native viewport/display/measure/capture/view metadata/saved views. Honest unavailable state, no canned substitute. |
| A13 Native visual evidence | Inspect the actual submitted binary at matched fixture/theme/DPI/zoom. Exercise size/blank/connected/hover/scroll/resize/interruption states, not only an idle screenshot. |

## Reject a build when any applies

- Lost native capability, user data, undo/export/binding; secret leakage; or agent authority expanded without its actual grant.
- Duplicated implementation of shared behavior where a native owner exists.
- Reintroduced generator-menu bloat, or removed Chat Train's separately specified Fork/Full access/Schedule.
- Wrong thumbnail/result/text target, cross-branch context contamination, interior history cut, stale-context resurrection or unbounded UI work.
- Clipped/disappearing text, reset authored width, reserved scrollbar gutter, wrong attached/detached layout, square/circle overlap, preview-triggered toolbar, wrong large-image preview offset, blister seam/fringing.
- Fictional provider capability, simulated output claimed as live, code without a real 3D artifact, or visible Rhino document required for the headless flow.
- Completion claimed from compilation, process ID, a synthetic test or a screenshot from a different binary alone.

## Evidence required from each builder

1. Changed owners/files and mapped acceptance/debrief IDs; stay within assigned ownership.
2. Capability reconciliation and DRY/common-owner clearance.
3. Relevant state/layout/native integration checks with real exit codes; report environment blocks honestly.
4. Native source/binary/build identity and captures at known viewport/DPI/zoom/themes; exercise changed gestures, not only idle paint.
5. Save/reload/undo and real provider/host evidence for changed durable/execution paths. A mock is labelled a mock; no secrets in evidence.
6. A deviation record for every necessary departure: original requirement, reason/evidence, capability impact, chosen behavior and reviewer decision.

## Transaction and capability rule

Dashboard's one-checkbox menu does not abolish native transactions. Generation/selection uses the existing transaction and journal; preserve prior outputs, undo and last valid output on cancel/failure. Keep source/capture/recovery reachable through appropriate existing surfaces without restoring rejected generator rows. Text replacement remains an explicit range-validated action; it is not automatically applied because Dashboard Apply Preview was removed.

## Engineering decisions still required

- Choose and validate a real supported headless Rhino API/kernel/transport and licensing/availability path. External unavailability is reported, not hidden by a visible-document fallback or a simulation.
- Declare/discover actual provider speed/effort and modality/input compatibility in the shared native capability owner; do not copy fixture availability.
- Map output selection to the existing artifact transaction/journal and retain non-menu source/recovery behavior, reviewed by capability and DRY moderators.
- Use native durable ID/range/serialization/cache owners. Protect Unicode, existing files, exports and preview invalidation.

These are engineering choices inside the authorized task, not new user-facing design alternatives. Escalate only a demonstrated native constraint that requires changing reviewed behavior or losing capability; record and review the departure.

## Outcome vocabulary

**Accept:** applicable gates pass with source-matched evidence.
**Accept with documented deviation:** necessary bounded difference recorded/reviewed; no hidden capability loss or authority expansion.
**Revise:** rejection condition remains or evidence does not establish the submitted behavior.

Design Intent judges flow/hover/spacing/radii/parity; capability moderator judges retained behavior; DRY reviewer judges ownership. Architect integrates and reports blockers. No pixel-parity certification without inspecting the actual native render, and no arbitrary tolerance silently granted.

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

