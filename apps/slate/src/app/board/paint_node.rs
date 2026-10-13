//! Board scene-node interpreter.
use super::*;

impl SlateApp {
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
                self.paint_image_interaction_shadow(painter, xf, node, chrome);
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
                            painter,
                            &outline,
                            srect,
                            node.id,
                            img.item,
                            img.text_override.as_deref(),
                            &path,
                            corner,
                            pointer,
                            z,
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
                                    &mut self.path_mesh_cache,
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
                if !chrome || !self.paint_image_palette_stroke(painter, xf, node, img) {
                    self.paint_image_mask_stroke(painter, xf, node, img, &outline, alpha);
                }
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
                    let laid = styled_text_galley(
                        painter.ctx(),
                        shown,
                        styled_typeface_font(painter.ctx(), t.family, draw_size, t.style),
                        node.rect.w.max(0.0),
                        t.align,
                        fade(rgba32(t.color)),
                        z,
                        t.style,
                    );
                    // A sticky centers its block by moving where the cached
                    // galley paints, as `center_galley_vertically` does for the editor.
                    let dy = if sticky {
                        ((srect.height() - laid.rect.height()) * 0.5).max(0.0)
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
                crate::app::board_dock_embed::paint_dock_strip(ui, xf, node, strip, self);
            }
        }
    }
}
