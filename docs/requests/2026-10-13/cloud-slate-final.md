# Final Slate Linux integration validation — HEAD `3949649d17ba9ca31bbfe13fd564dfb1e57c0aea`

Date: 2026-10-13  
Platform: Linux/non-Windows stubs  
Scope: final Slate application integration only. No native GUI, provider network calls, or secret-store access was performed.

## Fix made during validation

- Corrected the new picked-vertex toolbar cache regression test so it first lets the normal frame pass establish `shape_properties`, then queries the picked-corner bounds. This preserves the production ownership and cache behavior while making the regression exercise the real UI lifecycle rather than uninitialized presentation state.

## Required validation

- `cargo test --locked -p slate --lib -j4` — exit **101**. **1321 passed, 60 failed, 53 ignored, 0 measured, 0 filtered out** (1434 total). The ignored corpus/GUI/live/review tests remained ignored. The run emitted two pre-existing dead-code warnings (`place_pasted_png`, `ImageSource::Bitmap`).
- `cargo xtask contracts` — exit **0**. 40 dimensions; contracts, registry, and `decisions.json` agree.
- `cargo xtask kits` — exit **0**. 1 kit file and 8 tools validated.
- `cargo xtask size` — exit **0**. 786 Rust files scanned; 500-line cap enforced; 111 existing allowlisted oversized files; no allowlist was raised.
- `cargo fmt --all -- --check` — exit **0**.
- `cargo test --locked -p slate --lib -j4 a_picked_vertex_strip` (after the scoped test fix) — exit **0**. **1 passed, 0 failed, 0 ignored, 1433 filtered out**.

## Full-run failures (60)

- `app::board::board_menu::tests::add_image_as_layer_creates_its_own_layer_and_undoes_in_one_step`
- `app::board_agent::generator::tests::album::generated_album_resumes_at_its_resting_pick_and_reaches_new_results`
- `app::board_agent::generator::tests::an_authored_upstream_picture_takes_precedence_over_the_current_frame`
- `app::board_agent::generator::tests::rerunning_a_picked_result_captures_its_visible_crop_before_following_new_results`
- `app::board_agent::generator::tests::running_an_edited_generated_text_card_releases_its_override_as_one_undo`
- `app::board_agent::train_tests::image_blister::generator_inputs_and_controls_live_in_the_selected_agent_blister`
- `app::board_image_layers::result_palette::opacity::tests::early_escape_cancels_opacity_without_closing_palette_or_committing_on_release`
- `app::board_image_layers::result_palette::opacity::tests::scrubbing_changes_the_drawn_layer_alpha_without_raster_or_journal_changes`
- `app::board_image_layers::shape_fill::tests::generated::alt_generated_shape_fill_carries_runtime_output_and_full_stack_durably`
- `app::board_image_segment::tests::interaction::outside_image_press_disarms_brush_and_reselection_restores_only_the_palette`
- `app::board_properties::palette_ui::tests::a_picked_vertex_strip_stays_beside_the_vertex_instead_of_the_whole_shape`
- `app::board_properties::pan_tests::right_drag_pan_keeps_the_property_editor_open`
- `app::board_properties::tests::a_property_editor_clipped_by_the_canvas_top_opens_below_the_selection`
- `app::board_properties::tests::a_property_editor_over_a_stringer_paints_and_hits_above_it`
- `app::board_properties::tests::shape_property_eyedropper_click_and_drag_open_their_sampling_modes`
- `app::board_properties::tests::shape_property_toolbar_opens_from_real_pointer_without_moving_selection`
- `app::board_properties::tests::text_selection_outline_stays_while_navigating_the_text_editor`
- `app::board_properties::tests::wheel_over_the_typeface_list_does_not_zoom_the_board`
- `app::board_properties::tests::wire_palette_opens_on_selection_and_cancel_preserves_style`
- `app::external_drop_image::tests::browser_image_completion_waits_for_the_original_tab`
- `app::external_drop_image::tests::browser_picture_places_pixels_with_aspect_and_one_undo`
- `app::image_layer_tests::crop_toggles_fit_inside_the_shared_corners_panel`
- `app::image_layer_tests::editing::transparent_tab_gaps_allow_painting_without_collapsing_the_editor`
- `app::image_layer_tests::opacity::layer_name_commits_on_enter_and_menu_click_away`
- `app::image_layer_tests::opacity::layer_name_draft_never_renames_a_matching_layer_in_another_workbook`
- `app::image_layer_tests::opacity::layer_name_draft_stays_with_its_original_result_when_layer_ids_repeat`
- `app::image_layer_tests::recent_colors_are_small_dots_inside_the_layer_palette`
- `app::pdf::pages_album::tests::pages_album_is_a_thin_pallet_over_the_host`
- `app::tests::bbox::chrome::fillet_grip_press_beats_edge_band_but_nw_corner_still_resizes`
- `app::tests::bezier::open_shapes_offer_no_wire_ports_while_a_closed_polyline_keeps_them`
- `app::tests::bezier_close::a_closed_bezier_gets_the_closed_form_treatment`
- `app::tests::closed_form::corners::a_grip_pick_keeps_a_stroke_panel_that_is_still_offered`
- `app::tests::closed_form::corners::a_grip_press_commits_a_pending_stroke_preview_once`
- `app::tests::closed_form::corners::a_strip_button_over_a_rectangle_corner_keeps_its_click`
- `app::tests::closed_form::grips::a_direct_select_knob_on_a_soft_stroke_wins_over_a_strip_button`
- `app::tests::closed_form::grips::a_line_end_point_under_a_strip_button_drags`
- `app::tests::closed_form::grips::direct_select_picks_closed_form_vertices_as_anchors`
- `app::tests::closed_form::grips::text_editing_a_grouped_shape_keeps_the_text_panel`
- `app::tests::escape_drag::esc_mid_resize_restores_the_node`
- `app::tests::escape_drag::esc_mid_rotate_restores_the_node`
- `app::tests::media::assets::unbundle_word_of_an_unsaved_workbook_writes_into_the_data_dir`
- `app::tests::media::assets::unbundle_word_writes_page_images_beside_a_saved_workbook`
- `app::tests::media::paste_input::a_pasted_image_resolves_as_a_generator_input`
- `app::tests::media::unbundle::unbundle_of_a_single_page_does_not_place_a_grid`
- `app::tests::media::unbundle::unbundle_pdf_places_pages_through_dispatch_and_undoes_in_one_step`
- `app::tests::media::unbundle::unbundle_pptx_and_legacy_ppt_when_office_can_render`
- `app::tests::media::unbundle::unbundle_while_the_preview_is_pending_finishes_instead_of_failing`
- `app::tests::packs::pk2_generate_offers_add_api_key_until_a_key_is_stored`
- `app::tests::selection_chrome::the_eraser_preview_keeps_its_own_color`
- `app::tests::subobject::deleting_a_rectangle_edge_opens_its_outline`
- `app::tests::subobject::tiny_shape_body_moves_before_overlapping_transform_grips`
- `app::tests::text_zoom::canvas_text_keeps_its_breaks_and_line_offsets_through_a_zoom_sweep`
- `app::tests::tip_direct::handles::a_handle_knob_under_the_property_strip_drags_with_a_panel_open`
- `app::tests::tip_direct::handles::a_handle_knob_under_the_property_strip_still_drags`
- `app::tests::tip_direct::handles::the_property_strip_sits_beside_the_picked_vertices`
- `app::tests::wire_cursor::wire_grab_zone_uses_a_grab_cursor_on_every_node_kind`
- `app::tests::wire_grips::wire_grip_hit_grows_when_zoomed_out`
- `app::tests_wire_lanes::converging_wires_arrive_sorted_and_do_not_cross`
- `app::tests_wire_lanes::three_converging_wires_spread_across_the_zone_at_the_preferred_spacing`
- `app::tests_wire_lanes::twelve_converging_wires_pack_evenly_denser_inside_the_same_zone`

## Requested targeted report filters

The targeted commands were run because the full suite had failures. Each command used `cargo test --locked -p slate --lib -j4 <filter>`. Results below are the initial HEAD validation, except the explicit post-fix rerun recorded above.

| Filter | Exit | Result |
|---|---:|---|
| `feedback_polish` | 0 | ok. 4 passed; 0 failed; 0 ignored; 0 measured; 1430 filtered out; finished in 0.05s |
| `a_picked_vertex_strip` | 101 | FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 1433 filtered out; finished in 0.01s |
| `no_system_fonts_render` | 0 | ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1433 filtered out; finished in 0.10s |
| `text::reentry` | 0 | ok. 5 passed; 0 failed; 0 ignored; 0 measured; 1429 filtered out; finished in 0.09s |
| `typography` | 0 | ok. 0 passed; 0 failed; 0 ignored; 0 measured; 1434 filtered out; finished in 0.00s |
| `empty_shape_text` | 0 | ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1433 filtered out; finished in 0.06s |
| `paste` | 101 | FAILED. 13 passed; 1 failed; 1 ignored; 0 measured; 1419 filtered out; finished in 0.12s |
| `generated_album` | 101 | FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 1433 filtered out; finished in 0.27s |
| `image_agent_count` | 0 | ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1433 filtered out; finished in 0.05s |
| `submitting_the_image_editor` | 0 | ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1433 filtered out; finished in 0.02s |
| `wire_grips` | 101 | FAILED. 11 passed; 1 failed; 0 ignored; 0 measured; 1422 filtered out; finished in 0.14s |
| `selection_chrome` | 101 | FAILED. 3 passed; 1 failed; 0 ignored; 0 measured; 1430 filtered out; finished in 0.05s |
| `subobject` | 101 | FAILED. 8 passed; 2 failed; 0 ignored; 0 measured; 1424 filtered out; finished in 0.27s |
| `split_gp` | 0 | ok. 8 passed; 0 failed; 0 ignored; 0 measured; 1426 filtered out; finished in 0.08s |
| `tiny_shape_body` | 101 | FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 1433 filtered out; finished in 0.03s |
| `bottom_stringer` | 0 | ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1433 filtered out; finished in 0.00s |

Notes:

- The `typography` filter matched no tests (0 passed, 0 failed, 1434 filtered out); typography coverage did run under the named `text::reentry`, `no_system_fonts_render`, and `empty_shape_text` filters.
- The `paste` filter's one failure was `app::tests::media::paste_input::a_pasted_image_resolves_as_a_generator_input`.
- The failures above include Linux runtime limitations around PDFium/Office document rendering as well as unrelated/baseline integration failures. They are reported exactly rather than hidden or weakened.
- No tests were disabled or weakened, no production owner was duplicated, no `board_*.rs` owner was added, and no size allowlist was raised.
