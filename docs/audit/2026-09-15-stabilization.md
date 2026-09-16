# Stabilization implementation — 15 September 2026

First implementation batch following the [architectural audit](2026-09-14-lead-architect-review.md). Changes build on the existing working tree at `6338b1c`; unrelated work has been preserved. No Constitution amendment, new portal host, app-to-app extraction, or MCP server is included in this batch.

## Changes

### Staging safety

The staging owner now checks the actual saved workbook and the current state of every node before committing a proposal. A wrong-tab or unsaved-workbook refusal leaves the proposal available. Stale Patch/Remove commands are rejected without overwriting intervening human edits. Sequential commands are checked against the evolving proposed scene, and Add commands preserve proposal order. Live human gesture mutation semantics are unchanged.

The app honors the workbook's read-only lease. Acceptance writes an `applying` marker before its journal commit, then confirms `accepted`. An interrupted or unreadable decision requires review and cannot automatically replay. Result-write failures are surfaced; rejection is only reported complete after its result is saved. Proposal filenames are validated. The [file contract](../agent-link-contract.md) describes the save/crash limits: result files and the in-memory journal are not one atomic transaction, and acceptance does not imply that the workbook has been saved.

### Scene lookup

`slate-doc::Scene` owns a derived ID-to-position index. Present-ID lookups no longer scan the scene on every paint. Ordered nodes remain the authored z-order; serialization, undo and stable IDs retain their existing contracts. Geometry-only edits keep the lookup warm. Bulk Add avoids rebuilding the index after every insertion.

Because the node vector remains public, missing-ID lookups retain a linear fallback and cached slots are checked against their identity. IDs must remain unique. This change does not claim constant-time negative lookups or repair malformed duplicate-ID documents.

### Vector cache

The existing path owner now shares stroke and fill geometry under a **64 MiB payload budget** and **32,768-entry metadata limit**, using second-chance eviction. Cached geometry is shared through `Arc`, and warm paints avoid constructing a new world path. Rotation participates in the stroke key. Oversized geometry renders without flushing resident geometry.

This bounds cache memory rather than guaranteeing that arbitrary vector complexity fits. Egui still needs transformed output buffers; broader paint allocation and GPU work remain to be measured separately.

### Selection inspector and command interaction

The shared dock now owns explicit panel open/close requests and queries. Slate routes F3 and its menu to that state, then renders the existing Selection inspector body. Inspector fields remain a form in icon-strip mode, with the existing panel chrome. Other palettes retain their layout and pins. This makes proposal review and recovery controls reachable. The separate Lens-body gap remains recorded in DV-12.

Shared command-palette rows now give the command name priority and show an inline hint only when it fits beside that name. Full hints remain on hover, and long names elide within the row. A stationary pointer no longer overrides keyboard result navigation.

Slate shortcut dispatch now matches each keypress's own modifiers. Previously it combined a recorded keypress with the frame's final Ctrl/Shift/Alt state; a quick Ctrl+Z followed by releasing Ctrl before the next frame could arm bare-Z Zoom. Event-local matching preserves the intended command under this timing.

## Validation

- Current pure `slate-doc` source compiled with optimized Rust settings: **123 passed, 0 failed, 1 ignored**. All **17 staging regressions** passed, including an injected failure of only the final acceptance confirmation after the journal commit.
- Explicit ignored lookup benchmark passed: **50.8271 ms indexed versus 9.0832706 s linear** for one million lookups over 10,000 nodes, about **179×** for this operation. Compilation was running concurrently; this is a same-run comparison, not an isolated frame-rate certification.
- Release Slate vector-path suite: **12 passed, 0 failed**. It covers 10,000 warm stroke/fill pairs, memory/entry bounds, cache sharing, eviction, oversized geometry, picking, and rotation.
- Final Cargo release library run: **atlas-shell 105 passed; Slate 331 passed, 0 failed, 3 ignored**. This includes F3, dock-form, palette navigation, four event-modifier regressions, preference isolation, and the existing interaction/export suites. The initial broader run's five failures were investigated and corrected as described below; both logs are retained.
- Release binary built and exercised natively. After adding unit-test isolation, the normal release configurations of both Slate and File Atlas also passed `cargo check`.
- Focused formatting and whitespace checks passed.

Windows denied sandboxed execution of the test binaries and denied the minimal Cargo dependency build script even outside the sandbox. The document suite was therefore compiled directly from current source against matching cached dependencies; its optimized executable ran successfully through approved unsandboxed execution. The Slate test executable was built by Cargo and likewise ran successfully outside the sandbox. No security settings or exclusions were changed. This is **not a full workspace test pass**.

### Test isolation uncovered by the broader run

Slate's headless unit tests loaded personal settings, dock pins and recent-workbook history, and could save those records and launch Explorer during export. The new Selection test exposed this by saving its test pin state. Unit-test builds now use default settings/chrome and empty per-instance recents, suppress personal preference persistence and cover bakes, and suppress external shell launches. Normal application behavior is unchanged. A regression checks that one headless instance cannot seed the next through these preferences.

The object-snap fixtures now explicitly disable the independent smart guides they were not testing. The web-export fixture explicitly creates and binds a Web portal: folders intentionally route through the portal chooser now, so its old drop call had returned the folder and the fixture was accidentally resizing its slide frame. The original packaging and sandbox assertions remain intact. The click-placement and routing tests pass with isolated preferences; their behavior assertions were preserved.

After native verification, the test instance was closed. The original running app re-saved its retained chrome by selecting its already-current dock placement; its three pinned palettes and custom portal order were restored. Loading and closing one empty local `preference-recovery.slate` tab re-saved its retained recent-workbook list, removing the seven unit-test records. That explicit recovery fixture remains as one recent entry; the original unsaved board was preserved. Settings had not changed during this test batch. The isolated rerun left the recent-history file unchanged.

Exact logs and document-test provenance are in [the evidence directory](2026-09-15-evidence).

### Native 10,000-node comparison

The same mixed local fixture was measured at 1440 × 900, fitted at approximately 8%, with Pan armed and nothing selected. It contains 5,000 image placements, 4,000 closed vector paths, 900 text nodes and 100 web portals. Images reuse a small local asset set; portals largely show posters. This does not certify 10,000 unique decoded images or many simultaneously live browsers.

Across the latest 180-frame window, CPU app work averaged **39.29 ms**, versus **124.53 ms** in the audit baseline (about **68% less**). Warm tessellation misses fell from **8,000 per paint to zero**. Delivered interval p95 improved from **222.04 ms to 122.64 ms**. These are app/session timings, not GPU presentation timestamps. The board still misses the smooth-interaction target. Fit and wheel zoom were exercised visually; synthetic drag panning remains unverified.

The [matched runtime sample](2026-09-15-evidence/slate-10000-fit-1440.json) and [native screenshot](2026-09-15-evidence/slate-10000-fit-1440.png) record this comparison. The test process was separated from the user's original open Slate session.

### Native staging workflow

A local test proposal was discovered by the running app, accepted through its registered command, and visibly added a frame with an unsaved indicator. The result file reported `accepted`. Saving through the command palette cleared the indicator and persisted both the agent portal and added frame; [the saved workbook](2026-09-15-evidence/staging-accepted.slate) records the accepted scene. This validates the JSON staging workflow, not a model provider or an MCP server.

One Undo command removed that frame and marked the workbook dirty. A second save persisted the [undone scene](2026-09-15-evidence/staging-undone.slate), with only its original agent portal. The decision correctly remains `accepted`: it records consumption of the proposal, not whether the human later undid its edit.

After rebuilding the UI changes, the same workbook reopened with one node and no pending proposal badge. F3 closed and reopened the existing inspector on one press. Selecting the agent portal and expanding Portal exposed accessible Accept/Reject buttons in the form. A second proposal was accepted by clicking **Accept**, saved with **Ctrl+S**, then removed with **Ctrl+Z**. The keypress now invoked Undo rather than Zoom. [Visible controls](2026-09-15-evidence/inspector-visible-proposal.jpg), [saved acceptance](2026-09-15-evidence/inspector-accepted-saved.slate) and [keyboard Undo](2026-09-15-evidence/inspector-keyboard-undo.jpg) preserve the evidence.

The inspector's minimize control and one-press F3 reopening also worked. The [rebuilt palette](2026-09-15-evidence/palette-refined.jpg) shows readable command names with short hints retained and long hints no longer painted over names. Most canvas chrome still lacks useful accessibility names, and small form text remains a broader refinement issue.

## Remaining work

The original audit remains the wider roadmap. Selection wiring and the observed command defects are implemented and verified; the separate Lens panel remains unfinished.

Subsequent performance work includes bounded Atlas discovery within large flat directories, shared Atlas session ownership, Slate picking/drag work and texture completion budgets. The JSON file link is still the agent integration surface; an actual MCP adapter and richer unsaved-scene context remain later work.

A read-only vector geometry probe found about 10.09 MB of transformed fill/stroke output per overview paint for this fixture. A trial flattening tolerance of 1 world unit reduced that estimate to 5.76 MB. This is a candidate for screen-scale-aware geometry, not an implemented optimization: sampled curve-boundary error does not certify stroke joins, holes or topology. Any implementation needs DPI-aware cache keys and visual/geometry validation while retaining authored geometry and export fidelity.
