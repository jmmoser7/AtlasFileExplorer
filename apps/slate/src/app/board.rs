//! The Board view — Slate's open-world authored canvas.
//!
//! Frames, shapes, text, and placed images live in `slate_doc::scene`; this
//! module paints the scene with egui and turns pointer input into invertible
//! `SceneCmd` groups (see `scene.rs` — the command layer is the contract
//! shared by the UI, undo/redo, and the future MCP agent surface).
//!
//! Gesture rules:
//! - Live gestures (move / resize / draw / inspector scrubs) mutate the scene
//!   directly for immediate feedback and journal the *net* effect once, on
//!   release, so one gesture = one undo step.
//! - `Alt`+drag duplicates the grabbed selection; `Alt` on a scale handle
//!   scales a copy and leaves the original. `Ctrl+D` duplicates in place.
//!   Deleting and z-order moves are plain command groups.
//! - Smart guides align objects to each other while moving, resizing, or
//!   drawing (on by default). Create-tool corners — GhostFollow hover and
//!   both DragScale corners — use the same forcefield as a resize: the
//!   live rect's moving edges, not a 0-size point at the cursor. Hold
//!   `Alt` to bypass snapping; corner resize scales
//!   proportionally by default and `Shift` frees the aspect (distortion);
//!   `Ctrl` resizes from center (Office/PowerPoint convention).
//! - Armed area tools (frame, rect, ellipse, portals) click-release to
//!   place at default size, or press-drag-release to scale. The split is
//!   4 screen px (`place_tokens::DRAG_THRESHOLD`), on press/release — an
//!   egui click is not a drag.
//! - Windows-style bounding-box chrome is live on hover — no prior selection.
//!   Edges change the cursor only (no selection-look handles). Corners
//!   show 45° arrows even on a wide group box. A rotated resize pins the
//!   opposite handle in world space so the grabbed edge moves. Body hover
//!   eases a soft outline in and out. Selected objects get that same
//!   silhouette (fillet / ellipse / AABB) with no corner or midspan
//!   squares. Rotatable kinds (shapes, frames, text,
//!   images) show a 90° arc cursor just *outside*
//!   a corner; portals, connectors, and simple lines stay axis-aligned and
//!   never offer rotate. Rotation snaps at 45° intervals. Grid display and
//!   snap-to-grid are toolbar toggles. A Grasshopper-style align widget
//!   (left + bottom icon clusters, no second frame, no hover ghost)
//!   appears around a 2+ selection and commits `board.align.*` /
//!   `board.distribute.*`. Selection chrome is a per-shape silhouette,
//!   not a painted union box.
//!   Ctrl+Alt+Shift on a group grip repositions members without scaling
//!   them; the opposite union handle stays put on every corner and edge.
//! - Frames drag their members with them (geometric membership, captured at
//!   gesture start).

use super::{
    board_color, board_crop, board_forcefield, board_handles, board_icons, board_osnap, board_path,
    board_place, board_snap, board_web, kits, model3d, SlateApp, ThumbState,
};

pub use super::board_align::{BoardAlign, DistributeAxis};
use atlas_shell::file_picker::{self, PickRequest};
use atlas_shell::menu::{self, MenuIcon};
use atlas_shell::{canvas_scale, canvas_text};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke as EStroke, Vec2};
use slate_doc::scene::{
    Corner, Crop, Dash, ImageAdjust, ImageNode, Mirror, Node, NodeKind, PortalKind, PortalNode,
    Rgba, SceneCmd, ShapeKind, TextAlign, TextNode, Typeface, WorldRect, PORTAL_DEFAULT_H,
    PORTAL_DEFAULT_W,
};
use slate_doc::{ItemId, NodeId};
use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// A freshly placed agent portal, sized for its program grid (width, height).
pub(crate) const AGENT_PORTAL_SIZE: (f32, f32) = (384.0, 168.0);

/// One undo step. Scene steps stay on the tab journal; spreadsheet steps
/// write the linked file and ride the same Ctrl+Z order.
pub(crate) enum BoardMark {
    Scene,
    Sheet(SheetMark),
    /// Pasted and generated image locators moved into the workbook folder.
    Locators(slate_doc::RewriteLocators),
}

/// The node a scene command adds, removes, or patches.
pub(crate) fn cmd_node_id(cmd: &SceneCmd) -> NodeId {
    match cmd {
        SceneCmd::Add { node, .. }
        | SceneCmd::Remove { node, .. }
        | SceneCmd::LayerNodeAdd { node, .. }
        | SceneCmd::LayerNodeRemove { node, .. } => node.id,
        SceneCmd::Patch { after, .. } | SceneCmd::LayerNodePatch { after, .. } => after.id,
    }
}

pub(crate) struct SheetMark {
    pub item: ItemId,
    pub path: PathBuf,
    pub row: usize,
    pub col: usize,
    pub prior: atlas_core::office::PriorCell,
}

#[derive(Clone)]
pub(crate) struct SheetHit {
    pub node: NodeId,
    pub item: ItemId,
    pub row: usize,
    pub col: usize,
    pub rect: Rect,
    pub add: bool,
}

pub(crate) struct SheetEdit {
    pub node: NodeId,
    pub item: ItemId,
    pub row: usize,
    pub col: usize,
    pub buf: String,
    pub origin: String,
    /// The edit started as a new column, so naming it stays one undo step.
    pub fresh: bool,
    pub screen: Rect,
    pub font_px: f32,
}

/// A column or row boundary the open spreadsheet can drag.
#[derive(Clone, Copy)]
pub(crate) struct SheetGrip {
    pub node: NodeId,
    pub rect: Rect,
    /// Column to the left of this boundary. `None` is a row boundary.
    pub col: Option<usize>,
    pub row: Option<usize>,
}

/// In-flight click/drag text-box compose. Nothing is journaled until
/// [`SlateApp::commit_text_box_draft`] runs with non-empty content.
#[derive(Clone)]
pub(crate) struct TextBoxDraft {
    pub rect: WorldRect,
    pub buffer: String,
    pub color: Rgba,
    pub family: Typeface,
    pub size: f32,
    pub align: TextAlign,
    /// Drag-created boxes wrap at the drawn width; click-create grows with content.
    pub fixed_width: bool,
}

/// Typing into a blank linked text document (the Grasshopper panel entry).
/// Keystrokes mirror into the card's snippet cache and, off-thread, into the
/// linked file that the same gesture created. The scene is not touched.
#[derive(Clone)]
pub(crate) struct TextDocEdit {
    pub node: NodeId,
    pub item: ItemId,
    pub path: PathBuf,
    pub buffer: String,
    /// Take the keyboard from whatever held it (the closing canvas search).
    pub claim_focus: bool,
}

pub(crate) const TEXT_BOX_DEFAULT_W: f32 = 280.0;
pub(crate) const TEXT_BOX_DEFAULT_H: f32 = 48.0;
pub(crate) const TEXT_BOX_DEFAULT_SIZE: f32 = 24.0;

pub(crate) struct SheetResize {
    pub node: NodeId,
    pub col: Option<usize>,
    pub row: Option<usize>,
    pub start_px: f32,
    pub start_size: f32,
    pub cols: Vec<f32>,
    pub rows: Vec<f32>,
}

/// (group, tag list of (id, name, color)) rows for tag menus.
type TagRows = Vec<(slate_doc::TagId, String, [u8; 3])>;

const ZOOM_MIN: f32 = atlas_core::display::SLATE_CANVAS.min;
const ZOOM_MAX: f32 = atlas_core::display::SLATE_CANVAS.max;
pub(crate) const MIN_DRAW: f32 = 8.0;
/// Coalescing window for continuous inspector edits (one undo step).
const COALESCE: Duration = Duration::from_millis(1500);

/// Default placement size for images dropped onto the board.
pub const IMAGE_W: f32 = 240.0;
pub const IMAGE_H: f32 = 180.0;
/// One spreadsheet cell at zoom 1. The default card shows a dozen columns
/// and a dozen rows; a larger card shows more, and the rest scroll.
pub const SHEET_COL_WORLD: f32 = IMAGE_W / 12.0;
pub const SHEET_ROW_WORLD: f32 = IMAGE_H / 12.0;

/// How a spreadsheet card maps its rows and columns into a view rectangle.
/// Few columns stretch to the card. Extra rows and columns keep this cell
/// size and scroll.
pub(crate) struct SheetViewport {
    pub scroll: Vec2,
    pub col_w: f32,
    pub row_h: f32,
    pub max_scroll: Vec2,
}

pub(crate) fn sheet_viewport(view: Vec2, cols: usize, rows: usize, scroll: Vec2) -> SheetViewport {
    let cols = cols.max(1);
    let rows = rows.max(1);
    let max_scroll = Vec2::new(
        (cols as f32 * SHEET_COL_WORLD - view.x).max(0.0),
        (rows as f32 * SHEET_ROW_WORLD - view.y).max(0.0),
    );
    let col_w = if max_scroll.x > 0.0 {
        SHEET_COL_WORLD
    } else {
        view.x / cols as f32
    };
    SheetViewport {
        scroll: Vec2::new(
            scroll.x.clamp(0.0, max_scroll.x),
            scroll.y.clamp(0.0, max_scroll.y),
        ),
        col_w,
        row_h: SHEET_ROW_WORLD,
        max_scroll,
    }
}

/// Column widths and row heights for one card. Custom sizes scroll; an
/// unsized grid still stretches a short table to the card.
pub(crate) struct SheetTracks {
    pub scroll: Vec2,
    pub max_scroll: Vec2,
    pub cols: Vec<f32>,
    pub rows: Vec<f32>,
}

pub(crate) fn sheet_tracks(
    view: Vec2,
    col_n: usize,
    row_n: usize,
    custom_cols: &[f32],
    custom_rows: &[f32],
    scroll: Vec2,
) -> SheetTracks {
    if custom_cols.is_empty() && custom_rows.is_empty() {
        let vp = sheet_viewport(view, col_n, row_n, scroll);
        return SheetTracks {
            scroll: vp.scroll,
            max_scroll: vp.max_scroll,
            cols: vec![vp.col_w; col_n.max(1)],
            rows: vec![vp.row_h; row_n.max(1)],
        };
    }
    let cols: Vec<f32> = (0..col_n.max(1))
        .map(|i| {
            custom_cols
                .get(i)
                .copied()
                .filter(|w| *w >= MIN_DRAW)
                .unwrap_or(SHEET_COL_WORLD)
        })
        .collect();
    let rows: Vec<f32> = (0..row_n.max(1))
        .map(|i| {
            custom_rows
                .get(i)
                .copied()
                .filter(|h| *h >= MIN_DRAW)
                .unwrap_or(SHEET_ROW_WORLD)
        })
        .collect();
    let content_w: f32 = cols.iter().sum();
    let content_h: f32 = rows.iter().sum();
    let max_scroll = Vec2::new((content_w - view.x).max(0.0), (content_h - view.y).max(0.0));
    SheetTracks {
        scroll: Vec2::new(
            scroll.x.clamp(0.0, max_scroll.x),
            scroll.y.clamp(0.0, max_scroll.y),
        ),
        max_scroll,
        cols,
        rows,
    }
}

// Concern modules. Names stay on this module via `pub use`.

mod board_tools;
pub use board_tools::*;
mod board_drag;
pub use board_drag::*;
mod board_view;
#[cfg(test)]
mod shape_text_layout;
pub(crate) use board_view::*;
mod board_history;
mod board_mutate;
mod board_outline;
mod board_textures;
pub(crate) use board_outline::*;
mod board_outline_clip;
pub(crate) use board_outline_clip::*;
mod board_cards;
mod board_crop_ui;
mod board_dialogs;
mod board_gesture;
mod board_gesture_end;
mod board_menu;
mod board_model_hud;
mod board_model_view;
mod board_pick;
mod board_place_commit;
mod board_sheet;
mod board_sheet_resize;
mod board_sticky;
mod board_text_overlay;
mod board_text_place;
pub(crate) use board_dialogs::*;
#[cfg(test)]
mod board_tests;

impl SlateApp {
    /// Paint one node through a transform. `chrome` adds board-only adornment
    /// (frame titles/badges) that presentation mode and exports leave out.
    pub fn paint_board_node(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &Node,
        chrome: bool,
    ) {
        let srect = xf.rect_w2s(node.rect);
        let z = xf.z;
        let alpha = node.opacity.clamp(0.0, 1.0);
        let fade = |c: Color32| c.gamma_multiply(alpha);
        let rotated = node.rotation_deg.abs() > 0.01;
        let outline_world = node.rect.corners_rotated(node.rotation_deg);
        let outline_s: Vec<Pos2> = outline_world.map(|(x, y)| xf.w2s(Pos2::new(x, y))).to_vec();

        match &node.kind {
            NodeKind::Frame(f) => {
                let mut plate = corner_outline(srect, f.corner, z);
                if rotated {
                    plate = rotate_points(&plate, srect.center(), node.rotation_deg);
                }
                let palette = self.palette();
                let fill = if f.fill_follows_theme() {
                    palette.card
                } else {
                    rgba32(f.fill)
                };
                painter.add(egui::Shape::convex_polygon(
                    plate.clone(),
                    fade(fill),
                    EStroke::NONE,
                ));
                if !f.stroke.is_none() {
                    stroke_outline(painter, &plate, &f.stroke, z);
                }
                if chrome {
                    let order = self
                        .viewed_doc()
                        .scene
                        .frames_in_order()
                        .iter()
                        .position(|n| n.id == node.id)
                        .map(|i| i + 1)
                        .unwrap_or(0);
                    let title = canvas_scale::px(12.0, z);
                    if canvas_text::legible(title) {
                        paint_frame_label(
                            painter,
                            xf,
                            node,
                            false,
                            2.0,
                            6.0,
                            format!("{order} · {}", f.title),
                            FontId::proportional(title),
                            palette.sub,
                        );
                    }
                    if !f.assignments.is_empty() {
                        let tags: Vec<String> = f
                            .assignments
                            .values()
                            .filter_map(|t| {
                                self.viewed_doc().tag(*t).map(|(_, tag)| tag.name.clone())
                            })
                            .collect();
                        let tag_px = canvas_scale::px(10.5, z);
                        if canvas_text::legible(tag_px) {
                            paint_frame_label(
                                painter,
                                xf,
                                node,
                                true,
                                2.0,
                                6.0,
                                format!("⬦ {}", tags.join(", ")),
                                FontId::proportional(tag_px),
                                palette.accent,
                            );
                        }
                    }
                }
            }
            NodeKind::Image(img) => {
                // An agent's picture shows its newest result until one is picked.
                let generated = img.agent.is_some() && img.item.is_none();
                let (path, name) = if generated {
                    (
                        self.agent_shown_path(node.id).unwrap_or_default(),
                        String::new(),
                    )
                } else {
                    self.viewed_doc()
                        .item(img.item)
                        .map(|it| (it.path.clone(), it.file_name.clone()))
                        .unwrap_or_else(|| (std::path::PathBuf::new(), "missing".into()))
                };
                let kind = if generated {
                    slate_doc::MediaKind::Image
                } else {
                    slate_doc::media_kind(&path)
                };
                let corner = slate_doc::media::text_card_corner(&path, img.corner);
                let outline = if rotated {
                    rotate_points(
                        &corner_outline(srect, corner, z),
                        srect.center(),
                        node.rotation_deg,
                    )
                } else {
                    corner_outline(srect, corner, z)
                };
                let nested = self.slate_nesting();
                // Plain text and code always use the excerpt card. Word and
                // spreadsheets use it when text was extracted, and the
                // thumbnail card when it was not — the same split the
                // artifact export uses. Nested boards keep the file name:
                // item ids are not unique across workbooks.
                let sheet = if !nested && kind == slate_doc::MediaKind::Text {
                    self.sheet_for(img.item, &path)
                } else {
                    None
                };
                let show_excerpt = !nested
                    && kind == slate_doc::MediaKind::Text
                    && (sheet.is_some()
                        || self.snippet_for(img.item, &path).is_some()
                        || !slate_doc::media::structured_text_package(&path));

                if show_excerpt {
                    let pointer = ui.ctx().pointer_hover_pos();
                    if let Some(sheet) = &sheet {
                        self.paint_sheet_card(
                            painter, &outline, srect, node.id, img.item, &path, sheet, pointer, z,
                        );
                    } else {
                        self.paint_text_snippet_card(
                            painter, &outline, srect, img.item, &path, corner, pointer, z,
                        );
                    }
                } else if !nested && self.model_node_info(node.id).is_some() {
                    // 3D viewport: live render while unlocked, frozen-camera
                    // poster while locked (see model3d.rs for the lifecycle).
                    let tex_vertices = node_texture_vertices(
                        xf,
                        node.rect,
                        node.rect,
                        node.rotation_deg,
                        corner,
                        Crop::full(),
                        Mirror::default(),
                    );
                    self.paint_model_viewport(
                        ui,
                        painter,
                        &outline,
                        &tex_vertices,
                        srect,
                        node.id,
                        &name,
                        alpha,
                    );
                } else {
                    let desired_px =
                        srect.width().max(srect.height()) * ui.ctx().pixels_per_point();
                    let video_tex = if !nested && kind == slate_doc::MediaKind::Video {
                        self.video_texture(ui.ctx(), node.id, &img.adjust)
                    } else {
                        None
                    };
                    match if nested {
                        None
                    } else if generated {
                        self.agent_picture_texture(ui.ctx(), &path, &img.adjust, desired_px)
                    } else {
                        video_tex.or_else(|| {
                            self.board_texture(ui.ctx(), node.id, img.item, &img.adjust, desired_px)
                        })
                    } {
                        Some(tex) => {
                            // Node opacity = vertex tint on the textured mesh
                            // (matches CSS `opacity` compositing closely enough).
                            let tint = Color32::WHITE.gamma_multiply(alpha);
                            if let Some(clip) = &node.clip {
                                paint_clipped_texture(
                                    painter,
                                    xf,
                                    &tex,
                                    node,
                                    clip,
                                    img.crop,
                                    img.mirror(),
                                    tint,
                                );
                            } else {
                                let vertices = node_texture_vertices(
                                    xf,
                                    node.rect,
                                    node.rect,
                                    node.rotation_deg,
                                    corner,
                                    img.crop,
                                    img.mirror(),
                                );
                                paint_node_texture(painter, &tex, &vertices, tint);
                            }
                            if let Some(ov) = img.adjust.overlay {
                                painter.add(egui::Shape::convex_polygon(
                                    outline.clone(),
                                    fade(rgba32(ov)),
                                    EStroke::NONE,
                                ));
                            }
                            self.paint_image_paint_layers(
                                ui, painter, xf, node, img, &outline, srect, alpha, z,
                            );
                        }
                        None => {
                            let palette = self.palette();
                            painter.add(egui::Shape::convex_polygon(
                                outline.clone(),
                                palette.thumb_bg,
                                EStroke::NONE,
                            ));
                            let size = canvas_scale::px(11.0, z);
                            if canvas_text::legible(size) {
                                canvas_text::text(
                                    painter,
                                    srect.center(),
                                    Align2::CENTER_CENTER,
                                    atlas_shell::widgets::trunc(&name, 18),
                                    FontId::proportional(size),
                                    palette.sub,
                                );
                            }
                            self.paint_image_paint_layers(
                                ui, painter, xf, node, img, &outline, srect, alpha, z,
                            );
                        }
                    }
                }

                if kind == slate_doc::MediaKind::Video {
                    if !self.video_hides_badge(node.id) {
                        paint_play_badge(painter, srect, z);
                    }
                    self.paint_video_chrome(painter, xf, node);
                }
                // A 3D model card is kept as clean as a picture: its render,
                // or its gap message, says what it is.
                let model_card =
                    kind == slate_doc::MediaKind::Model || self.model_node_info(node.id).is_some();
                if !(matches!(kind, slate_doc::MediaKind::Image) || show_excerpt || model_card) {
                    paint_ext_badge(painter, srect, &slate_doc::media::ext_badge(&path), z);
                }
                stroke_outline(painter, &outline, &img.stroke, z);
                if img.agent.is_some() && !nested {
                    self.paint_agent_media(ui, painter, xf, node, srect);
                }
            }
            NodeKind::Shape(s) => {
                let scope = self.closed_form_scope();
                let styled = self
                    .path_mesh_cache
                    .closed_form_paint_shape(scope, node.id, s, node.rect);
                match s.shape {
                    _ if styled.is_some() => {
                        if let Some(styled) = &styled {
                            if let Some(path) = &styled.path {
                                board_path::paint_path_shape(
                                    self, painter, xf, node, styled, path, &fade,
                                );
                            }
                        }
                    }
                    ShapeKind::Rect => {
                        // Corner treatment first, then rotate the outline about
                        // the rect center (screen rotation matches world rotation
                        // under the uniform board zoom).
                        let mut outline = corner_outline(srect, s.corner, z);
                        if rotated {
                            outline = rotate_points(&outline, srect.center(), node.rotation_deg);
                        }
                        if let Some(fill) = s.fill {
                            painter.add(egui::Shape::convex_polygon(
                                outline.clone(),
                                fade(rgba32(fill)),
                                EStroke::NONE,
                            ));
                        }
                        stroke_outline(painter, &outline, &s.stroke, z);
                    }
                    ShapeKind::Ellipse => {
                        // One adaptive outline for fill and stroke. egui's
                        // EllipseShape tessellates too coarsely and reads faceted.
                        let mut pts = ellipse_outline(srect);
                        if rotated {
                            pts = rotate_points(&pts, srect.center(), node.rotation_deg);
                        }
                        if let Some(fill) = s.fill {
                            painter.add(egui::Shape::convex_polygon(
                                pts.clone(),
                                fade(rgba32(fill)),
                                EStroke::NONE,
                            ));
                        }
                        if !s.stroke.is_none() {
                            stroke_outline(painter, &pts, &s.stroke, z);
                        }
                    }
                    ShapeKind::RegularPolygon => {
                        let world = slate_doc::geom::regular_polygon_world_outline(
                            node.rect,
                            node.rotation_deg,
                            s.sides,
                            s.phase_deg,
                            s.corner,
                            0.25 / z,
                        );
                        let pts: Vec<Pos2> = world
                            .into_iter()
                            .map(|p| xf.w2s(Pos2::new(p[0], p[1])))
                            .collect();
                        if let Some(fill) = s.fill {
                            painter.add(egui::Shape::convex_polygon(
                                pts.clone(),
                                fade(rgba32(fill)),
                                EStroke::NONE,
                            ));
                        }
                        stroke_outline(painter, &pts, &s.stroke, z);
                    }
                    ShapeKind::Line => {
                        let (mut a, mut b) = if s.flip {
                            (srect.left_bottom(), srect.right_top())
                        } else {
                            (srect.left_top(), srect.right_bottom())
                        };
                        if rotated {
                            let ends = rotate_points(&[a, b], srect.center(), node.rotation_deg);
                            a = ends[0];
                            b = ends[1];
                        }
                        let w = canvas_scale::px(s.stroke.width.max(1.0), z);
                        let color = fade(rgba32(s.stroke.color));
                        match s.stroke.dash {
                            Dash::Solid => {
                                painter.line_segment([a, b], EStroke::new(w, color));
                            }
                            Dash::Dashed => {
                                painter.add(egui::Shape::dashed_line(
                                    &[a, b],
                                    EStroke::new(w, color),
                                    canvas_scale::px(12.0, z),
                                    canvas_scale::px(8.0, z),
                                ));
                            }
                            Dash::Dotted => {
                                painter.add(egui::Shape::dashed_line(
                                    &[a, b],
                                    EStroke::new(w, color),
                                    (w * 1.2).max(2.0),
                                    (w * 2.2).max(4.0),
                                ));
                            }
                        }
                    }
                    ShapeKind::Path => {
                        if let Some(ref path) = s.path {
                            board_path::paint_path_shape(self, painter, xf, node, s, path, &fade);
                        }
                    }
                }
                if slate_doc::scene::shape_hosts_text(s) {
                    self.paint_hosted_text(painter, xf, node, s.text.as_ref(), srect, &fade);
                }
            }
            NodeKind::Text(t) => {
                // Background fill (sticky notes are a Text preset with a
                // fill) — mirrors the artifact's `background` on the node.
                if let Some(fill) = t.fill {
                    Self::paint_sticky_shadow(painter, xf, node, srect, z, &fade);
                    if let Some(clip) = &node.clip {
                        paint_clip_fill(painter, xf, node, clip, fade(rgba32(fill)));
                    } else {
                        painter.add(egui::Shape::convex_polygon(
                            outline_s.clone(),
                            fade(rgba32(fill)),
                            EStroke::NONE,
                        ));
                    }
                }
                stroke_outline(painter, &outline_s, &t.stroke, z);
                if self
                    .text_edit
                    .as_ref()
                    .is_some_and(|(edit_id, _)| *edit_id == node.id)
                {
                    return;
                }
                // A note an agent writes shows its reply until it is edited.
                let reply = self.agent_note_reply(node.id);
                let shown = reply.as_deref().unwrap_or(&t.text);
                let sticky = t.fill.is_some();
                let draw_size = if sticky {
                    self.sticky_font_size(
                        painter.ctx(),
                        node.id,
                        shown,
                        t.family,
                        t.size,
                        node.rect.w,
                        node.rect.h,
                        t.align,
                    )
                } else {
                    t.size
                };
                // Drop the words when they are too small to read. Do not
                // clamp the size up — that would hold a screen constant.
                if canvas_text::legible(canvas_text::authored_px(draw_size, z)) {
                    let laid = zoom_text_galley(
                        painter.ctx(),
                        shown,
                        typeface_font(t.family, draw_size),
                        node.rect.w.max(0.0),
                        t.align,
                        fade(rgba32(t.color)),
                        z,
                    );
                    // A sticky centers its block by moving where the cached
                    // galley paints, as `center_galley_vertically` does for the editor.
                    let dy = if sticky {
                        let dy = (srect.height() - laid.rect.height()) * 0.5;
                        if dy > 0.5 {
                            dy
                        } else {
                            0.0
                        }
                    } else {
                        0.0
                    };
                    let galley = laid;
                    let text_pos = srect.min + egui::vec2(0.0, dy);
                    if let Some(clip) = &node.clip {
                        paint_clipped_galley(painter, xf, node, clip, text_pos, &galley);
                    } else {
                        painter.with_clip_rect(srect.expand(2.0)).galley(
                            text_pos,
                            galley,
                            Color32::WHITE,
                        );
                    }
                }
                if t.agent.is_some() && !self.slate_nesting() {
                    self.paint_agent_note(ui, painter, xf, node, srect);
                }
            }
            NodeKind::Connector(conn) => {
                // Derived bezier through the path-mesh cache; Faint = 40%,
                // arrowheads + label match the artifact (see board_wire.rs).
                let conn = conn.clone();
                self.paint_connector(painter, xf, node, &conn);
            }
            NodeKind::Portal(p) => {
                let portal = p.clone();
                match portal.kind {
                    PortalKind::Slate => {
                        self.paint_slate_portal(ui, painter, xf, node, &portal);
                    }
                    _ if self.slate_nesting() => {
                        self.paint_nested_host_poster(ui, painter, xf, node, &portal);
                    }
                    PortalKind::Agent => {
                        self.paint_agent_portal(ui, painter, xf, node, &portal);
                    }
                    PortalKind::Web => {
                        self.paint_web_portal(ui, painter, xf, node, &portal);
                    }
                    PortalKind::FileAtlas => {
                        self.paint_atlas_portal(ui, painter, xf, node, &portal);
                    }
                }
            }
            NodeKind::DockStrip(strip) => {
                super::board_dock_embed::paint_dock_strip(ui, xf, node, strip, self);
            }
        }
    }

    // ----- main board entry -----------------------------------------------------

    pub fn board_canvas(&mut self, ui: &mut egui::Ui, rect: Rect) {
        self.validate_image_paint_session();
        self.tick_bumper_glide(ui.ctx());
        self.fit_agent_cards(ui.ctx());
        let _span = atlas_core::session_log::span("slate.board.paint");
        brush_prof::lap("paint-start");
        self.stamp_sync_px = 0.0;
        self.settle_erase_checks();
        self.path_mesh_cache.tess_misses = 0;
        self.board_snap_guides.clear();
        self.board_osnap_hit = None;
        self.board_point_snap = None;
        self.board_draw_rect = None;
        self.ortho_feedback = None;
        // The Tab lock lives only while a segment is pending (Esc, cancel,
        // and placement all end it).
        if self.draft_lock.is_some() && self.pending_segment_origin().is_none() {
            self.draft_lock = None;
        }
        self.sync_crop_mode();
        // Connector AABBs follow their endpoints; synced once per scene
        // generation (journal commits / undo / redo), never per frame.
        self.sync_connector_rects();
        let palette = self.palette();
        let painter = ui.painter_at(rect);
        let resp = ui.allocate_rect(rect, Sense::click_and_drag());
        let pointer = ui.ctx().pointer_latest_pos();
        let xf = self.board_xf();
        let wp = pointer.map(|p| xf.s2w(p));
        let editing_text = self.text_compose_active();

        // Object chrome runs before gestures so it can capture clicks.
        let agent_controls_capture = self.agent_spawn_input(ui, &xf);
        let other_toolbar_captures = self.shape_properties_ui(ui, &xf)
            | self.corner_entry_ui(ui, &xf)
            | self.tip_numeric_ui(ui, &xf)
            | self.image_paint_palette_ui(ui, &xf);
        let shot_captures = self.paint_model_screenshot_popup(ui.ctx());
        let model_toolbar_captures =
            agent_controls_capture || other_toolbar_captures || shot_captures;
        self.lock_models_pressed_outside(ui, &xf, pointer, model_toolbar_captures);

        let now = ui.input(|i| i.time);
        let mut canvas_nav = false;

        // One web portal may hold the pointer and keyboard (D17/D22). This runs
        // before the camera because that is the whole point: with the pointer
        // inside a focused page, the wheel scrolls the page instead of zooming
        // the board. Its chrome strip and a thin border band stay Slate targets,
        // so the frame can always be grabbed and released.
        self.peel_contents_focus_if_clicked_outside(ui, &xf, pointer);
        self.peel_sheet(ui, &xf, pointer);
        let agent_capture = self.agent_shelf_captures(&xf, pointer);
        let page_capture = self.web_input_frame(ui, &xf, pointer);
        let external_capture = page_capture || self.atlas_input_frame(ui, &xf, pointer);
        // The shelf still receives the wheel. A drag moves the train card
        // unless a text field is the action under the pointer.
        let web_capture = external_capture || self.agent_text_editing_captures(&xf, pointer);
        let _ = self.dock_embed_frame(
            ui.ctx(),
            &xf,
            pointer,
            web_capture || model_toolbar_captures || editing_text,
        );
        let over_dock_strip = wp.is_some_and(|w| self.dock_embed_node_at(w.x, w.y).is_some());
        // Hovering a dock palette or an object tool strip must not eat zoom.
        // An overflowing dock body still keeps the wheel when it can scroll.
        let dock_nav = atlas_shell::dock::dock_pointer_nav(ui.ctx());
        let palette_wheel =
            dock_nav.canvas_wheel() || (other_toolbar_captures && !dock_nav.wheel_scrolls);
        let over_agent_card = self.pointer_over_agent_card(&xf, pointer);
        let over_image_album = self.pointer_over_image_album(&xf, pointer);
        // The project list and an overflowing card the person sized scroll.
        // Every other agent card still zooms the board.
        // An open navigable menu scrolls itself (P0.10).
        let card_scrolls = self.pointer_over_project_picker(&xf, pointer)
            || self.pointer_over_scrolling_agent_card(&xf, pointer)
            || atlas_shell::menu_wheel::wheel_owned(ui.ctx());

        // --- camera ---
        if !card_scrolls
            && !over_image_album
            && (resp.hovered()
                || over_agent_card
                || ((agent_capture || agent_controls_capture)
                    && pointer.is_some_and(|p| rect.contains(p)))
                || palette_wheel)
            && !external_capture
        {
            // egui hands a Shift wheel over as a horizontal delta.
            let (scroll, shift) = ui.input(|i| {
                let d = i.smooth_scroll_delta + i.raw_scroll_delta;
                let shift = i.modifiers.shift;
                (if shift { d.x + d.y } else { d.y }, shift)
            });
            if scroll.abs() > 0.0 {
                // Scroll over an unlocked 3D viewport zooms the model, not
                // the board (Rhino wheel semantics while live).
                let live_model = wp.and_then(|w| self.live_model_at(w.x, w.y));
                let sheet_scrolled =
                    live_model.is_none() && wp.is_some_and(|w| self.scroll_sheet(w, scroll, shift));
                if let Some(id) = live_model {
                    self.model_scroll(id, scroll);
                } else if !sheet_scrolled && shift {
                    let zc = self.tab().cam.z;
                    self.tab_mut().cam.offset.x -= scroll / zc;
                    canvas_nav = true;
                } else if !sheet_scrolled {
                    if let Some(p) = pointer {
                        self.board_zoom_at(
                            p,
                            atlas_core::display::SLATE_CANVAS.wheel_factor(scroll),
                        );
                        canvas_nav = true;
                    }
                }
                if over_agent_card && !card_scrolls && live_model.is_none() {
                    ui.ctx().input_mut(|i| {
                        i.smooth_scroll_delta.y = 0.0;
                        i.raw_scroll_delta.y = 0.0;
                        if shift {
                            i.smooth_scroll_delta.x = 0.0;
                            i.raw_scroll_delta.x = 0.0;
                        }
                    });
                }
            }
        }
        let space = ui.input(|i| i.key_down(egui::Key::Space));
        let hand_pan = self.board_tool == BoardTool::Pan;
        // A live page that took the pointer owns the right button too (D22):
        // no tip HUD chord, turbo pan, right-drag pan, or board menu.
        let (secondary_down, secondary_pressed) = ui.input(|i| {
            (
                i.pointer.button_down(egui::PointerButton::Secondary) && !page_capture,
                i.pointer.button_pressed(egui::PointerButton::Secondary) && !page_capture,
            )
        });
        // The right-button chord has to see the modifiers on the press
        // itself. A frame-start snapshot misses a key that arrives with
        // the click, and turbo pan reads `modifiers.ctrl` while the rest
        // of the app historically read `command`.
        let pointer_mods = ui.input(|i| {
            let mods = i.modifiers;
            let press = i.events.iter().find_map(|event| match event {
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Secondary,
                    pressed: true,
                    modifiers,
                } => Some((*pos, *modifiers)),
                _ => None,
            });
            (mods, press)
        });
        self.alt_down = pointer_mods.0.alt || pointer_mods.1.is_some_and(|(_, m)| m.alt);
        self.shift_down = pointer_mods.0.shift || pointer_mods.1.is_some_and(|(_, m)| m.shift);
        self.ctrl_down = pointer_mods.0.ctrl
            || pointer_mods.0.command
            || pointer_mods.1.is_some_and(|(_, m)| m.ctrl || m.command);
        let hud_pointer = pointer_mods.1.map(|(pos, _)| pos).or(pointer);
        self.hud_pointer = hud_pointer;
        let right_held = secondary_down || secondary_pressed;
        self.hud_right_held = right_held;
        let brush_armed = self.tip_hud_armed();
        // Alt+right is size. Shift+right is opacity. Ctrl+right is the color
        // wheel. Ctrl and Alt beat Shift. Each chord owns the button before
        // turbo pan or a plain right-drag pan.
        let claim_right = self.brush_hud.is_some()
            || self.tip_numeric_eats()
            || (brush_armed && right_held && self.alt_down)
            || (self.tip_hud_has_color() && right_held && self.ctrl_down)
            || (brush_armed && right_held && self.shift_down && !self.ctrl_down && !self.alt_down);
        let mut cam_offset_tmp = self.tab().cam.offset;
        let ctx2 = ui.ctx().clone();
        let turbo_pan_active = if claim_right || page_capture {
            false
        } else {
            self.turbo_pan
                .step(&ctx2, rect, pointer, &mut cam_offset_tmp)
        };
        if turbo_pan_active {
            let zc = self.tab().cam.z;
            let old = self.tab().cam.offset;
            self.tab_mut().cam.offset = old - (cam_offset_tmp - old) / zc;
            canvas_nav = true;
        }
        // Precise pan: middle-drag, Space+left-drag, right-drag (File Atlas
        // parity), or Hand tool (H) left-drag. A focused page owns the buttons
        // it is given, so a drag inside it selects text instead of panning.
        let through_palette = atlas_shell::commands::chrome_pass_pan_delta(
            ui.ctx(),
            &resp,
            rect,
            !turbo_pan_active && !web_capture,
            dock_nav.canvas_pan() || other_toolbar_captures,
            space || hand_pan,
        );
        let brush_right = self.drive_brush_hud(hud_pointer, secondary_down, secondary_pressed);
        if let Some((_, target)) = self.brush_cursor_warp.take() {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::CursorPosition(target));
        }
        let hold_right = claim_right || brush_right;
        let panning = !web_capture
            && (resp.dragged_by(egui::PointerButton::Middle)
                || (space && resp.dragged_by(egui::PointerButton::Primary))
                || (resp.dragged_by(egui::PointerButton::Secondary)
                    && !turbo_pan_active
                    && !hold_right)
                || (hand_pan && resp.dragged_by(egui::PointerButton::Primary))
                || through_palette.is_some());
        if hand_pan && resp.hovered() {
            ui.ctx().set_cursor_icon(if panning {
                egui::CursorIcon::Grabbing
            } else {
                egui::CursorIcon::Grab
            });
        }
        if panning {
            let delta = through_palette.unwrap_or_else(|| resp.drag_delta());
            let zc = self.tab().cam.z;
            self.tab_mut().cam.offset -= delta / zc;
            canvas_nav = true;
        }
        if canvas_nav {
            self.bump_grid_fade(now);
        }

        // Z zoom tool: while armed, the primary button belongs to the tool
        // (click = step, drag = zoom window); pans keep their buttons.
        let zoom_tool =
            !web_capture && self.zoom_tool_frame(ui, &resp, rect, space || panning || hand_pan);

        // Connector grips: Select tool, idle pointer near a node edge.
        if self.board_tool == BoardTool::Select
            && self.board_drag.is_none()
            && !panning
            && !zoom_tool
            && !editing_text
            && !web_capture
            && self.board_crop.is_none()
            && resp.hovered()
        {
            self.update_wire_grips(pointer, &xf);
        } else if !matches!(self.board_drag, Some(BoardDrag::Wire(_))) {
            // Keep the source grips visible during a wire drag only.
            self.wire_grips = None;
        }

        // Drawing tools consume ordered events at their own positions. A moving
        // click must not be reclassified by egui or moved to the frame's last cursor.
        let ordered_drawing = matches!(
            self.board_tool,
            BoardTool::Line
                | BoardTool::Polyline
                | BoardTool::Arc
                | BoardTool::BezierSpan
                | BoardTool::Pen
                | BoardTool::Brush
                | BoardTool::Eraser
                | BoardTool::Smooth
        );
        if ordered_drawing
            && !space
            && !panning
            && !zoom_tool
            && !model_toolbar_captures
            && !self.tip_numeric_eats()
            && !over_dock_strip
            && !web_capture
        {
            let events = ui.input(|i| i.events.clone());
            for event in events {
                match event {
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers,
                    } if rect.contains(pos) && resp.hovered() => {
                        let world = xf.s2w(pos);
                        self.board_align_eat_press = true;
                        match self.board_tool {
                            BoardTool::Line => {
                                if self.line_begin(world, modifiers.shift) {
                                    self.board_drag = Some(BoardDrag::LineDraw { started: true });
                                } else {
                                    self.line_release(world, false, modifiers.shift);
                                    self.board_drag = None;
                                }
                            }
                            BoardTool::Polyline | BoardTool::Arc => {
                                self.path_tool_click(world);
                            }
                            BoardTool::BezierSpan => {
                                self.board_drag = self.begin_gesture(pos, world, modifiers);
                            }
                            BoardTool::Eraser | BoardTool::Smooth if self.brush_hud.is_none() => {
                                self.board_drag = self.begin_gesture(pos, world, modifiers);
                            }
                            BoardTool::Pen | BoardTool::Brush => {
                                if self.board_tool == BoardTool::Brush
                                    && modifiers.shift
                                    && !modifiers.alt
                                {
                                    self.brush_straight = Some(super::board_color::BrushStraight {
                                        start: world,
                                        start_screen: pos,
                                        tip: self.tip_now(),
                                        anchor: self.brush_line_anchor(),
                                    });
                                    self.board_drag = None;
                                } else if self.board_tool == BoardTool::Pen
                                    && modifiers.shift
                                    && !modifiers.alt
                                {
                                    self.begin_pen_straight(world, pos);
                                    self.board_drag = None;
                                } else if self.board_tool == BoardTool::Brush && modifiers.alt {
                                    self.brush_mod_click =
                                        Some(super::board_color::BrushModClick { origin: pos });
                                    self.board_drag = None;
                                } else if self.board_tool == BoardTool::Brush
                                    && self.brush_hud.is_some()
                                {
                                } else {
                                    self.brush_mod_click = None;
                                    self.board_drag = self.begin_gesture(pos, world, modifiers);
                                }
                            }
                            _ => {}
                        }
                    }
                    egui::Event::PointerMoved(pos) if self.board_drag.is_some() => {
                        self.update_gesture(xf.s2w(pos), ui.input(|i| i.modifiers));
                    }
                    egui::Event::PointerMoved(pos) if self.pen_straight.is_some() => {
                        self.update_pen_straight(xf.s2w(pos), ui.input(|i| i.modifiers.shift));
                    }
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers,
                    } => {
                        if self.board_drag.is_some() {
                            self.brush_mod_click = None;
                            self.brush_straight = None;
                            self.end_gesture(xf.s2w(pos), Some(pos), modifiers);
                        } else if self.brush_straight.is_some() {
                            self.release_brush_straight(pos, xf.s2w(pos), modifiers.shift);
                        } else if self.pen_straight.is_some() {
                            self.release_pen_straight(pos, xf.s2w(pos), modifiers.shift);
                        } else if let Some(click) = self.brush_mod_click.take() {
                            if pos.distance(click.origin) <= super::board_color::BRUSH_MOD_CLICK_PX
                            {
                                self.eyedropper_click(xf.s2w(pos), false);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        // Portal maximize sits on the node; take the press before a move
        // or resize can claim the same corner.
        if !space
            && !panning
            && !zoom_tool
            && !model_toolbar_captures
            && !over_dock_strip
            && !web_capture
            && self.board_crop.is_none()
            && ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary))
        {
            if let Some(p) = pointer {
                if self.try_portal_maximize_press(p, &xf) {
                    self.board_align_eat_press = true;
                }
            }
        }

        // Align widget: press-to-commit (Grasshopper). Must run before
        // drag_started so a click on an icon cannot become a move.
        if self.board_tool == BoardTool::Select
            && !space
            && !panning
            && !zoom_tool
            && !model_toolbar_captures
            && !over_dock_strip
            && !web_capture
            && self.board_crop.is_none()
            && !self.board_align_eat_press
            && ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary))
        {
            if let Some(p) = pointer {
                let _ = self.try_align_press(ui.ctx(), p);
            }
        }

        // --- DragRect place: press / release, not drag_started ---
        // A click never becomes an egui drag, so waiting for drag_started
        // dropped ClickPlace (P2.DragShape / tool-arming D04). Same split
        // as the Line tool: travel on this press chooses ClickPlace vs
        // DragScale.
        let place_rect = self.board_tool.places_by_drag_rect();
        let place_ok = !space
            && !panning
            && !zoom_tool
            && !model_toolbar_captures
            && !over_dock_strip
            && !web_capture
            && !self.board_align_eat_press
            && resp.hovered();
        if place_rect && place_ok {
            if ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary)) {
                if let Some(p) = pointer {
                    if !self.pointer_on_portal_maximize(p, &xf) {
                        let mods = ui.input(|i| i.modifiers);
                        self.board_drag = self.begin_gesture(p, xf.s2w(p), mods);
                    }
                }
            }
            if ui.input(|i| i.pointer.button_down(egui::PointerButton::Primary))
                && matches!(self.board_drag, Some(BoardDrag::Draw { .. }))
            {
                if let Some(w) = wp {
                    let mods = ui.input(|i| i.modifiers);
                    self.update_gesture(w, mods);
                }
            }
            if ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary))
                && matches!(self.board_drag, Some(BoardDrag::Draw { .. }))
            {
                let w = wp.unwrap_or(match &self.board_drag {
                    Some(BoardDrag::Draw { start_world, .. }) => *start_world,
                    _ => Pos2::ZERO,
                });
                let mods = ui.input(|i| i.modifiers);
                self.end_gesture(w, pointer, mods);
            }
        }

        // Deck: press/release, same reason as DragRect — a click never
        // becomes an egui drag, and travel past 4 screen px is the stroke.
        let decking = self.board_tool == BoardTool::Deck;
        if decking && place_ok {
            if ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary)) {
                if let Some(p) = pointer {
                    if !self.pointer_on_portal_maximize(p, &xf) {
                        self.board_align_eat_press = true;
                        let mods = ui.input(|i| i.modifiers);
                        self.board_drag = self.begin_gesture(p, xf.s2w(p), mods);
                    }
                }
            }
            if ui.input(|i| i.pointer.button_down(egui::PointerButton::Primary))
                && matches!(self.board_drag, Some(BoardDrag::DeckStroke { .. }))
            {
                if let Some(w) = wp {
                    let mods = ui.input(|i| i.modifiers);
                    self.update_gesture(w, mods);
                }
            }
            if ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary))
                && matches!(self.board_drag, Some(BoardDrag::DeckStroke { .. }))
            {
                let w = wp.unwrap_or(Pos2::ZERO);
                let mods = ui.input(|i| i.modifiers);
                self.end_gesture(w, pointer, mods);
            }
        }

        // Corner grip: press / release, not drag_started. egui's drag
        // threshold held the grip still for the first pixels, and a click on
        // the grip opens its typed radius (P1.node.corner-grip).
        if self.board_tool == BoardTool::Select
            && !space
            && !panning
            && !zoom_tool
            && !model_toolbar_captures
            && !web_capture
            && !self.board_align_eat_press
            && self.board_drag.is_none()
            && ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary))
        {
            if let Some(p) = pointer {
                if let Some(drag) = self.begin_fillet_drag(p, xf.s2w(p)) {
                    self.board_drag = Some(drag);
                    self.board_align_eat_press = true;
                } else if let Some(g) = self.polygon_sides_glyph_at(p, &xf) {
                    let request = super::board_transform::SidesRequest {
                        id: g.id,
                        vertex: g.vertex,
                        add: g.add,
                    };
                    let ctx = ui.ctx().clone();
                    self.dispatch(
                        &ctx,
                        atlas_commands::CommandId("board.shape.sides"),
                        serde_json::to_string(&request).ok(),
                    );
                    self.board_align_eat_press = true;
                    self.board_sides_pressed = true;
                }
            }
        }
        // A second click on a grip is not a canvas double-click: that would
        // collapse a multi-selection and open text editing.
        let mut grip_released = false;
        if self.board_sides_pressed
            && ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary))
        {
            self.board_sides_pressed = false;
            grip_released = true;
        }
        if matches!(self.board_drag, Some(BoardDrag::FilletRadius { .. })) {
            let mods = ui.input(|i| i.modifiers);
            if ui.input(|i| i.pointer.button_down(egui::PointerButton::Primary)) {
                if let Some(w) = wp {
                    self.update_gesture(w, mods);
                }
            }
            if ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary)) {
                grip_released = true;
                let w = wp.unwrap_or(Pos2::ZERO);
                self.end_gesture(w, pointer, mods);
            }
        }

        // Crosstalk port: press / release like the corner grip. A press
        // selects the card, which can refit it and move a small port away
        // before egui's drag threshold fires.
        if self.board_tool == BoardTool::Select
            && !space
            && !panning
            && !zoom_tool
            && !web_capture
            && !self.board_align_eat_press
            && self.board_drag.is_none()
            && ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary))
        {
            if let Some((card, side)) = pointer.and_then(|p| self.crosstalk_port_under(p, &xf)) {
                self.board_drag = Some(BoardDrag::Wire(super::board_wire::WireDrag {
                    mode: super::board_wire::WireMode::Add {
                        from: (card, side, 0.5),
                    },
                    cursor: wp.unwrap_or(Pos2::ZERO),
                    snap: None,
                }));
                self.board_align_eat_press = true;
            }
        }
        if matches!(
            &self.board_drag,
            Some(BoardDrag::Wire(wd)) if matches!(wd.mode, super::board_wire::WireMode::Add { from } if self.is_crosstalk_port(from))
        ) {
            let mods = ui.input(|i| i.modifiers);
            if ui.input(|i| i.pointer.button_down(egui::PointerButton::Primary)) {
                if let Some(w) = wp {
                    self.update_gesture(w, mods);
                }
            }
            if ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary)) {
                let w = wp.unwrap_or(Pos2::ZERO);
                self.end_gesture(w, pointer, mods);
            }
        }

        // Measure picks: press / release, not drag_started. Each pick is a
        // plain click, which never becomes an egui drag (media D14).
        if self.board_tool == BoardTool::Select
            && !space
            && !panning
            && !zoom_tool
            && !model_toolbar_captures
            && !web_capture
            && !self.board_align_eat_press
            && self.board_drag.is_none()
            && ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary))
        {
            if let Some(p) = pointer {
                if let Some(id) = self.model_measure_press_at(p, xf.s2w(p)) {
                    if !self.board_sel.contains(&id) {
                        self.board_sel.clear();
                        self.board_sel.insert(id);
                    }
                    self.board_drag = Some(BoardDrag::ModelMeasure {
                        id,
                        start_screen: p,
                    });
                    self.board_align_eat_press = true;
                }
            }
        }
        if matches!(self.board_drag, Some(BoardDrag::ModelMeasure { .. }))
            && ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary))
        {
            let w = wp.unwrap_or(Pos2::ZERO);
            let mods = ui.input(|i| i.modifiers);
            self.end_gesture(w, pointer, mods);
        }
        if self.board_drag.is_none() && ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO) {
            if let Some(p) = pointer {
                self.model_measure_hover(p);
            }
        }

        // --- gesture start ---
        // Hit-test at the pointer *press origin*: by the time egui's drag
        // threshold fires, a fast drag has often already left the tiny
        // handle, which used to degrade corner scaling into a node move.
        if resp.drag_started_by(egui::PointerButton::Primary)
            && !ordered_drawing
            && !space
            && !panning
            && !zoom_tool
            && !model_toolbar_captures
            && !web_capture
            && !place_rect
            && !decking
            && (self.board_tool != BoardTool::Line || over_dock_strip)
            && !self.board_align_eat_press
        {
            let origin = ui.input(|i| i.pointer.press_origin()).or(pointer);
            if let Some(p) = origin {
                let on_sheet_editor = self
                    .sheet_edit
                    .as_ref()
                    .is_some_and(|edit| edit.screen.contains(p));
                let on_open_sheet = self.sheet_open.is_some_and(|id| {
                    self.doc()
                        .scene
                        .node(id)
                        .is_some_and(|n| xf.rect_w2s(n.rect).contains(p))
                });
                if let Some(grip) = self
                    .sheet_grips
                    .iter()
                    .find(|g| g.rect.contains(p))
                    .copied()
                {
                    self.begin_sheet_resize(grip, p);
                } else if self.sheet_add_hit(p).is_some() || on_sheet_editor || on_open_sheet {
                    // The add control and the cell editor own this press.
                } else if self.align_action_at(p).is_none()
                    && !self.pointer_on_portal_maximize(p, &xf)
                {
                    let mods = ui.input(|i| i.modifiers);
                    self.board_drag = self.begin_gesture(p, xf.s2w(p), mods);
                }
            }
        }

        // --- live gesture update ---
        if self.sheet_resize.is_some() {
            if let Some(p) = pointer {
                self.update_sheet_resize(p);
            }
        }
        if resp.dragged_by(egui::PointerButton::Primary) && !panning && !ordered_drawing {
            if let Some(BoardDrag::ModelMeasure { id, .. }) = &self.board_drag {
                if let Some(p) = pointer {
                    if let Some(n) = self.doc().scene.node(*id) {
                        let srect = xf.rect_w2s(n.rect);
                        self.model_measure_preview(*id, p, srect);
                    }
                }
            } else if let Some(w) = wp {
                let mods = ui.input(|i| i.modifiers);
                self.update_gesture(w, mods);
            }
        }

        // --- gesture end ---
        if resp.drag_stopped_by(egui::PointerButton::Primary)
            && !ordered_drawing
            && self.board_tool != BoardTool::Line
            && !place_rect
            && !decking
        {
            if let Some(w) = wp {
                let mods = ui.input(|i| i.modifiers);
                self.end_gesture(w, pointer, mods);
            }
        }
        // egui can end its drag with nothing above to commit it: Esc aborts
        // the drag (an Esc the cancel stack never saw, with the palette or a
        // text field focused), and a release outside the window leaves no
        // pointer position. A node drag still live now goes back (P0.1).
        if resp.drag_stopped() {
            self.cancel_node_drag();
        }

        // A small movement turns egui's click into a drag. The + lives on
        // that edge, so the release is what adds the column.
        let mut ate_plus = false;
        if ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary)) {
            if let Some(origin) = ui.input(|i| i.pointer.press_origin()) {
                if let Some(hit) = self.sheet_add_hit(origin) {
                    let moved = pointer.map(|p| origin.distance(p)).unwrap_or(0.0);
                    if moved < 8.0 {
                        self.add_sheet_column(hit);
                        ate_plus = true;
                    }
                }
            }
        }

        // --- clicks (the armed zoom tool owns the primary button) ---
        if resp.clicked() && !ate_plus && !zoom_tool && !web_capture && !self.board_align_eat_press
        {
            if self.sheet_open.is_some() && self.sheet_prompt {
                // The save reminder owns the pointer until it is answered.
            } else if let Some(p) = pointer {
                if self.sheet_save_hit.is_some_and(|r| r.contains(p)) {
                    let _ = self.save_open_sheet();
                } else if self.sheet_open.is_some() {
                    if let Some(hit) = self
                        .sheet_hits
                        .iter()
                        .find(|hit| !hit.add && hit.rect.contains(p))
                        .cloned()
                    {
                        self.open_sheet_cell(hit);
                    }
                }
            }
            if self.sheet_open.is_none() {
                if self
                    .sheet_edit
                    .as_ref()
                    .is_some_and(|edit| pointer.is_some_and(|p| !edit.screen.contains(p)))
                {
                    self.commit_sheet_edit();
                    if editing_text {
                        let outside = pointer
                            .map(|p| self.text_compose_outside(p, &xf))
                            .unwrap_or(false);
                        if outside {
                            self.finish_text_compose_on_click_away();
                            if let Some(w) = wp {
                                let mods = ui.input(|i| i.modifiers);
                                if !self.try_dock_embed_click(ui.ctx(), w) {
                                    self.board_click(w, mods);
                                }
                            }
                        }
                    } else if let Some(w) = wp {
                        let mods = ui.input(|i| i.modifiers);
                        if !self.try_dock_embed_click(ui.ctx(), w) {
                            self.board_click(w, mods);
                        }
                    }
                } else if self.sheet_edit.is_some() {
                    // The press landed in the cell editor.
                } else if editing_text {
                    // Click-off commits the in-flight compose, then selection.
                    let outside = pointer
                        .map(|p| self.text_compose_outside(p, &xf))
                        .unwrap_or(false);
                    if outside {
                        self.finish_text_compose_on_click_away();
                        if let Some(w) = wp {
                            let mods = ui.input(|i| i.modifiers);
                            if !self.try_dock_embed_click(ui.ctx(), w) {
                                self.board_click(w, mods);
                            }
                        }
                    }
                } else if let Some(w) = wp {
                    let mods = ui.input(|i| i.modifiers);
                    if !self.try_dock_embed_click(ui.ctx(), w) {
                        self.board_click(w, mods);
                    }
                }
            }
        }
        if ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary)) {
            self.board_align_eat_press = false;
            if self.sheet_resize.is_some() {
                self.finish_sheet_resize();
            }
        }
        // Two quick picks on curve grips or closed-form vertices are two
        // picks (P1.curve.grips, P1.shape.vertex-style), not the canvas
        // palette or text editing.
        let on_pick_point = || {
            self.board_tool == BoardTool::Select
                && ui
                    .input(|i| i.pointer.press_origin())
                    .or(pointer)
                    .is_some_and(|p| self.curve_grip_under(p) || self.hovered_vertex(p).is_some())
        };
        if resp.double_clicked() && !zoom_tool && !web_capture && !grip_released && !on_pick_point()
        {
            let on_context = pointer.is_some_and(|p| self.context_auto_under(p, &xf).is_some());
            if !on_context {
                if let Some(w) = wp {
                    self.board_double_click(w);
                }
            }
        }

        // Crosshair while a drawing tool is armed (D10). The Line tool also
        // resolves its rubber-band cursor on plain hover (a live press
        // updates through update_gesture instead).
        if board_place::armed_cursor(self.board_tool) == board_place::ArmedCursor::Crosshair
            && resp.hovered()
            && !panning
            && !zoom_tool
        {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        if matches!(self.board_tool, BoardTool::Trim | BoardTool::Split)
            && resp.hovered()
            && !panning
            && !zoom_tool
        {
            if let Some(w) = wp {
                let shift = ui.input(|i| i.modifiers.shift);
                self.trim_hover(w, shift);
            }
        }
        if self.board_tool == BoardTool::Line
            && resp.hovered()
            && !panning
            && !zoom_tool
            && self.board_drag.is_none()
        {
            if let Some(w) = wp {
                let shift = ui.input(|i| i.modifiers.shift);
                if self.line_draft.is_some() {
                    self.line_hover(w, shift);
                } else {
                    let _ = self.resolve_point_snap(w, &[], None, false, false);
                }
            }
        }
        if matches!(
            self.board_tool,
            BoardTool::Polyline | BoardTool::Arc | BoardTool::BezierSpan
        ) && resp.hovered()
            && !panning
            && !zoom_tool
            && self.board_drag.is_none()
        {
            if let Some(w) = wp {
                let from = self.pending_segment_origin();
                let _ = self.resolve_segment_point(from, w, self.shift_down);
            }
        }
        if matches!(
            self.board_tool,
            BoardTool::Frame
                | BoardTool::RectShape
                | BoardTool::Ellipse
                | BoardTool::AgentPortal
                | BoardTool::WebPortal
                | BoardTool::AtlasPortal
                | BoardTool::SlatePortal
                | BoardTool::Text
                | BoardTool::Sticky
        ) && resp.hovered()
            && !panning
            && !zoom_tool
        {
            // Area tools never take F8/Shift as ortho (Shift is aspect).
            // DragScale snaps the live rect; GhostFollow snaps the hotspot.
            match &self.board_drag {
                Some(BoardDrag::Draw {
                    start_world, tool, ..
                }) => {
                    if let Some(w) = wp {
                        let start = *start_world;
                        let tool = *tool;
                        let _ = self.resolve_draw_rect(
                            start,
                            w,
                            tool,
                            self.shift_down,
                            board_place::draws_from_center(tool, self.ctrl_down),
                        );
                    }
                }
                None => {
                    if let Some(w) = wp {
                        let _ = self.resolve_point_snap(w, &[], None, false, false);
                    }
                }
                Some(_) => {}
            }
        }
        let secondary = resp.secondary_clicked()
            && !page_capture
            && !self.turbo_pan.should_suppress_context_menu()
            && !hold_right;
        self.turbo_pan.acknowledge_context_menu();
        if secondary {
            if let (Some(p), Some(w)) = (pointer, wp) {
                if let Some(id) = self.board_pick_node(w.x, w.y) {
                    self.board_menu = Some((id, p));
                } else {
                    // Empty canvas: show/unlock-all discoverability menu
                    // (only when there is something to reveal).
                    let (hidden, locked) = self.hidden_locked_counts();
                    if hidden > 0 || locked > 0 {
                        self.board_empty_menu = Some(p);
                    }
                }
            }
        }

        // Crop-mode hover cursors: resize arrows on the window handles,
        // Grab/Grabbing over the interior (content pan).
        if self.board_crop.is_some() && resp.hovered() && !panning {
            if let Some(BoardDrag::CropEdge { id, handle, .. }) = &self.board_drag {
                if let Some(n) = self.doc().scene.node(*id) {
                    let geom = board_handles::selection_geom(&xf, n.rect, n.rotation_deg);
                    ui.ctx().set_cursor_icon(board_handles::cursor_for_resize(
                        board_handles::ResizeHandle::from_u8(*handle),
                        &geom,
                    ));
                }
            } else if let Some(p) = pointer {
                if let Some((_, handle, geom)) = self.crop_handle_under(p) {
                    ui.ctx()
                        .set_cursor_icon(board_handles::cursor_for_resize(handle, &geom));
                }
            }
        }

        brush_prof::lap("input");
        // Hover cursors / rotate zones. Selection is not required.
        self.board_hover_hit = None;
        self.board_hover_node = None;
        self.board_sides_hover = None;
        let hover_live = resp.hovered()
            && !panning
            && !zoom_tool
            && !editing_text
            && self.board_tool == BoardTool::Select
            && self.board_drag.is_none()
            && self.board_crop.is_none();
        let align_hovered = if hover_live {
            self.hover_align_widget(pointer, ui.ctx())
        } else {
            self.board_align_hover = None;
            false
        };
        if hover_live && !align_hovered {
            let wire_grip_hovered = self
                .wire_grips
                .as_ref()
                .is_some_and(|g| g.hovered.is_some());
            self.hover_transform_chrome(pointer, &xf, ui.ctx(), wire_grip_hovered);
        }
        if hover_live && self.board_hover_hit.is_none() {
            if let Some(w) = wp {
                self.video_pointer(w);
            }
        } else {
            self.video_clear_hover();
        }
        let hover_target = if hover_live && !align_hovered {
            self.hover_preview_target(wp)
        } else {
            None
        };
        let dt = ui.input(|i| i.unstable_dt);
        self.tick_hover_preview(hover_target, dt, ui.ctx());
        self.tick_image_segment_hover(ui, &xf, hover_live && !align_hovered, pointer, wp);
        brush_prof::lap("hover");

        // Adjustment previews must show authored color/stroke without selection tint.
        // One painter also fades wire/line endpoint fills, not just their outlines.
        let selection_painter = atlas_shell::selection_tools::selection_painter(
            &painter,
            egui::Id::new(("property_selection_fade", self.tab().id)),
            self.property_panel_fades_selection(),
        );

        if self.board_show_grid {
            let _grid = atlas_core::session_log::span("slate.board.grid");
            let grid_alpha = self.tab().grid_fade.alpha(now);
            self.paint_board_grid(&painter, rect, &palette, &xf, grid_alpha);
        }
        brush_prof::lap("grid");

        self.sheet_hits.clear();
        self.sheet_grips.clear();
        self.sheet_save_hit = None;

        // --- paint scene ---
        // Hidden nodes are skipped everywhere (paint, hit-test, marquee,
        // cycling, present, export) — scene-flags semantics matrix.
        // Viewport cull uses the spatial index (Art. II); off-screen nodes
        // are not cloned or painted.
        self.begin_agent_paint();
        let _nodes_span = atlas_core::session_log::span("slate.board.nodes");
        let mut nodes = self.board_paint_nodes(rect);
        // A Shift preview that continues a stroke paints that stroke inside
        // its own canvas once the workers have stamped it there, so the
        // scene copy stays out of those frames. Rasters land before this
        // decision, so one frame never paints the stroke from both.
        if let Some(canvas) = self.brush_live.as_mut() {
            canvas.take_landed(&mut self.brush_tiles);
        }
        if let Some(id) = self.brush_straight_extends() {
            let key = || {
                self.doc()
                    .scene
                    .node(id)
                    .and_then(board_path::node_stamp_key)
            };
            let ppp = ui.ctx().pixels_per_point();
            let covered = self
                .brush_live
                .as_ref()
                .is_some_and(|c| c.covers_anchor(id, key, &xf, rect, ppp));
            if covered {
                nodes.retain(|n| n.id != id);
            }
        }
        // Ctrl+F: dim non-matching nodes to ~35% at paint time only — the
        // opacity tweak lives on this per-frame clone, never in the scene
        // and never in the journal.
        if let Some(matches) = self.search_node_matches() {
            for n in &mut nodes {
                if !matches.contains(&n.id) {
                    n.opacity *= 0.35;
                }
            }
        }
        // Eraser scrub feedback: touched strokes render at 30% until release.
        if let Some(BoardDrag::Erase { touched, .. }) = &self.board_drag {
            for n in &mut nodes {
                if touched.contains(&n.id) {
                    n.opacity *= 0.3;
                }
            }
        }
        // A saved view held over a model: its picture sinks in (media D38).
        self.sink_view_drop_picture(&mut nodes);
        for n in nodes.iter().filter(|n| n.is_frame()) {
            self.paint_board_node(ui, &painter, &xf, n, true);
        }
        // Wires sit on the frame plate, then every other node paints over
        // them. A wire meets its host at the edge and does not cross that face.
        for n in nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Connector(_)))
        {
            self.paint_board_node(ui, &painter, &xf, n, true);
        }
        self.paint_wire_grips(&selection_painter, &xf);
        self.paint_agent_history_rails(&painter, &xf);
        brush_prof::lap("collect");
        board_path::tiles::paint_rest(self, ui, &painter, &xf, rect, &nodes);
        drop(_nodes_span);
        brush_prof::lap("nodes");
        self.paint_deck(ui.ctx(), &painter, &xf, palette.accent);
        // Ctrl+H feedback: just-hidden nodes ghost out over 150 ms.
        self.paint_hide_ghosts(ui, &painter, &xf);
        self.paint_context_retract(ui, &painter, &xf);
        self.paint_image_drop_capsules(ui, &painter);
        // The search hit the camera last flew to gets a select-tint ring.
        if let Some(super::overlays::SearchHit::Node(hit)) = self.search_current_hit() {
            if let Some(n) = self.doc().scene.node(hit) {
                let outline = self.node_screen_outline(ui.ctx(), &xf, n);
                let highlight_painter = if self.board_sel.contains(&hit) {
                    &selection_painter
                } else {
                    &painter
                };
                highlight_painter.add(egui::Shape::closed_line(
                    outline,
                    EStroke::new(canvas_scale::px(2.0, xf.z), palette.select),
                ));
            }
        }

        // Selection adornment: a subtle silhouette of each selected shape
        // (fillet / ellipse / path), never a union bounding box. Rotate
        // affordance stays on the single-select hover. An entered portal
        // or media frame (text, sheet, crop, live 3D) keeps the selection
        // but drops this cast. The crop-mode node draws its own adornment
        // (below). Locked nodes force-selected via Ctrl+Shift+click show a
        // grayed outline.
        let preview = atlas_shell::tokens::current().board_preview.clone();
        let select_tint = if self.selection_has_locked() {
            palette.select.gamma_multiply(0.45 * preview.select_opacity)
        } else {
            palette.select.gamma_multiply(preview.select_opacity)
        };
        let outline_w = canvas_scale::px(preview.select_line_weight, xf.z);
        if self.board_sel.len() == 1 && self.board_crop.is_none() {
            if let Some(id) = self.board_sel.iter().next() {
                if !self.frame_chrome_suppressed(*id) {
                    if let Some(n) = self.doc().scene.node(*id).cloned() {
                        self.paint_selected_node(
                            &selection_painter,
                            &xf,
                            &n,
                            select_tint,
                            outline_w,
                            true,
                        );
                    }
                }
            }
        } else {
            for id in self.board_sel.clone() {
                if self.frame_chrome_suppressed(id) {
                    continue;
                }
                if self.board_crop.is_some() && self.croppable_image(id) {
                    continue;
                }
                if let Some(n) = self.doc().scene.node(id) {
                    self.paint_selected_node(
                        &selection_painter,
                        &xf,
                        n,
                        select_tint,
                        outline_w,
                        false,
                    );
                }
            }
        }
        self.paint_agent_spawn_preview(&painter, &xf);
        self.paint_crosstalk(ui, &painter, &xf);
        self.paint_trim_preview(&painter, &xf);
        self.paint_align_widget(&painter, &xf, &palette, select_tint);
        self.paint_hover_or_segment(ui, &selection_painter, &xf, palette.select);

        // Crop-mode overlay: ghosted full image, scrim, crop border +
        // handles, content grabber.
        self.paint_crop_overlay(ui, &painter, &xf);

        // Mid-gesture cursor: keep the resize arrow / rotate glyph pinned
        // while the drag is active, even when the pointer leaves the handle.
        match &self.board_drag {
            Some(BoardDrag::Resize { id, handle, .. }) => {
                let (id, handle) = (*id, *handle);
                if let Some(n) = self.doc().scene.node(id) {
                    let geom = board_handles::selection_geom(&xf, n.rect, n.rotation_deg);
                    ui.ctx().set_cursor_icon(board_handles::cursor_for_resize(
                        board_handles::ResizeHandle::from_u8(handle),
                        &geom,
                    ));
                }
            }
            Some(BoardDrag::GroupResize { handle, .. }) => {
                let handle = *handle;
                if let Some(gb) = self.board_group_bounds() {
                    let geom = board_handles::selection_geom(&xf, gb, 0.0);
                    ui.ctx().set_cursor_icon(board_handles::cursor_for_resize(
                        board_handles::ResizeHandle::from_u8(handle),
                        &geom,
                    ));
                }
            }
            Some(BoardDrag::FilletRadius { id, vertex, .. }) => {
                if let Some(n) = self.doc().scene.node(*id) {
                    if let Some(edge) = self.node_grip_edge(n, *vertex) {
                        ui.ctx()
                            .set_cursor_icon(board_handles::cursor_along(Vec2::from(edge.dir)));
                    }
                    if let Some(p) = pointer {
                        let r = self.node_grip_amount(n, *vertex);
                        let label = format!("{} u", atlas_shell::selection_tools::number(r));
                        canvas_text::text(
                            &painter,
                            p + Vec2::new(12.0, -18.0),
                            Align2::LEFT_BOTTOM,
                            label,
                            canvas_scale::font(12.0, 1.0),
                            palette.select,
                        );
                    }
                }
            }
            _ => {}
        }

        // Rotate cursor: egui has no native rotate icon, so the OS cursor is
        // hidden over rotate zones and a circular-arrow glyph is painted at
        // the pointer (also during an active rotate drag).
        let rotate_cursor = matches!(
            self.board_hover_hit,
            Some(board_handles::BoardHitTarget::Rotate(_))
        ) || matches!(
            self.board_drag,
            Some(BoardDrag::Rotate { .. }) | Some(BoardDrag::GroupRotate { .. })
        );
        if rotate_cursor {
            if let Some(p) = pointer {
                ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                board_handles::paint_rotate_cursor(&painter, p);
            }
        }

        // Armed create-tool chrome (P2.GhostFollow): tinted pointer + small
        // silhouette until the first press. During DragScale the silhouette
        // yields to the live rubber-band; the pointer stays. The rectangle
        // and ellipse keep the OS crosshair in place of the tinted pointer.
        let armed_kind = board_place::ghost_kind(self.board_tool);
        let crosshair =
            board_place::armed_cursor(self.board_tool) == board_place::ArmedCursor::Crosshair;
        if armed_kind.is_some()
            && self.text_box_draft.is_none()
            && resp.hovered()
            && !panning
            && !zoom_tool
            && !rotate_cursor
            && !web_capture
        {
            if let Some(p) = pointer {
                // Glyph stays screen-space (P0.9); the hotspot is the snapped
                // world point so the armed cursor is not a naked hunt.
                let hot = self.board_point_snap.map(|w| xf.w2s(w)).unwrap_or(p);
                if !crosshair {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                    board_place::paint_armed_pointer(&painter, hot, palette.accent);
                }
                let drawing = match &self.board_drag {
                    Some(BoardDrag::Draw { start_screen, .. }) => {
                        (p - *start_screen).length() > board_place::place_tokens::DRAG_THRESHOLD
                    }
                    _ => false,
                };
                if !drawing {
                    if let Some(kind) = armed_kind {
                        board_place::paint_ghost(
                            &painter,
                            hot,
                            kind,
                            palette.accent,
                            palette.portal,
                            rgba32(board_color::STICKY_FILL),
                        );
                    }
                }
            }
        }

        // Smart guides: forcefield pulse from the impact, then fade.
        let guide_color = palette.accent;
        let now = ui.input(|i| i.time);
        let ff = atlas_shell::tokens::current().board_forcefield;
        if atlas_shell::tuning::forcefield_preview_locked() {
            let world = xf.s2w(rect.center());
            board_forcefield::sync_preview(&mut self.board_forcefield, world, now, ff);
        } else {
            board_forcefield::sync(&mut self.board_forcefield, &self.board_snap_guides, now, ff);
        }
        if board_forcefield::paint(
            &painter,
            &xf,
            rect,
            &self.board_forcefield,
            now,
            ff,
            guide_color,
        ) {
            ui.ctx().request_repaint();
        }
        if let Some(hit) = self.board_osnap_hit {
            board_osnap::paint_osnap_marker(&painter, &xf, hit, guide_color.gamma_multiply(0.85));
        }

        // Draw-gesture preview.
        if let (
            Some(BoardDrag::Draw {
                start_world,
                start_screen,
                tool,
            }),
            Some(w),
        ) = (&self.board_drag, wp)
        {
            let travel = pointer.map(|p| (p - *start_screen).length()).unwrap_or(0.0);
            if travel <= board_place::place_tokens::DRAG_THRESHOLD {
                // Still a click: keep the ghost, no MIN_DRAW speck.
            } else {
                let mods = ui.input(|i| i.modifiers);
                let accent = palette.accent;
                let preview = self
                    .board_draw_rect
                    .map(|r| xf.rect_w2s(r))
                    .unwrap_or_else(|| {
                        let end = self.preview_snap_point(w);
                        self.draw_preview_screen_rect(&xf, *start_world, end, *tool, mods)
                    });
                match tool {
                    BoardTool::Ellipse => {
                        painter.add(egui::Shape::closed_line(
                            ellipse_outline(preview),
                            EStroke::new(1.5_f32, accent),
                        ));
                    }
                    BoardTool::Polygon => {
                        painter.add(egui::Shape::closed_line(
                            board_place::polygon_outline(preview),
                            EStroke::new(1.5_f32, accent),
                        ));
                    }
                    _ => {
                        painter.rect_stroke(
                            preview,
                            0.0,
                            EStroke::new(1.5_f32, accent),
                            egui::StrokeKind::Inside,
                        );
                    }
                }
            }
        }

        if let Some(draft) = &self.text_box_draft {
            self.paint_text_box_draft(&painter, &xf, draft);
        }

        let draft_painter = self.image_paint_draft_painter(ui.ctx(), &painter, &xf);
        if self.board_tool == BoardTool::BezierSpan {
            let now = ui.input(|i| i.time);
            if let Some(left) = self.bezier_close_hover(pointer.filter(|_| resp.hovered()), now) {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_secs_f64(left.max(0.0)));
            }
        }
        let draft_builds = self.draft_ink.builds;
        if let Some(draft) = &self.board_path_draft {
            let zoom = self.tab().cam.z.max(f32::EPSILON);
            // Hovering: the point the next click places (resolve_segment_point
            // above, with ortho, the Tab lock, and snaps). A handle drag keeps
            // its own ortho against the press.
            let hover = self
                .board_drag
                .is_none()
                .then(|| self.board_point_snap.or(wp))
                .flatten();
            let cursor = hover
                .or_else(|| self.board_osnap_hit.map(|h| h.point))
                .or_else(|| {
                    if board_snap::effective_ortho(self.board_ortho, self.shift_down) {
                        let from = match draft {
                            board_path::BoardPathDraft::Polyline { points, .. } => {
                                points.last().copied()
                            }
                            board_path::BoardPathDraft::Bezier {
                                anchors, placing, ..
                            } => anchors
                                .last()
                                .map(|(p, _)| *p)
                                .or_else(|| placing.map(|(p, _)| p)),
                            _ => None,
                        };
                        if let (Some(last), Some(w)) = (from, wp) {
                            return Some(board_snap::ortho_snap_point(last, w));
                        }
                    }
                    wp
                });
            let close_first = match draft {
                board_path::BoardPathDraft::Bezier { anchors, .. } if anchors.len() >= 2 => cursor
                    .is_some_and(|c| {
                        (c - anchors[0].0).length() * zoom <= board_snap::SNAP_SCREEN_PX
                    }),
                _ => false,
            };
            let style = board_path::PathDraftPaintStyle {
                tip: self.placed_tip(
                    self.armed_stroke_tool()
                        .unwrap_or(slate_doc::StrokeTool::Polyline),
                ),
                overlay: super::path_edit_overlay::PathEditAnchorColors {
                    select: palette.select,
                    bg: palette.bg,
                    accent: palette.accent,
                    sub: palette.sub,
                },
                zoom,
                close_first_anchor: close_first,
            };
            board_path::paint_path_draft(
                &draft_painter,
                &xf,
                draft,
                cursor,
                style,
                &mut self.draft_ink,
            );
        }
        // Line draft: rubber band in the fg color the committed stroke will
        // use (D09).
        if self.board_tool == BoardTool::Line {
            if let Some((a, b, tips)) = self.line_draft_preview() {
                self.draft_ink.paint_line(&draft_painter, &xf, a, b, tips);
            }
        }
        // Pen Shift segment: the same rubber band, from the last Pen stroke.
        if let Some((a, b, tips)) = self.pen_line_preview() {
            self.draft_ink.paint_line(&draft_painter, &xf, a, b, tips);
        }
        // The Tab direction lock's padlock beside the pointer, any tool (D10).
        if let (Some(p), true) = (pointer, resp.hovered()) {
            self.paint_draft_lock_glyph(&painter, p);
        }
        if let (Some(BoardDrag::FreehandPen { stroke }), Some(w)) = (&self.board_drag, wp) {
            let now = self.placed_tip(slate_doc::StrokeTool::Pen);
            let cursor_tip = stroke.tip_at(w, now);
            self.draft_ink.paint_pen(
                &draft_painter,
                &xf,
                &stroke.points,
                &stroke.tips,
                w,
                cursor_tip,
            );
        } else if !matches!(self.board_drag, Some(BoardDrag::FreehandPen { .. })) {
            self.draft_ink.forget_pen();
        }
        let built = self.draft_ink.builds.wrapping_sub(draft_builds);
        self.path_mesh_cache.tess_misses = self.path_mesh_cache.tess_misses.saturating_add(built);
        // Brush drag preview: the screen-aligned canvas holds the same radial
        // stamp the release stores, each sample at its own tip. A Shift
        // segment that continues a stroke draws that stroke into the canvas
        // too, so the joint shows the committed max-coverage result.
        let live_freehand = matches!(
            &self.board_drag,
            Some(BoardDrag::FreehandBrush { stroke }) if !stroke.points.is_empty()
        );
        let live_line = wp.and_then(|w| {
            let end = self.brush_straight_end(w, self.shift_down)?;
            let (from, start, _) = self.brush_straight_from()?;
            Some((from, start, end))
        });
        match (live_freehand, live_line) {
            (true, _) => {
                let canvas = board_path::BrushLiveCanvas::ensure(
                    &mut self.brush_live,
                    &mut self.brush_tiles,
                    &draft_painter,
                    &xf,
                    rect,
                    None,
                    None,
                    Vec::new,
                );
                if let Some(BoardDrag::FreehandBrush { stroke }) = &self.board_drag {
                    canvas.add_freehand(&stroke.points, &stroke.tips);
                }
                canvas.paint(&draft_painter, &xf);
            }
            (false, Some((from, start, w))) => {
                let end = self.tip_now();
                let anchor_id = self.brush_straight_extends().filter(|id| {
                    self.doc().scene.node(*id).is_some_and(|n| !n.hidden)
                        || self.session_layer_mark(*id).is_some()
                });
                let ppp = ui.ctx().pixels_per_point();
                let tolerance = (0.5 / (xf.z * ppp).max(1.0e-3)) as f64;
                let scene = &self.tab().doc.scene;
                let anchor_node = anchor_id.and_then(|id| scene.node(id).cloned());
                let anchor_key = anchor_node.as_ref().and_then(board_path::node_stamp_key);
                let canvas = board_path::BrushLiveCanvas::ensure(
                    &mut self.brush_live,
                    &mut self.brush_tiles,
                    &draft_painter,
                    &xf,
                    rect,
                    anchor_id,
                    anchor_key,
                    || match anchor_node.as_ref().map(|n| (n, &n.kind)) {
                        Some((n, NodeKind::Shape(s))) => match s.path.as_ref() {
                            Some(p) => board_path::stamped_contours(n, s, p, tolerance),
                            None => Vec::new(),
                        },
                        _ => Vec::new(),
                    },
                );
                canvas.set_line(
                    vector_ink::TipPoint {
                        pos: [from.x, from.y],
                        tip: start.stamp(),
                    },
                    vector_ink::TipPoint {
                        pos: [w.x, w.y],
                        tip: end.stamp(),
                    },
                );
                canvas.pump(&mut self.brush_tiles, ui.ctx());
                canvas.paint(&draft_painter, &xf);
            }
            _ => {
                if let Some(canvas) = self.brush_live.as_mut() {
                    canvas.park();
                    canvas.pump(&mut self.brush_tiles, ui.ctx());
                }
            }
        }
        board_path::paint_erase_band(self, &draft_painter, &xf);
        // Wire drag preview (rubber-band bezier, snap ring, modifier glyph).
        if let Some(BoardDrag::Wire(wd)) = &self.board_drag {
            let mods = ui.input(|i| i.modifiers);
            self.paint_wire_drag(&painter, &xf, wd, mods);
        }
        // Direct-selection anchor adornment (A tool).
        if self.board_tool == BoardTool::DirectSelect {
            self.paint_direct_overlay(&painter, &xf);
        }
        if self.board_tool == BoardTool::Select {
            self.paint_picked_edges(&painter, &xf);
            self.paint_curve_grips(&painter, &xf);
        }

        // Ortho feedback: subtle hash ticks through the drag origin along
        // the snapped axis while an ortho-constrained drag is live.
        if let Some((origin, axis)) = self.ortho_feedback {
            let o = xf.w2s(origin);
            let a = axis.normalized();
            let dir = egui::Vec2::new(a.x, a.y);
            let perp = egui::Vec2::new(-dir.y, dir.x);
            let tint = palette.accent.gamma_multiply(0.6);
            painter.add(egui::Shape::dashed_line(
                &[o - dir * 72.0, o + dir * 72.0],
                EStroke::new(1.0_f32, tint),
                6.0,
                6.0,
            ));
            for k in [-48.0f32, -24.0, 0.0, 24.0, 48.0] {
                let c = o + dir * k;
                painter.line_segment(
                    [c - perp * 3.5, c + perp * 3.5],
                    EStroke::new(1.0_f32, tint),
                );
            }
        }

        // Marquee preview (node marquee and the A-tool anchor marquee).
        let marquee_start = match &self.board_drag {
            Some(BoardDrag::Marquee { start_screen, .. }) => Some(*start_screen),
            Some(BoardDrag::Direct(super::board_direct::DirectDrag::Marquee {
                start_screen,
                ..
            })) => Some(*start_screen),
            _ => None,
        };
        if let (Some(start_screen), Some(p)) = (marquee_start, pointer) {
            let r = Rect::from_two_pos(start_screen, p);
            let node_marquee = matches!(self.board_drag, Some(BoardDrag::Marquee { .. }));
            let crossing = node_marquee && p.x < start_screen.x;
            let tokens = atlas_shell::tokens::current().board_marquee;
            let color = if crossing {
                tokens.crossing_color(palette.dark_mode)
            } else {
                palette.select
            };
            painter.rect_filled(r, 0.0, color.gamma_multiply(tokens.fill_alpha));
            if crossing {
                painter.add(egui::Shape::dashed_line(
                    &[
                        r.left_top(),
                        r.right_top(),
                        r.right_bottom(),
                        r.left_bottom(),
                        r.left_top(),
                    ],
                    EStroke::new(1.0_f32, color),
                    tokens.dash_on,
                    tokens.dash_off,
                ));
            } else {
                painter.rect_stroke(
                    r,
                    0.0,
                    EStroke::new(1.0_f32, color),
                    egui::StrokeKind::Inside,
                );
            }
        }

        // Tool cursors: tip circle for Brush/Eraser/Smooth/Pen (the OS
        // cursor hides under it), sampling ring for the eyedropper (also
        // spring-loaded via Alt while Brush is armed).
        // The size HUD and color wheel are pointer-attached chrome.
        if let Some(p) = pointer {
            if self.brush_hud.is_some() {
                self.paint_brush_hud(&painter, p, palette.accent);
            } else if rect.contains(p) && !panning && !zoom_tool && !self.pointer_on_shape_chrome(p)
            {
                if let Some(w) = wp {
                    if self.eyedropper_active() {
                        self.paint_eyedropper_cursor(&painter, p, w);
                    } else if board_place::armed_cursor(self.board_tool)
                        == board_place::ArmedCursor::TipCircle
                    {
                        let _cursor = atlas_core::session_log::span("slate.board.brush_cursor");
                        if resp.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                        }
                        self.paint_width_cursor(&painter, p);
                    }
                }
            }
        }

        // 3D viewport captions (double-click hint, measure prompt, auto-lock).
        self.model_status_hints(ui, &xf);
        self.place_enscape_window(ui, &xf);

        // In-viewport measurement overlays (live only).
        self.paint_model_measurements(&painter, &xf);
        self.paint_view_drop_file(&painter, &xf);

        // Empty-board hint.
        if self.doc().scene.is_empty() {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "An open board — choose Frame in the create toolbar for a slide,\n\
                 drop files anywhere, or place images from the Grid view (right-click).",
                FontId::proportional(14.0),
                palette.sub,
            );
        }

        // Shared minimap overlay (M): board model = node rects by kind.
        if let Some(model) = self
            .minimap_on
            .then(|| self.board_minimap_model())
            .flatten()
        {
            self.show_minimap(ui, rect, model);
        }

        // Overlays. (The create toolbar now lives in the shared bottom dock —
        // see `ui/tools.rs::floating_tools_dock`.)
        self.frame_custom_dialog(ui.ctx(), rect);
        self.sheet_edit_overlay(ui.ctx());
        self.sheet_prompt_frame(ui.ctx());
        self.text_edit_overlay(ui.ctx(), &xf);
        self.wire_label_overlay(ui.ctx(), &xf);
        self.board_action_menu(ui.ctx());
        self.board_empty_canvas_menu(ui.ctx());

        brush_prof::lap("tail");
        if self
            .textures
            .values()
            .any(|t| matches!(t, ThumbState::Pending))
        {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(120));
        }
    }
    fn update_gesture(&mut self, world: Pos2, mods: egui::Modifiers) {
        let deck_zoom = self.tab().cam.z;
        if let Some(BoardDrag::DeckStroke { points, .. }) = &mut self.board_drag {
            let far = points
                .last()
                .is_none_or(|p| (*p - world).length() * deck_zoom >= 0.5);
            if far {
                points.push(world);
            }
            return;
        }
        if matches!(
            self.board_drag,
            Some(BoardDrag::FreehandPen { .. } | BoardDrag::FreehandBrush { .. })
        ) {
            // A tip chord's scrub is not ink, nor is the frame it closes on.
            if self.brush_hud.is_some() || std::mem::take(&mut self.freehand_resume) {
                return;
            }
            let zoom = self.tabs[self.active_tab].cam.z;
            let far = |last: Pos2| {
                (world - last).length() * zoom >= board_path::FREEHAND_SAMPLE_SPACING_PX
            };
            let pen_tip = self.placed_tip(slate_doc::StrokeTool::Pen);
            let brush_tip = self.tip_now().span();
            match &mut self.board_drag {
                Some(BoardDrag::FreehandPen { stroke }) if far(stroke.last()) => {
                    stroke.push(world, pen_tip, zoom);
                }
                Some(BoardDrag::FreehandBrush { stroke }) if far(stroke.last()) => {
                    stroke.push(world, brush_tip, zoom);
                }
                _ => {}
            }
            return;
        }
        if let Some(BoardDrag::BezierAnchor { press }) = &self.board_drag {
            let mut w = world;
            if board_snap::effective_ortho(self.board_ortho, mods.shift) {
                w = board_snap::ortho_snap_point(*press, world);
            }
            self.bezier_anchor_move(*press, w, mods.alt);
            return;
        }
        if let Some(BoardDrag::BezierEdit {
            hit,
            start,
            anchors0,
        }) = &self.board_drag
        {
            let (hit, start, anchors0) = (*hit, *start, anchors0.clone());
            self.bezier_draft_edit(hit, start, &anchors0, world, mods);
            return;
        }
        if matches!(self.board_drag, Some(BoardDrag::LineDraw { .. })) {
            self.line_hover(world, mods.shift);
            return;
        }
        if let Some(BoardDrag::Draw {
            start_world, tool, ..
        }) = &self.board_drag
        {
            let start = *start_world;
            let tool = *tool;
            let _ = self.resolve_draw_rect(
                start,
                world,
                tool,
                mods.shift,
                board_place::draws_from_center(tool, mods.ctrl),
            );
            return;
        }
        if let Some(BoardDrag::LineGrip { id, end, .. }) = &self.board_drag {
            let (id, end) = (*id, *end);
            self.line_grip_update(id, end, world, mods.shift);
            return;
        }
        // Eraser scrub: accumulate strokes under the circle (removed on
        // release; Esc cancels with no journal).
        if matches!(self.board_drag, Some(BoardDrag::Erase { .. })) {
            self.update_erase(world);
            return;
        }
        if matches!(self.board_drag, Some(BoardDrag::Smooth { .. })) {
            self.update_smooth(world);
            return;
        }
        if matches!(self.board_drag, Some(BoardDrag::Wire(_))) {
            let Some(BoardDrag::Wire(mut wd)) = self.board_drag.take() else {
                unreachable!();
            };
            self.wire_drag_update(&mut wd, world, mods.shift);
            self.board_drag = Some(BoardDrag::Wire(wd));
            return;
        }
        if matches!(self.board_drag, Some(BoardDrag::Direct(_))) {
            self.update_direct_drag(world, mods);
            return;
        }
        match &self.board_drag {
            Some(BoardDrag::Move {
                ids,
                before,
                start_world,
                dup,
                ..
            }) => {
                let ids = ids.clone();
                let before = before.clone();
                let start = *start_world;
                let dup = *dup;
                let mut d = world - start;
                // Ortho (F8, Shift inverts): the drag vector snaps to 45°
                // steps from the gesture origin.
                let ortho = board_snap::effective_ortho(self.board_ortho, mods.shift);
                let ortho_axis = board_snap::ortho_axis(d);
                if ortho {
                    d = board_snap::ortho_snap_vec(d);
                    self.ortho_feedback = Some((start, ortho_axis));
                }
                let mut pairs: Vec<(NodeId, WorldRect)> = ids
                    .iter()
                    .zip(before.iter())
                    .map(|(id, b)| (*id, b.rect.translated(d.x, d.y)))
                    .collect();

                // Object snaps beat smart guides; both yield to Alt (Rhino).
                let snap_off = mods.alt || dup;
                let mut osnap_moved = false;
                if !snap_off {
                    let rects: Vec<WorldRect> = pairs.iter().map(|(_, r)| *r).collect();
                    if let Some(union) = board_snap::union_rect(&rects) {
                        let set = self.board_osnap;
                        let radius = self.osnap_radius_world();
                        if let Some((snapped, hit)) = board_osnap::snap_bbox_osnap(
                            union,
                            &self.doc().scene,
                            &ids,
                            set,
                            radius,
                        ) {
                            let mut ax = snapped.x - union.x;
                            let mut ay = snapped.y - union.y;
                            if ortho {
                                let along = ax * ortho_axis.x + ay * ortho_axis.y;
                                ax = ortho_axis.x * along;
                                ay = ortho_axis.y * along;
                            }
                            if ax != 0.0 || ay != 0.0 {
                                for (_, r) in pairs.iter_mut() {
                                    r.x += ax;
                                    r.y += ay;
                                }
                            }
                            self.board_osnap_hit = Some(hit);
                            osnap_moved = true;
                        } else if self.board_smart_guides {
                            let all = self.board_node_rects();
                            let (snapped, guides) =
                                board_snap::snap_bbox_scoped(union, &ids, &all, self.snap_scope());
                            let mut ax = snapped.x - union.x;
                            let mut ay = snapped.y - union.y;
                            if ortho {
                                let along = ax * ortho_axis.x + ay * ortho_axis.y;
                                ax = ortho_axis.x * along;
                                ay = ortho_axis.y * along;
                            }
                            if ax != 0.0 || ay != 0.0 {
                                for (_, r) in pairs.iter_mut() {
                                    r.x += ax;
                                    r.y += ay;
                                }
                            }
                            self.board_snap_guides = guides;
                        }
                    }
                }
                // Grid snap would pull the origin off the constrained axis,
                // so ortho suspends it. Object snap overrides grid.
                if self.board_snap_grid && !snap_off && !ortho && !osnap_moved {
                    for (_, r) in pairs.iter_mut() {
                        *r = board_snap::snap_rect_origin(*r, true);
                    }
                }

                if !snap_off
                    && !ortho
                    && before
                        .iter()
                        .all(|n| slate_doc::agent_chat::agent(n).is_some())
                {
                    if let Some((_, first)) = pairs.first() {
                        let snapped = board_snap::agent_datum(
                            *first,
                            &ids,
                            &self.doc().scene,
                            self.board_xf().z,
                        );
                        let delta = Vec2::new(snapped.x - first.x, snapped.y - first.y);
                        for (_, rect) in &mut pairs {
                            rect.x += delta.x;
                            rect.y += delta.y;
                        }
                    }
                }
                self.bumper_drag(&ids, &before, &mut pairs, mods.alt);
                let scene = &mut self.doc_mut().scene;
                for ((id, r), b) in pairs.into_iter().zip(before.iter()) {
                    if let Some(n) = scene.node_mut(id) {
                        // Free connector endpoints travel with the drag
                        // (anchored ends stay glued — geometry is derived).
                        if let (NodeKind::Connector(c), NodeKind::Connector(cb)) =
                            (&mut n.kind, &b.kind)
                        {
                            let dd = Vec2::new(r.x - b.rect.x, r.y - b.rect.y);
                            for (end, end_b) in [(&mut c.a, &cb.a), (&mut c.b, &cb.b)] {
                                if let slate_doc::scene::ConnectorEnd::Free { point } = end_b {
                                    *end = slate_doc::scene::ConnectorEnd::Free {
                                        point: [point[0] + dd.x, point[1] + dd.y],
                                    };
                                }
                            }
                        }
                        n.rect = r;
                    }
                }
                self.update_image_drop_offer(world, &ids);
            }
            Some(BoardDrag::ModelOrbit { id, last_screen }) => {
                let id = *id;
                let last = *last_screen;
                let xf = self.board_xf();
                let screen = xf.w2s(world);
                let delta = screen - last;
                if delta != Vec2::ZERO {
                    let viewport_h = self
                        .doc()
                        .scene
                        .node(id)
                        .map(|n| n.rect.h * xf.z)
                        .unwrap_or(1.0);
                    let pan_mode = self.shift_down;
                    self.model_drag(id, delta.x, delta.y, pan_mode, viewport_h);
                    if let Some(BoardDrag::ModelOrbit { last_screen, .. }) = &mut self.board_drag {
                        *last_screen = screen;
                    }
                }
            }
            Some(BoardDrag::ModelMeasure { .. }) | Some(BoardDrag::DeckStroke { .. }) => {}
            Some(BoardDrag::Resize {
                id, before, handle, ..
            }) => {
                let node_id = *id;
                let handle = *handle;
                let before_rect = before.rect;
                let rotation_deg = before.rotation_deg;
                // A picture dragged past its opposite edge mirrors instead of
                // clamping (P1.node.transform); other kinds keep the clamp.
                let mirrors = matches!(before.kind, NodeKind::Image(_))
                    && slate_doc::mirror::node_mirrors(self.doc(), before);
                // Corner drags scale proportionally by default; Shift frees
                // the aspect (distortion). Edge drags are single-axis, with
                // Shift locking the aspect instead.
                let is_corner = matches!(handle, 0 | 2 | 4 | 6);
                let lock_aspect = if is_corner { !mods.shift } else { mods.shift };
                let from_center = mods.ctrl;
                let resize = |pointer: Pos2| {
                    if mirrors {
                        board_snap::resize_from_handle_mirroring(
                            before_rect,
                            pointer,
                            handle,
                            MIN_DRAW,
                            lock_aspect,
                            from_center,
                            rotation_deg,
                        )
                    } else {
                        let r = board_snap::resize_from_handle(
                            before_rect,
                            pointer,
                            handle,
                            MIN_DRAW,
                            lock_aspect,
                            from_center,
                            rotation_deg,
                        );
                        (r, [false, false])
                    }
                };
                let (mut r, mut crossed) = resize(world);

                if !mods.alt {
                    if is_corner {
                        let rotated = rotation_deg.abs() > f32::EPSILON;
                        if !rotated && crossed == [false, false] {
                            let pointer = self.resolve_corner_scale(
                                world,
                                before_rect,
                                r,
                                handle,
                                MIN_DRAW,
                                lock_aspect,
                                from_center,
                                &[node_id],
                            );
                            (r, crossed) = resize(pointer);
                        } else {
                            let pointer =
                                self.resolve_point_snap(world, &[node_id], None, false, false);
                            (r, crossed) = resize(pointer);
                        }
                    } else if self.board_smart_guides {
                        let all = self.board_node_rects();
                        // Past the far edge, the moving edge is the opposite one.
                        let moving = if crossed[0] || crossed[1] {
                            (handle + 4) % 8
                        } else {
                            handle
                        };
                        let edges = board_snap::ResizeSnapEdges::for_handle(moving);
                        let (snapped, guides) = board_snap::snap_resize_rect_scoped(
                            r,
                            &[node_id],
                            &all,
                            self.snap_scope(),
                            edges,
                        );
                        r = snapped;
                        self.board_snap_guides = guides;
                    }
                }

                if mirrors {
                    self.sync_resize_mirror(node_id, crossed);
                }
                if let Some(n) = self.doc_mut().scene.node_mut(node_id) {
                    n.rect = r;
                }
            }
            Some(BoardDrag::CropEdge {
                id,
                before,
                handle,
                peers,
            }) => {
                let node_id = *id;
                let handle = *handle;
                let peers = peers.clone();
                let (before_rect, before_crop, rot) = match &before.kind {
                    NodeKind::Image(img) => (before.rect, img.crop, before.rotation_deg),
                    _ => return,
                };
                // Rotated nodes: do the rect math in the node's local axes
                // about the gesture-start center (see board_crop docs).
                let (cx, cy) = before_rect.center();
                let local = board_crop::to_local(world.x, world.y, cx, cy, rot);
                let (r, c) = board_crop::edge_drag(before_rect, before_crop, handle, local);
                if let Some(n) = self.doc_mut().scene.node_mut(node_id) {
                    n.rect = r;
                    if let NodeKind::Image(img) = &mut n.kind {
                        img.crop = c;
                    }
                }
                for peer in peers {
                    let NodeKind::Image(img) = &peer.kind else {
                        continue;
                    };
                    let (rect, crop) = board_crop::place_crop(peer.rect, img.crop, c);
                    if let Some(n) = self.doc_mut().scene.node_mut(peer.id) {
                        n.rect = rect;
                        if let NodeKind::Image(live) = &mut n.kind {
                            live.crop = crop;
                        }
                    }
                }
            }
            Some(BoardDrag::CropPan {
                id,
                before,
                start_world,
            }) => {
                let node_id = *id;
                let (before_rect, before_crop, rot) = match &before.kind {
                    NodeKind::Image(img) => (before.rect, img.crop, before.rotation_deg),
                    _ => return,
                };
                let d = world - *start_world;
                let delta = board_crop::delta_local(d.x, d.y, rot);
                let c = board_crop::pan_drag(before_rect, before_crop, delta);
                if let Some(n) = self.doc_mut().scene.node_mut(node_id) {
                    if let NodeKind::Image(img) = &mut n.kind {
                        img.crop = c;
                    }
                }
            }
            Some(BoardDrag::Rotate {
                id,
                before,
                start_angle,
            }) => {
                let node_id = *id;
                let (cx, cy) = before.rect.center();
                let angle = (world.y - cy).atan2(world.x - cx);
                let mut rot = before.rotation_deg + (angle - start_angle).to_degrees();
                while rot > 180.0 {
                    rot -= 360.0;
                }
                while rot < -180.0 {
                    rot += 360.0;
                }
                if !mods.alt {
                    rot = board_snap::snap_rotation_deg(rot, board_snap::ROTATION_SNAP_DEG);
                }
                if let Some(n) = self.doc_mut().scene.node_mut(node_id) {
                    n.rotation_deg = rot;
                }
            }
            Some(BoardDrag::GroupResize {
                ids,
                before,
                group_before,
                handle,
                ..
            }) => {
                let ids = ids.clone();
                let before = before.clone();
                let gb = *group_before;
                let handle = *handle;
                // Same convention as single-node resize: corners scale
                // proportionally by default, Shift distorts. Ctrl+Alt+Shift
                // is a dedicated group mode: the box still follows the
                // handle, but members keep their size and only translate.
                let is_corner = matches!(handle, 0 | 2 | 4 | 6);
                let reposition = mods.ctrl && mods.alt && mods.shift;
                let lock_aspect = if reposition {
                    is_corner
                } else if is_corner {
                    !mods.shift
                } else {
                    mods.shift
                };
                let from_center = !reposition && mods.ctrl;
                let mut pointer = world;
                if !mods.alt && is_corner {
                    let proposed = board_snap::resize_from_handle(
                        gb,
                        world,
                        handle,
                        MIN_DRAW,
                        lock_aspect,
                        from_center,
                        0.0,
                    );
                    pointer = self.resolve_corner_scale(
                        world,
                        gb,
                        proposed,
                        handle,
                        MIN_DRAW,
                        lock_aspect,
                        from_center,
                        &ids,
                    );
                }
                let mut new_group = board_snap::resize_from_handle(
                    gb,
                    pointer,
                    handle,
                    MIN_DRAW,
                    lock_aspect,
                    from_center,
                    0.0,
                );
                if !mods.alt && !is_corner && self.board_smart_guides {
                    let all = self.board_node_rects();
                    let edges = board_snap::ResizeSnapEdges::for_handle(handle);
                    let (snapped, guides) = board_snap::snap_resize_rect_scoped(
                        new_group,
                        &ids,
                        &all,
                        self.snap_scope(),
                        edges,
                    );
                    new_group = snapped;
                    self.board_snap_guides = guides;
                }
                let mut sx = new_group.w / gb.w.max(0.001);
                let mut sy = new_group.h / gb.h.max(0.001);
                if !reposition {
                    // No member may collapse below MIN_DRAW world units (but a
                    // member already smaller than that never blocks the gesture).
                    let min_w = before
                        .iter()
                        .map(|n| n.rect.w)
                        .fold(f32::INFINITY, f32::min);
                    let min_h = before
                        .iter()
                        .map(|n| n.rect.h)
                        .fold(f32::INFINITY, f32::min);
                    if min_w.is_finite() {
                        sx = sx.max((MIN_DRAW / min_w.max(0.001)).min(1.0));
                    }
                    if min_h.is_finite() {
                        sy = sy.max((MIN_DRAW / min_h.max(0.001)).min(1.0));
                    }
                }
                let mean = (sx + sy) * 0.5;
                let rects: Vec<WorldRect> = before.iter().map(|n| n.rect).collect();
                let scaled = board_snap::apply_group_box_scale(
                    &rects,
                    gb,
                    sx,
                    sy,
                    handle,
                    from_center,
                    reposition,
                );
                let scene = &mut self.doc_mut().scene;
                for ((id, b), r) in ids.iter().zip(before.iter()).zip(scaled.iter()) {
                    if let Some(n) = scene.node_mut(*id) {
                        n.rect = *r;
                        if !reposition {
                            // Text scales with the group; stroke widths stay
                            // fixed (CSS keeps stroke width on resize).
                            if let (NodeKind::Text(t), NodeKind::Text(tb)) = (&mut n.kind, &b.kind)
                            {
                                t.size = (tb.size * mean).max(4.0);
                            }
                        }
                    }
                }
            }
            Some(BoardDrag::GroupRotate {
                ids,
                before,
                center,
                start_angle,
            }) => {
                let ids = ids.clone();
                let before = before.clone();
                let (cx, cy) = *center;
                let start = *start_angle;
                let angle = (world.y - cy).atan2(world.x - cx);
                let mut delta = (angle - start).to_degrees();
                if !mods.alt {
                    delta = board_snap::snap_rotation_deg(delta, board_snap::ROTATION_SNAP_DEG);
                }
                let scene = &mut self.doc_mut().scene;
                for (id, b) in ids.iter().zip(before.iter()) {
                    if let Some(n) = scene.node_mut(*id) {
                        if Self::node_allows_rotation(b) {
                            let mut rot = b.rotation_deg + delta;
                            while rot > 180.0 {
                                rot -= 360.0;
                            }
                            while rot < -180.0 {
                                rot += 360.0;
                            }
                            n.rotation_deg = rot;
                        }
                        // Orbit the rect center around the group center;
                        // width/height are unchanged by rotation. Portals
                        // stay axis-aligned while they follow the group.
                        let (bx, by) = b.rect.center();
                        let (nx, ny) = board_snap::orbit_point((cx, cy), (bx, by), delta);
                        n.rect.x = nx - b.rect.w * 0.5;
                        n.rect.y = ny - b.rect.h * 0.5;
                    }
                }
            }
            Some(BoardDrag::FilletRadius { .. }) => self.update_fillet_drag(world, mods.shift),
            _ => {}
        }
    }
}
