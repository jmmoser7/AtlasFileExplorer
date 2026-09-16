# Untitled.slate: reproduced frame stalls

**Update: code repair built and verified later on September 16.** See the final
section for the repaired build; the investigation below records the original
behavior and temporary workaround.

## Diagnosis

The Link health readout performs synchronous filesystem existence checks for every linked item on every UI frame. Disabling that one readout removes the dominant stall; re-enabling it restores the stall immediately. This was reproduced through native Computer Use with the user's workbook open in the current release executable on September 16, 2026.

The workbook is 182,322 bytes and contains 161 linked items, but only 80 board nodes: 72 images and eight web portals. Of the linked paths, 135 use P: and 26 use C:. Small workbook size does not bound the cost of checking its external sources.

## Controlled comparison

The same process, workbook, camera, window size, content and visible web portals were retained. Only bottom-left readout gear → Link health was changed. No document edits were saved.

- Initial enabled state: repeated approximately 450–590 ms app frames; canvas paint was approximately 0.1–0.4 ms. The first rolling summary also included earlier Home frames, so it is not used as a steady-state average.
- Disabled: the 180-frame rolling app average was **2.282 ms**, maximum **9.617 ms**. Delivered frame interval p95 was **25.246 ms**.
- Re-enabled: **34 consecutive recorded slow frames** averaged **452.981 ms**, median **446.244 ms**, p95 **472.895 ms**, maximum **613.605 ms**. Sample timestamps: 1789575742296–1789575757897 Unix milliseconds. These are recorded stall events, not a capture of every fast frame.
- Disabled again: the 180-frame rolling app average was **2.733 ms**, maximum **9.608 ms**. Delivered frame interval p95 was **35.402 ms**.

This is approximately a 166–199× reduction in measured app work. It is **not** proof of locked 60 fps: delivered intervals still exceed 16.67 ms, and this test does not measure continuous dragging or all content at every zoom. A right-arrow pan and matching left-arrow return also worked with Link health disabled.

The previous user-session log separately contains 851–987 ms app frames immediately before the earlier session ended. Those raw events were recovered into the evidence directory.

## Source trace and architectural implications

`apps/slate/src/app/ui/readouts.rs:103–109` scans `doc.items` and calls `link_status` for every item. `crates/slate-doc/src/link.rs:11–16` calls `Path::exists()`. This runs synchronously on the UI thread, before the measured board paint. Offscreen and unplaced linked items are included. That explains both the severe latency on a small board and why the existing paint spans failed to attribute it.

The same synchronous helper is called for visible items in Grid/Venn at `apps/slate/src/app/canvas.rs:904`. Hiding the readout is consequently only a Board workaround, not a complete product fix.

Article IX.3 already requires tri-state link health (Ok / Missing / Unknown) and forbids blocking a user-facing operation on a source round trip. The current two-state helper also treats an existence-check failure as Missing. The displayed “40 missing links” is therefore not evidence that 40 sources were deleted; errors and inaccessible sources need to remain distinguishable.

The web source metadata path already uses background workers and guards in-flight/stale results. Its ordinary lifecycle work is measured in `slate.web.pump`; focused portal input is included in board paint. Both were small during the reproduced stalls. Preserve those streaming and worker boundaries. Disabling web portals or delaying population would not address this cause.

## Next fix

1. Establish one shared, background link-health cache with generation checks, bounded work, refresh policy and explicit Unknown/error state. Readouts and Grid/Venn must consume the same results.
2. Maintain aggregate counts when results change; do not rescan all links on every frame. Invalidate correctly on add, remove, relink and workbook changes. Preserve useful missing-link feedback rather than deleting it.
3. Add timing around readouts and other currently unmeasured outer-frame work. Add a regression using a deliberately slow source to prove paint/update never waits for source I/O, including stale-result handling after relink or tab switch.
4. Repeat this exact native comparison with Link health enabled after the fix, then separately profile the remaining delivered-frame tail. The present result does not certify 10,000-node performance.

## Session state and evidence

Left Untitled open at its original board position with **Link health disabled** as the temporary workaround. Portal content remains enabled. The workbook SHA-256 is unchanged: `C60E8BD2A1D887DC471904F103A835DF722C63A61AA12DF5739F34333580ADD0`.

No product source was changed or binary rebuilt in this evaluation. The tested release executable SHA-256 is `9086B270BA0342A835C09528195BD71CEAE96DF63CD3B38E0C2FEA36399FDEEF`. Source inspection reflects the current checkout; the toggle experiment independently confirms the running binary's behavior.

Local evidence is in `target/untitled-perf-2026-09-16/`: input hashes, before/after snapshots, repeated-on raw events and summary, recovered previous-user stalls, and final workbook hash. `launch-home-baseline.json` is a launch/Home snapshot, not a sample of the user's original stall. Linked asset bytes were not inspected by diagnostic scripts.

## Repair and native verification

Implemented `slate_doc::LinkHealthCache` in the existing link-health owner. Two
lazy workers perform metadata-only checks, with at most sixteen outstanding
jobs, generation-checked results, and a thirty-second refresh. The UI drains and
schedules bounded work and reads cached counts/status. Both the readout and
Grid/Venn now use it. A runtime-only path revision invalidates membership after
add/remove/relink without reacting to scene drags or changing workbook JSON.
Unknown sources are shown as unverified rather than incorrectly missing.

The release build of Slate and File Atlas succeeded. Desktop and Start menu
shortcuts were refreshed. New Slate executable SHA-256:
`46917B1016C494318AB0D347DD6E40A9FF04B269490B07030B738B214B88FE0E`.

Native Computer Use reopened the exact user workbook. Link health was enabled,
displaying 40 missing links, with the three original visible web portals loaded.
An observed 180-frame sample averaged 1.522 ms app time; the saved settled
sample averaged **1.870 ms**, maximum **8.246 ms**, delivered interval p95
**20.853 ms**, maximum **24.008 ms**. This replaces approximately 453 ms of app
work in the original enabled-counter reproduction. New spans show
`slate.link_health:0.0` and `slate.readouts:0.1` ms in captured events. Refresh
remained active beyond its first thirty-second interval. Right-arrow pan and
left-arrow return worked; the workbook was left open at its original position
with Link health enabled. The delivered tail still does not establish locked
60 fps.

Validation limitation: eight regression tests were added (six cache tests,
one document-revision test, one app lifecycle test). `cargo test -p slate-doc
-p slate --lib` compiled successfully, but Windows refused to launch the Slate
test executable with **Access is denied (os error 5)**. An earlier targeted
document-test launch received the same error. Therefore no automated test pass
is claimed. Release launch and native performance verification succeeded.

Evidence: `fixed-inputs.json`, `fixed-link-health-on.json`, and
`fixed-link-health-on.jsonl` in the same local evidence directory. No workbook
content was saved during the repair verification.
