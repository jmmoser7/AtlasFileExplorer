# Studio design iteration

This is the primary workflow for iterating on interface designs in this project, directed by the owner on 30 October 2026.

1. Start from the actual existing element. Inspect its source, captures and contract. Preserve what the user has not asked to change.
2. Put one element in one Studio tab and one independent canvas. Chat Train stays first. Avoid broad multi-modality previews or control dashboards inside the element.
3. Keep Studio chrome outside the element. Make interactions concrete and reversible so the user can try them during the conversation.
4. Refine through the conversation. Apply explicit corrections without repeatedly asking for confirmation. Do not invent approval or repeat a completed survey.
5. Consult contracts/PATTERNS.md and related locked contracts before adding menus, configuration, toolbars or gestures. Reuse design and implementation owners; record requested deviations.
6. Keep the element's standalone debrief current using contracts/TEMPLATE.md and the native tool-contract vocabulary. Record retained baseline, agreed changes, behavior matrix, constants, golden paths, owners, verification and limits. Accepted rows go in contracts/decisions.json.
7. Lock only what the user approved. Chat Train v1 is agreed. Future tabs remain draft until reviewed; do not silently apply unfinished designs to production or rewrite another locked contract.
8. Test affected behavior and inspect the browser result. Retain conversations, drafts and notes. Identify simulated provider/credential interactions clearly.
9. Native implementation must reconcile these agreements with the native contract registry and shared source owners. This review folder does not authorize forks of production chrome, provider logic, secrets or commands.
10. Keep the Studio a fast, self-contained HTML/JavaScript/CSS prototype. Native code, measurements and screenshots are read-only reference material. Do not modify or build Rust, or require a native runtime service, just to preview a Studio iteration unless the user explicitly requests that approach.

Primary references: contracts/chat-train.md, contracts/PATTERNS.md, contracts/decisions.json, and the repository tool-contract skill/template. WORKLOG.md is history, not a competing current specification.

11. Every new canvas must follow Slate navigation (S08) and reuse canvas-navigation.js. Do not invent per-tab mouse or wheel rules. The live HTML, captured states and element contract together form the implementation specification; match them in Rust only when explicitly requested.

## Automatic Image Studio handoff

This applies in every phase. Before final handoff, automatically copy every
requested Studio image generated, edited or captured for review into versioned
Studio-local assets and register it under the **same element tab** in phase 00
Image Generation. No further user reminder, manual import or import approval is
required.

Retain a stable image ID and variant label, source and prompt/edit receipt or
capture metadata, and reuse the existing per-image message composer. Preserve
originals, earlier variants, baselines and associated drafts. Visibly verify that
the image and its composer load before handing it back.

This handoff adds no paid inference or further generation. Registration does not
imply design approval or native/runtime parity.

## Phase workspaces

The static home links to six workspaces: [00 Image Generation](phases/00-images/AGENTS.md), [01 JS](phases/01-js/AGENTS.md), [02 Rust Component](phases/02-components/AGENTS.md), [03 Rust Canvas](phases/03-canvas/AGENTS.md), [04 Integration Time Report](phases/04-integration/AGENTS.md) and [05 Release Time Report](phases/05-release/AGENTS.md). Read the selected phase's instructions before its work; the shared chrome exposes them.

- Start in any phase, skip phases, and return in any order. The numbers describe workspaces, not required approval gates. Relevant final checks and the existing native acceptance gate still apply.
- All phases share one element-tab registry and stable IDs. Chat Train stays first; Studio stays last for the workflow image gallery and global time audit. An element may have no artifact in a phase yet; keep that empty state honest rather than inventing content or another tab registry.
- Rule 10 governs the home and JS preview. Only specifically authorized element work in 02/03 may implement or build Rust; opening a stage, attaching evidence, or changing this static shell does not authorize it. The current 02/03 pages are workspace scaffolds, not an already-built native harness.
- Reuse the real production widget and policy owners in both Rust phases. Do not copy a Studio-only Rust widget or import the whole Slate app for a small specimen. Deterministic stubs belong at external I/O boundaries, never in place of the behavior being reviewed.
- Keep iteration nimble: use the smallest relevant existing check and a stable warm target/profile. Proposed warm-loop goals are under 30 seconds for components and under 60 seconds for canvas work; measure cold and warm separately, report misses, and reassess scope. They are not promises, hard deadlines, or permission to terminate work or omit checks. Do not add profiles, DLL schemes, full-workspace runs per edit, or automatic CI skips.
- Reports in 04/05 read actual recorded work after it happened. They neither start builds nor imply approval, readiness or publication. Preserve failures and unknowns; a missing duration is **not recorded**, not zero.

The repository [development loop](../../docs/dev-loop.md#intent-to-running-evidence), [release readiness rules](../../docs/dev-loop.md#release-readiness-and-fixed-candidates), contracts and [native acceptance gate](native-implementation/ACCEPTANCE.md) remain authoritative. Component evidence certifies only the component; ordinary native entry, integration and published-artifact evidence retain their separate scopes.
