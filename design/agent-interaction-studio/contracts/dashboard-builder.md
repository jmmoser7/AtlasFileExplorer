# Dashboard Builder interaction contract

Status: draft, first best-effort specimen — 30 October 2026
Family: generated artifact / web portal
Reference: existing artifact generator, shared Chat Train and Images Studio agreements
Command: none added by this browser-only specification
Inherits: Studio S01–S08 where applicable
Owner: native artifact generator, shared generator chrome, artifact revision store; browser shared element runtime
Scope: Dashboard Builder Studio tab, interactive sample dashboard and staged revisions

## What it is

A dashboard artifact on an independent canvas, with the same agent picker, continuous dialogue, branching history and progress presentation as the other reviewed agents. Dashboard content is an editable HTML specimen: project metrics, monthly completion chart, team workload and upcoming milestones. The specimen demonstrates concrete widget selection and revision review; it is not a prescribed template for every future dashboard.

## Preserved native baseline

Read-only source evidence: `apps/slate/src/app/board_agent/artifact/editor.rs` uses the shared generator editor, the prompt **Describe the dashboard or the next change**, model selection, artifact revision history, current status and approvals. Its pending actions include **Apply preview**, **Discard preview**, **Fork**, **Fresh eyes**, **Inspect memory** and **Open source**. `apps/slate/src/app/board_agent/generator_chrome.rs` provides model capabilities, artifact Full access and the public activity headline. Model selection is persistent and unavailable parameter choices are not invented.

The prior broad Studio specimen (`scene.js`, dashboardSvg) already used a Project overview with Completed, In progress, On schedule, Completed by month and Work by team. This first pass reuses that recognizable content vocabulary while making its content selectable and rendering the milestone list in actual DOM. It does not certify native pixel parity of arbitrary generated dashboard content.

## Explicit user requirements

- Independent Dashboard Builder Studio, with shared reviewed visual and interaction principles.
- Visible progress for long tasks, including generated scripts and tool activity; an idle spinner alone is insufficient.
- Shared model selection and conversation behavior with the new text and Rhino agents.
- Browser-only rapid iteration. No Rust edit, build, deployment or model request.

## First-pass proposals for review

| ID | Proposed behavior | Basis |
|---|---|---|
| B01 | Project overview uses three metrics, a bar chart, team progress and a milestone table | Existing Studio content, improved to interactive DOM |
| B02 | Selecting a widget gives a subtle outline and names it in the footer | Inference: a focused next instruction should have visible scope |
| B03 | Overview, Milestones and Team show alternate views of the same data | Sample dashboard interaction, not native chrome mandate |
| B04 | Six months / This month changes the sample reporting period | Sample data interaction |
| B05 | Generated changes remain a staged preview until Apply preview | Native artifact baseline |
| B06 | Discard preview restores the applied content while preserving conversation | Native artifact baseline |
| B07 | Public progress reports inspect, update, validate and ready stages, with generated HTML/JS | Explicit user request |

These proposals are not locked as final visual design. No new shared rule is claimed merely because this first pass contains it.

## Behavior matrix

| Area | Behavior |
|---|---|
| Initiation | Open Dashboard Builder tab and interact with the dashboard on its own canvas |
| Model / access | Owned by the shared element runtime and reviewed catalogue; never ask for keys in chat |
| Prompt | Describe the dashboard or the next change; selected widget remains visible as scope |
| Output | Rendered HTML dashboard with stable sample data; applying a revision is local simulation |
| History | Shared runtime owns in-place dialogue, downstream trains and branch isolation |
| Selection | Click one widget; outline is local to that widget, not the entire canvas |
| Progress | Public work updates and generated script excerpts; no private reasoning traces |
| Stop / error | Shared runtime owns interruption and errors; applied revision remains available |
| Export / native | Not implemented; this tab is a design specification |

## Feel constants

760 × 510 world-unit initial preview; 22-unit side inset; 17px title; 12px body; 31px metric values; 9–11px supporting labels. All text is DOM or SVG, never a flattened image. Reuse theme variables and ChatTrainUbuntu. Progress and menu metrics are owned by shared runtime, not this content module.

## Golden paths

1. Open Dashboard Builder → select Completed by month → enter a refinement → watch activity → inspect staged preview → Apply preview.
2. Switch Overview / Milestones / Team → confirm the selected data view changes without spawning another agent.
3. Generate a new revision → Discard preview → confirm the applied content returns while conversation remains.
4. Use shared conversation mode and a downstream history branch → confirm independent work without changing the upstream artifact.
5. Stop during a simulated run → applied dashboard remains usable; generated code is available as public task output.

## Implementation reuse and limits

The content module is `elements/dashboard-builder.js`; styling is `elements/dashboard-builder.css`. The common element runtime owns canvas navigation, model picker, menus, dialogue, persistent settings, activity, script preview and history gestures. Native source owners remain untouched.

Preview metrics, chart values and staged revisions are deterministic samples. No external account, dashboard service, filesystem, model API, provider permission or native artifact is changed. Overall visual approval remains open. Browser code syntax checked; parent integration owns full visual and interaction verification.
