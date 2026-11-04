## LATEST freeze — raster-aligned fitted transcript paint boundary (31 October)

Fresh app transcript result remained5/6 passing: the strict physical-scissor test proved a real ppp1 crop when fitted clip.bottom and ink.bottom both equal610.4, because the GPU rounds the clip to610. This is distinct from earlier one-ULP diagnostic-only issues. Current two-file production correction is formatted/frozen for independent review and focused execution; **not yet claimed passing**.

`board_agent/portal_bound.rs` retains the exact logical transcript rectangle for layout, ScrollArea max-height/offset/content measurements, and hit ownership. Only an Open automatically fitted transcript (`fit_height` or legacy auto train with no authored size) gets a separate PAINT clip whose bottom is ceil-aligned to the physical raster. X/top remain unchanged. Expansion is clamped to the real body/caller viewport and, on composer tails, before the actual composer/key strip; it uses existing bottom padding/gap rather than adding document space. The outer paint clip and ScrollArea inner paint reset both respect this; nonexpanded/manual cases retain their previous inner intersection. Manual authored height, capped Window, Partial and Collapsed viewports keep their genuine clipping. No document rect, width, wraps, line gaps, or selection geometry changes.

New `board_agent/portal_bound/clip_tests.rs` tests `fitted_transcript_clip_reserves_the_complete_raster_pixel_without_layout_changes` across3displayppp×5canvaszooms×4fractionalphases, exact constraints/placement and stable repeats; `manual_partial_and_window_transcript_viewports_keep_genuine_clipping` checks real physical610.4 crop remains for manual/capped/folded viewports. Existing integrated strict Pair oracle and its610.4negativeproof remain unchanged. Python float32 arithmetic sanity matrix passed, but no Rust execution by implementer.

Hashes/filter receipt: `%TEMP%/studio-pair-app-scissor-o1my62ei/fitted-transcript-rasterclip-receipt.json`. No Cargo run. Prior5 transcript passes and shell final metric-space1/1 pass remain attributed to their previous source snapshots.

## LATEST freeze — correct local reservation oracle and actual parent scissor (31 October)

Exact rerun established zoom0.72/plain rowbottom `251.830001831` vs translated probe inkbottom `251.830017090` (one ULP), while the emitted local mesh bottom and measured local union bottom both equal `151.830001831`; the real parent contains all ink. Residual0.629999995 confirms the near-unit paint shortcut did not run. `TextShape::visual_bounding_rect()` calls `Rect::translate`, which reconstructs `translated min + size` and can incur a different one-ULP addition/subtraction result from the row's reserved height. This is not evidence of the original0.4 ink underreservation.

**Test-only oracle correction:** `generator_activity/tests.rs` now strictly compares actual emitted scaled/local mesh Y bounds with measured pretranslation union. It verifies exact rowbottom=`rowtop+unionheight` and exact paintorigin=`rowmin-uniontop`, then exact `(localunion,height,row,origin)` equality across three checked frames and both selectable/plain modes at all eleven zooms. A row is a layout reservation, so it is not rounded as a fictitious scissor. Actual parent scissor versus unrounded global ink, raw vertical parent containment, exact original-parent scissor equivalence, and610.4crop negative proof atppp1/1.5/2 remain. All comprehensive diagnostics stay before every assertion.

Current file SHA256 `4051c127470d36395a5a02a3ce7e1f6e1d66fa0902fb5c8431665cf7fb751f98`; formatted/diffcheck clean and frozen for independent source gate and focused rerun. No production changes or arbitraryepsilon/padding. Receipt `%TEMP%/studio-options-more-icon-d5nwir_t/pair-ink-reservation-receipt.json` updated. No Cargo run by implementation worker; remaining shell case/fresh integrated Pair are not yet claimed passing.

## LATEST freeze — centralized pre-assert ink diagnostics (31 October)

The physical-scissor correction progresses past zoom0.5. Exact rerun now stops at the raw row reservation assertion at zoom0.72/plain: one-decimal row and ink both end251.8, so cause remains unproven. **Diagnostic-only change** in `generator_activity/tests.rs` builds one comprehensive payload before every row, rawY, parent-scissor, physical-ink, and repeat-height assertion. It includes nine-decimal row/ink/clip edges and rawbits, signed room and ULP deltas against both row and clip, actual text origin/relative ink/logical/mesh bounds, pre-raster residual/nearunit shortcut/mesh, vertical union edges+height, parent, displayppp/target. All existing strict checks and610.4crop negative proof stay. No tolerance, geometry, or production changes.

Current test-file SHA256 `1bf0a71540f9a0c0a39ba6be51e61b39f6a25c255d1be3b99435546bd05b1794`; formatted/diffcheck clean and frozen for exact diagnostic rerun. Receipt updated at `%TEMP%/studio-options-more-icon-d5nwir_t/pair-ink-reservation-receipt.json`. No Cargo run by implementation worker; no pass claimed for remaining row case or fresh app Pair.

## LATEST freeze — confirmed one-ULP clip transform, renderer-space test fix (31 October)

Exact diagnostic proved the remaining shell fixture mismatch: at zoom0.5/selectable/frame1, returned global clip.left `100.000007629` versus unrounded ink.left `100.000000000` (one ULP), original parent.left `100.0`, residual raster scale `0.520278096`. Strict row bottom and ink bottom both `205.248321533`; vertical ink reservation passed. No geometry defect demonstrated by this X-only floating transform roundtrip.

**One test-only correction, no production change:** `crates/atlas-shell/src/selection_tools/generator_activity/tests.rs` uses actual backend scissor semantics (`egui-wgpu::ScissorRect::new`, equivalent positive-coordinate Glow): returned global clip points times `FullOutput.pixels_per_point`, round each edge, cast/clamp to framebuffer dimensions derived from the same logical2400px screen and ppp. Ink remains unrounded and is converted using the same display ppp (canvas zoom is already in global shapes). Asserts actual transformed clip has exactly the same physical scissor as original parent; strict raw vertical clip and row reservation plus exact repeat heights stay. Explicit ppp1/1.5/2 negative checks verify original inkbottom610.4 vs clipbottom610 still fails. The existing real-parent-crop test and integrated Pair strict raw containment are unchanged.

Formatted/diffcheck clean, frozen pending independent final source gate and focused rerun. Current file SHA256 `e481505bbb41b397d5d87cc3577461ca8acd575b6440795949b6340601a1eadf`; receipt `%TEMP%/studio-options-more-icon-d5nwir_t/pair-ink-reservation-receipt.json`. No Cargo run by implementation worker. Earlier ten affected shell tests passed before this final fixture correction; do not report the remaining case passing until execution.

## LATEST freeze — exact edge diagnostics for remaining ink fixture mismatch (31 October)

Focused row rerun now reaches bounds: strict row vertical reservation passes; parent containment fails at zoom0.5 despite one-decimal print appearing inside (`parent100,100..424,1900`, `ink100,100.8..421.9,205.2`). Root requested full scalar/ULP evidence before any semantic change. **Only test diagnostics changed:** `generator_activity/tests.rs` emits nine-decimal clip/ink edges, signed per-edge room, ULP deltas, mode/frame, original parent, raster scale, pre/posttransform logical and mesh bounds, row, and text origin. Strict row/parent/stability assertions remain. Current SHA256 `b1e620328e87503f9f8da781bab4662eed1999d31dacd0b6df392e3f6ed33f23`; formatted/frozen for exact diagnostic execution, not claimed passing.

Possible inverse/forward transform roundoff is not yet confirmed; renderer physical scissor semantics can be considered only after exact evidence, without rounding ink or hiding original0.4bottomcrop. No production/tolerance/layout changes in this diagnostic followup. Earlier ten affected shell cases remain passing on the previous checked source; actual Pair app test still needs fresh execution.

## LATEST freeze — row ink test native Area setup (31 October)

Validator built the ink correction and executed **10/11 affected shell tests passing**: actual overhang/parentclip/hit safety, existing selection, six activity tests, and borderline overflow passed. New row stability test stopped before bounds assertions because it searched for a Text shape on the newly created selectable Area's initial invisible sizing frame. egui `containers/area.rs` sets sizing for absent state, then `content_ui` deliberately uses `sizing_pass().invisible()` and requests another frame.

**Test-only correction:** `crates/atlas-shell/src/selection_tools/generator_activity/tests.rs` now uses the real registered canvas Panel/child and settles one setup frame per plain/selectable mode, then still asserts three complete paint frames at each of eleven zooms. Strict ink containment, exact repeat heights, cross-mode equality, and real-rung overhang assertion remain. Absent-shape diagnostics include zoom/mode/row/all shapes. No production changes in this followup; formatted/diffcheck clean, focused rerun pending. Current SHA256 `e5f83ae0cc62a830b453218750cf3fe98d981f86faed0da6820fba6e7fb38c1c`. Receipt updated at `%TEMP%/studio-options-more-icon-d5nwir_t/pair-ink-reservation-receipt.json`.

## LATEST freeze — real raster ink reservation for Pair Full (31 October)

Current executed app result remains **29/30 affected cases passed**. Sole failure was strict actual ink bottom 610.4 against transcript clip 610.0 with the complete card on-screen. This is a production logical-line/raster-ink mismatch, not an assertion tolerance issue. New six-file correction is formatted/frozen for independent review and focused execution; **not yet claimed passing**. No Cargo run by this worker.

- `crates/atlas-shell/src/canvas_text.rs`: selectable generated shapes paint within the actual transformed parent viewport; logical selection/hit band remains unchanged.
- `crates/atlas-shell/src/selection_tools/generator_activity.rs`: shared transcript rows reserve the vertical union of reference line box and actual scaled raster mesh bounds. Negative top overhang shifts the block into its reservation; wrapping, intra-block glyph positions, and explicit turn gaps remain unchanged. No arbitrary padding.
- `apps/slate/src/app/board_agent/portal_bound.rs`: content-height cache retains any measured growth rather than discarding real ink growth <=1 world unit. It is monotonic and does not feed row layout, preventing cumulative reservation feedback.
- Tests in `canvas_text/selection_tests.rs`: `selectable_ink_overhang_uses_parent_clip_without_expanding_hit_band` exercises unchanged real glyph mesh with shorter reference box at 0.5/1/2 scales, full parent vs genuine parent cropping, and no expanded logical hit ownership.
- Tests in `generator_activity/tests.rs`: `transcript_rows_reserve_raster_ink_and_repeat_without_height_growth` uses the actual failing long assistant text, eleven zooms/rungs, selectable/plain paint, strict vertical ink reservation and parent containment, plus identical repeat heights.
- `apps/slate/src/app/board_agent/history_tests/transcript.rs`: existing `full_from_real_message_pair_menu_retains_divider_presentation_and_measured_height` retains strict full mesh containment and adds repeated-frame exact fitted rect/content-height stability.

Six-file hashes and pending execution status: `%TEMP%/studio-options-more-icon-d5nwir_t/pair-ink-reservation-receipt.json`. Other 29 affected current tests, including linked query bytes, first-use Coffee navigation and pinned pixels, have passed in validator's fresh binary; those results predate this ink correction. Final native visual acceptance remains separate.

# Native integration handoff — 30 October 2026

## Authoritative current status — latest resumed UI checkpoint


### LATEST freeze — final picker row-space test correction (31 October)

Focused rebuilt shell binary passed11/12 prior failures. Remaining
thought_and_speed_return_to_the_compact_root_in_the_same_popup compared label
visual ink centers to30px pitch. epaint TextShape::visual_bounding_rect is the
translated galley mesh_bounds, while canvas_text anchors the logical galley box;
words with different ascenders/descenders need not share an ink-center offset.
Only that test now probes each actual popup row Response under its painted label
clip, picks the smallest containing popup widget without using expected height,
transforms its rect by the real layer-to-global transform, and asserts30px row
height AND30px pitch at original.1 tolerance for .5/1.01/2canvas zoom. Label paint
clip top/bottom must match hit-row bounds. Full ink/clip/row/zoom/delta diagnostics
are included; all content/navigation/open/root-height assertions remain.

Changed one Rust test file only: menu_selector/generator_tests/picker.rs.
No production layout/row-height/tolerance change, no Cargo by this worker. rustfmt
and focused git diff--check passed; frozen for validator. Current hash recorded
in glyph-stage shell-last-row-space-receipt.json. This is a test coordinate
correction; runtime30px geometry remains pending fresh execution, not a new pass
claim from source inspection alone.



### LATEST freeze — shell12 executed failures classified, test-only corrections (31 October)

Fresh shell selection suite compiled and executed133pass/12fail. New More actual
circle/click/accessibility test PASSED. All12 causes traced to outdated setup or
expectations against the approved shared owners; no new production regression was
established. Seven test-only source paths are formatted/frozen and diff--check0;
independent source read and fresh shell execution are required. No Cargo by this
worker. Exact current hashes: glyph-stage shell-12-followup-receipt.json.

Per-test classification and retained proof:

1. image_agent_count_arrows_accept_repeated_clicks_at_each_layout_and_zoom:
   font-measuring shared datum was called before first Context::run. Fixture now
   paints one actual editor frame first; all repeated count/click/Stop assertions
   unchanged, no production font workaround.
2. borderline_overflow_does_not_toggle_scroll_position_with_zoom: test subtracted
   old8u origin; actual prompt begins at face.shrink(6u), so unscrolled was -2.
   Uses actual6u viewport origin; .03 tolerance, continuous zoom sweep and both
   .5/1.5 overflow cases retained. Added actual viewport/offset diagnostics.
3. a_large_indexed_album_materializes_only_the_visible_global_indices: reviewed
   18u circles+2u gaps legitimately fit43 chips into900u with36u navigation.
   Exact fetched-global-range assertion passed before arbitrary<32 failure. Now
   also bounds work by viewport/pitch and compares identical viewport/selection
   for one million versus one billion outputs (1000x growth adds no work).
   Selected index remains fetched. Ordinary albums and production loop untouched.
4. port_notch_is_screen_sized_and_radius_changes_invalidate_the_cached_host:
   reviewed radial radius7.4/6 gives3.7/7.4, not stale4/8. Exact .01 assertions
   corrected; screen-zoom equality and radius-cache invalidation retained.
5. every_generator_settings_action_selects_inside_the_attached_apron:
6. a_real_outside_click_closes_the_attached_generator_settings:
7. selection_memory_snapshot_can_restore_the_dropdown_after_an_off_click:
   these searched removed Settings text. Shared fixture now records actual
   existing14u Options Response rectangle and clicks it. Legacy-choice response
   mapping, outer-apron popup ownership, outside dismiss, quick-return and close
   assertions remain. No product menu/actions added.
8. attached_activity_chevrons_remain_live_after_following_an_overflowing_transcript:
   searched intentionally removed Ready label and old horizontal fold locations.
   Uses actual two stacked fold Response rectangles, checks14u targets and no
   duplicate Ready/Activity text; followed17-entry overflow, first-click fold
   choices at every zoom/order and returned snapshot remain asserted.
9. rich_model_speed_thought_and_access_actions_select_inside_the_attached_apron:
   current Effort display replaces provider-advertised Thought level wording;
   choices keep the same popup open and return root. Exact returned opaque values,
   access action and outside-apron click ownership preserved.
10. thought_and_speed_return_to_the_compact_root_in_the_same_popup: root has5
    fixed screen-sized rows Provider/Model/Effort/Speed/global Full access, not4
    zoom-scaled rows. Asserts five identities and30px row pitch, bounded frame;
    expanded header collapses its own page (no removed Back row), one popup owner
    and same returned root height.
11. bottom_edge_attached_popups_fit_the_viewport_and_lower_rows_are_reachable:
    navigation label now Effort; actual bottom-edge bounds, long-catalog scrolling,
    fully visible last/action rows, exact returned data and click ownership remain.
12. leaving_a_long_model_page_resets_the_next_visit_but_an_off_click_preserves_it:
    uses real stationary Model header as scroll sentinel instead of absent Back;
    quick-return must keep it scrolled out, choices retain popup root, another
    expanded header returns root, a new Model visit restores header at top.
    Same-page offset preservation, page reset and single-popup counts remain.

Changed only tests: selection_tools/{agent_image_tests.rs,
generator_attached_tests.rs,generator_history/tests.rs,image_surface/host.rs
(test block only),menu_selector/generator_tests.rs,
menu_selector/generator_tests/picker.rs,menu_selector/generator_tests/activity.rs}.
No global row behavior, scroll tolerances, album geometry/work cap, notch geometry,
popup controller, production menu, or authority changed for these12 tests.



### LATEST freeze — native More icon APPLIED; four executed-test diagnoses (31 October)

Independently cleared shell-only More patch SHA256
`b335e832d512de18f5d29136bb7c08a276b6e81dcffbf01e8bcff97e9d6b2f4b`
was applied after validator/root source release and fresh3 original +2 absent
guards. Normalized5/5 matches reviewed stage, original newline conventions kept.
Separate glyph apply receipt:
`C:/Users/jeffr/AppData/Local/Temp/studio-options-more-icon-d5nwir_t/applied-receipt.json`.
Actual missing-font square is replaced by existing native More geometry (three
circles), shared with presence More. Same14u hitbox/ID/popup, model caret, tooltip
and new explicit accessible Options label. No app Rust/API/command changes for
this icon patch. Existing Plus API re-export unchanged. Native GPU proof pending.

Latest app binary executed193pass/4fail across197 unique cases. The pinned exact
same-binary retry passed0.11s; previous failures were retained in evidence. Four
narrow test-only follow-ups are formatted/frozen, no request lifecycle/crop code
changed and no assertions removed:

1. Linked Text query byte preservation: independent trace found setup
   text_output.write_now(original,samebody) still asynchronous. std::fs::write
   can expose transient empty bytes before it completes. Query capture/dispatch
   exact intent correctly skips begin_run, and text_query_runs survives metadata
   completion. Test now waits for writer.pump completion matching original item/
   path BEFORE starting query (matching bytes alone was insufficient), retaining
   byte/override/range/revision assertions and query→ordinary→query transitions.
2. Pair long-turn clip: finite viewport was centered for pre-Full160-height card.
   Test now fits/recenters the actual grown logical card before its STILL STRICT
   full-galley containment assertion, asserts the card itself fits canvas, records
   original viewport bounds, and prints painted/clip/body/canvas/zoom if failing.
   Divider/assistant datums now compare world offsets from card top so camera
   reposition cannot invalidate them. Same divider/mode/width/complete text/
   measured height/session checks retained. No epsilon or renderer crop changed;
   fresh execution/diagnostic is still required to establish root cause fully.
3. First-use Text query: original test painted immediately without any initial
   frame and did not open its recorded answer. Default shared activity is
   Collapsed. Fixture now settles real initial source/camera; after exact original
   request/metadata/oneUndo checks, drives recorded origin+response through existing
   apply_text_answer_navigation, asserts exact viewed-dialogue identity, then
   centers real frame and retains Coffee paint assertion. No production source
   query behavior changed. Fresh execution must verify this lifecycle setup.
4. Pinned preparation: completion remains existing poll_agent_input_preparation
   readiness, not elapsed time. Test-only deadlock backstop3→15s permits release
   validation contention, and reports elapsed/source/replacement/bake dependency
   counts. Actual pinned source/pixel/layer/crop assertions unchanged. No product
   timeout or latency contract changed.

Four paths/current hashes are in glyph-stage app-fixture-followup-receipt.json:
board_agent/session/text_answer_tests.rs;
board_agent/generator_tests/daisychain.rs;
board_flow/editor_helpers/tests.rs;
board_agent/history_tests/transcript.rs.
Targeted rustfmt and diff--check pass. Reviewer/root and validator coordinate next
focused execution; NO Cargo by this worker. All new source is frozen.

Filters: `linked_text_query_records_shared_answer_metadata_without_override_or_source_write`;
`full_from_real_message_pair_menu_retains_divider_presentation_and_measured_height`;
`new_text_starter_queries_unicode_passage_on_original_source_and_shared_dialogue`;
`a_pinned_picture_keeps_its_own_layer_stack_and_submission_crop_after_further_edits`;
`compact_options_paints_shared_ellipsis_and_clicks_without_changing_model_caret`.
The latest actual Model UI proved corrected Codex/shared composer/no duplicate
Generate on prior binary; corrected More, empty-model guidance and full query/Pair
long-content native acceptance remain separate pending evidence.



### LATEST freeze — reopened Model/Text Escape lifecycle corrected (31 October)

Validator interrupted only its owned Cargo compile and explicitly released source.
Actual corrected Model now reaches shared Codex UI (no GPT Image/duplicate Generate),
but reopening the saved workbook exposed a separate popup lifecycle defect:
attached_editor_body synchronized shared presentation -> draft popup flags only
when image=true. Model/Text false left draft.model_open=true; Escape cleared shared
state correctly, then same-frame paint reopened it from the stale draft. Common
attached adapter now synchronizes model/aspect popup flags for ALL attached hosts.
No is_flow_node, selection memory, draft content, mode, or authority changes.

New test `reopened_model_and_text_picker_escape_and_outside_click_do_not_reopen_stale_drafts`
in board_flow/editor_helpers/tests.rs saves/reloads actual Model Chat and Native
Text Text host shapes, pointer-opens the painted shared label, verifies the real
Provider popup, first Escape closes popup through attached repaint, second closes
parent, reopens then clicks host outside popup, and checks no next-frame reopening.
Source content, prompt and journal remain unchanged. Reopened Model has no discovered
provider rows to cover the actual disabled-root case. Test is NOT RUN; rustfmt and
focused git diff --check pass. Rust frozen for validator, no Cargo by this worker.

Two previous compiler-only adapters are also applied: portal_picker labels borrow
as_str to match shared &[&str] API; model regression uses existing
set_agent_programs_for_test instead of private runtime field. IDs/catalog/native
assertions unchanged. Current hashes for all23 Rust paths are refreshed in composed
scratch final-freeze-receipt.json; original reviewed-stage hashes remain historical
apply proof, not current source after these narrow corrections.



### CURRENT freeze — composed Pair/glyph + first-use Model/Text + explicit Fit correction APPLIED (31 October)

This section supersedes older status below. Independent review cleared combined
patch SHA256 `a59ff4feb8e71003d56069b5780281cd2126382ffcbd6226758aa7fa9fbb287a`.
Validator released source; all17 original raw hashes +4 absent-file guards freshly
matched before exact unified-hunk apply. Applied normalized content matched reviewed
stage21/21, original newline conventions retained. Two narrow executed-test diagnoses
were then corrected: explicit Fit production one-liner independently cleared, plus
canonical timestamp fixture adjustment. All23 affected Rust files are now frozen,
rustfmt parsed and targeted git diff --check passed. **No Cargo by this worker;
post-apply tests/GPU remain required.** No staging/commit/revert occurred.

Review packet + original/apply/final current hashes:
`C:/Users/jeffr/AppData/Local/Temp/studio-pair-model-text-composed-lck382nz/`:
`HANDOFF.md`, `original-sha256.json`, `manifest.json`, `applied-receipt.json`,
`final-freeze-receipt.json`. The final receipt includes timestamp/Fit follow-ups;
older individual scratch packages must not overwrite combined source.

Applied behavior and evidence:

- Pair Full preserves actual Detail::Pair presentation/divider while shared Open
  fits all text at authored width. The menu check recognizes fitted/open Pair.
  New regression drives the actual first pair dots/Full row with long user and
  assistant content and checks divider, untruncated clips, height, width and
  unchanged session/range. Native failure screenshots are pre-fix evidence only.
- Generator options uses existing ellipsis with no extra caret. Model picker
  retains its own caret; hit rectangle/spacing unchanged. Focused glyph/click test
  added. Existing PickerRows moved to child module to stay one row owner.
- Blank/empty Model modality remains based on existing model_node_info, independent
  of drawable meshes. Bound Chat Model/Text media now reaches existing attached
  shared generator, with no extra Chat Train starter row. Model paint bypasses
  legacy image-only Wire Note/Generate overlay; existing model preview/result owner
  still paints source/history. Saved Chat/provider/source/style/camera unchanged.
- Bound Chat media uses existing Chat Train catalog/default owner
  portal_picker::chat_model_options and registered portal.agent.model. Codex's
  default displays Codex, advertised model selection retains provider/view/session
  and native binding. Provider metadata/dispatch filters actual Chat programs,
  excluding codex-text/openai-text/image aliases. Ordinary Image and unbound Text
  generation engines/child spawn remain on existing paths.
- First-use native model context is owned by existing membership/native authority.
  Runtime stage revalidates worker/source/document identity independently of scene
  generation, rejects changed-key Send before rewriting grant identity, preserves
  same-document revision consent, and never automatically grants/resumes. Existing
  MODEL picker has a distinct conditional Board access… row with isolated-model
  active/Resume-required state; dispatches existing portal.agent.board_access and
  existing settings/Resume. Global Full access and Preserve-only dots remain
  separate. Pure attached_model_native_access(id)->Option<bool> has no file probes;
  true describes readable runnable membership, not mutation autonomy.
- Unbound Text highlighted query binds original source in one existing SceneCmd
  transaction, retaining source text/style/rect/linked override; request captures
  exact Unicode byte range against original host. No query child/wire/composer or
  output-file slot is created. Empty/read-only/locked queries cannot bind. Reply
  attaches shared answer metadata and uses existing shared dialogue. Non-query
  starter submission still spawns via existing owner.

Two latest executed failure causes/fixes:

1. `a_resize_records_its_size_and_fit_to_text_forgets_it`: real regression.
   Production D23 says explicit Fit to text clears authored size. Only
   agent_fit_to_text now sets chat.size=None before shared write_fold(Open).
   Full/double-chevron Open keep authored width. Existing sizeNone/width-change/
   two-Undo collapsed restoration assertions retained; added one-Fit-Undo,
   fit_height and exact manual size/rect restoration checks. Independent source
   reviewer cleared actual applied change; execution pending.
2. `single_window_uses_shared_user_indent_at_native_zoom_without_mutating_turns`:
   geometry/paint passed; fixture expected assistant.at0 while existing
   review_chat_lines canonicalizes index1. Expected timestamp corrected to1;
   full turn equality and all three zoom/ink assertions retained. No painter or
   canonical transcript semantics changed.

Exact new/follow-up focused filters (post-apply execution pending):
`full_from_real_message_pair_menu_retains_divider_presentation_and_measured_height`;
`compact_options_paints_shared_ellipsis_and_clicks_without_changing_model_caret`;
`empty_model_chat_binding_opens_shared_generator_without_image_starter_or_legacy_prompt`;
`new_text_starter_queries_unicode_passage_on_original_source_and_shared_dialogue`;
`attached_codex_model_uses_chat_catalog_and_registered_choice_without_losing_native_binding`;
`native_board_access_row_is_distinct_from_global_preference_and_absent_without_scope`;
`attached_identity_events_without_scene_edits_fence_send_until_explicit_resume`;
`attached_model_send_requires_explicit_access_and_preserves_revocation`;
`attached_model_mutation_uses_existing_scoped_permit_and_revision_does_not_nag`;
`attached_model_read_only_stale_and_external_sources_cannot_gain_mutation_scope`;
`a_resize_records_its_size_and_fit_to_text_forgets_it`;
`single_window_uses_shared_user_indent_at_native_zoom_without_mutating_turns`.
Existing Full/width, normal Window ceiling, wired Chat/native permission, Image,
ordinary Text/linked query lifecycle regressions must remain passing. Validator
reported previous artifact/image/text-override executed failures now pass on prior
snapshot; that does not validate newly applied code.

21-file applied list: app `board_agent/{bundle.rs,header_menu.rs,
history_tests/transcript.rs,picture.rs,generator_chrome.rs,portal_picker.rs,
generator_chrome/editor.rs}`, `board_flow.rs`,
`board_flow/{editor_helpers.rs,editor_helpers/tests.rs}`,
`agent_presence/{chat_train.rs,chat_train/attached.rs,chat_train/attached/tests.rs}`,
`application_connector/headless.rs`; shell
`selection_tools/{menu_selector.rs,generator_options.rs,generator_picker.rs,
generator_picker/rows.rs,generator_picker/tests.rs,generator_access.rs,generator.rs}`.
Two additional follow-up files: `board_agent/{spawn_chrome.rs,picker_tests.rs}`.

Residual gates: fresh app compile/focused tests; actual corrected Pair Full divider
and all-text/width, generator ellipsis, new headless Agent/Codex picker + explicit
Board access/Resume/Send/no-repeat consent, new Text highlight/query/exact-answer
navigation. Model test uses existing trusted native .3dm fixture and real frame/
registered owners, not a paid provider or SDK invocation. No new native acceptance
claim is made from source review or mocked completion alone.


### Current authoritative freeze — Full applied + four executed-test diagnoses (31 October)

Validator explicitly released source after its compiled-binary groups. Independently cleared Full/width patch SHA256 `2782c8250699fd532b765f054fbaccf138776d15c424d8af1ee438a2de9d3e6e` was applied only after all11 original guards freshly matched. Original Full-source LF/CRLF conventions retained. Full11 plus seven disjoint follow-up Rust files are formatted/frozen (18total); rustfmt0 and targeted git diff --check0. **No Cargo or corrected native GPU pass by this worker.** Current test/native results predate this new source snapshot. Current post-apply file hashes are `C:/Users/jeffr/AppData/Local/Temp/studio-chat-full-width-4fex5mpu/applied-receipt.json`.

Full behavior: durable defaultfalse `ChatView.fit_height` separates authored width from an explicit height ceiling. Full acts even when prior detail alreadyFull/manual500, refits with actual font context through existing transaction and no540 cap. Open/Full retain size[0] and fit all content. Width-only E/W preserves fit policy (post-Full reflowsY; ordinary legacy Window keeps its manual ceiling); Y/diagonal/Shift restores manual height. Full menu check reflects detail+fitted+open state. Existing pair/train painter and draft/bundle branches remain. `.slate` roundtrip is covered; no new locators. Export still renders private Chat as a host poster with stored node rectangle (`slate-artifact/render.rs403,426–462`), not transcript content; that existing export limitation is explicit.

Four executed-test failures classified/corrected without dropping required assertions:

1. **Artifact new child panel/focus:** stale expectation. This test spawns an independent blank child via actual production command; current user requires closed menus/no unsolicited focus. It now asserts panelNone, no flow-focus request, blank durable/runtime prompt, new Preserve false, source node and bytes unchanged. Node/wire counts, one-step Undo and Redo assertions remain.
2. **Single-window role lookup:** fixture fault. Raw local_turns injection preceded runtime binding registration and was discarded by session sync. The test now settles the existing binding and uses existing `review_chat_lines`; actual28-unit role geometry assertions at1/.72/2zoom and unchanged canonical turns remain. No role painter change in this follow-up.
3. **Image squares while Agent open:** stale pre-DI19 expectation. Closed square placement/overlap assertions remain; open Agent now requires no duplicate texture squares below circle history; closing restores below-chrome squares. No production DI19 rollback.
4. **Ordinary Text override/file reset with Preserve on:** real lifecycle defect. `oneshot` describes model replay, not output ownership. Accepted ordinary non-query Text Run now clears an override and starts its output run independently of Preserve. The queued request's exact `!query` gate is the single clear-file admission; removed stale current-session query recheck which could suppress ordinary Run after a prior query. Streaming retains current-query source protection and additionally refuses a nonempty mismatched stale session.request. Provider replay/oneshot choices are unchanged. Independent reviewer closed these three production guards in source.

Ordinary rerun regression now loops Preserve off/on and preserves strict one-Undo/file/export/downstream assertions. Linked query regression explicitly Preserve-on, keeps original source+no-override query, then tests query->ordinary reset/output with old-query completion refused, then ordinary->query keeps an explicit authored override and source file. Mock preserved history correctly appends checkpoints2/4/6 and asserts request histories remain available.

Full11 paths: `crates/slate-doc/src/agent_chat.rs`; app `board_agent/{bundle.rs,card_metrics.rs,card_fit.rs,spawn_chrome.rs,card_fit_artifact_tests.rs,header_menu.rs,history_tests/transcript.rs}`, `dispatch.rs`, `board/board_gesture_end.rs`, `tests/agent/place.rs`. Seven follow-up paths: `board_agent/{generator.rs,generator_pump.rs,generator_tests.rs,session/text_answer_tests.rs,artifact/tests.rs,image_album_tests.rs}` and `board_flow.rs`.

Focused filters awaiting execution: `full_action_fits_all_terminal_turns_and_preserves_width_through_edges_fold_and_reload`; `ordinary_window_keeps_its_manual_ceiling_on_width_only_resize_until_explicit_open`; `legacy_manual_size_and_explicit_fitted_height_roundtrip_without_sentinels`; `fourth_spawn_choice_creates_one_web_host_and_content_wire_without_running`; `single_window_uses_shared_user_indent_at_native_zoom_without_mutating_turns`; existing closed image blister strip test; `running_an_edited_generated_text_card_releases_its_override_as_one_undo`; `linked_text_query_records_shared_answer_metadata_without_override_or_source_write`. Validator owns quickcheck/tests and corrected oldFull/defaultfalse + Summary native Full action/width/fold/reload evidence. No further source writes planned before diagnostics.


### Registered headless human dispatch route corrected — 31 October

Native owned-empty-tab reproduction exposed a central routing omission: registered `board.model.headless.*` commands never reached `dispatch_application_connector`, because `dispatch.rs` admitted only the older connector prefix. The same central match now includes both prefixes; registry/availability gating remains before it. Existing human setup validation/native admission is unchanged. The Stop adapter now reports handled when it cancels a pending empty-model job with no placed node.

Changed Rust only: `apps/slate/src/app/dispatch.rs`, `application_connector/actions.rs`, `application_connector/headless/blank.rs`. The latter contains a test-thread-only fake worker reply, so tests exercise existing root dispatch and job ownership without launching Rhino. New regressions: `registered_root_dispatch_schedules_blank_without_target_and_stop_cancels_it` checks unsupported IDs, targeted-agent setup rejection, read-only handling, no-detail New scheduling and Stop cancellation without document/journal edits; `registered_root_dispatch_routes_activate_generate_and_stop_to_the_bound_job_owner` uses the existing in-memory connector peer and verifies real registered root dispatch queues/cancels generation without skipping native admission or changing binding. Both tests are unrun; rustfmt and targeted `git diff --check` passed. No Cargo by this worker; Rust frozen for validator.


### Reviewed visual correction batch APPLIED — 31 October

Independent review cleared all five staged patches. This worker freshly verified all16 original SHA256 guards, applied exact unified hunks, rustfmt-parsed all16 changed Rust files, and verified normalized resulting content equals the reviewed stage for every file. `git diff --check` exit0. Original LF/CRLF convention was reconstructed against guarded original hashes and retained per file. No Cargo, staging or commits; Rust is now frozen for validator compile/tests. Graft worker confirmed no path overlap with its cut lifecycle edits. Running native binary/captures remain **pre-fix evidence**, not validation of the applied source.

Applied behavior: DI19 Images canvas squares and hidden square hits are suppressed while Agent circles own history; DI09 first Escape dismisses popup before parent panel, second Escape retains explicit parent close, and Chat header dots outside primary-press resets the existing egui BarState without consuming the new selection press; DI13 single-window transcript consumes shared28-unit user indent/assistant72% ink and matching fit-wrap metrics, preserving train/pair paths; DI12 Preserve-only checked glyph uses existing menu Check painter (native save probe confirmed underlying toggle dispatch works); stale popup-layer test now checks actual nested Area, above-chrome order and real popup bounds.

Files: app `board_flow/result_actions.rs` + `result_actions/tests.rs`; `board_agent/picture/album.rs`; `board_agent/stop_tests.rs`; `board_agent/generator_chrome.rs` + `generator_chrome/tests.rs`; `dispatch/input_state.rs`; `board_properties/selection/tests.rs`; `board_agent/header_menu.rs`; `board_agent/portal_bound.rs`; `board_agent/card_fit.rs`; `board_agent/history_tests/transcript.rs`; shell `selection_tools/generator_activity.rs`, `menu_selector.rs`, `generator_options.rs`, `generator_picker.rs`.

Affected regression filters: `image_generator_history_replaces_canvas_squares_and_releases_hidden_hits`; `the_model_list_is_a_popup_above_canvas_chrome`; `escape_dismisses_provider_popup_before_agent_panel_on_actual_frames`; `explicit_close_and_escape_forget_artifact_generator_memory`; `chat_dots_dismiss_on_other_text_or_empty_canvas_press_without_scene_edits`; `single_window_uses_shared_user_indent_at_native_zoom_without_mutating_turns`; `preserve_toggle_paints_check_after_click_and_keeps_the_options_open`. These have **not been run on the newly applied source**. New compiler or native evidence may still require narrow corrections.

Full reviewed API/behavior/test notes and current applied hash receipt remain at `C:/Users/jeffr/AppData/Local/Temp/studio-di19-triage-erujll9b/HANDOFF.md` and `applied-receipt.json`; each stage's `original-sha256.json` is preserved. Backing suspicion was withdrawn and no backing code changed. Header ellipsis/caret appearance remains separate pending design triage.


### Focused executed-test corrections — latest freeze

The pre-parity app test binary executed generator_chrome10 (8 passed, 2 failed) and artifact39 (36 passed, 3 failed). Corrections below are formatted and awaiting fresh validator execution; those failures are not reported as passing.

- `dispatch.rs`: moved exact `portal.artifact.full_access` handling before the `portal.artifact.*` catch-all. The catch-all previously parsed the boolean preference detail as an artifact request and returned false, leaving every host's cached global preference disabled. Existing all-host/restart/denial/read-only/binding assertions remain intact.
- Command-parity worker independently replaced selected Text's coalescible patch with shared prepared `commit_scene`. That fixes the pre-parity test's select-old undo depth (selection had merged into the preceding fixture edit). The test's strict +1 selection step and edit Undo assertions remain unchanged; no further selection production patch here.
- `artifact/tests.rs`: split legacy lineage setup from `spawn_current`, which dispatches only the production creation command. The two transaction-count tests use `spawn_current`; their previous helper additionally reset the new child's Some(false) history compatibility field to None in another journal step. Counts and Undo assertions are unchanged; other legacy lineage fixtures keep their compatibility setup.
- `artifact/tests/feedback.rs`: replaced obsolete fixed40 prompt expectation with shared `GENERATOR_PROMPT_MIN_HEIGHT` and a separate assertion that actual painted prompt bottom + existing8 inset equals measured editor bottom across all tested widths/catalog states. Current shared one-line composer is32; no layout production change.

No Cargo by this worker. Validator owns latest evidence. Exact Rust files changed in this correction: `apps/slate/src/app/dispatch.rs`, `apps/slate/src/app/board_agent/artifact/tests.rs`, `apps/slate/src/app/board_agent/artifact/tests/feedback.rs`.


### Latest narrow re-review correction — GR2 and exact manifest identity

Source frozen and rustfmt-parsed; no Cargo executed by this worker. This batch modifies only `board_agent/inputs.rs`, `generator_chrome.rs`, `generator_chrome/text_outputs.rs`, and `generator_chrome/tests.rs`; the concurrent A04 worker owns separate AccessView/preference hunks in the latter two shared files.

- **GR2 active editing:** `active_authored_text(id)` is the shared effective editor policy. The selected-output resolver suspends preview using it; input preparation places the same native/document buffer into its output map before pure snapshot validation, and applies that buffer to resulting wired/context items after the pure snapshot's persisted `shown_text` dispatch. Linked card fallback now consults the resolved effective selection instead of only the durable field. Live editor inputs bypass pending cached-document readiness, because the existing buffer is already available. No durable selection is cleared on editor open, Escape or no-op; existing authored commit/Undo owner remains unchanged.
- **Exact-origin navigation:** `text_output_is_current(id, reference)` compares both session identity and resolved manifest locator. Selected text resolution, answer navigation and activity display all use it. Same-session/custom-manifest navigation therefore reads the recorded source cache instead of displaying the active transcript. Provider memory, channel and grants are untouched.
- **Regression evidence (added, execution pending):** `generator_chrome::tests::text_output_selection_is_persisted_display_state_without_rewinding_latest_reply` now snapshots an actual typed downstream wire and graft plan during native and linked editing, then checks Escape/no-op, one-journal commit, Undo and serialized reload. New `generator_chrome::tests::answer_navigation_same_session_uses_exact_recorded_manifest` loads distinct recorded/active assistant text under the same session name, asserts the recorded dialogue is shown and active memory/binding remain unchanged. Validator owns the next compile/tests; earlier passing results do not validate this batch.


This section supersedes older chronological notes below. Latest source is parse-formatted and handed to the validator; it has NOT yet passed a new app compile. Check4 (earlier snapshot) passed `cargo check --locked --release -p slate --lib --jobs 2`, exit0 in2m13s. All subsequent fixes below require the validator's new check/test evidence. This worker ran no Cargo during the resumed pass, staged nothing and committed nothing.

### Required DRY findings

1. **Visible versus transmitted selected Text:** native Text and linked-text card display use the same exact selected-output resolver that GR2 input snapshots consume. An unavailable explicit source produces a viewport status and resolver Err; canonical latest `agent_reply` is unchanged. Existing inline/document editing suspends the preview while its buffer is active; actual authored change clears durable shown_text inside `slate_doc::authored_text`'s existing node mutation and the same journal transaction. Escape/no-op preserves it. Existing source originals are untouched. Targeted app selection test now exercises authored content, select-old, editor suspension, no-op, commit, one undo, serialization and missing checkpoint.
2. **Ordinary versus generator strip:** one `home/album_strip.rs` painter now has explicit ordinary/generated policy. `album_index_strip` uses original22-unit squares/run gaps/active-hover outlines; generator entry uses24-unit squares/4-unit radius/gap/eight-column running bond/no border. New regression invokes both actual public paint entry points and inspects painted shape size/stroke, rather than only testing a geometry helper.
3. **Credential probes in paint:** `GeneratorChromeData::provider` only reads `PackSession::credential_present`. Existing background pack probe returns bool presence at startup/refresh; explicit key save immediately updates cached presence and invalidates an older in-flight probe. Cursor save uses the same event. Repeated row lookup does no secret/file probing. Tests cover1000 cached lookups/no probe scheduling and the existing fake-store key-save lifecycle.

### Additional source-backed fixes completed in this checkpoint

- Model circles and squares consume headless `model_generator_outputs`, `model_generator_preview_info`, and `select_model_generator_output` via `generator_chrome/model_outputs.rs`. Only visible output identities request the existing model poster cache; no current poster is relabeled as a prior output. Selection uses shared ResultSelection/Shift and the registered `portal.agent.model_output`; no dialogue activation or kernel rebind.
- Shared `MediaPortNotch.half_span` extends the existing scallop into the capped graft capsule, with shared1.4 radial clearance/40% shoulder policy. The outline adapter and port painter use the same displayed center and height. Tests cover blank/populated geometry and the four-terminal cap (>4 identical cached geometry).
- Linked-text annotations live in default-empty `ImageNode.answers`, with the same TextAnswer/range/rebase model as native Text. Compatible optional `TextAnswer.source_revision` validates cached-source refresh; `origin:SourceUri` records the exact transcript manifest. Merely querying creates no text_override. The existing TextDocEdit captures selection, linked card calls the same answer hit/hover renderer, and explicit override edits rebase through the shared pure owner using the editor's cached original.
- Request-specific passage queries are distinguished from ordinary generate-to-file in the existing queue/stream writer path: query avoids clearing override/source output and preserves the original linked file. Ordinary generation retains its prior behavior. Query completion still requires exact submitted source/revision/request/session.
- Exact answer navigation carries the recorded `TextOutputRef` into a display-only viewed-dialogue entry in existing FlowUi. Current or archived custom-bundle manifests load through existing AgentSources; pending sources remain retained, and no provider history/session/grant is restored. New cut/reopen/custom-origin regression exercises this path.
- Text export now obtains `Result` from the data resolver before launching/writing export; pending/missing explicit selected output fails actionably and never serializes a UI status/newest fallback. Native Text selected data overrides authored source only in the exported view. Linked-text export uses the same annotated range serializer, filters stale source revisions, preserves the original-file link and omits private origin paths/transcript contents.
- Headless locator owner was notified of finalized Native Text/Image answers.origin and owns canonical rebase/collection traversal. This worker made no locator traversal or model publication changes.

### Current files/APIs and tests

Pure/schema: `crates/slate-doc/src/{authored_text,text_answers}.rs`, `scene/image.rs`; `TextAnswer::{origin,source_revision,is_valid_for_revision}`, `authored_text::apply_document_text_from`.
Credential lifecycle: `crates/atlas-ai/src/{packs,packs_tests,agent}.rs`; `PackSession::{credential_present,credential_saved}`, read-only `AgentSources::peek/snapshot`.
Shell: `home/album_strip.rs`, `selection_tools/{generator,generator_history,result_actions,image_surface/host}.rs` and shared geometry adapters/examples (plus earlier shared shell files).
App: `board_agent/generator_chrome{.rs,/editor.rs,/text_outputs.rs,/model_outputs.rs,/tests.rs}`, `board_agent/{session,generator,generator_pump,prompt}.rs`, `board_agent/session/text_answer_tests.rs`, `board_flow{.rs,/state.rs,/ports.rs,/result_actions.rs,/text_note/answers.rs}`, `board/{board_cards,paint_node,board_model_view}.rs`, `board/board_text_overlay/document.rs`, `board/board_text_place/drafts.rs`, `board_properties/image_palette/outline.rs`, `artifact_export.rs`, `commands.rs`, `dispatch.rs`. Artifact editor's dead legacy action dispatch removed; that file is now released to picker worker for key reachability.
Export: `crates/slate-artifact/src/render{.rs,/image.rs,/image/tests.rs}`, selected data-map render fixture in `lib.rs`.

Validator filters: slate-doc `authored_text`, `text_answers`; artifact `text_answers`, `linked_text_answers_share_native_range_model_and_never_export_private_origin`; shell `public_saved_view_and_generator_entry_points_keep_distinct_geometry_and_selection_style`, `capped_capsule_scallops`, existing generator/activity; AI `credential_presence_lookup_is_a_cached_read_without_probing`, `a_stored_key_turns_add_api_key_into_a_normal_openai_option`; app `board_agent::generator_chrome::tests`, `board_agent::session::text_answer_tests` (new linked-file query and custom-origin cut/reopen tests). Tests are added/updated; this worker does not claim their execution. Validator's prior doc4+text5/artifact2 passes predate these schema fixes.

### Residual acceptance gaps / ownership

- **A03 PENDING:** compact closed effort/speed, correct0/1/2 bolts, long-name behavior and artifact-spawned Chat Train picker parity. `picker_parity_worker` owns shared generator_picker/layout + portal_header/public picker integration, targeted generator_chrome provider metadata, and artifact Cursor key row reachability. Current actual model metadata APIs exist; do not copy widgets/fixture capabilities.
- **A04 PENDING native proof:** explicit masked Add/Change renewal and key visibility on every host, durable global-access UX/effective grant behavior. Cached presence fixes probe placement only. Existing access preference/grants remain separate; no broader authority was added.
- Native matched-size/theme/DPI/zoom hover/bridge/scroll/fold/resize/width/draft/save-reload/Undo evidence is still required. All source fixes above await the new compiler/focused tests and independent re-review; no visual certification claimed.
- Linked-file range querying is wired for bound linked Text hosts and the shared TextDocEdit; exact untouched-source/source-refresh/cut-origin tests are supplied. Review the production unbound starter-to-new-agent selection propagation in native acceptance; no separate composer/parser was added.
- Query ordinary-generation compatibility, source-backed export error behavior and per-output model/poster identity need focused test results and source-matched native exercise. No private transcript bytes are collected/exported.
- Capture-harness MRU/prune/cover/prefs guards received narrow independent source review here: no scope regression found for explicit SLATE_SHOT; no runtime acceptance. Presence-only launch without paired SLATE_SHOT is outside those guards.

---


Scope is difficult integration. Root orchestrates only; no Cargo was run by this worker. No files were staged or committed; existing dirty changes and protected baseline were preserved. Validation worker serializes builds with two jobs.

## Shared call map

- `board_agent/generator_chrome.rs`: actual advertised model parameter data and available program rows; transcript borrows existing session user/assistant turns plus request-scoped public work events; dispatches model/key/history/count intents to existing app owners. Existing machine-local dashboard access preference and explicit effective-grant checks retained.
- `generator_chrome/editor.rs`: Text and selected-model adapter into `atlas_shell::selection_tools::generator_editor`/`GeneratorOptions`; existing prompt/draft lifecycle remains in `board_flow::attached_editor_body`/`editor_helpers`. Text history borrows assistant turn snapshots; model preview calls existing `model_poster_texture`. No alternate text buffer, model viewport or provider fixture.
- `board_flow/image_editor.rs`: existing Images adapter, now consumes provider/key/preserve responses; image count mirrors the compatible common settings field. Non-image bound/selected-model fallback routes to the shared adapter above.
- `board_agent/artifact/editor.rs`: Dashboard uses shared count/preserve controls and a history-only generator menu; legacy command capabilities remain registered. Actual thumbnail requests resolve exact immutable revision materialized locators and only the shared measured visible history range.
- `board_web.rs`: existing asynchronous StillCache owns disk requests and generation-tagged current captures. Independent small texture cache keyed by source locator retains actual revision thumbnails as mutable live posters change; bounded128 textures. Cache misses stay pending, never use another revision's poster.
- `board_flow/result_actions.rs` and `board_web/presentation.rs`: artifact circles and closed-panel squares use the same ResultSelection/controller and existing shell album_result_strip; Shift ranges use immutable revision IDs. Image result primary clicks select without opening the action menu; secondary click retains image actions.
- `atlas-shell/home/album_strip.rs`: generator-only24-unit borderless squares,4-unit corners/gaps,eight-column running bond. Ordinary album/Cover Flow geometry stays with its prior owner.
- `atlas-shell/selection_tools/generator_activity.rs`: continuous shared presentation now has one-shot focus_index; exact text answer navigation validates session/checkpoint, maps raw turn index through filtered dialogue, and scrolls without provider/history mutation.
- `board_agent/session.rs`: consumes existing text answer navigation intent through the common adapter. Missing/mismatched source uses stored passage answer availability; never navigates to another response.
- `board_properties/selection.rs`: Text joins existing1500ms return memory, carrying picker/fold state. `palette_ui/apron.rs` measures Text/model content through shared layout.
- `hit_regions.rs` and `image_palette/outline.rs`: shared transient port center is1.427 outside at radius6; radial clearance1.4; shoulder40% indent radius. Graft worker owns matching dot/capsule/wire consumption; durable slot/t identities stay unchanged. Shared history circle diameter18 and center y10.

## Central compatibility and connector patches

Published `AgentPortalRef.generator: GeneratorSettings` with serde-default optional count/preserve and stable grafted slot IDs. None preserves legacy state. Schema and algorithms are now exclusively graft/history worker owned; that worker added cut/archive semantics. This worker only supplies UI setters and commands.

Registered `portal.agent.generator_settings`, `board.input.graft`, and human headless activate/generate/stop. Existing dispatch/source authority remains authoritative. Headless module/runtime/pump/mode guards are wired through the existing connector Session; exact implementation/admission/receipt ownership belongs to headless runtime worker. Headless sessions do not request visible native focus.

Dashboard valid Publish completion now inserts pending then calls existing guarded `accept_artifact_output`; undo, read-only/current-target checks and prior valid output retention stay with the transaction owner. Artifact pump ownership transferred to graft worker for queue integration.

## Validation and status

Rustfmt parsing completed successfully for changed batches. First app check found three shell visibility errors; fixed. Second app check found only prior TextNode.answers/remove_temp(Default) integration errors; fixed narrowly. Final current app check is owned by validation worker, requested against a frozen snapshot. No paid model/API test sessions launched.

Source-reviewed partial mappings: A01 shared owners; A03 provider-advertised parameter/key intent wiring; A04 existing durable access/grant separation retained; A05 existing1500ms timer extended; A06 shared continuous dialogue/focus routing; A08 real artifact image source/cache and shared selection/geometry; A09/A10 delegated semantic layer; A11 exact source reference navigation. These are not native acceptance claims.

Untested or incomplete: matching native captures/gesture evidence; real provider renewal and key row visibility in artifact Cursor flow; closed picker effort/speed labels and artifact-spawned Chat Train parity; width/fold/draft retention/save-reload/undo regressions; text square previews and Shift result selection; model prior-output revision catalog (current saved source only); unbound model durable preserve setting and direct headless prompt route; linked-file text answer parity; blank3D creation and installed licensed headless runtime proof. Review cache budget accounting and ordinary album regression tests after changed generator geometry; update obsolete click-toolbar test expectations only where design explicitly changed behavior. Final native contract registry check remains mandatory.


## Subsequent Text display batch

Text result browsing now stays separate from answered-passage navigation. `portal.agent.text_output` journals `GeneratorSettings.shown_text: TextOutputRef { source, response:{session,through} }`; browsing does not open/focus dialogue, rewind provider history, change a grant or replace authored note text. Both circles and borderless squares use shared ResultSelection and Shift ranges. Text slots use a content callback in the existing album_result_strip implementation and the same native canvas_text preview helper as the circular history. Selected result squares stay visible through shared selection state.

`generator_chrome/text_outputs.rs::resolved_selected_text_output(&self,id) -> Result<Option<String>,String>` is the explicit-source adapter for GR2: Ok(None) means no selected output; missing selected manifest/checkpoint is Err; there is no newest-response substitution. `displayed_text_output` wraps it for display only. Canonical `agent_reply` is unchanged. Existing AgentSources now exposes a read-only snapshot accessor; session pump read-only polls/retains selected source directories through its existing worker. Generated empty-note display consumes the shown reference; authored note text remains authoritative. Linked-file historical output display remains incomplete.

Focused tests added for selection persistence/roundtrip/canonical-latest separation and generator-only running-bond geometry; no execution claimed. Graft/history review found separate blockers in output browsing versus history_cut and cut versus queued requests; that frozen owner will repair them, not this worker. Check3 had one Vec2 import error, now qualified. Root authorized this worker as sole Cargo owner for check4 (`check --locked --release -p slate --lib --jobs 2`), with headless worker's short freeze; result pending under target/studio-native-validation/check-slate-4.*.


## Check4 and worker release checkpoint

`cargo check --locked --release -p slate --lib --jobs 2` completed exit0 in2m13s against the frozen shared/Text/Artifact and headless HR1 snapshot. Evidence: `target/studio-native-validation/check-slate-4.log`, `.exit.txt`, `.status.txt`. No tests or additional Cargo checks were run by this worker.

After that green check, two small follow-ups parse-formatted but not compiled: bound-model shared editor submit dispatches the ordinary `portal.agent.send` lifecycle (suppresses parent media generation and stale draft restoration); registered `board.model.headless.new` metadata for the headless worker's action implementation. Removed newly dormant shared-shell model_menu wrapper, unused GeneratorBody.running, obsolete narrow-width constant and unused fit/import paths; legacy option selection now consumes its existing field. Adjusted existing shell tests to the shared visible-history measurement. These dead paths followed the shared builder's consolidation; this is not unrelated warning cleanup. Pre-existing DESCRIBE_STYLE remains; prior Text worker text_answer_selection is newly unused and still needs actual selection hookup/linked-file parity review. Headless worker owns its unused revision provenance parameter.

Remaining UI blockers at release: Text linked-file historical preview and annotation parity; direct text passage selection hookup; displayed-text explicit wire snapshot mismatch (GR2 reviewer owns inputs.rs dependency using exact resolver); closed picker effort/speed/bolt presentation and artifact-spawned Chat Train picker parity; artifact Cursor Add/Change key row visibility; actual 3D prior-output catalog adapter (headless worker owns durable catalog/API, no copied posters); unbound model controls durable binding while headless worker implements blank/new source flow. Need context-menu graft control hookup/capsule cutout growth, exact native silhouette/hover/scroll/resize captures and title/width/fold/draft regression evidence. Current source-only geometry changes are not pixel acceptance.

Tests added, unrun: `board_agent::generator_chrome::tests::text_output_selection_is_persisted_display_state_without_rewinding_latest_reply`; `atlas_shell::home::album_strip::generator_geometry_tests::generator_running_bond_uses_reviewed_geometry_without_changing_albums`. Retain existing app result-strip and album tests, updating only expectations explicitly changed by reviewed preview click/running-bond design. Shell activity/attached menu tests, artifact acceptance/undo/source tests, text passage tests, contracts check and native interaction evidence still required. No new provider/paid runtime session was used for verification.