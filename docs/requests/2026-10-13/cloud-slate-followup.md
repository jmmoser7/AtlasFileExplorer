# Cloud Slate follow-up — 13 October 2026

## Provenance and checkout limitation

This is the authorized Linux verification follow-up for parent Codex chat
`01a135d1-428e-7131-aae4-96de004b8a38`, requested by Jeffrey to spare local
Windows CPU. The checkout started clean at `3949649d17ba9ca31bbfe13fd564dfb1e57c0aea`.

The supplied checkout has no configured Git remotes (`git remote -v` is empty).
Consequently, the required `git fetch origin` failed with exit 128 (`origin`
does not appear to be a Git repository), and this runner could not fast-forward
to the parent agents' newer branch or obtain `cloud-slate-final.md`. No reset was
performed. Results below therefore describe exactly pushed HEAD `3949649d`, not
a claimed latest branch state.

## Commands and counts

- `cargo test --locked -p slate --lib -j4 --no-run`: exit 0; initial clean-cloud
  compile finished in 33m 20s with two existing dead-code warnings. Complete log:
  `target/cloud-verification/slate-lib-no-run-initial.log` (ignored build output).
- `git fetch origin`: exit 128 because this checkout has no `origin`. Complete
  log: `target/cloud-verification/fetch-origin.log`.
- `cargo test --locked -p slate --lib -j4 -- --nocapture`: Cargo reported exit 101
  (the logging pipeline itself returned 0 because `tee` was not run under
  `pipefail`); **1321 passed, 60 failed, 53 ignored**, 0 measured, 0 filtered,
  finished in 350.45s. Complete stdout/stderr:
  `target/cloud-verification/slate-lib-full-first.log` (ignored build output).

Provisioning attempt: the crate primary README identifies the bblanchon prebuilt
dynamic libraries and states that `pdfium-render` does not bundle Pdfium. No local
`libpdfium.so` exists under `/usr` or `/opt`. A download of the matching release
`chromium/7543` to ignored `target/cloud-verification/pdfium/` failed before any
bytes were received with HTTP 403, so no binary was committed or installed.

PDF/Office failures consistently printed `[atlas] pdfium.dll not found — PDF
previews limited to shell handlers` and the assertions below prove the affected
unbundle tests fail after the runtime cannot read/render the prepared PDF.

## Complete failure assertion output

The following blocks are copied verbatim from the complete `--nocapture` log;
they include every panic assertion/location and its surrounding test output.

```
test app::bench_web::web_board_frame_time ... ignored

thread 'app::board::board_menu::tests::add_image_as_layer_creates_its_own_layer_and_undoes_in_one_step' panicked at apps/slate/src/app/board/board_menu/tests.rs:248:5:
assertion failed: layers[0].nodes.is_empty()
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
test app::board::board_menu::tests::add_image_as_layer_creates_its_own_layer_and_undoes_in_one_step ... FAILED

test app::board_agent::fork_tests::wires_into_a_train_start_calm_gray_and_keep_a_chosen_color ... ok

thread 'app::board_agent::generator::tests::an_authored_upstream_picture_takes_precedence_over_the_current_frame' panicked at apps/slate/src/app/board_agent/generator_tests.rs:58:5:
assertion `left == right` failed
  left: 1
 right: 2
test app::board_agent::generator::tests::an_authored_upstream_picture_takes_precedence_over_the_current_frame ... FAILED

test app::board_agent::generator::tests::an_in_frame_run_reads_its_picture_without_adding_chat_or_live_context ... ok

thread 'app::board_agent::generator::tests::album::generated_album_resumes_at_its_resting_pick_and_reaches_new_results' panicked at apps/slate/src/app/board_agent/image_album_tests.rs:78:9:
assertion `left == right` failed
  left: Some(0)
 right: Some(2)
test app::board_agent::generator::tests::album::generated_album_resumes_at_its_resting_pick_and_reaches_new_results ... FAILED

test app::board_agent::generator::tests::album::generated_album_resumes_at_its_resting_pick_and_reaches_new_results ... FAILED

thread 'app::board_agent::generator::tests::running_an_edited_generated_text_card_releases_its_override_as_one_undo' panicked at apps/slate/src/app/board_agent/generator_tests.rs:288:10:
called `Result::unwrap()` on an `Err` value: "Connected text is empty."
test app::board_agent::generator::tests::running_an_edited_generated_text_card_releases_its_override_as_one_undo ... FAILED

test app::board_agent::life::tests::agent_life_picking_a_conversation_bundles_its_history_in_one_undo ... ok

thread 'app::board_agent::generator::tests::rerunning_a_picked_result_captures_its_visible_crop_before_following_new_results' panicked at apps/slate/src/app/board_agent/generator_tests.rs:191:10:
prepared edit dispatched
test app::board_agent::generator::tests::rerunning_a_picked_result_captures_its_visible_crop_before_following_new_results ... FAILED

test app::board_agent::train_tests::full_access_is_a_local_grant_per_conversation_and_never_journaled ... ok

thread 'app::board_agent::train_tests::image_blister::generator_inputs_and_controls_live_in_the_selected_agent_blister' panicked at apps/slate/src/app/board_agent/train_tests/image_blister.rs:81:5:
the wired prompt input circle selects its source
test app::board_agent::train_tests::image_blister::generator_inputs_and_controls_live_in_the_selected_agent_blister ... FAILED

test app::board_image_layers::result_palette::ghost::tests::cropped_rotated_mirrored_ghost_reuses_overlay_uvs_without_raster_jobs ... ok

thread 'app::board_image_layers::result_palette::opacity::tests::early_escape_cancels_opacity_without_closing_palette_or_committing_on_release' panicked at apps/slate/src/app/board_image_layers/result_palette/opacity_tests.rs:21:17:
index out of bounds: the len is 0 but the index is 0
test app::board_image_layers::result_palette::opacity::tests::early_escape_cancels_opacity_without_closing_palette_or_committing_on_release ... FAILED

test app::board_image_layers::result_palette::opacity::tests::early_escape_cancels_opacity_without_closing_palette_or_committing_on_release ... FAILED

thread 'app::board_image_layers::result_palette::opacity::tests::scrubbing_changes_the_drawn_layer_alpha_without_raster_or_journal_changes' panicked at apps/slate/src/app/board_image_layers/result_palette/opacity_tests.rs:91:17:
index out of bounds: the len is 0 but the index is 0
test app::board_image_layers::result_palette::opacity::tests::scrubbing_changes_the_drawn_layer_alpha_without_raster_or_journal_changes ... FAILED

test app::board_image_layers::shape_fill::tests::file_intake_fills_directly_without_a_choice_popover ... ok

thread 'app::board_image_layers::shape_fill::tests::generated::alt_generated_shape_fill_carries_runtime_output_and_full_stack_durably' panicked at apps/slate/src/app/board_image_layers/shape_fill/tests/generated.rs:219:5:
assertion `left == right` failed
  left: [PaintLayerMeta { layer: PaintLayerId(10), name: "Layer 1", order: 0 }]
 right: [PaintLayerMeta { layer: PaintLayerId(9), name: "Layer 1", order: 0 }]
test app::board_image_layers::shape_fill::tests::generated::alt_generated_shape_fill_carries_runtime_output_and_full_stack_durably ... FAILED

test app::board_image_segment::tests::interaction::mask_drag_out_queues_independent_copy_and_preserves_source ... ok

thread 'app::board_image_segment::tests::interaction::outside_image_press_disarms_brush_and_reselection_restores_only_the_palette' panicked at apps/slate/src/app/image_segment_tests/interaction.rs:95:5:
assertion `left == right` failed
  left: None
 right: Some(Brush)
test app::board_image_segment::tests::interaction::outside_image_press_disarms_brush_and_reselection_restores_only_the_palette ... FAILED

test app::board_properties::dimensions::tests::polygon_stringers_measure_current_sides_and_phase_on_rotated_local_axes ... ok

thread 'app::board_properties::palette_ui::tests::a_picked_vertex_strip_stays_beside_the_vertex_instead_of_the_whole_shape' panicked at apps/slate/src/app/board_properties/palette_ui/tests.rs:31:47:
picked corner bounds
test app::board_properties::palette_ui::tests::a_picked_vertex_strip_stays_beside_the_vertex_instead_of_the_whole_shape ... FAILED

test app::board_properties::palette_ui::tests::deselecting_an_image_never_falls_back_to_the_generic_toolbar ... ok

thread 'app::board_properties::pan_tests::right_drag_pan_keeps_the_property_editor_open' panicked at apps/slate/src/app/board_properties/pan_tests.rs:59:5:
the gesture must actually pan the board
test app::board_properties::pan_tests::right_drag_pan_keeps_the_property_editor_open ... FAILED

test app::board_properties::selection::tests::quick_image_reselection_restores_the_panel_but_a_later_return_is_collapsed ... ok

thread 'app::board_properties::tests::a_property_editor_clipped_by_the_canvas_top_opens_below_the_selection' panicked at apps/slate/src/app/board_properties.rs:3026:9:
assertion failed: editor.top() > selection.bottom()
test app::board_properties::tests::a_property_editor_clipped_by_the_canvas_top_opens_below_the_selection ... FAILED

test app::board_properties::tests::a_property_editor_clipped_by_the_canvas_top_opens_below_the_selection ... FAILED

thread 'app::board_properties::tests::a_property_editor_over_a_stringer_paints_and_hits_above_it' panicked at apps/slate/src/app/board_properties.rs:2440:9:
precondition: [[878.0 74.0] - [1298.0 248.0]] covers the width stringer at [720.0 289.0]
test app::board_properties::tests::a_property_editor_over_a_stringer_paints_and_hits_above_it ... FAILED

test app::board_properties::tests::shape_property_color_scrub_commits_one_group_and_one_recent_color ... ok

thread 'app::board_properties::tests::shape_property_eyedropper_click_and_drag_open_their_sampling_modes' panicked at apps/slate/src/app/board_properties.rs:2380:9:
assertion `left == right` failed
  left: None
 right: Some(Fill)
test app::board_properties::tests::shape_property_eyedropper_click_and_drag_open_their_sampling_modes ... FAILED

test app::board_properties::tests::shape_property_tool_switch_discards_pending_preview ... ok

thread 'app::board_properties::tests::shape_property_toolbar_opens_from_real_pointer_without_moving_selection' panicked at apps/slate/src/app/board_properties.rs:2270:9:
assertion `left == right` failed
  left: None
 right: Some(Fill)
test app::board_properties::tests::shape_property_toolbar_opens_from_real_pointer_without_moving_selection ... FAILED

test app::board_properties::tests::text_documents_skip_photo_filters ... ok

thread 'app::board_properties::tests::text_selection_outline_stays_while_navigating_the_text_editor' panicked at apps/slate/src/app/board_properties.rs:2892:9:
assertion `left == right` failed
  left: None
 right: Some(Text)
test app::board_properties::tests::text_selection_outline_stays_while_navigating_the_text_editor ... FAILED

test app::board_properties::tests::the_wheel_over_an_open_typeface_list_scrolls_it_and_leaves_the_camera ... ok

thread 'app::board_properties::tests::wheel_over_the_typeface_list_does_not_zoom_the_board' panicked at apps/slate/src/app/board_properties.rs:2892:9:
assertion `left == right` failed
  left: None
 right: Some(Text)
test app::board_properties::tests::wheel_over_the_typeface_list_does_not_zoom_the_board ... FAILED

test app::board_properties::tests::wire_properties_batch_preview_commit_undo_and_persistence ... ok

thread 'app::board_properties::tests::wire_palette_opens_on_selection_and_cancel_preserves_style' panicked at apps/slate/src/app/board_properties.rs:2027:9:
assertion `left == right` failed
  left: None
 right: Some(Wire)
test app::board_properties::tests::wire_palette_opens_on_selection_and_cancel_preserves_style ... FAILED

test app::external_drop_classify::tests::linked_img_uses_image_and_not_anchor_or_source_page ... ok

thread 'app::external_drop_image::tests::browser_image_completion_waits_for_the_original_tab' panicked at apps/slate/src/app/external_drop_image.rs:289:9:
assertion `left == right` failed
  left: 0
 right: 1
test app::external_drop_image::tests::browser_image_completion_waits_for_the_original_tab ... FAILED

test app::image_layer_tests::a_sloppy_click_on_a_file_drop_capsule_still_chooses ... ok

thread 'app::image_layer_tests::crop_toggles_fit_inside_the_shared_corners_panel' panicked at apps/slate/src/app/image_layer_tests.rs:481:5:
the shared image panel keeps the fillet editor height: [[600.0 534.5] - [840.0 580.6]]
test app::image_layer_tests::crop_toggles_fit_inside_the_shared_corners_panel ... FAILED

test app::image_layer_tests::editing::image_body_click_keeps_each_editor_open_until_a_canvas_click ... ok

thread 'app::image_layer_tests::editing::transparent_tab_gaps_allow_painting_without_collapsing_the_editor' panicked at apps/slate/src/app/image_layer_tests/editing.rs:91:5:
assertion `left == right` failed
  left: None
 right: Some(Brush)
test app::image_layer_tests::editing::transparent_tab_gaps_allow_painting_without_collapsing_the_editor ... FAILED

test app::image_layer_tests::opacity::context_visibility_and_delete_are_independently_undoable ... ok

thread 'app::image_layer_tests::opacity::layer_name_commits_on_enter_and_menu_click_away' panicked at apps/slate/src/app/image_layer_tests/opacity.rs:135:5:
assertion `left == right` failed
  left: "Ink 1"
 right: "Ink"
test app::image_layer_tests::opacity::layer_name_commits_on_enter_and_menu_click_away ... FAILED

test app::image_layer_tests::opacity::layer_name_commits_on_enter_and_menu_click_away ... FAILED

thread 'app::image_layer_tests::opacity::layer_name_draft_never_renames_a_matching_layer_in_another_workbook' panicked at apps/slate/src/app/image_layer_tests/opacity.rs:175:5:
assertion `left == right` failed
  left: "Only in the first workbook 1"
 right: "Only in the first workbook"
test app::image_layer_tests::opacity::layer_name_draft_never_renames_a_matching_layer_in_another_workbook ... FAILED

test app::image_layer_tests::opacity::layer_name_draft_never_renames_a_matching_layer_in_another_workbook ... FAILED

thread 'app::image_layer_tests::opacity::layer_name_draft_stays_with_its_original_result_when_layer_ids_repeat' panicked at apps/slate/src/app/image_layer_tests/opacity.rs:230:5:
assertion `left == right` failed
  left: "Original result ink 1"
 right: "Original result ink"
test app::image_layer_tests::opacity::layer_name_draft_stays_with_its_original_result_when_layer_ids_repeat ... FAILED

test app::image_layer_tests::painting_suppresses_the_selection_cast_and_the_dimension_stringers ... ok

thread 'app::image_layer_tests::recent_colors_are_small_dots_inside_the_layer_palette' panicked at apps/slate/src/app/image_layer_tests.rs:255:5:
assertion `left == right` failed: one dot per recent color
  left: 0
 right: 3
test app::image_layer_tests::recent_colors_are_small_dots_inside_the_layer_palette ... FAILED

test app::pdf::documents::tests::requests_are_bounded_and_results_cannot_mutate_a_workbook ... ok

thread 'app::pdf::pages_album::tests::pages_album_is_a_thin_pallet_over_the_host' panicked at apps/slate/src/app/pdf/pages_album.rs:53:9:
assertion failed: layout.pallet.height() <= 48.0
test app::pdf::pages_album::tests::pages_album_is_a_thin_pallet_over_the_host ... FAILED

test app::tests::arming::arming_gp3_rect_shift_drag_is_square ... ok

thread 'app::external_drop_image::tests::browser_picture_places_pixels_with_aspect_and_one_undo' panicked at apps/slate/src/app/external_drop_image.rs:243:13:
image drop did not finish
test app::external_drop_image::tests::browser_picture_places_pixels_with_aspect_and_one_undo ... FAILED

test app::tests::bbox::chrome::ctrl_edge_scale_keeps_the_center ... ok

thread 'app::tests::bbox::chrome::fillet_grip_press_beats_edge_band_but_nw_corner_still_resizes' panicked at apps/slate/src/app/tests/bbox/chrome.rs:238:5:
assertion failed: matches!(h.app.begin_gesture(nw, xf.s2w(nw), egui::Modifiers::NONE),
    Some(board::BoardDrag::Resize { id: hit, handle: 0, .. }) if hit == id)
test app::tests::bbox::chrome::fillet_grip_press_beats_edge_band_but_nw_corner_still_resizes ... FAILED

test app::tests::bezier::bezier_press_on_a_draft_anchor_or_handle_edits_it_instead_of_placing ... ok

thread 'app::tests::bezier::open_shapes_offer_no_wire_ports_while_a_closed_polyline_keeps_them' panicked at apps/slate/src/app/tests/bezier.rs:371:5:
assertion `left == right` failed: a closed polyline keeps its ports
  left: 0
 right: 3
test app::tests::bezier::open_shapes_offer_no_wire_ports_while_a_closed_polyline_keeps_them ... FAILED

test app::tests::bezier::bezier_stationary_click_places_a_corner_anchor_without_handles ... ok

thread 'app::tests::bezier_close::a_closed_bezier_gets_the_closed_form_treatment' panicked at apps/slate/src/app/tests/bezier_close.rs:100:5:
assertion `left == right` failed: wire ports
  left: 0
 right: 3
test app::tests::bezier_close::bezier_click_on_the_start_before_the_close_delay_edits_it ... ok
test app::tests::bezier_close::a_closed_bezier_gets_the_closed_form_treatment ... FAILED

test app::tests::closed_form::corners::a_grip_pick_closes_a_strip_panel_it_no_longer_offers ... ok

thread 'app::tests::closed_form::corners::a_grip_pick_keeps_a_stroke_panel_that_is_still_offered' panicked at apps/slate/src/app/tests/closed_form/corners.rs:445:5:
the other anchor lies outside the strip and its panel
test app::tests::closed_form::corners::a_grip_pick_keeps_a_stroke_panel_that_is_still_offered ... FAILED

test app::tests::closed_form::corners::a_grip_pick_keeps_a_stroke_panel_that_is_still_offered ... FAILED

thread 'app::tests::closed_form::corners::a_grip_press_commits_a_pending_stroke_preview_once' panicked at apps/slate/src/app/tests/closed_form/corners.rs:486:5:
assertion `left == right` failed
  left: Some((NodeId(1), [1]))
 right: Some((NodeId(1), [2]))
test app::tests::closed_form::corners::a_grip_press_commits_a_pending_stroke_preview_once ... FAILED

test app::tests::closed_form::corners::a_rectangle_corner_drag_promotes_it_to_a_closed_path ... ok

thread 'app::tests::closed_form::corners::a_strip_button_over_a_rectangle_corner_keeps_its_click' panicked at apps/slate/src/app/tests/closed_form/corners.rs:361:10:
a strip button reaches the corner
test app::tests::closed_form::corners::a_strip_button_over_a_rectangle_corner_keeps_its_click ... FAILED

test app::tests::closed_form::corners::strip_corner_rounding_at_a_picked_rectangle_corner_sets_only_that_corner ... ok

thread 'app::tests::closed_form::grips::a_direct_select_knob_on_a_soft_stroke_wins_over_a_strip_button' panicked at apps/slate/src/app/tests/closed_form/grips.rs:263:10:
a strip button covers the knob
test app::tests::closed_form::grips::a_direct_select_knob_on_a_soft_stroke_wins_over_a_strip_button ... FAILED

test app::tests::closed_form::grips::a_direct_select_knob_on_a_soft_stroke_wins_over_a_strip_button ... FAILED

thread 'app::tests::closed_form::grips::a_line_end_point_under_a_strip_button_drags' panicked at apps/slate/src/app/tests/closed_form/grips.rs:308:10:
a strip button reaches the end point
test app::tests::closed_form::grips::a_line_end_point_under_a_strip_button_drags ... FAILED

test app::tests::closed_form::grips::closed_form_vertex_stroke_round_trips_and_exports_as_the_board_paints ... ok

thread 'app::tests::closed_form::grips::direct_select_picks_closed_form_vertices_as_anchors' panicked at apps/slate/src/app/tests/closed_form/grips.rs:391:5:
assertion `left == right` failed
  left: [(84.0, [20, 20, 20, 255]), (84.0, [20, 20, 20, 255]), (84.0, [20, 20, 20, 255])]
 right: [(4.0, [20, 20, 20, 255]), (4.0, [20, 20, 20, 255]), (4.0, [20, 20, 20, 255])]
test app::tests::closed_form::grips::direct_select_picks_closed_form_vertices_as_anchors ... FAILED

test app::tests::closed_form::grips::polygon_side_glyphs_still_step_sides_beside_a_picked_vertex ... ok

thread 'app::tests::closed_form::grips::text_editing_a_grouped_shape_keeps_the_text_panel' panicked at apps/slate/src/app/tests/closed_form/grips.rs:212:5:
assertion `left == right` failed: the Text panel stays up
  left: None
 right: Some(Text)
test app::tests::closed_form::grips::text_editing_a_grouped_shape_keeps_the_text_panel ... FAILED

test app::tests::escape_drag::esc_mid_move_restores_three_nodes_and_keeps_the_selection ... ok

thread 'app::tests::escape_drag::esc_mid_resize_restores_the_node' panicked at apps/slate/src/app/tests/escape_drag.rs:70:5:
the corner starts a resize
test app::tests::escape_drag::esc_mid_resize_restores_the_node ... FAILED

test app::tests::escape_drag::esc_mid_resize_restores_the_node ... FAILED

thread 'app::tests::escape_drag::esc_mid_rotate_restores_the_node' panicked at apps/slate/src/app/tests/escape_drag.rs:96:5:
the outside-corner zone starts a rotate
test app::tests::escape_drag::esc_mid_rotate_restores_the_node ... FAILED

test app::tests::line::ortho::tab_again_or_escape_releases_the_segment_lock ... ok

thread '<unnamed>' panicked at apps/slate/src/app/board_path.rs:3467:9:
test hook: this line job is lost
test app::tests::line::stroke_bitmaps::a_lost_line_job_is_asked_again ... ok
test app::tests::line::width::changing_the_line_width_leaves_the_arc_alone ... ok
test app::tests::line::width::the_width_chord_mid_draw_sets_the_line_being_drawn ... ok
test app::tests::line::width::the_width_chord_mid_draw_sets_the_path_being_drawn ... ok
test app::tests::line::width::the_width_chord_sets_the_arc_point_being_placed ... ok
test app::tests::line::width::the_width_chord_tapers_a_bezier_span_smoothly ... ok
test app::tests::line::width::the_width_chord_tapers_a_line_from_its_first_point ... ok
test app::tests::line::width::the_width_chord_tapers_a_polyline_between_points ... ok
test app::tests::media::aspect::placed_pasted_and_generated_images_keep_source_aspect ... ok
test app::tests::media::assets::collect_assets_rewrites_data_dir_images_and_refuses_user_files ... ok
test app::tests::media::assets::save_as_elsewhere_stays_resolvable_while_assets_copy ... ok
test app::tests::media::assets::saving_files_data_dir_images_beside_the_workbook_and_undo_restores_locators ... ok
[atlas] pdfium.dll not found — PDF previews limited to shell handlers
test app::tests::line::stroke_bitmaps::closing_a_tab_frees_every_bitmap_of_its_strokes ... ok

thread 'app::tests::media::assets::unbundle_word_of_an_unsaved_workbook_writes_into_the_data_dir' panicked at apps/slate/src/app/tests/media/assets.rs:69:5:
Added 1 file(s)
Word did not produce a readable PDF. Check the PDF preview runtime.
test app::tests::media::assets::unbundle_word_of_an_unsaved_workbook_writes_into_the_data_dir ... FAILED

test app::tests::media::assets::unbundle_word_of_an_unsaved_workbook_writes_into_the_data_dir ... FAILED

thread 'app::tests::media::paste_input::a_pasted_image_resolves_as_a_generator_input' panicked at apps/slate/src/app/tests/media/paste_input.rs:97:5:
assertion `left == right` failed: a cropped pasted picture feeds its visible window
  left: (40, 20)
 right: (20, 20)
test app::tests::media::paste_input::a_pasted_image_resolves_as_a_generator_input ... FAILED

test app::tests::media::unbundle::media_unbundle_places_a_selected_page_grid_and_undoes ... ok

thread 'app::tests::media::assets::unbundle_word_writes_page_images_beside_a_saved_workbook' panicked at apps/slate/src/app/tests/media/assets.rs:26:5:
Added 1 file(s)
Word did not produce a readable PDF. Check the PDF preview runtime.
test app::tests::media::assets::unbundle_word_writes_page_images_beside_a_saved_workbook ... FAILED

test app::tests::media::assets::unbundle_word_writes_page_images_beside_a_saved_workbook ... FAILED

thread 'app::tests::media::unbundle::unbundle_of_a_single_page_does_not_place_a_grid' panicked at apps/slate/src/app/tests/media/unbundle.rs:470:5:
Added 1 file(s)
Could not read PDF pages. Check the document and the PDF preview runtime.
test app::tests::media::unbundle::unbundle_of_a_single_page_does_not_place_a_grid ... FAILED

test app::tests::media::unbundle::unbundle_of_a_single_page_does_not_place_a_grid ... FAILED

thread 'app::tests::media::unbundle::unbundle_pdf_places_pages_through_dispatch_and_undoes_in_one_step' panicked at apps/slate/src/app/tests/media/unbundle.rs:316:5:
pdf unbundle did not finish: Added 1 file(s)
Could not read PDF pages. Check the document and the PDF preview runtime.
test app::tests::media::unbundle::unbundle_pdf_places_pages_through_dispatch_and_undoes_in_one_step ... FAILED

test app::tests::media::unbundle::unbundle_reports_when_office_is_not_installed ... ok

thread 'app::tests::media::unbundle::unbundle_pptx_and_legacy_ppt_when_office_can_render' panicked at apps/slate/src/app/tests/media/unbundle.rs:385:9:
deck.pptx: Added 1 file(s)
PowerPoint did not produce a readable PDF. Check the PDF preview runtime.
test app::tests::media::unbundle::unbundle_pptx_and_legacy_ppt_when_office_can_render ... FAILED

test app::tests::packs::a_missing_pack_document_keeps_its_poster_and_names_the_pack ... ok

thread 'app::tests::packs::pk2_generate_offers_add_api_key_until_a_key_is_stored' panicked at apps/slate/src/app/tests/packs.rs:126:5:
assertion `left == right` failed: [("comfy", "", "ComfyUI · Auto"), ("openai-image", "gpt-image-2", "GPT Image 2 · add API key"), ("openai-image", "gpt-image-1-mini", "GPT Image 1 mini · add API key")]
  left: 3
 right: 1
test app::tests::packs::pk2_generate_offers_add_api_key_until_a_key_is_stored ... FAILED

test app::tests::path_cuts::trim_open_line_by_a_closed_cutter ... ok

thread 'app::tests::media::unbundle::unbundle_while_the_preview_is_pending_finishes_instead_of_failing' panicked at apps/slate/src/app/tests/media/unbundle.rs:362:5:
Added 1 file(s)
PowerPoint did not produce a readable PDF. Check the PDF preview runtime.
test app::tests::media::unbundle::unbundle_while_the_preview_is_pending_finishes_instead_of_failing ... FAILED

test app::tests::selection_chrome::rotate_hover_and_drag_show_a_quarter_arc_in_the_windows_cursor_scheme ... ok

thread 'app::tests::selection_chrome::the_eraser_preview_keeps_its_own_color' panicked at apps/slate/src/app/tests/selection_chrome.rs:52:5:
the eraser tip took the brush color
test app::tests::selection_chrome::the_eraser_preview_keeps_its_own_color ... FAILED

test app::tests::subobject::deleting_a_primitive_vertex_promotes_it_and_undo_restores_it ... ok

thread 'app::tests::subobject::deleting_a_rectangle_edge_opens_its_outline' panicked at apps/slate/src/app/tests/subobject.rs:128:5:
assertion `left == right` failed
  left: []
 right: [1]
test app::tests::subobject::deleting_a_rectangle_edge_opens_its_outline ... FAILED

test app::tests::subobject::deleting_a_rectangle_edge_opens_its_outline ... FAILED

thread 'app::tests::subobject::tiny_shape_body_moves_before_overlapping_transform_grips' panicked at apps/slate/src/app/tests/subobject.rs:31:9:
assertion `left != right` failed
  left: 0.0
 right: 0.0
test app::tests::subobject::tiny_shape_body_moves_before_overlapping_transform_grips ... FAILED

test app::tests::text::zoom_stable::canvas_text_keeps_its_breaks_and_scales_uniformly_at_every_zoom ... ok

thread 'app::tests::text_zoom::canvas_text_keeps_its_breaks_and_line_offsets_through_a_zoom_sweep' panicked at apps/slate/src/app/tests/text_zoom.rs:16:9:
chat reply did not paint at zoom 1; painted ["+", "Untitled •", "×", "⚙", "1 linked file(s) · 0 tagged · 1 uncategorized", "· 1 link(s) unverified", "100%", "Message", "Local agent", ""]
test app::tests::text_zoom::canvas_text_keeps_its_breaks_and_line_offsets_through_a_zoom_sweep ... FAILED

test app::tests::tip_direct::handles::a_dragged_handle_keeps_its_grab_offset_and_snaps_its_tip ... ok

thread 'app::tests::tip_direct::handles::a_handle_knob_under_the_property_strip_drags_with_a_panel_open' panicked at apps/slate/src/app/tests/tip_direct/handles.rs:419:10:
a strip button covers the knob
test app::tests::tip_direct::handles::a_handle_knob_under_the_property_strip_drags_with_a_panel_open ... FAILED

test app::tests::tip_direct::handles::a_handle_knob_under_the_property_strip_drags_with_a_panel_open ... FAILED

thread 'app::tests::tip_direct::handles::a_handle_knob_under_the_property_strip_still_drags' panicked at apps/slate/src/app/tests/tip_direct/handles.rs:361:10:
a strip button covers the knob
test app::tests::tip_direct::handles::a_handle_knob_under_the_property_strip_still_drags ... FAILED

test app::tests::tip_direct::handles::shift_drag_of_a_handle_keeps_its_angle_and_changes_only_length ... ok

thread 'app::tests::tip_direct::handles::the_property_strip_sits_beside_the_picked_vertices' panicked at apps/slate/src/app/tests/tip_direct/handles.rs:107:5:
above the picked point
test app::tests::tip_direct::handles::the_property_strip_sits_beside_the_picked_vertices ... FAILED

test app::tests::web::pool::gp9_export_packages_a_local_page_and_points_at_a_remote_one ... ok

thread 'app::tests::wire_cursor::wire_grab_zone_uses_a_grab_cursor_on_every_node_kind' panicked at apps/slate/src/app/tests/wire_cursor.rs:33:5:
assertion `left == right` failed: text grip did not arm
  left: None
 right: Some(Some(Right))
test app::tests::wire_cursor::wire_grab_zone_uses_a_grab_cursor_on_every_node_kind ... FAILED

test app::tests::wire_grips::wire_drop_on_empty_canvas_keeps_a_free_end ... ok

thread 'app::tests::wire_grips::wire_grip_hit_grows_when_zoomed_out' panicked at apps/slate/src/app/tests/wire_grips.rs:176:5:
assertion `left == right` failed: the same 9 px reaches the grown hit
  left: None
 right: Some(Top)
test app::tests::wire_grips::wire_grip_hit_grows_when_zoomed_out ... FAILED

test app::tests_tip_chord::an_unchanged_draft_frame_does_not_re_tessellate ... ok

thread 'app::tests_wire_lanes::converging_wires_arrive_sorted_and_do_not_cross' panicked at apps/slate/src/app/tests_wire_lanes.rs:210:5:
assertion `left == right` failed: every wire exports
  left: 0
 right: 5
test app::tests_wire_lanes::converging_wires_arrive_sorted_and_do_not_cross ... FAILED

test app::tests_tip_chord::the_tip_hud_edits_every_stamped_tip_of_a_whole_brush_stroke ... ok

thread 'app::tests_wire_lanes::three_converging_wires_spread_across_the_zone_at_the_preferred_spacing' panicked at apps/slate/src/app/tests_wire_lanes.rs:210:5:
assertion `left == right` failed: every wire exports
  left: 0
 right: 3
test app::tests_wire_lanes::three_converging_wires_spread_across_the_zone_at_the_preferred_spacing ... FAILED

test app::workbook_assets::deferred::tests::inactive_export_stays_queued_while_save_replays_by_tab_id ... ok

thread 'app::tests_wire_lanes::twelve_converging_wires_pack_evenly_denser_inside_the_same_zone' panicked at apps/slate/src/app/tests_wire_lanes.rs:210:5:
assertion `left == right` failed: every wire exports
  left: 0
 right: 12
test app::workbook_assets::tests::a_disconnected_old_save_cannot_clear_the_current_save_and_current_failure_is_retryable ... ok
test app::tests_wire_lanes::twelve_converging_wires_pack_evenly_denser_inside_the_same_zone ... FAILED
```

## Harness failure list and exact final result

```
failures:

failures:
    app::board::board_menu::tests::add_image_as_layer_creates_its_own_layer_and_undoes_in_one_step
    app::board_agent::generator::tests::album::generated_album_resumes_at_its_resting_pick_and_reaches_new_results
    app::board_agent::generator::tests::an_authored_upstream_picture_takes_precedence_over_the_current_frame
    app::board_agent::generator::tests::rerunning_a_picked_result_captures_its_visible_crop_before_following_new_results
    app::board_agent::generator::tests::running_an_edited_generated_text_card_releases_its_override_as_one_undo
    app::board_agent::train_tests::image_blister::generator_inputs_and_controls_live_in_the_selected_agent_blister
    app::board_image_layers::result_palette::opacity::tests::early_escape_cancels_opacity_without_closing_palette_or_committing_on_release
    app::board_image_layers::result_palette::opacity::tests::scrubbing_changes_the_drawn_layer_alpha_without_raster_or_journal_changes
    app::board_image_layers::shape_fill::tests::generated::alt_generated_shape_fill_carries_runtime_output_and_full_stack_durably
    app::board_image_segment::tests::interaction::outside_image_press_disarms_brush_and_reselection_restores_only_the_palette
    app::board_properties::palette_ui::tests::a_picked_vertex_strip_stays_beside_the_vertex_instead_of_the_whole_shape
    app::board_properties::pan_tests::right_drag_pan_keeps_the_property_editor_open
    app::board_properties::tests::a_property_editor_clipped_by_the_canvas_top_opens_below_the_selection
    app::board_properties::tests::a_property_editor_over_a_stringer_paints_and_hits_above_it
    app::board_properties::tests::shape_property_eyedropper_click_and_drag_open_their_sampling_modes
    app::board_properties::tests::shape_property_toolbar_opens_from_real_pointer_without_moving_selection
    app::board_properties::tests::text_selection_outline_stays_while_navigating_the_text_editor
    app::board_properties::tests::wheel_over_the_typeface_list_does_not_zoom_the_board
    app::board_properties::tests::wire_palette_opens_on_selection_and_cancel_preserves_style
    app::external_drop_image::tests::browser_image_completion_waits_for_the_original_tab
    app::external_drop_image::tests::browser_picture_places_pixels_with_aspect_and_one_undo
    app::image_layer_tests::crop_toggles_fit_inside_the_shared_corners_panel
    app::image_layer_tests::editing::transparent_tab_gaps_allow_painting_without_collapsing_the_editor
    app::image_layer_tests::opacity::layer_name_commits_on_enter_and_menu_click_away
    app::image_layer_tests::opacity::layer_name_draft_never_renames_a_matching_layer_in_another_workbook
    app::image_layer_tests::opacity::layer_name_draft_stays_with_its_original_result_when_layer_ids_repeat
    app::image_layer_tests::recent_colors_are_small_dots_inside_the_layer_palette
    app::pdf::pages_album::tests::pages_album_is_a_thin_pallet_over_the_host
    app::tests::bbox::chrome::fillet_grip_press_beats_edge_band_but_nw_corner_still_resizes
    app::tests::bezier::open_shapes_offer_no_wire_ports_while_a_closed_polyline_keeps_them
    app::tests::bezier_close::a_closed_bezier_gets_the_closed_form_treatment
    app::tests::closed_form::corners::a_grip_pick_keeps_a_stroke_panel_that_is_still_offered
    app::tests::closed_form::corners::a_grip_press_commits_a_pending_stroke_preview_once
    app::tests::closed_form::corners::a_strip_button_over_a_rectangle_corner_keeps_its_click
    app::tests::closed_form::grips::a_direct_select_knob_on_a_soft_stroke_wins_over_a_strip_button
    app::tests::closed_form::grips::a_line_end_point_under_a_strip_button_drags
    app::tests::closed_form::grips::direct_select_picks_closed_form_vertices_as_anchors
    app::tests::closed_form::grips::text_editing_a_grouped_shape_keeps_the_text_panel
    app::tests::escape_drag::esc_mid_resize_restores_the_node
    app::tests::escape_drag::esc_mid_rotate_restores_the_node
    app::tests::media::assets::unbundle_word_of_an_unsaved_workbook_writes_into_the_data_dir
    app::tests::media::assets::unbundle_word_writes_page_images_beside_a_saved_workbook
    app::tests::media::paste_input::a_pasted_image_resolves_as_a_generator_input
    app::tests::media::unbundle::unbundle_of_a_single_page_does_not_place_a_grid
    app::tests::media::unbundle::unbundle_pdf_places_pages_through_dispatch_and_undoes_in_one_step
    app::tests::media::unbundle::unbundle_pptx_and_legacy_ppt_when_office_can_render
    app::tests::media::unbundle::unbundle_while_the_preview_is_pending_finishes_instead_of_failing
    app::tests::packs::pk2_generate_offers_add_api_key_until_a_key_is_stored
    app::tests::selection_chrome::the_eraser_preview_keeps_its_own_color
    app::tests::subobject::deleting_a_rectangle_edge_opens_its_outline
    app::tests::subobject::tiny_shape_body_moves_before_overlapping_transform_grips
    app::tests::text_zoom::canvas_text_keeps_its_breaks_and_line_offsets_through_a_zoom_sweep
    app::tests::tip_direct::handles::a_handle_knob_under_the_property_strip_drags_with_a_panel_open
    app::tests::tip_direct::handles::a_handle_knob_under_the_property_strip_still_drags
    app::tests::tip_direct::handles::the_property_strip_sits_beside_the_picked_vertices
    app::tests::wire_cursor::wire_grab_zone_uses_a_grab_cursor_on_every_node_kind
    app::tests::wire_grips::wire_grip_hit_grows_when_zoomed_out
    app::tests_wire_lanes::converging_wires_arrive_sorted_and_do_not_cross
    app::tests_wire_lanes::three_converging_wires_spread_across_the_zone_at_the_preferred_spacing
    app::tests_wire_lanes::twelve_converging_wires_pack_evenly_denser_inside_the_same_zone

test result: FAILED. 1321 passed; 60 failed; 53 ignored; 0 measured; 0 filtered out; finished in 350.45s

error: test failed, to rerun pass `-p slate --lib`
```
