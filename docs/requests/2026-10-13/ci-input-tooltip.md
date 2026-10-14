# Native tooltip runtime and remaining scrolling failure

This report covers [Linux CI run 38504869702](https://github.com/jmmoser7/AtlasFileExplorer/actions/runs/38504869702), job 115659940134. PR head was `88fde2c1491875e962e80793443e1efb42394408`; Actions checked out synthetic merge `269f032a601a57dcca75150ddaa246fd74802c7d`.

`cargo test --locked --workspace --no-fail-fast`, through `tee` with `pipefail`, exited **101**: **2762 passed / 2 failed / 72 ignored**. Compilation took **6m 24s**. Slate executed **179.84s**, reporting **1,389 passed / 2 failed / 53 ignored**. Linux Clippy with `-D warnings`, formatting and Windows all-target compilation passed. The matching Windows runtime run is still pending; these Linux outcomes must not be used as its result.

## Named checks

- PASS: `a_selected_picture_offers_the_agent_and_its_editor_paints_the_choices`.
- PASS: `text_selection_outline_is_suppressed_while_navigating_the_text_editor`.
- FAIL: `wheel_over_the_typeface_list_does_not_zoom_the_board`, now reaches the real scrolling assertion at `board_properties/tests/wheel.rs:104`: **the list actually scrolled**. Text and family opening and the preceding unchanged-camera checks pass; later outside-zoom checks were not reached.
- PASS: `deleting_a_rectangle_edge_opens_its_outline`.
- PASS: `tiny_shape_body_moves_before_overlapping_transform_grips`.
- PASS: `wire_grab_zone_uses_a_grab_cursor_on_every_node_kind`.
- PASS: `a_popup_over_the_text_squircle_owns_the_click`.
- PASS: `releasing_a_text_squircle_press_outside_does_not_activate_it`.
- PASS: `opaque_body_blocks_subselect_of_a_curve_behind_it`.
- PASS: `covering_area_and_open_editor_block_raw_edge_input`.
- PASS: `a_first_native_text_tooltip_preserves_the_immediate_real_click`.
- PASS: `a_real_popup_still_owns_the_click_above_native_label_chrome`.

The tooltip fix now passes both its actual first-hover click test and the unchanged app outline/navigation test. No geometric click fallback or covering-layer bypass was introduced.

The other failure is `agent_life_publishing_context_never_decodes_pictures_on_the_frame_loop`, at `board_agent/life.rs:1070`. Its unchanged-crop timestamp assertion expected `SystemTime { tv_sec: 1700000000, tv_nsec: 0 }` and observed `SystemTime { tv_sec: 1791947868, tv_nsec: 133472146 }`, with **0 decodes on the frame loop**. The older recovery report also recorded this failure followed by a passing focused rerun. That history establishes intermittency, not a reason to ignore this failure; diagnosis is pending.

Pdfium prerequisites succeeded and PDF/Office tests ran successfully. There are no PDF environment failures in this run. Existing ignored corpus/benchmark tests were not enabled.

## Per-binary results

All summaries with a nonzero passed, failed or ignored count are retained below. Zero-test binaries/doctests also ran successfully. These counts are from this run alone.

- `unittests src/lib.rs`: 14 passed / 0 failed / 0 ignored.
- `unittests src/lib.rs`: 76 passed / 0 failed / 2 ignored.
- `unittests src/lib.rs`: 6 passed / 0 failed / 1 ignored.
- `unittests src/lib.rs`: 9 passed / 0 failed / 2 ignored.
- `unittests src/lib.rs`: 19 passed / 0 failed / 0 ignored.
- `unittests src/lib.rs`: 153 passed / 0 failed / 0 ignored.
- `tests/cloud_guard.rs`: 0 passed / 0 failed / 2 ignored.
- `tests/folder_probe.rs`: 0 passed / 0 failed / 1 ignored.
- `tests/scan_bench.rs`: 0 passed / 0 failed / 1 ignored.
- `tests/thumb_bench.rs`: 0 passed / 0 failed / 1 ignored.
- `unittests src/lib.rs`: 5 passed / 0 failed / 0 ignored.
- `unittests src/lib.rs`: 3 passed / 0 failed / 1 ignored.
- `unittests src/lib.rs`: 5 passed / 0 failed / 0 ignored.
- `unittests src/lib.rs`: 7 passed / 0 failed / 0 ignored.
- `unittests src/lib.rs`: 4 passed / 0 failed / 0 ignored.
- `unittests src/lib.rs`: 277 passed / 0 failed / 2 ignored.
- `unittests src/lib.rs`: 2 passed / 0 failed / 0 ignored.
- `unittests src/lib.rs`: 14 passed / 0 failed / 0 ignored.
- `unittests src/lib.rs`: 25 passed / 0 failed / 0 ignored.
- `tests/extract_smoke.rs`: 10 passed / 0 failed / 0 ignored.
- `tests/pipeline_smoke.rs`: 1 passed / 0 failed / 0 ignored.
- `tests/self_layout_stats.rs`: 0 passed / 0 failed / 1 ignored.
- `tests/layout.rs`: 17 passed / 0 failed / 0 ignored.
- `unittests src/lib.rs`: 5 passed / 0 failed / 0 ignored.
- `tests/readers.rs`: 8 passed / 0 failed / 0 ignored.
- `unittests src/lib.rs`: 43 passed / 0 failed / 1 ignored.
- `unittests src/lib.rs`: 21 passed / 0 failed / 0 ignored.
- `tests/read_fixtures.rs`: 9 passed / 0 failed / 0 ignored.
- `unittests src/lib.rs`: 1389 passed / 2 failed / 53 ignored.
- `unittests src/lib.rs`: 75 passed / 0 failed / 1 ignored.
- `tests/cutout_sticker.rs`: 1 passed / 0 failed / 0 ignored.
- `tests/export_smoke.rs`: 4 passed / 0 failed / 0 ignored.
- `tests/hosted_shape_text.rs`: 1 passed / 0 failed / 0 ignored.
- `tests/membership.rs`: 1 passed / 0 failed / 0 ignored.
- `unittests src/lib.rs`: 290 passed / 0 failed / 1 ignored.
- `tests/image_paint_layers.rs`: 3 passed / 0 failed / 0 ignored.
- `tests/migration.rs`: 6 passed / 0 failed / 1 ignored.
- `unittests src/lib.rs`: 67 passed / 0 failed / 0 ignored.
- `unittests src/lib.rs`: 143 passed / 0 failed / 0 ignored.
- `tests/bench_grain.rs`: 0 passed / 0 failed / 1 ignored.
- `tests/pen_fit.rs`: 2 passed / 0 failed / 0 ignored.
- `tests/texture_rules.rs`: 4 passed / 0 failed / 0 ignored.
- `unittests src/lib.rs`: 27 passed / 0 failed / 0 ignored.
- `tests/contracts.rs`: 4 passed / 0 failed / 0 ignored.
- `tests/kits.rs`: 3 passed / 0 failed / 0 ignored.
- `tests/metrics.rs`: 4 passed / 0 failed / 0 ignored.
- `tests/size.rs`: 1 passed / 0 failed / 0 ignored.
- `tests/theme.rs`: 1 passed / 0 failed / 0 ignored.
- `circle_pack`: 1 passed / 0 failed / 0 ignored.
- `collage`: 1 passed / 0 failed / 0 ignored.
- `slate_kit`: 1 passed / 0 failed / 0 ignored.

The full decoded job output is retained in ignored `target/feedback-ci-linux-tooltip.log` and the retrievable Actions job log.

## Reviewed fixture corrections awaiting runtime

The scrolling probe could select static parent-editor text because its painted
position and canvas clip intersect the dropdown. It now requires a known
`Typeface::ALL` label and a clip bounded vertically by the measured list. A
vertical ScrollArea intentionally retains its parent's horizontal clip, so
full two-dimensional clip containment would reject genuine rows. Actual row
motion, visible-extension probing, unchanged camera/idle tail, selection,
list-open and outside-zoom assertions remain required.

The timestamp race is in fixture readiness: a fast worker can publish the
initial crop before that same frame's card fitting journals a geometry update.
The next scene revision then legitimately schedules preparation to the same
derived path after the test has stamped it. The first-publication predicate now
also awaits the current revision through the existing `published_train_inputs`
preparation owner. The test explicitly asserts unchanged app/document revisions
through republication before checking the original timestamp. Its original
timeout, sleeps, zero frame-loop decode count, changed-crop and hidden-source
assertions remain intact. Production preparation/cache code is unchanged.

Both fixture changes received independent read-only approval. Runtime outcomes
must be taken from their subsequent snapshot; this report remains the record
of the preceding failed run.

Windows all-target workspace Clippy with `-D warnings` passed on the fixture
correction, exit **0**, **20.95s** after the final formatting-only path alias,
retained in
`target/feedback-fixture-clippy-final.log`. Formatting and the unchanged size
audit passed: **797 Rust files / 111 allowlist entries / zero findings**.

## Matching Windows result

[Windows run 38504869663](https://github.com/jmmoser7/AtlasFileExplorer/actions/runs/38504869663), job 115660364559, tested the same head/merge identity and reported **2,789 passed / 2 failed / 86 ignored**. Compilation took **12m 25s**; Slate executed **305.71s**, with **1,401 passed / 1 failed / 61 ignored**. The same wheel row-motion assertion failed; Text opening/outline, native-tooltip checks and the other named input ownership checks passed. The image-cache timestamp test passed on Windows, consistent with the Linux timing race. The test step exited **1**, without an independent Cargo exit marker; packaging/publishing were skipped.

File Atlas reported **43 passed / 1 failed / 1 ignored** in **12.33s**. Its Delete test again failed at the original **the file is still there** assertion after the new dispatch assertion passed. This establishes successful dispatch, not successful backend completion. The core Recycle Bin test passed earlier in the same run. Two intermediate Windows runs passed this app test; the initial baseline and this run failed. No production Delete guard, timeout, file type, or permanent-delete behavior has been changed. The next snapshot adds final operation/progress/toast diagnostics while retaining the filesystem, canvas and keeper assertions. A thumbnail/shell overlap is a source-based hypothesis, not an established cause.

That diagnostic addition compiles with Windows `cargo clippy --locked -p native-file-atlas --all-targets -j2 -- -D warnings`, exit **0**, **17.01s** (`target/feedback-delete-diagnostic-clippy.log`). Full decoded Windows output is retained in ignored `target/feedback-ci-windows-tooltip.log` and the retrievable job log.

## Local preparation

Before dispatch, Windows `cargo clippy --locked --workspace --all-targets -j2 -- -D warnings` passed, exit **0**, **27.46s**. Formatting and the exact physical-row audit passed: **797 Rust files / 111 unchanged allowlist entries / zero findings**. No local denied unit-test executable was retried.

Release libraries on this source subsequently built with `cargo build --locked --release -p native-file-atlas -p slate --lib -j2`, exit **0**, **2m 27s**. This does not replace the running executable. The owner's unsaved workbook and existing process remain preserved while the save/close choice is pending.
