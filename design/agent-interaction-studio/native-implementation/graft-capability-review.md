# Graft/history capability gate

30 October 2026. Independent read-only source review; **changes required**. Moderate scope: focused packet and graft/history owners only. No Rust changes, builds, tests or provider operations were run. This report is a gate, not an implementation success claim.

Comparison uses protected dirty baseline `target/studio-native-baseline/20261030-204228` (manifest HEAD plus tracked.patch and saved untracked files). The artifact generation/pump/spawn and board_agent/inputs files have no hunks in that baseline patch, so HEAD is also their protected tracked baseline. Other owners were read with the retained dirty work in mind; unrelated existing connector, Chat Train and UI changes are not attributed to grafting.

## Required corrections

### GR1 — P1: valid wired-only artifact output rejects its own guard

`apps/slate/src/app/board_agent/artifact/pump.rs:37,425–435` uses editor prompt or `agent.instruction` for the semantic guard, but start journals editor prompt or the **composed request** into instruction. With no editor entry and empty authored instruction, artifact planning synthesizes a prompt from wired text. Starting then changes instruction from empty to wired text. Publication sees a different guard and discards the valid completion at `pump.rs:225–230`; queued siblings are removed too. Grafted branch prompts can likewise overwrite the authored value.

Minimal remedy: retain the submitted authored/editor prompt separately on `Plan`, before wired composition/branch selection. Journal that exact authored value at start; keep `request.prompt` exclusively the immutable provider request. Keep semantic validation of actual later user edits. Do not merely regenerate the guard after arbitrary state changes or remove prompt from it.

Tests: absent editor + empty instruction + wired-only prompt through real prepare/start/publish adapter; Count=2 and grafted prompt branches; authored text persists unchanged; actual prompt edit during preparation/running still rejects. Existing tests supply prompt_mut and miss this path.

### GR2 — P1: wired Text input ignores selected historical output

`apps/slate/src/app/board_agent/inputs.rs:289–296,312–316` always chooses the last idle assistant turn. New persisted `generator.shown_text` can display an older response, including an archived source, while downstream snapshots/grafts consume the newest response. The latest-only lookup existed in the protected baseline; this is an integration failure with the newly introduced selected-output capability, not an unrelated old defect.

Minimal remedy: route explicit selections through the UI worker's shared `resolved_selected_text_output(id) -> Result<Option<String>, String>` resolver in `generator_chrome/text_outputs.rs`. Use latest only for Ok(None); propagate missing selected source/checkpoint errors instead of silently substituting latest. Keep file loading asynchronous through AgentSources. Native Text snapshot handling must also respect an explicit selected output wherever authored text otherwise takes precedence.

Tests: visible old output equals downstream plain and grafted Text snapshot; archived selected source survives reload/workbook source resolution; missing/corrupt/incorrect checkpoint fails; no selection keeps legacy behavior. Coordinate with the UI worker's resolver and schema; avoid a second selection reader.

### GR3 — P1: revision browsing can resurrect cut artifact memory

`apps/slate/src/app/board_agent/artifact/pump.rs:277` accepts a history pick via `accept_artifact_output`, which unconditionally sets `history_cut=false` at `pump.rs:499`. After a terminal cut, picking a retained old revision and re-enabling Preserve allows `artifact/generation.rs:145` to choose that old revision/checkpoint as active memory. Browsing completed output must not undo a severed model-history boundary.

Minimal remedy: distinguish newly completed generation acceptance from output-only revision selection. Only a valid generation performed after the cut may establish a new active checkpoint; a historical pick must retain the cut boundary. Do not erase the protection as a side effect of the shared acceptance helper.

Tests: cut -> browse pre-cut revision -> Preserve on -> plan has no old memory/history; inspect/export still use exact chosen revision; genuinely fresh generation after cut establishes its new checkpoint; Undo restores the previous document boundary and selection correctly.

### GR4 — P1: a history cut leaves queued image/text replay alive

`apps/slate/src/app/board_agent/artifact/spawn.rs:144–195` now handles generic generator-memory cuts, changes the target session and clears loaded sessions/bindings, but does not cancel/invalidate its image/text queue. `generator.rs:378–391` already copied preserved turns into queued requests; `generator_pump.rs:105` later dispatches these against the target's current session. A Count/graft batch with a remaining sibling can therefore replay pre-cut turns into the new session, even though preservation is now false and the incoming memory wire is gone. Document Undo cannot protect against that later inference side effect.

Minimal remedy: invalidate pending requests and terminate/reject old-session late work through the existing generation lifecycle owner when committing a history cut. Alternatively, explicitly block the cut until generation is idle; do not silently rebind a prepared replay to the fresh session. Retain accepted partial results. Scope cancellation to this target so shared upstream sessions/other generators are unaffected.

Tests: cut with a queued Count/graft sibling containing preserved turns -> no sibling dispatched with those turns; cut while capture is pending; late old-session result cannot install output/current-request state; accepted partial outputs remain; Undo restores document wires/session metadata without restarting cancelled inference.

## Preserved paths and remaining evidence

Source inspection supports longest-list matching with last-value repeat, copied ungrafted bundles, shared declared Table/Control/View sources, image Count per branch, text/artifact Count expansion and eight-job limits. Dynamic sources share submission-local capture state; successful captures stay frozen while another source loads. These are source observations, not executed validation.

Cut commands journal wire removal and session/bundle/preservation changes together. Retained image sources are resolved against the workbook before seeding the fresh manifest. The new manifest has no transcript, provider conversation or grants. Generator-memory target checks exclude Chat Train, and consumed non-generator rails retain detach refusal. No change to immutable Chat Train rails is requested by this review.

The handoff accurately limits cross-modality memory: artifact planners require artifact checkpoint pins; image/text inherited artifact turns currently depend on loaded source sessions. Archived-transcript display and result-level provenance UI remain pending. Do not label those paths complete. Real capture fidelity, reload/export/source rebasing and provider partial-failure behavior still require the already assigned targeted validation; this review adds no full-workspace audit or build requirement.

Gate stays changes-required until GR1–GR4 are corrected and the named focused regressions pass on final integrated source.
