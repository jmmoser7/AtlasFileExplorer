# Final cloud runtime verification — 2026-10-13

## Provenance

- Requested immutable HEAD: `b9d43087f7aebb5d84922f80adb195308ec2cf3d`.
- Verified `git rev-parse HEAD`: `b9d43087f7aebb5d84922f80adb195308ec2cf3d` (exact match).
- Requested branch name: `codex/feedback-final-20261013`.
- Checked-out local branch at verification time: `work`. No fetch or branch switch was performed; the checked-out commit was the requested immutable source.

## Required command

```text
cargo test --locked -p slate -p atlas-shell -p slate-doc -p slate-artifact --no-fail-fast -j2
```

- True Cargo exit code (captured through Bash `PIPESTATUS[0]` while teeing stdout/stderr): **101**.
- Build completed and all independent test binaries ran. The sole failed target was `-p slate --lib`.

## Per-binary results

| Test binary | Passed | Failed | Ignored | Result |
|---|---:|---:|---:|---|
| `atlas-shell` unit tests | 270 | 0 | 2 | PASS |
| `slate` library unit tests | 1363 | 20 | 53 | FAIL |
| `slate` binary unit tests | 0 | 0 | 0 | PASS |
| `slate-artifact` unit tests | 75 | 0 | 1 | PASS |
| `slate-artifact/tests/crossing_wires.rs` | 1 | 0 | 0 | PASS |
| `slate-artifact/tests/export.rs` | 4 | 0 | 0 | PASS |
| `slate-artifact/tests/hosted_shape_text.rs` | 1 | 0 | 0 | PASS |
| `slate-artifact/tests/membership.rs` | 1 | 0 | 0 | PASS |
| `slate-doc` unit tests | 290 | 0 | 1 | PASS |
| `slate-doc/tests/image_paint_layers.rs` | 3 | 0 | 0 | PASS |
| `slate-doc/tests/migration.rs` | 6 | 0 | 1 | PASS |
| Doc tests (`atlas-shell`, `slate`, `slate-artifact`, `slate-doc`) | 0 | 0 | 0 | PASS |

## Environmental PDF/Office failures (6)

Linux has no usable Pdfium runtime (`[atlas] pdfium.dll not found — PDF previews limited to shell handlers`). The previously recorded Pdfium download attempt was HTTP 403, so it was not retried. Assertions were not ignored or weakened.

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

## Remaining source/assertion failures (14)

1. `app::board_agent::generator::tests::running_an_edited_generated_text_card_releases_its_override_as_one_undo`
   ```text
   assertion `left == right` failed
     left: None
    right: Some("person's authored words")
   ```
2. `app::board_image_segment::tests::interaction::outside_image_press_disarms_brush_and_reselection_restores_only_the_palette`
   ```text
   assertion `left == right` failed: quick reselect after 2.609846ms; recovery expires after 1500 ms
     left: None
    right: Some(Brush)
   ```
3. `app::board_properties::tests::a_property_editor_clipped_by_the_canvas_top_opens_below_the_selection`
   ```text
   [[828.0 74.0] - [1248.0 248.0]] below [[620.0 101.0] - [820.0 221.0]]
   ```
4. `app::board_properties::tests::shape_property_eyedropper_click_and_drag_open_their_sampling_modes`
   ```text
   assertion failed: editor.contains(dropper)
   ```
5. `app::board_properties::tests::text_selection_outline_is_suppressed_while_navigating_the_text_editor`
   ```text
   assertion `left == right` failed
     left: None
    right: Some(Text)
   ```
6. `app::board_properties::tests::wheel_over_the_typeface_list_does_not_zoom_the_board`
   ```text
   assertion `left == right` failed
     left: None
    right: Some(Text)
   ```
7. `app::image_layer_tests::crop_toggles_fit_inside_the_shared_corners_panel`
   ```text
   Fillet at [[602.0 538.9] - [621.7 549.3]] sits inside the shared panel [[612.0 546.5] - [828.0 568.6]]
   ```
8. `app::pdf::pages_album::tests::pages_album_is_a_thin_pallet_below_the_host`
   ```text
   assertion failed: layout.pallet.height() <= 48.0
   ```
9. `app::tests::bbox::chrome::fillet_grip_press_beats_edge_band_but_nw_corner_still_resizes`
   ```text
   assertion failed: matches!(h.app.begin_gesture(nw, xf.s2w(nw), egui::Modifiers::NONE),
       Some(board::BoardDrag::Resize { id: hit, handle: 0, .. }) if hit == id)
   ```
10. `app::tests::closed_form::grips::direct_select_picks_closed_form_vertices_as_anchors`
    ```text
    assertion `left == right` failed: a pick keeps the primitive
      left: Path
     right: Rect
    ```
11. `app::tests::packs::pk2_generate_offers_add_api_key_until_a_key_is_stored`
    ```text
    [("openai-image", "gpt-image-2", "GPT Image 2 · add API key"), ("openai-image", "gpt-image-1-mini", "GPT Image 1 mini · add API key")]
    ```
12. `app::tests::subobject::deleting_a_rectangle_edge_opens_its_outline`
    ```text
    assertion `left == right` failed
      left: []
     right: [1]
    ```
13. `app::tests::subobject::tiny_shape_body_moves_before_overlapping_transform_grips`
    ```text
    assertion `left != right` failed
      left: 0.0
     right: 0.0
    ```
14. `app::tests::wire_cursor::wire_grab_zone_uses_a_grab_cursor_on_every_node_kind`
    ```text
    assertion `left == right` failed: text grip did not arm
      left: None
     right: Some(Some(Right))
    ```

## Focused reproduction

Each of the 14 non-environmental failures was rerun alone, sequentially, against the warm `slate` test binary with an exact test-name filter. All 14 reproduced (Cargo exit 101 for every invocation), ruling out test-thread interference. No source repair was made in this verification pass; the behavioral assertions, timeouts, and ignore set remain unchanged.
