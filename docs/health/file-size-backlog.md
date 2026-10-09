# File-size backlog (god files)

Ordered by **exploration cost × change frequency** (`lines × git touches`, last ~60 days).
Split work is **behavior-preserving moves only**: one module per concern, named owner
(Art. XII), `cargo test --workspace` and clippy green, no public API churn beyond
`pub(crate)` re-exports.

Proposed modules for `board.rs`, `board_agent.rs`, and `tests.rs` are in
**Proposed seams** below (health run 2026-10-09, tree `eb3a207`). Line
numbers are that commit. Each module is a behavior-preserving move,
re-exported from the parent, and stays under 500 physical lines.

## Top ten — likely seams

| File | Lines | Seams (banner / module, line ranges) |
|------|------:|--------------------------------------|
| `apps/slate/src/app/tests.rs` | 28,167 | `app/tests/` slices in Proposed seams. One function is 606 lines and needs a helper extract first |
| `apps/slate/src/app/board_agent.rs` | 14,333 | existing submodules stay; parent body splits into `board_agent/*` in Proposed seams |
| `apps/slate/src/app/board.rs` | 10,743 | facade plus the `board_*` modules in Proposed seams. Crop paint calls `board_crop`; do not grow `board_place` |
| `apps/file-atlas/src/app/mod.rs` | 8,650 | UI section L5182; extract load/scan, tree, session, panels into `app/*` modules |
| `apps/slate/src/app/board_path.rs` | 7,174 | `board_path/tiles.rs` already split; extract stroke mesh, eraser, ink fit, commit paths |
| `crates/slate-doc/src/scene.rs` | 6,841 | geometry L29; style vocab L179; nodes L1264; connectors L2632; derived connector L2817; scene L3142; commands/journal L3463 |
| `apps/slate/src/app/board_properties.rs` | 5,457 | split by inspector facet (appearance, layout, portal, agent) |
| `crates/atlas-shell/src/dock.rs` | 5,336 | palette chrome vs dock layout vs interaction |
| `apps/slate/src/app/model3d.rs` | 5,077 | preview host vs material/lighting vs capture |
| `crates/atlas-shell/src/selection_tools.rs` | 4,693 | capsule menus vs property strips vs shared widgets |

## Proposed seams (2026-10-09, `eb3a207`)

Measured sizes: `board.rs` 10,742, `board_agent.rs` 14,332, `tests.rs` 28,166.
Ranges are inclusive. A slice is cut on a `fn` boundary (or on a `// ---`
banner) so the moved text is under 480 lines, leaving room for the `use`
block the new file will need. `board.rs` after the move is the preamble
(1–278) plus `mod` lines; re-measure it, and extract types if that facade
crosses 500. Same for `board_agent.rs` (the `mod` list at 29–34 stays).

Do not merge into an owner that is already near the cap. `board_crop.rs`
is pure math; crop pointer and overlay go to `board_crop_ui.rs` and call
it. `board_place.rs` is already about 509 lines; place-commit stays
`board_place_commit.rs` and calls `board_place`. Dry-review before any
new function. Three functions sit on the cap after a pure move
(`paint_board_node` 499, the eraser half of `update_gesture` 499,
`paint_agent_chat_header` 485). The builder stops rather than adding to them.

### `board.rs` → facade + modules

| Module | Lines | Concern |
|---|---|---|
| `board_tools.rs` | 279–754 | `FramePreset`, `BoardTool`: label, grammar, hotkey |
| `board_drag.rs` | 755–995 | `BoardDrag`, `BoardXf` camera math |
| `shape_text_layout.rs` | 996–1152 | lift the nested mod already at 996 |
| `board_view.rs` | 1153–1350 | camera, fit, tool arm, active recipe |
| `board_mutate.rs` | 1351–1648 | journaled patch, add, delete (banner at 1351) |
| `board_history.rs` | 1649–2132 | undo/redo, duplicate, place items into frames |
| `board_textures.rs` | 2133–2453 | texture lookup, selection glyphs (banner at 2130) |
| `board_outline.rs` | 2454–2880 | `node_screen_outline` through the centroid-clip comment |
| `board_outline_clip.rs` | 2881–3092 | clipped triangles. The "outline geometry" banner at 2509 sits inside `node_screen_outline`; the clip comment is the seam |
| `board_cards.rs` | 3098–3501 | snippet card and sheet card |
| `board_model_view.rs` | 3502–3678 | model viewport paint |
| `board_sticky.rs` | 3679–3867 | sticky fit and image-draft painter |
| `board_node_paint.rs` | 3868–4366 | `paint_board_node` (499 lines — do not grow) |
| `board_input.rs` | 4369–4770 | `board_canvas` head: wheel ownership, camera, pan, zoom (banners at 4448, 4979, 5363) |
| `board_input_place.rs` | 4771–4978 | DragRect, deck, corner grip, crosstalk port, measure |
| `board_input_gesture.rs` | 4979–5362 | gesture start / update / end, clicks, cursors |
| `board_paint_scene.rs` | 5363–5644 | scene paint, cull, selection adornment |
| `board_paint_preview.rs` | 5645–6032 | guides, rubber bands, brush, wire, minimap |
| `board_model_hud.rs` | 6033–6226 | model status, Enscape window, measurements |
| `board_crop_ui.rs` | 6227–6559 | crop pointer and overlay. Calls `board_crop`; does not join it |
| `board_gesture.rs` | 6562–6961 | `begin_gesture`, pick, `update_gesture` until the eraser comment at 6962 |
| `board_gesture_eraser.rs` | 6962–7460 | eraser scrub through the end of `update_gesture` (499 — do not grow) |
| `board_gesture_end.rs` | 7461–7775 | `end_gesture` |
| `board_place_commit.rs` | 7776–8082 | draw-rect resolve and `place_*`. Calls `board_place` |
| `board_text_place.rs` | 8083–8463 | text drafts, `finish_draw`, portal builders |
| `board_pick.rs` | 8464–8757 | click and double-click |
| `board_sheet.rs` | 8761–9262 | frame dialog and sheet edit, through the resize comment at 9263 |
| `board_sheet_resize.rs` | 9263–9430 | sheet resize grips |
| `board_text_overlay.rs` | 9431–9843 | text compose overlays |
| `board_menu.rs` | 9844–10251 | action menu (banner "overlays" at 8758, "dialogs" at 10289) |
| `board_dialogs.rs` | 10252–10355 | z-order, add-to-frame, export |
| `board_tests.rs` | 10356–10742 | the `mod tests` at the bottom of `board.rs` |

`board_canvas` (4369–6032, 1,664 lines) is one function. The four input/paint
modules are extracted phases it calls. That extract is the hard card
(gesture state crosses the function). The banner moves above it are easy.

### `board_agent.rs` → modules under `board_agent/`

`crosstalk`, `life`, `outputs`, `schedule`, and `train_ux` stay. The parent
file's body:

| Module | Lines | Concern |
|---|---|---|
| `runtime.rs` | 36–513 | `LivePreview`, `InputRole`, `GeneratorView`, `PublishClips`, `AgentRuntime` |
| `card_metrics.rs` | 514–956 | handle dot, responding label, composer height |
| `card_fit.rs` | 958–1335 | fit cards to the transcript |
| `spawn.rs` | 1336–1782 | spawn command and spawn-input hit targets |
| `spawn_chrome.rs` | 1783–2154 | spawn preview, rename, collapse, full-access grant |
| `train.rs` | 2155–2650 | projection, rechunk, retarget wires |
| `bundle.rs` | 2651–3115 | bundle, history rails, summary |
| `composer.rs` | 3116–3509 | collapsed composer and context blurbs |
| `artifacts.rs` | 3510–3991 | artifact paint and hit-test |
| `artifacts_open.rs` | 3992–4285 | open, build, provenance wires |
| `context.rs` | 4286–4637 | pocket, retract, fork |
| `programs.rs` | 4638–4863 | program grid and binding |
| `generator.rs` | 4864–5095 | generator view and request |
| `generator_pump.rs` | 5096–5476 | Comfy queue, live pump, steer |
| `inputs.rs` | 5477–5763 | input snapshots and export |
| `results.rs` | 5764–6239 | place results, stop, model menu |
| `picture.rs` | 6240–6557 | generation preview and agent picture |
| `session.rs` | 6558–6878 | `agent_pump`, session pump |
| `prompt.rs` | 6879–7367 | send prompt, sidecar boot |
| `awaits.rs` | 7368–7701 | awaits, provider launch, atlas place |
| `stage.rs` | 7702–7878 | accept and reject proposals |
| `focus.rs` | 7879–8294 | wheel capture, focus, bind project |
| `portal_unbound.rs` | 8295–8443 | unbound portal paint |
| `portal_picker.rs` | 8444–8753 | project and chat pick list |
| `portal_header.rs` | 8754–9238 | `paint_agent_chat_header` (485 lines, one function — do not grow) |
| `portal_bound.rs` | 9239–9683 | collapse toggle and bound-card paint |
| `portal_composer.rs` | 9684–10076 | key entry, composer, stop, channel load |
| `models.rs` | 10077–10279 | model list and live toggle |
| `connection.rs` | 10280–10532 | connection pump, fork payload, chat pump |
| `cursor_ide.rs` | 10533–10765 | Cursor IDE pump (`pump_cursor_ide` is 234 lines) |
| `await_tests.rs` | 10821–11295 | generator, provider, live-frame tests |
| `train_tests.rs` | 11296–11754 | hover chips, trains, bundles |
| `history_tests.rs` | 11755–12202 | history cache, failures, harness `board` |
| `fork_tests.rs` | 12203–12678 | fork, collapse, streaming tail |
| `picker_tests.rs` | 12679–13146 | resize, pocket, model-list zoom |
| `stop_tests.rs` | 13147–13606 | picker wheel, stop square |
| `present_tests.rs` | 13607–14049 | stop folder, presentation switches |
| `draft_tests.rs` | 14050–14332 | drafts across presentation switches |

`ProgramBinding` (10766–10820, 55 lines) stays on the `board_agent.rs` facade.

### `tests.rs` → `apps/slate/src/app/tests/`

Banner sections already under 500 stay one module. Larger banners are cut
on a `fn` boundary. The region 9046–18697 has banners only at its start
(open/closed trim) and then interleaves brush, eraser, sheet, and image
paint; those slices stay in source order so helpers do not have to move
twice. A later card may regroup them by name.

| Module | Lines | Concern |
|---|---|---|
| `harness.rs` | 1–440 | clock, drops, media menu, pdf fixtures |
| `unbundle.rs` | 441–917 | unbundle and asset collect |
| `app_smoke.rs` | 918–1220 | invariants, tabs, save/reopen |
| `board_canvas.rs` | 1221–1449 | authored canvas (banner at 1221) |
| `media_guards.rs` | 1450–1740 | workbook guards, video trim (banner at 1450) |
| `previews.rs` | 1741–1872 | lazy full-resolution previews |
| `keymap.rs` | 1873–2331 | keymap wave 2b, strokes and sticky |
| `text_draft.rs` | 2332–2771 | text-box draft |
| `line_gp.rs` | 2772–3237 | line golden paths |
| `line_hit.rs` | 3238–3369 | closed-polyline pick |
| `osnap.rs` | 3370–3606 | object snaps |
| `kits.rs` | 3607–4082 | kit recipes and deck prefix |
| `kits_atlas.rs` | 4083–4478 | deck skip and atlas lens focus |
| `arming.rs` | 4479–4929 | tool-arming preview |
| `web_portal.rs` | 4930–5400 | web portal golden paths (banner at 4930) |
| `web_agent.rs` | 5401–5824 | agent send failures inside the web section |
| `web_keys.rs` | 5825–6221 | maximize, escape, file walk |
| `page_focus.rs` | 6222–6697 | focused page (banner at 6222) |
| `wire_grips.rs` | 6698–7157 | wire grips and edge scale |
| `bbox_chrome.rs` | 7158–7578 | live bbox chrome, portals do not rotate |
| `align.rs` | 7579–8058 | align widget golden paths |
| `align_rotate.rs` | 8059–8154 | rotated resize and the group-box corner |
| `trim.rs` | 8155–8593 | trim golden paths |
| `join.rs` | 8594–8891 | join golden paths |
| `split_gp.rs` | 8892–9045 | split golden paths |
| `ink_01.rs` … `ink_21.rs` | 9046–18697 | source-order slices, each ≤478 lines: 9046–9516, 9517–9975, 9976–10449, 10450–10927, 10928–11402, 11403–11870, 11871–12340, 12341–12758, 12759–13202, 13203–13676, 13677–14123, 14124–14554, 14555–15013, 15014–15462, 15463–15924, 15925–16388, 16389–16832, 16833–17309, 17310–17775, 17776–18253, 18254–18697 |
| `bezier.rs` | 18698–19177 | Bézier span drafting |
| `curve_grips.rs` | 19178–19483 | parametric grips |
| `bezier_close.rs` | 19484–19619 | close on the start anchor |
| `crop.rs` | 19620–20004 | image crop |
| `crop_scale.rs` | 20005–20459 | corner scale and neighbour edges |
| `crop_copy.rs` | 20460–20928 | copied picture, eraser preview color |
| `crop_vertex.rs` | 20929–21276 | vertex drag, alt-copy |
| `vertex_width.rs` | 21277–21638 | per-vertex stroke width |
| `vertex_color.rs` | 21639–21845 | per-vertex stroke color |
| `vertex_style.rs` | 21846–22303 | style survives trim, split, join |
| `vertex_join.rs` | 22304–22773 | object join and seam merge |
| `vertex_arrow.rs` | 22774–23249 | arrow head, opacity |
| `vertex_texture.rs` | 23250–23375 | texture restamp |
| `tip_hud.rs` | 23376–23806 | whole-curve tip HUD |
| `tip_anchor.rs` | 23807–24278 | anchor delete and command entry |
| `tip_scrub.rs` | 24279–24756 | HUD scrub |
| `tip_handle.rs` | 24757–25236 | handle drag |
| `tip_direct.rs` | 25237–25460 | direct-select modifiers |
| `subobject.rs` | 25461–25895 | sub-object edges |
| `curve_style_frames.rs` | 25896–26182 | `curve_style_visual_frames` |
| `visual_frames.rs` | 26183–26458 | `visual_verification_frames` |
| `wire_drop.rs` | 26459–27064 | **not a pure move.** `a_wire_dropped_on_empty_board_offers_an_agent_chat_train` is 606 lines. Extract helpers until the test file is under 500, then move |
| `closed_form.rs` | 27065–27510 | vertex picks on closed forms |
| `closed_form_strip.rs` | 27511–27966 | strip buttons over corners |
| `closed_form_line.rs` | 27967–28166 | line end point under a strip button |

## Waves (one agent per file)

**Gate — Slate board core:** split `board.rs` and `board_agent.rs` only when no
feature branch is touching them (coordinate on `main`).

| Wave | Files (exclusive) | Notes |
|------|-------------------|-------|
| **A** | `tests.rs` | Single agent; group by banner / module under test |
| **B** | `board_path.rs`, `board_path/tiles.rs` | One agent owns path stack |
| **C** | `board_properties.rs`, `board_color.rs`, `board_snap.rs` | Property/snap cluster |
| **D** | `apps/slate/src/app/mod.rs`, `commands.rs`, `dispatch.rs` | App integration shell |
| **E** | `scene.rs`, `wire.rs`, `agent_inputs.rs`, `geom.rs` | `slate-doc` model |
| **F** | `apps/file-atlas/src/app/mod.rs`, file-atlas tests | Atlas app |
| **G** | `atlas-shell`: `dock.rs`, `selection_tools.rs`, `tokens.rs`, `home.rs`, … | Chrome owners |
| **H** | `slate-artifact`: `render.rs`, `lib.rs` | Export interpreter |
| **I** | Remaining >500-line files (vector-ink, atlas-core, xtask, …) | Lowest score first within wave |

## Full inventory

| Path | Lines | Owner crate | 60d touches | Score |
|------|------:|-------------|-------------|------:|
| `apps/slate/src/app/tests.rs` | 28167 | apps/slate | 169 | 4760223 |
| `apps/slate/src/app/board.rs` | 10743 | apps/slate | 161 | 1729623 |
| `apps/slate/src/app/board_agent.rs` | 14333 | apps/slate | 72 | 1031976 |
| `apps/slate/src/app/board_path.rs` | 7174 | apps/slate | 77 | 552398 |
| `crates/slate-doc/src/scene.rs` | 6841 | crates/slate-doc | 74 | 506234 |
| `apps/slate/src/app/board_properties.rs` | 5457 | apps/slate | 75 | 409275 |
| `apps/file-atlas/src/app/mod.rs` | 8650 | apps/file-atlas | 41 | 354650 |
| `apps/slate/src/app/mod.rs` | 2888 | apps/slate | 102 | 294576 |
| `apps/slate/src/app/board_color.rs` | 3328 | apps/slate | 61 | 203008 |
| `apps/slate/src/app/commands.rs` | 2462 | apps/slate | 76 | 187112 |
| `crates/slate-artifact/src/render.rs` | 3282 | crates/slate-artifact | 54 | 177228 |
| `apps/slate/src/app/model3d.rs` | 5077 | apps/slate | 30 | 152310 |
| `apps/slate/src/app/board_web.rs` | 4459 | apps/slate | 28 | 124852 |
| `crates/atlas-shell/src/selection_tools.rs` | 4693 | crates/atlas-shell | 24 | 112632 |
| `apps/slate/src/app/dispatch.rs` | 1979 | apps/slate | 54 | 106866 |
| `crates/atlas-shell/src/dock.rs` | 5336 | crates/atlas-shell | 18 | 96048 |
| `crates/slate-artifact/src/lib.rs` | 1944 | crates/slate-artifact | 40 | 77760 |
| `apps/slate/src/app/board_flow.rs` | 2808 | apps/slate | 24 | 67392 |
| `apps/slate/src/app/board_path/tiles.rs` | 2365 | apps/slate | 27 | 63855 |
| `crates/atlas-shell/src/tokens.rs` | 3047 | crates/atlas-shell | 20 | 60940 |
| `crates/slate-doc/src/wire.rs` | 2497 | crates/slate-doc | 21 | 52437 |
| `apps/slate/src/app/board_direct.rs` | 1874 | apps/slate | 27 | 50598 |
| `apps/slate/src/app/board_atlas.rs` | 2474 | apps/slate | 20 | 49480 |
| `apps/slate/src/app/board_wire.rs` | 1595 | apps/slate | 26 | 41470 |
| `crates/slate-doc/src/agent_inputs.rs` | 2159 | crates/slate-doc | 18 | 38862 |
| `apps/slate/src/app/clipboard.rs` | 1427 | apps/slate | 25 | 35675 |
| `crates/slate-doc/src/geom.rs` | 1768 | crates/slate-doc | 20 | 35360 |
| `apps/slate/src/app/board_agent/crosstalk.rs` | 3482 | apps/slate | 10 | 34820 |
| `apps/slate/src/app/board_image_layers.rs` | 1258 | apps/slate | 26 | 32708 |
| `crates/atlas-shell/src/home.rs` | 2616 | crates/atlas-shell | 12 | 31392 |
| `apps/slate/src/app/board_snap.rs` | 2094 | apps/slate | 14 | 29316 |
| `crates/slate-doc/src/vertex_style.rs` | 1878 | crates/slate-doc | 15 | 28170 |
| `apps/slate/src/app/board_transform.rs` | 953 | apps/slate | 28 | 26684 |
| `apps/file-atlas/src/app/tests.rs` | 1717 | apps/file-atlas | 15 | 25755 |
| `apps/slate/src/app/ui/tools.rs` | 1017 | apps/slate | 25 | 25425 |
| `apps/slate/src/app/board_web_win.rs` | 2492 | apps/slate | 10 | 24920 |
| `apps/slate/src/app/board_portal_chrome.rs` | 1236 | apps/slate | 20 | 24720 |
| `apps/slate/src/app/model_screenshot.rs` | 1339 | apps/slate | 18 | 24102 |
| `apps/slate/src/app/bench_brush_tiles.rs` | 1091 | apps/slate | 18 | 19638 |
| `crates/vector-ink/src/stamp.rs` | 1693 | crates/vector-ink | 11 | 18623 |
| `crates/atlas-shell/src/tuning.rs` | 1827 | crates/atlas-shell | 10 | 18270 |
| `crates/atlas-core/src/thumbs.rs` | 1350 | crates/atlas-core | 13 | 17550 |
| `apps/slate/src/app/board_handles.rs` | 910 | apps/slate | 19 | 17290 |
| `apps/slate/src/app/board_tip_hud.rs` | 1279 | apps/slate | 13 | 16627 |
| `apps/file-atlas/src/app/ui/tools.rs` | 1007 | apps/file-atlas | 16 | 16112 |
| `crates/atlas-shell/src/widgets.rs` | 1060 | crates/atlas-shell | 15 | 15900 |
| `crates/vector-ink/src/stroke.rs` | 1196 | crates/vector-ink | 13 | 15548 |
| `apps/slate/src/app/board_trim.rs` | 1163 | apps/slate | 12 | 13956 |
| `crates/atlas-shell/src/tabs.rs` | 901 | crates/atlas-shell | 15 | 13515 |
| `apps/file-atlas/src/app/commands.rs` | 825 | apps/file-atlas | 16 | 13200 |
| `apps/slate/src/app/board_osnap.rs` | 941 | apps/slate | 13 | 12233 |
| `crates/slate-doc/src/doc.rs` | 840 | crates/slate-doc | 14 | 11760 |
| `crates/slate-artifact/src/paint_layers.rs` | 808 | crates/slate-artifact | 14 | 11312 |
| `crates/atlas-agent/src/lib.rs` | 1402 | crates/atlas-agent | 8 | 11216 |
| `apps/slate/src/app/board_slate.rs` | 891 | apps/slate | 12 | 10692 |
| `crates/atlas-ai/src/sidecar.rs` | 960 | crates/atlas-ai | 11 | 10560 |
| `crates/atlas-ai/src/agent.rs` | 971 | crates/atlas-ai | 10 | 9710 |
| `apps/slate/src/app/board_agent/life.rs` | 1483 | apps/slate | 6 | 8898 |
| `crates/vector-ink/src/mesh.rs` | 680 | crates/vector-ink | 13 | 8840 |
| `crates/slate-doc/tests/migration.rs` | 623 | crates/slate-doc | 14 | 8722 |
| `apps/slate/src/app/board_agent_outputs.rs` | 1734 | apps/slate | 5 | 8670 |
| `apps/slate/src/app/bench_brush.rs` | 574 | apps/slate | 15 | 8610 |
| `apps/slate/src/app/pdf.rs` | 772 | apps/slate | 11 | 8492 |
| `crates/atlas-shell/src/canvas_text.rs` | 1368 | crates/atlas-shell | 6 | 8208 |
| `apps/slate/src/app/overlays.rs` | 670 | apps/slate | 12 | 8040 |
| `apps/slate/src/app/tests_tip_chord.rs` | 1307 | apps/slate | 6 | 7842 |
| `crates/atlas-shell/src/sidebar.rs` | 775 | crates/atlas-shell | 10 | 7750 |
| `crates/slate-doc/src/wire_host.rs` | 860 | crates/slate-doc | 9 | 7740 |
| `crates/atlas-codex/src/lib.rs` | 1100 | crates/atlas-codex | 7 | 7700 |
| `crates/atlas-core/src/tree.rs` | 1252 | crates/atlas-core | 6 | 7512 |
| `crates/vector-ink/src/trim.rs` | 1216 | crates/vector-ink | 6 | 7296 |
| `crates/slate-kit/src/recipe.rs` | 639 | crates/slate-kit | 11 | 7029 |
| `crates/slate-doc/src/stage.rs` | 978 | crates/slate-doc | 7 | 6846 |
| `crates/slate-doc/src/agent_chat.rs` | 784 | crates/slate-doc | 8 | 6272 |
| `crates/slate-doc/src/osnap.rs` | 992 | crates/slate-doc | 6 | 5952 |
| `crates/atlas-comfy/src/lib.rs` | 1898 | crates/atlas-comfy | 3 | 5694 |
| `apps/slate/src/app/tests_ink_fit.rs` | 805 | apps/slate | 7 | 5635 |
| `apps/slate/src/app/board_place.rs` | 509 | apps/slate | 11 | 5599 |
| `crates/atlas-shell/src/dock_advanced.rs` | 1351 | crates/atlas-shell | 4 | 5404 |
| `crates/atlas-shell/src/desktop_color.rs` | 671 | crates/atlas-shell | 8 | 5368 |
| `crates/slate-artifact/src/assets.rs` | 618 | crates/slate-artifact | 8 | 4944 |
| `crates/atlas-shell/src/folder_map/paint.rs` | 1222 | crates/atlas-shell | 4 | 4888 |
| `crates/atlas-shell/src/menu.rs` | 959 | crates/atlas-shell | 5 | 4795 |
| `crates/atlas-ai/src/runtime.rs` | 682 | crates/atlas-ai | 7 | 4774 |
| `apps/slate/src/app/imagefx.rs` | 552 | apps/slate | 8 | 4416 |
| `crates/atlas-shell/src/covers.rs` | 620 | crates/atlas-shell | 7 | 4340 |
| `crates/atlas-shell/src/menubar.rs` | 696 | crates/atlas-shell | 6 | 4176 |
| `crates/vector-ink/src/edit.rs` | 949 | crates/vector-ink | 4 | 3796 |
| `crates/atlas-core/src/types.rs` | 745 | crates/atlas-core | 5 | 3725 |
| `apps/slate/src/app/bench_web.rs` | 908 | apps/slate | 4 | 3632 |
| `crates/atlas-shell/src/file_picker.rs` | 1177 | crates/atlas-shell | 3 | 3531 |
| `crates/atlas-shell/examples/shape_palettes.rs` | 560 | crates/atlas-shell | 6 | 3360 |
| `crates/atlas-core/src/session_log.rs` | 1098 | crates/atlas-core | 3 | 3294 |
| `crates/atlas-shell/src/timeline.rs` | 1582 | crates/atlas-shell | 2 | 3164 |
| `crates/atlas-ai/src/outputs.rs` | 764 | crates/atlas-ai | 4 | 3056 |
| `crates/atlas-core/src/workbook_assets.rs` | 925 | crates/atlas-core | 3 | 2775 |
| `crates/slate-doc/src/bumper.rs` | 688 | crates/slate-doc | 4 | 2752 |
| `crates/atlas-core/src/video.rs` | 890 | crates/atlas-core | 3 | 2670 |
| `crates/atlas-core/src/office/text.rs` | 1332 | crates/atlas-core | 2 | 2664 |
| `crates/slate-doc/src/crosstalk.rs` | 884 | crates/slate-doc | 3 | 2652 |
| `crates/atlas-core/src/rasterthumb.rs` | 627 | crates/atlas-core | 4 | 2508 |
| `crates/code-lens/src/layout.rs` | 1151 | crates/code-lens | 2 | 2302 |
| `crates/atlas-shell/src/feedback/ui.rs` | 558 | crates/atlas-shell | 4 | 2232 |
| `crates/code-lens/src/model.rs` | 555 | crates/code-lens | 4 | 2220 |
| `apps/slate/src/app/image_layer_tests.rs` | 635 | apps/slate | 3 | 1905 |
| `crates/atlas-ai/src/schedule.rs` | 616 | crates/atlas-ai | 3 | 1848 |
| `apps/slate/src/app/board_web_stills.rs` | 587 | apps/slate | 3 | 1761 |
| `xtask/src/contracts.rs` | 857 | xtask | 2 | 1714 |
| `apps/slate/src/app/enscape_host.rs` | 562 | apps/slate | 3 | 1686 |
| `apps/slate/src/app/bench_model3d.rs` | 798 | apps/slate | 2 | 1596 |
| `crates/vector-ink/src/bspline.rs` | 715 | crates/vector-ink | 2 | 1430 |
| `apps/slate/src/app/board_align.rs` | 713 | apps/slate | 2 | 1426 |
| `crates/atlas-openai/src/lib.rs` | 594 | crates/atlas-openai | 2 | 1188 |
| `crates/atlas-core/src/office/outlook.rs` | 561 | crates/atlas-core | 2 | 1122 |
| `apps/slate/src/app/board_video.rs` | 803 | apps/slate | 1 | 803 |
| `crates/circle-pack/src/venn.rs` | 642 | crates/circle-pack | 1 | 642 |
| `crates/collage/tests/layout.rs` | 598 | crates/collage | 1 | 598 |
| `crates/slate-kit/src/resolve.rs` | 557 | crates/slate-kit | 1 | 557 |
| `crates/vector-ink/src/collide.rs` | 522 | crates/vector-ink | 1 | 522 |
