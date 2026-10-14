# Input-owner baseline: hosted CI

This is a failed baseline run, not final verification. [CI run 38499089898](https://github.com/jmmoser7/AtlasFileExplorer/actions/runs/38499089898), job 115641508866, provides retrievable logs. PR head is `4ed9d4ef3628ba55d90cc636270b6c822e1c11dd`; Actions checked out synthetic merge `8394c3737453897da95f67802974c524043f2f39` into ancestor base `1a8c59a8bae376817e5ad1fc4aca4f1ba2283cd4`. Source content matches the reviewed `8fd679e1` corrections except canonicalized line endings. It predates the actual-menu-bounds wheel correction.

The job installed the workflow's pinned Pdfium successfully. Formatting and segmentation installer checks passed. Exact test command:

```bash
set -o pipefail
cargo test --locked --workspace --no-fail-fast 2>&1 | tee test-output.log
```

Cargo exited **101**. Compilation took **6m 12s**; Slate's test binary took **183.68s**. Summing Cargo's reported binary/doctest summaries gives **2,752 passed, 6 failed, 72 ignored**. No PDF/Office failure remains in this environment. Local full decoded evidence is retained in ignored `target/feedback-ci-linux-baseline.log`; the relevant assertions are copied below.

## Required input checks

- `a_selected_picture_offers_the_agent_and_its_editor_paints_the_choices`: **PASS**.
- `text_selection_outline_is_suppressed_while_navigating_the_text_editor`: **FAIL**.
- `wheel_over_the_typeface_list_does_not_zoom_the_board`: **FAIL**.
- `deleting_a_rectangle_edge_opens_its_outline`: **PASS**.
- `tiny_shape_body_moves_before_overlapping_transform_grips`: **FAIL**.
- `wire_grab_zone_uses_a_grab_cursor_on_every_node_kind`: **PASS**.
- `a_popup_over_the_text_squircle_owns_the_click`: **PASS**.
- `releasing_a_text_squircle_press_outside_does_not_activate_it`: **PASS**.
- `opaque_body_blocks_subselect_of_a_curve_behind_it`: **PASS**.
- `covering_area_and_open_editor_block_raw_edge_input`: **PASS**.

All four negative ownership checks passed. The additional failures involve direct curve knobs under the property strip; positive target passes alone would have missed these regressions.

## Failure diagnostics

```text
---- app::board_properties::tests::text_selection_outline_is_suppressed_while_navigating_the_text_editor stdout ----

thread 'app::board_properties::tests::text_selection_outline_is_suppressed_while_navigating_the_text_editor' (16555) panicked at apps/slate/src/app/board_properties.rs:2845:9:
assertion `left == right` failed
  left: None
 right: Some(Text)
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

---- app::board_properties::tests::wheel_over_the_typeface_list_does_not_zoom_the_board stdout ----

thread 'app::board_properties::tests::wheel_over_the_typeface_list_does_not_zoom_the_board' (16607) panicked at apps/slate/src/app/board_properties.rs:2845:9:
assertion `left == right` failed
  left: None
 right: Some(Text)

---- app::tests::closed_form::grips::a_direct_select_knob_on_a_soft_stroke_wins_over_a_strip_button stdout ----

thread 'app::tests::closed_form::grips::a_direct_select_knob_on_a_soft_stroke_wins_over_a_strip_button' (23028) panicked at apps/slate/src/app/tests/closed_form/grips.rs:274:5:
the handle follows the drag: [100.0 25.0] -> [100.0 25.0]

---- app::tests::subobject::tiny_shape_body_moves_before_overlapping_transform_grips stdout ----

thread 'app::tests::subobject::tiny_shape_body_moves_before_overlapping_transform_grips' (29903) panicked at apps/slate/src/app/tests/subobject.rs:32:9:
assertion `left != right` failed: tiny body moves at zoom 0.5
  left: 0.0
 right: 0.0

---- app::tests::tip_direct::handles::a_handle_knob_under_the_property_strip_drags_with_a_panel_open stdout ----

thread 'app::tests::tip_direct::handles::a_handle_knob_under_the_property_strip_drags_with_a_panel_open' (30841) panicked at apps/slate/src/app/tests/tip_direct/handles.rs:431:5:
the handle lengthens: [0.0 25.0] -> [0.0 25.0]

---- app::tests::tip_direct::handles::a_handle_knob_under_the_property_strip_still_drags stdout ----

thread 'app::tests::tip_direct::handles::a_handle_knob_under_the_property_strip_still_drags' (30851) panicked at apps/slate/src/app/tests/tip_direct/handles.rs:378:5:
the handle lengthens: [0.0 25.0] -> [0.0 25.0]


```

## Linux Cargo binary counts

Each entry is passed / failed / ignored. Zero-test binaries and zero-test doctests are omitted.

- `Running unittests src/lib.rs (target/debug/deps/atlas_agent-868ddd7db1a3b352)`: **14 / 0 / 0** (0.00s).
- `Running unittests src/lib.rs (target/debug/deps/atlas_ai-fc9d6d55f4fad9b0)`: **76 / 0 / 2** (1.10s).
- `Running unittests src/lib.rs (target/debug/deps/atlas_codex-7030017e31c7f3d2)`: **6 / 0 / 1** (0.00s).
- `Running unittests src/lib.rs (target/debug/deps/atlas_comfy-8eeb2f1fb437cc14)`: **9 / 0 / 2** (0.00s).
- `Running unittests src/lib.rs (target/debug/deps/atlas_commands-592cd8ddc9d059d2)`: **19 / 0 / 0** (0.00s).
- `Running unittests src/lib.rs (target/debug/deps/atlas_core-f3a56eaf98717513)`: **153 / 0 / 0** (0.45s).
- `Running tests/cloud_guard.rs (target/debug/deps/cloud_guard-dc149420de5a9a8b)`: **0 / 0 / 2** (0.00s).
- `Running tests/folder_probe.rs (target/debug/deps/folder_probe-90bcaabb1ad26150)`: **0 / 0 / 1** (0.00s).
- `Running tests/scan_bench.rs (target/debug/deps/scan_bench-f35282e6ba4c032b)`: **0 / 0 / 1** (0.00s).
- `Running tests/thumb_bench.rs (target/debug/deps/thumb_bench-5544aaa0dc572e39)`: **0 / 0 / 1** (0.00s).
- `Running unittests src/lib.rs (target/debug/deps/atlas_curl-3387587cbece218e)`: **5 / 0 / 0** (0.02s).
- `Running unittests src/lib.rs (target/debug/deps/atlas_ollama-301e8ffa9772fdb5)`: **3 / 0 / 1** (0.06s).
- `Running unittests src/lib.rs (target/debug/deps/atlas_openai-2cb96cb27b463864)`: **5 / 0 / 0** (0.00s).
- `Running unittests src/lib.rs (target/debug/deps/atlas_packs-31cfaeda8774505f)`: **7 / 0 / 0** (0.40s).
- `Running unittests src/lib.rs (target/debug/deps/atlas_segment-72a7cf4d9ff2e7c9)`: **4 / 0 / 0** (0.00s).
- `Running unittests src/lib.rs (target/debug/deps/atlas_shell-b7e715c6c713a824)`: **272 / 0 / 2** (0.14s).
- `Running unittests src/lib.rs (target/debug/deps/atlas_update-aad69b91a19485dd)`: **2 / 0 / 0** (0.00s).
- `Running unittests src/lib.rs (target/debug/deps/circle_pack-89d1fd6d98715942)`: **14 / 0 / 0** (0.00s).
- `Running unittests src/lib.rs (target/debug/deps/code_lens-b725795e3091ced2)`: **25 / 0 / 0** (1.10s).
- `Running tests/extract_smoke.rs (target/debug/deps/extract_smoke-901592ad648416e1)`: **10 / 0 / 0** (5.02s).
- `Running tests/pipeline_smoke.rs (target/debug/deps/pipeline_smoke-454736cc0b4d3df9)`: **1 / 0 / 0** (0.00s).
- `Running tests/self_layout_stats.rs (target/debug/deps/self_layout_stats-4dc0236e8120a209)`: **0 / 0 / 1** (0.00s).
- `Running tests/layout.rs (target/debug/deps/layout-50e157866859af49)`: **17 / 0 / 0** (0.00s).
- `Running unittests src/lib.rs (target/debug/deps/model_preview-eb6967e36ed72b96)`: **5 / 0 / 0** (0.00s).
- `Running tests/readers.rs (target/debug/deps/readers-c198534a4b1eaab8)`: **8 / 0 / 0** (0.00s).
- `Running unittests src/lib.rs (target/debug/deps/native_file_atlas-11a56db651709192)`: **43 / 0 / 1** (0.39s).
- `Running unittests src/lib.rs (target/debug/deps/rhino_mesh-554b54a1162cf6eb)`: **21 / 0 / 0** (0.00s).
- `Running tests/read_fixtures.rs (target/debug/deps/read_fixtures-bcb8dde5a6728419)`: **9 / 0 / 0** (0.03s).
- `Running unittests src/lib.rs (target/debug/deps/slate-a25e2aebbe53daba)`: **1384 / 6 / 53** (183.68s).
- `Running unittests src/lib.rs (target/debug/deps/slate_artifact-7b0ec6c7b5774374)`: **75 / 0 / 1** (0.08s).
- `Running tests/cutout_sticker.rs (target/debug/deps/cutout_sticker-33f8891e8bf86803)`: **1 / 0 / 0** (0.00s).
- `Running tests/export_smoke.rs (target/debug/deps/export_smoke-0373fff57bcc3a9a)`: **4 / 0 / 0** (0.02s).
- `Running tests/hosted_shape_text.rs (target/debug/deps/hosted_shape_text-2c8b39ed46e43c82)`: **1 / 0 / 0** (0.02s).
- `Running tests/membership.rs (target/debug/deps/membership-167c818f844fa3a4)`: **1 / 0 / 0** (0.00s).
- `Running unittests src/lib.rs (target/debug/deps/slate_doc-e42093395d473a15)`: **290 / 0 / 1** (1.28s).
- `Running tests/image_paint_layers.rs (target/debug/deps/image_paint_layers-6e1f7e973a397821)`: **3 / 0 / 0** (0.00s).
- `Running tests/migration.rs (target/debug/deps/migration-45ff3c25ce4bf398)`: **6 / 0 / 1** (0.00s).
- `Running unittests src/lib.rs (target/debug/deps/slate_kit-046e2ed525da16d7)`: **67 / 0 / 0** (0.01s).
- `Running unittests src/lib.rs (target/debug/deps/vector_ink-5694f3f2a3eab416)`: **143 / 0 / 0** (0.11s).
- `Running tests/bench_grain.rs (target/debug/deps/bench_grain-e51ab95019259998)`: **0 / 0 / 1** (0.00s).
- `Running tests/pen_fit.rs (target/debug/deps/pen_fit-c0def689729a6233)`: **2 / 0 / 0** (0.00s).
- `Running tests/texture_rules.rs (target/debug/deps/texture_rules-322a083a500f640b)`: **4 / 0 / 0** (0.07s).
- `Running unittests src/lib.rs (target/debug/deps/xtask-bed5c0ee5b9c6db8)`: **27 / 0 / 0** (0.00s).
- `Running tests/contracts.rs (target/debug/deps/contracts-519f851facd601a5)`: **4 / 0 / 0** (0.01s).
- `Running tests/kits.rs (target/debug/deps/kits-ee9dd1743391b7d1)`: **3 / 0 / 0** (0.03s).
- `Running tests/metrics.rs (target/debug/deps/metrics-851173f00e68fc8f)`: **4 / 0 / 0** (4.33s).
- `Running tests/size.rs (target/debug/deps/size-2b7578a622f2ae2a)`: **1 / 0 / 0** (0.03s).
- `Running tests/theme.rs (target/debug/deps/theme-bac86385345bf555)`: **1 / 0 / 0** (0.07s).
- `Doc-tests circle_pack`: **1 / 0 / 0** (0.11s).
- `Doc-tests collage`: **1 / 0 / 0** (0.07s).
- `Doc-tests slate_kit`: **1 / 0 / 0** (0.19s).

## Other checks

The same CI run's Windows compile job 115641509137 passed. Clippy job 115641509079 failed under Rust 1.99 with exit 101: `crates/slate-doc/src/image_adjust.rs:220`, `clippy::chunks_exact_to_as_chunks`, promoted by `-D warnings`. Its suggested array-chunk iteration was subsequently implemented without changing complete-pixel or trailing-byte behavior; that later source passed local Windows workspace/all-target Clippy with `-D warnings`. It does not change this baseline's recorded result.

## Windows runtime result

[Windows run 38499089904](https://github.com/jmmoser7/AtlasFileExplorer/actions/runs/38499089904), job 115641509220, tested the same PR merge source. The real installation/coordinated upgrade checks passed. `cargo test --locked --workspace --no-fail-fast` compiled in **8m 34s**. Its reported binary/doctest summaries total **2,778 passed, 7 failed, 86 ignored**. The workflow step exited **1**; no separate Cargo exit marker was captured, so the report does not substitute a guessed Cargo code. Packaging and publishing were skipped after test failure. Full decoded evidence is retained in ignored `target/feedback-ci-windows-baseline.log`, with retrievable job logs in the linked run.

Slate reported **1,395 passed, 6 failed, 61 ignored**, with the same six failures and assertions as Linux; its test execution took **185.19s**. File Atlas reported **43 passed, 1 failed, 1 ignored** in **3.42s**. Its additional assertion was:

```text
---- app::tests::delete_key_removes_the_card_under_the_cursor stdout ----
[atlas] scan complete: 2 files in 7ms (mode=fresh)
[atlas] warming thumbnail cache for 2 files in background

thread 'app::tests::delete_key_removes_the_card_under_the_cursor' (6456) panicked at apps\file-atlas\src\app\tests.rs:1600:5:
the file is still there
```

The separate Recycle Bin backend test passed. Read-only tracing confirmed that the app fixture enables Edit mode, approves the small deletion, uses the registered hover-capable Delete chord, and has no missing asynchronous preflight for deletes. Keyboard-focus attribution remains unproven. The existing test was extracted to `tests/delete.rs` without removing any assertion or changing its ten-second bound; a stronger dispatch assertion now reports focus, hover, mode, confirmation and toast state. No production filesystem guard was relaxed.
