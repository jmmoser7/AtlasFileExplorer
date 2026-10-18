use super::*;

/// One recipe for the docked flyout and a canvas `DockStrip`. `visible`
/// empty means the full default set; otherwise it is the drop snapshot.
pub(crate) fn palette_strip_items<'a>(
    app: &'a SlateApp,
    palette_id: &str,
    visible: &[String],
) -> Vec<FlyoutItem<'a>> {
    let mut items = match palette_id {
        "tool.media" => vec![
            FlyoutItem { id: "media.image", label: "Image", description: "Images and print media: JPG, PNG, PDF, and PowerPoint.", hotkey: None, icon: DockIcon::Custom(icon_image), active: false, group: Some("media"), role: FlyoutRole::Icon },
            FlyoutItem { id: "media.model", label: "3D", description: "Place a 3D model. OBJ, STL, glTF, and Rhino preview in the viewport. Blender, DWG, and SketchUp use the same card.", hotkey: None, icon: DockIcon::Custom(icon_model), active: false, group: Some("media"), role: FlyoutRole::Icon },
            FlyoutItem { id: "media.video", label: "Video", description: "Place a video. Hover across it to scrub the whole clip; click to play. mp4, mov, webm, and other types the system can open.", hotkey: None, icon: DockIcon::Custom(icon_video), active: false, group: Some("media"), role: FlyoutRole::Icon },
            FlyoutItem { id: "media.text", label: "Text", description: "Word, Excel, CSV, spreadsheets, and source code. Shows an excerpt when the file can be read.", hotkey: None, icon: DockIcon::Custom(icon_text_doc), active: false, group: Some("media"), role: FlyoutRole::Icon },
        ],
        "tool.frame" => {
            let preset = app.board_frame_preset;
            vec![
                FlyoutItem {
                    id: "frame.letter",
                    label: FramePreset::Letter.label(),
                    description: "Letter slide frame (8.5 × 11).",
                    hotkey: None,
                    icon: DockIcon::Custom(icon_frame_letter),
                    active: preset == FramePreset::Letter,
                    group: Some("frame"),
                    role: FlyoutRole::Icon,
                },
                FlyoutItem {
                    id: "frame.tabloid",
                    label: FramePreset::Tabloid.label(),
                    description: "Tabloid slide frame, landscape (17 × 11).",
                    hotkey: None,
                    icon: DockIcon::Custom(icon_frame_tabloid),
                    active: preset == FramePreset::Tabloid,
                    group: Some("frame"),
                    role: FlyoutRole::Icon,
                },
                FlyoutItem {
                    id: "frame.wide",
                    label: FramePreset::Wide169.label(),
                    description: "16:9 slide frame.",
                    hotkey: None,
                    icon: DockIcon::Custom(icon_frame_wide),
                    active: preset == FramePreset::Wide169,
                    group: Some("frame"),
                    role: FlyoutRole::Icon,
                },
                FlyoutItem {
                    id: "frame.custom",
                    label: "Custom…",
                    description: "Type a custom frame size.",
                    hotkey: None,
                    icon: DockIcon::Custom(icon_frame_custom),
                    active: matches!(preset, FramePreset::Custom { .. }),
                    group: Some("frame"),
                    role: FlyoutRole::Icon,
                },
            ]
        }
        "tool.portals" => {
            let tool = app.board_tool;
            let mut items = vec![
                FlyoutItem {
                    id: "portal.agent",
                    label: "Agent portal",
                    description: "Host portal for a local Cursor agent session.",
                    hotkey: None,
                    icon: DockIcon::Custom(icon_agent),
                    active: tool == BoardTool::AgentPortal && app.armed_kit_id.is_none(),
                    group: Some("portals"),
                    role: FlyoutRole::Icon,
                },
                FlyoutItem {
                    id: "portal.web",
                    label: "Web portal",
                    description: "Host a URL or local HTML dashboard on the board.",
                    hotkey: None,
                    icon: DockIcon::Custom(icon_web),
                    active: tool == BoardTool::WebPortal && app.armed_kit_id.is_none(),
                    group: Some("portals"),
                    role: FlyoutRole::Icon,
                },
                FlyoutItem {
                    id: "portal.atlas",
                    label: "File Atlas",
                    description:
                        "Live folder map on the board — File Atlas's canvas, not its window.",
                    hotkey: None,
                    icon: DockIcon::Custom(icon_atlas),
                    active: tool == BoardTool::AtlasPortal && app.armed_kit_id.is_none(),
                    group: Some("portals"),
                    role: FlyoutRole::Icon,
                },
                FlyoutItem {
                    id: "portal.slate",
                    label: "Slate board",
                    description: "Load another workbook's board inside this frame.",
                    hotkey: None,
                    icon: DockIcon::Custom(icon_frame),
                    active: tool == BoardTool::SlatePortal && app.armed_kit_id.is_none(),
                    group: Some("portals"),
                    role: FlyoutRole::Icon,
                },
            ];
            for extra in app.kits.derived_in_group("portals") {
                items.push(FlyoutItem {
                    id: extra.id,
                    label: crate::app::kits::intern_label(&extra.name),
                    description: crate::app::kits::intern_label(&extra.description),
                    hotkey: None,
                    icon: kit_dock_icon(app, extra.id),
                    active: app.armed_kit_id.as_deref() == Some(extra.id),
                    group: Some(extra.group),
                    role: FlyoutRole::Icon,
                });
            }
            items
        }
        "tool.shapes" => {
            let tools = [
                (
                    BoardTool::RectShape,
                    "shape.rect",
                    Some(BoardTool::RectShape.hotkey()),
                    "shapes",
                ),
                (
                    BoardTool::Ellipse,
                    "shape.ellipse",
                    Some(BoardTool::Ellipse.hotkey()),
                    "shapes",
                ),
                (
                    BoardTool::Polygon,
                    "shape.polygon",
                    Some(BoardTool::Polygon.hotkey()),
                    "shapes",
                ),
                (
                    BoardTool::Line,
                    "shape.line",
                    Some(BoardTool::Line.hotkey()),
                    "curves",
                ),
                (BoardTool::Arc, "shape.arc", None, "curves"),
                (BoardTool::Polyline, "shape.polyline", None, "curves"),
                (BoardTool::BezierSpan, "shape.bezier", None, "curves"),
                (BoardTool::Pen, "shape.pen", Some("P"), "curves"),
                (BoardTool::Brush, "shape.brush", Some("B"), "ink"),
                (BoardTool::Eraser, "shape.eraser", Some("E"), "ink"),
                (BoardTool::Smooth, "shape.smooth", Some("S"), "ink"),
            ];
            let mut items: Vec<FlyoutItem<'_>> = tools
                .into_iter()
                .map(|(tool, id, hotkey, group)| FlyoutItem {
                    id,
                    label: tool.label(),
                    description: tool_flyout_desc(tool),
                    hotkey,
                    icon: tool_dock_icon(tool),
                    active: app.board_tool == tool,
                    group: Some(group),
                    role: FlyoutRole::Icon,
                })
                .collect();
            for extra in app.kits.derived_in_group("shapes") {
                items.push(FlyoutItem {
                    id: extra.id,
                    label: crate::app::kits::intern_label(&extra.name),
                    description: crate::app::kits::intern_label(&extra.description),
                    hotkey: None,
                    icon: kit_dock_icon(app, extra.id),
                    active: app.armed_kit_id.as_deref() == Some(extra.id),
                    group: Some(extra.group),
                    role: FlyoutRole::Icon,
                });
            }
            items
        }
        "tool.text" => {
            let tool = app.board_tool;
            vec![
                FlyoutItem {
                    id: "text.block",
                    label: "Text",
                    description: "Place a text block on the board.",
                    hotkey: Some("T"),
                    icon: DockIcon::Custom(icon_text),
                    active: tool == BoardTool::Text,
                    group: Some("text"),
                    role: FlyoutRole::Icon,
                },
                FlyoutItem {
                    id: "text.sticky",
                    label: "Sticky note",
                    description: "Place a sticky note on the board.",
                    hotkey: Some("N"),
                    icon: DockIcon::Custom(icon_sticky),
                    active: tool == BoardTool::Sticky,
                    group: Some("text"),
                    role: FlyoutRole::Icon,
                },
            ]
        }
        "tool.actions" => vec![
            FlyoutItem {
                id: "action.trim",
                label: "Trim",
                description: "Pick cutters, then click the part to delete.",
                hotkey: Some("Ctrl+T"),
                icon: DockIcon::Custom(icon_trim),
                active: app.board_tool == BoardTool::Trim,
                group: Some("actions"),
                role: FlyoutRole::Icon,
            },
            FlyoutItem {
                id: "action.split",
                label: "Split",
                description: "Pick cutters, then click an object to keep every piece.",
                hotkey: Some("Ctrl+Shift+T"),
                icon: DockIcon::Custom(icon_split),
                active: app.board_tool == BoardTool::Split,
                group: Some("actions"),
                role: FlyoutRole::Icon,
            },
            FlyoutItem {
                id: "action.join",
                label: "Join",
                description: "Join open paths, or union closed regions.",
                hotkey: Some("Ctrl+J"),
                icon: DockIcon::Custom(icon_join),
                active: false,
                group: Some("actions"),
                role: FlyoutRole::Icon,
            },
            FlyoutItem {
                id: "action.mirror_h",
                label: "Mirror horizontal",
                description: "Flip selected pictures, paths, and lines left to right.",
                hotkey: None,
                icon: DockIcon::Custom(icon_mirror_h),
                active: false,
                group: Some("actions"),
                role: FlyoutRole::Icon,
            },
            FlyoutItem {
                id: "action.mirror_v",
                label: "Mirror vertical",
                description: "Flip selected pictures, paths, and lines top to bottom.",
                hotkey: None,
                icon: DockIcon::Custom(icon_mirror_v),
                active: false,
                group: Some("actions"),
                role: FlyoutRole::Icon,
            },
        ],
        "object.properties" => {
            let mut items = Vec::new();
            if app.doc().view.active_view == ViewKind::Board {
                items.push(FlyoutItem {
                    id: "prop.colors",
                    label: "Colors",
                    description: "Ink and paper — switch to the stacked list to edit the picker.",
                    hotkey: None,
                    icon: DockIcon::Custom(icon_colors),
                    active: false,
                    group: Some("colors"),
                    role: FlyoutRole::Icon,
                });
                items.push(FlyoutItem {
                    id: "prop.swap",
                    label: "Swap",
                    description: "Swap foreground ⇄ background (X).",
                    hotkey: Some("X"),
                    icon: DockIcon::Swap,
                    active: false,
                    group: Some("colors"),
                    role: FlyoutRole::Icon,
                });
                items.push(FlyoutItem {
                    id: "prop.reset",
                    label: "Reset",
                    description: "Reset to the theme ink/paper (D).",
                    hotkey: Some("D"),
                    icon: DockIcon::Reset,
                    active: false,
                    group: Some("colors"),
                    role: FlyoutRole::Icon,
                });
            }
            items
        }
        "document.settings" => {
            use slate_doc::SnapKind;
            let mut items = vec![
                FlyoutItem {
                    id: "settings.grid",
                    label: "grid",
                    description: "Toggle the 20-unit board grid.",
                    hotkey: None,
                    icon: DockIcon::Grid,
                    active: app.board_show_grid,
                    group: Some("grid"),
                    role: FlyoutRole::Toggle,
                },
                FlyoutItem {
                    id: "settings.snap_grid",
                    label: "snap",
                    description: "F9 — snap picks and moves to the board grid.",
                    hotkey: Some("F9"),
                    icon: DockIcon::SnapGrid,
                    active: app.board_snap_grid,
                    group: Some("grid"),
                    role: FlyoutRole::Toggle,
                },
                FlyoutItem {
                    id: "settings.osnap",
                    label: "osnap",
                    description: "Master switch. Alt suspends snaps for one pick.",
                    hotkey: None,
                    icon: DockIcon::Osnap,
                    active: app.board_osnap.enabled,
                    group: Some("object snaps"),
                    role: FlyoutRole::Toggle,
                },
                FlyoutItem {
                    id: "settings.smart_guides",
                    label: "guides",
                    description: "Align to nearby objects in the same row or column.",
                    hotkey: None,
                    icon: DockIcon::SnapGrid,
                    active: app.board_smart_guides,
                    group: Some("object snaps"),
                    role: FlyoutRole::Toggle,
                },
                FlyoutItem {
                    id: "settings.reach.tight",
                    label: "tight",
                    description: crate::app::settings::SnapReach::Tight.hint(),
                    hotkey: None,
                    icon: DockIcon::Custom(icon_reach_tight),
                    active: app.board_snap_reach == crate::app::settings::SnapReach::Tight,
                    group: Some("reach"),
                    role: FlyoutRole::Icon,
                },
                FlyoutItem {
                    id: "settings.reach.nearby",
                    label: "nearby",
                    description: crate::app::settings::SnapReach::Nearby.hint(),
                    hotkey: None,
                    icon: DockIcon::Custom(icon_reach_nearby),
                    active: app.board_snap_reach == crate::app::settings::SnapReach::Nearby,
                    group: Some("reach"),
                    role: FlyoutRole::Icon,
                },
                FlyoutItem {
                    id: "settings.reach.wide",
                    label: "wide",
                    description: crate::app::settings::SnapReach::Wide.hint(),
                    hotkey: None,
                    icon: DockIcon::Custom(icon_reach_wide),
                    active: app.board_snap_reach == crate::app::settings::SnapReach::Wide,
                    group: Some("reach"),
                    role: FlyoutRole::Icon,
                },
            ];
            for kind in SnapKind::ALL {
                items.push(FlyoutItem {
                    id: osnap_item_id(kind),
                    label: kind.label(),
                    description: kind.hint(),
                    hotkey: None,
                    icon: snap_dock_icon(kind),
                    active: app.board_osnap.is_kind_remembered(kind),
                    group: Some("object snaps"),
                    role: FlyoutRole::Toggle,
                });
            }
            items
        }
        _ => Vec::new(),
    };
    if !visible.is_empty() {
        items.retain(|it| visible.iter().any(|v| v == it.id));
        items.sort_by_key(|it| {
            visible
                .iter()
                .position(|v| v == it.id)
                .unwrap_or(usize::MAX)
        });
    }
    items
}
