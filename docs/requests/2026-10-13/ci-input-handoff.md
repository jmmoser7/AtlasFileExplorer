# Input-owner handoff runtime check

This is an intermediate failed run. [CI run 38501180510](https://github.com/jmmoser7/AtlasFileExplorer/actions/runs/38501180510), Linux job 115648252060, tested PR head `66fd982edbdd30371cd479e87a19e4bfce14cac1` through synthetic merge `8f897700975f65052c55f9a6193b8270bd45b495`. It includes the clipped chrome Area, actual menu bounds, native grip-drag handoff and strengthened wheel test. It predates the press-origin strip-capture correction and Linux portability follow-up.

## Executed tests

`cargo test --locked --workspace --no-fail-fast` (with `pipefail` through `tee`) exited **101**. Compilation took **6m 25s**. Cargo binary/doctest summaries total **2,759 passed, 3 failed, 72 ignored**. Slate reported **1,388 passed, 3 failed, 53 ignored** in **178.04s**. All other independent binaries completed without failures. Full decoded evidence is retained in ignored `target/feedback-ci-linux-handoff.log` and the retrievable job log.

All three previously failing curve-handle cases now pass:

- `a_direct_select_knob_on_a_soft_stroke_wins_over_a_strip_button`
- `a_handle_knob_under_the_property_strip_still_drags`
- `a_handle_knob_under_the_property_strip_drags_with_a_panel_open`

All four new ownership/geometry guards from the previous baseline pass. New shared tests also pass:

- `selection_chrome_interacts_inside_its_parent_clip_and_does_not_occlude_outside`
- `the_visible_menu_extension_owns_the_wheel_and_scrolls`
- `clicking_a_visible_extended_row_selects_it_and_an_outside_click_dismisses`
- `grip_drag_handoff_respects_covering_area_inline_editor_and_space`

Agent-editor paint, rectangle-edge deletion and wire Grab checks still pass. These successes do not establish the remaining app-level Text or wheel checks.

## Remaining failures

Both `text_selection_outline_is_suppressed_while_navigating_the_text_editor` and `wheel_over_the_typeface_list_does_not_zoom_the_board` fail in their shared real-click opening helper, before the paint/wheel assertions:

```text
apps/slate/src/app/board_properties.rs:2845:9
assertion `left == right` failed
  left: None
 right: Some(Text)
```

`tiny_shape_body_moves_before_overlapping_transform_grips` still fails:

```text
apps/slate/src/app/tests/subobject.rs:32:9
assertion `left != right` failed: tiny body moves at zoom 0.5
  left: 0.0
 right: 0.0
```

Source tracing identified a deterministic strip-capture error at the tiny test's drag-threshold frame: its current pointer crosses a strip button although the press began on the body. The existing UI owner now uses its press-origin probe for strip capture while held. It retains real clicked actions and the aggregate guards. That follow-up remains to be executed.

The Text helper now arms a `cfg(test)` UI trace and includes pointer/response/layer/clip state in the original assertion's failure message. No input events, waits or expectations were changed.

## Other checks and platform evidence

Windows compilation passed. Linux Clippy job 115648252445 exited **101** under Rust 1.99: complete-pixel chunk lints in hosted-text/composite code and dead Windows-only clipboard constructors/parser paths. The follow-up preserves complete-array iteration and remainder behavior, gates Windows-only intake/parser production paths, keeps Linux parser tests, adds Bitmap precedence coverage, and documents only the enum constructor boundary. Local Windows Rust/Cargo 1.97.1 workspace/all-target Clippy passed on that follow-up (22.51s); the later tiny-capture/trace batch also passed (20.90s). Hosted Clippy must verify the Linux paths separately.

Release libraries for `66fd982e` built locally with `cargo build --locked --release -p native-file-atlas -p slate --lib -j2`, exit **0**, **3m 03s**. The existing Slate process was preserved. GUI activation returned `failed to activate captured window` on the initial attempt and one fresh-selection retry; no further GUI input was attempted. The owner was asked to save/close the previously observed unsaved workbook before executable replacement.

The parallel [Windows runtime run 38501180601](https://github.com/jmmoser7/AtlasFileExplorer/actions/runs/38501180601), job 115648253138, is recorded separately once its logs become retrievable. No Windows pass is inferred from the compile result.
