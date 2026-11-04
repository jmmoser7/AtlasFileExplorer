# 03 — Rust Canvas

Purpose: exercise real shared components in a minimal production canvas flow.
[Shared rules](../../AGENTS.md) apply. Populated elements link to their native
specimen and evidence. Empty tabs are workspace scaffolds.

Current element plan: [Frames](../../contract-viewer.html?document=contracts/frame-phase-03-plan.md).

## Keep the loop light

- Limit implementation/builds to the specifically authorized element journey.
  Use the same production components as 02; no independent copy for this stage.
- Reuse production canvas navigation, scale, selection, wires and journal
  where applicable. Include the real policy and state needed by that journey.
- Keep fonts, theme, icons, renderer and transforms aligned with production.
  Deterministic stubs replace only external I/O, not the interaction under test.
- Exclude unrelated WebView, Rhino, provider and whole-app dependency closures.
  Add a dependency only when the selected real flow demonstrably needs it.
- Reuse established profiles and the valid warm target; coordinate one build
  owner. Do not add DLL schemes, new profiles or full workspace runs per edit.
- Measure cold versus warm work separately. Under 60 seconds for the warm
  loop is a proposed goal, not a deadline or reason to kill work or omit gates.

## Cheapest valid checks

- Review shared-owner/dependency scope; format and type-check relevant code
  and test configuration before building the small native artifact.
- Exercise the changed gesture from its real canvas entry at representative
  zoom/DPI/theme, including cancellation, selection and undo/redo where relevant.
- Run focused component/canvas checks, broadening for demonstrated integration
  impact. Keep exact commands/exits and failed attempts; do not weaken assertions.
- A canvas specimen proves only its exercised scope. App imports, actual
  providers/hosts, persistence and full native flows still need relevant evidence.

## Handoff

- Attach source/binary identity, actual artifact/captures, fixture and entry
  actions, configuration and measured cold/warm compiler/runtime results.
- Map the exercised behavior to the element contract and shared production
  owners; mark missing cases and external stubs explicitly.
- Follow the existing [native acceptance gate](../../native-implementation/ACCEPTANCE.md)
  for ordinary app journeys and [release readiness](../../../../docs/dev-loop.md#release-readiness-and-fixed-candidates)
  for candidate claims. Starting here does not require completing earlier phases.
