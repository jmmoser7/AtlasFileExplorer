# Cloud assertion verification — 2026-10-13

## Provenance and scope

- Requested immutable HEAD: `ac812a2524348cb076d16dd474b7ad3b2edc848b`.
- Verified `git rev-parse HEAD`: `ac812a2524348cb076d16dd474b7ad3b2edc848b`
  (exact match before validation).
- Requested branch name: `codex/feedback-assertions-20261013`.
- Checked-out local branch: `work`. No fetch or branch switch was performed; the
  checked-out commit was the requested immutable source.
- Validation-only: no production source, fixture, assertion, ignore set, timeout,
  dependency, secret, provider, or cloud-placeholder content was changed or read.
  Only this report was added.

## Required integrated run

```text
cargo test --locked -p slate -p atlas-shell -p atlas-ai --no-fail-fast -j2
```

- True Cargo exit: **101**, captured from `PIPESTATUS[0]` while retaining combined
  stdout/stderr with `tee`.
- The command was run directly without `/usr/bin/time` or another optional wrapper.
- Compilation completed in 29m 44s. `--no-fail-fast` ran every independent test
  binary and all three doc-test binaries.

| Test binary | Passed | Failed | Ignored | Result |
|---|---:|---:|---:|---|
| `atlas-ai` unit tests | 76 | 0 | 2 | PASS |
| `atlas-shell` unit tests | 272 | 0 | 2 | PASS |
| `slate` library unit tests | 1374 | 12 | 53 | FAIL |
| `slate` binary unit tests | 0 | 0 | 0 | PASS |
| Doc tests (`atlas-ai`, `atlas-shell`, `slate`) | 0 | 0 | 0 | PASS |
| **Total** | **1722** | **12** | **57** | **FAIL** |

The repaired/narrow regressions that passed in the integrated run include:

- `selection_tools::corner_layout_tests::narrow_corner_rows_keep_every_control_positive_and_inside_the_apron`
- `image_layer_tests::editing::an_expanding_corner_palette_keeps_final_geometry_and_clips_hidden_controls`
- `selection_tools::dimensions::disabled_inline_number_leaves_pointer_to_the_canvas`
- `tests::subobject::ctrl_shift_click_picks_an_edge_and_its_two_vertices`
- `tests::subobject::ctrl_shift_click_still_selects_one_group_member`
- `tests::subobject::ctrl_shift_sweeps_pick_contained_or_crossing_segments`
- `tests::subobject::subselect_respects_space_pan_and_off_canvas_chrome`
- `tests::closed_form::grips::direct_select_picks_closed_form_vertices_as_anchors`
- `tests::bbox::chrome::fillet_grip_press_beats_edge_band_but_nw_corner_still_resizes`
- `tests::packs::pk2_generate_offers_add_api_key_until_a_key_is_stored`
- `pdf::pages_album::tests::pages_album_is_a_thin_pallet_below_the_host`
- `image_layer_tests::crop_toggles_fit_inside_the_shared_corners_panel`

The integrated output also shows the new Atlas AI stale-probe/readiness cases pass:
`packs::tests::a_refresh_during_a_probe_runs_after_it_instead_of_being_dropped`,
`packs::tests::an_answer_recorded_mid_probe_is_not_undone_by_the_older_probe`, and
`packs::tests::a_stored_key_turns_add_api_key_into_a_normal_openai_option`.

## Environmental PDF/Office failures (6)

Linux has no usable Pdfium runtime. The exact runtime diagnostic was
`[atlas] pdfium.dll not found — PDF previews limited to shell handlers`. Per the
request, the previously denied download was not retried and these checks were
neither suppressed nor changed.

1. `app::tests::media::assets::unbundle_word_of_an_unsaved_workbook_writes_into_the_data_dir`
   ```text
   Added 1 file(s)
   Word did not produce a readable PDF. Check the PDF preview runtime.
   ```
2. `app::tests::media::assets::unbundle_word_writes_page_images_beside_a_saved_workbook`
   ```text
   Added 1 file(s)
   Word did not produce a readable PDF. Check the PDF preview runtime.
   ```
3. `app::tests::media::unbundle::unbundle_of_a_single_page_does_not_place_a_grid`
   ```text
   Added 1 file(s)
   Could not read PDF pages. Check the document and the PDF preview runtime.
   ```
4. `app::tests::media::unbundle::unbundle_pdf_places_pages_through_dispatch_and_undoes_in_one_step`
   ```text
   pdf unbundle did not finish: Added 1 file(s)
   Could not read PDF pages. Check the document and the PDF preview runtime.
   ```
5. `app::tests::media::unbundle::unbundle_pptx_and_legacy_ppt_when_office_can_render`
   ```text
   deck.pptx: Added 1 file(s)
   PowerPoint did not produce a readable PDF. Check the PDF preview runtime.
   ```
6. `app::tests::media::unbundle::unbundle_while_the_preview_is_pending_finishes_instead_of_failing`
   ```text
   Added 1 file(s)
   PowerPoint did not produce a readable PDF. Check the PDF preview runtime.
   ```

## Remaining non-environment failures (6)

Each remaining non-environment failure was rerun alone once with an exact test
filter and `--nocapture`. Every focused command exited **101** and reproduced the
same diagnostic, so these are runtime failures, not inferred source-review results.

1. `app::board_flow::tests::a_selected_picture_offers_the_agent_and_its_editor_paints_the_choices`
   ```text
   the prompt face shows its hint: ["+", "Untitled •", "×", "⚙", "1 linked file(s) · 0 tagged · 1 uncategorized", "100%", "ComfyUI · Missing", "ComfyUI · Auto", "Source", "Single", "Settings"]
   ```
2. `app::board_properties::tests::text_selection_outline_is_suppressed_while_navigating_the_text_editor`
   ```text
   assertion `left == right` failed
     left: None
    right: Some(Text)
   ```
3. `app::board_properties::tests::wheel_over_the_typeface_list_does_not_zoom_the_board`
   ```text
   assertion `left == right` failed
     left: None
    right: Some(Text)
   ```
4. `app::tests::subobject::deleting_a_rectangle_edge_opens_its_outline`
   ```text
   assertion `left == right` failed
     left: []
    right: [1]
   ```
5. `app::tests::subobject::tiny_shape_body_moves_before_overlapping_transform_grips`
   ```text
   assertion `left != right` failed: tiny body moves at zoom 0.5
     left: 0.0
    right: 0.0
   ```
6. `app::tests::wire_cursor::wire_grab_zone_uses_a_grab_cursor_on_every_node_kind`
   ```text
   text fixture must offer eligible wire ports: WorldRect { x: 0.0, y: 0.0, w: 160.0, h: 40.0 }
   ```

This snapshot therefore does **not** completely repair the remaining assertions.
Eight of the prior fourteen non-environment failures are now green, while five of
those fourteen remain red; the prompt-face assertion above is an additional red
check in the required integrated suite.

## Audits

### xtask

```text
cargo test --locked -p xtask -j2
```

Exit **0**. Results: xtask library 27 passed; contracts 4 passed; kits 3 passed;
metrics 4 passed; size 1 passed; theme 1 passed; xtask binary/doc tests had zero
tests. Total: **40 passed, 0 failed, 0 ignored**. This executed the contracts,
kits, and Rust source-size audits requested by the handoff.

### Formatting

```text
cargo fmt --all -- --check
```

Exit **0**, with no output.

### Existing warnings

The integrated build emitted existing `dead_code` warnings in Slate for
`place_pasted_png` and the unused external-drop classifier symbols. They were not
expanded into production cleanup during this validation-only task.

### Scoped Clippy

```text
cargo clippy --locked -p slate -p atlas-shell -p atlas-ai --all-targets -j2
```

Exit **0**. Existing warnings remain, including two `redundant_closure` warnings
in `atlas-shell`, Slate's duplicate test-only `cfg` attribute, the dead-code
warnings noted above, and existing `nonminimal_bool` suggestions. These unrelated
warnings were recorded rather than broadening this validation-only change.
