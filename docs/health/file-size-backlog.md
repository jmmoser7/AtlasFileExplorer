# File-size backlog (god files)

Ordered by **exploration cost × change frequency** (`lines × git touches`, last ~60 days).
Split work is **behavior-preserving moves only**: one module per concern, named owner
(Art. XII), `cargo test --workspace` and clippy green, no public API churn beyond
`pub(crate)` re-exports.

> **Health-check seams (in progress):** branch `health/2026-10-09` report
> `docs/health/reports/2026-10-09.md` will propose detailed seams for
> `board.rs`, `board_agent.rs`, and `tests.rs` — merge those rows here when it lands.

## Top ten — likely seams

| File | Lines | Seams (banner / module, line ranges) |
|------|------:|--------------------------------------|
| `apps/slate/src/app/tests.rs` | 28,167 | board canvas L1221; media/guards L1450; lazy previews L1741; keymap wave 2b L1873; line GP L2772; object snap L3370; kits L3607; tool arming L4479; web portal L4930; align L7579; trim/join/split L8155–9046; Bézier L18698+ |
| `apps/slate/src/app/board_agent.rs` | 14,333 | submodules `crosstalk`, `life`, `outputs`, `schedule`, `train_ux` (L29–34); remainder = session pump, card paint, generator (await health report) |
| `apps/slate/src/app/board.rs` | 10,743 | tools & gestures L279; state helpers L1153; outline geometry L2509; painting L3093; nested `shape_text_layout` mod L996 |
| `apps/file-atlas/src/app/mod.rs` | 8,650 | UI section L5182; extract load/scan, tree, session, panels into `app/*` modules |
| `apps/slate/src/app/board_path.rs` | 7,174 | `board_path/tiles.rs` already split; extract stroke mesh, eraser, ink fit, commit paths |
| `crates/slate-doc/src/scene.rs` | 6,841 | geometry L29; style vocab L179; nodes L1264; connectors L2632; derived connector L2817; scene L3142; commands/journal L3463 |
| `apps/slate/src/app/board_properties.rs` | 5,457 | split by inspector facet (appearance, layout, portal, agent) |
| `crates/atlas-shell/src/dock.rs` | 5,336 | palette chrome vs dock layout vs interaction |
| `apps/slate/src/app/model3d.rs` | 5,077 | preview host vs material/lighting vs capture |
| `crates/atlas-shell/src/selection_tools.rs` | 4,693 | capsule menus vs property strips vs shared widgets |

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
