//! Property palette presentation over the shared shell owners.
use super::*;

#[cfg(test)]
mod tests;

#[derive(Clone)]
pub(super) struct LastChrome {
    pub(super) bounds: WorldRect,
    pub(super) items: Vec<StripItem>,
    blister: bool,
    image: bool,
}

impl SlateApp {
    /// Runs before board input. Layout is recomputed from the host transform every frame.
    pub(crate) fn shape_properties_ui(&mut self, ui: &mut egui::Ui, xf: &BoardXf) -> bool {
        let ctx = ui.ctx().clone();
        self.clear_image_palette_frame(&ctx);
        self.sync_shape_properties();
        self.seed_document_colors();
        self.shape_properties.chrome_hits.clear();
        let editing_hosted_text = self.text_edit.as_ref().is_some_and(|(id, _)| {
            self.doc().scene.node(*id).is_some_and(|n| match &n.kind {
                NodeKind::Text(_) => true,
                NodeKind::Shape(s) => scene::shape_hosts_text(s),
                _ => false,
            })
        });
        // A sticky's first job is the caret. The fill/text strip returns
        // once editing ends and the note is simply selected.
        let editing_sticky = self.text_edit.as_ref().is_some_and(|(id, _)| {
            self.doc()
                .scene
                .node(*id)
                .is_some_and(|n| matches!(&n.kind, NodeKind::Text(t) if t.fill.is_some()))
        });
        let composing_text_box = self.text_box_draft.is_some();
        let vertex_bounds = self.picked_vertex_bounds();
        let direct_picks = self.board_tool == BoardTool::DirectSelect && vertex_bounds.is_some();
        let live = (self.board_drag.is_none() || image_palette::handles(self, true))
            && !editing_sticky
            && !composing_text_box
            && (editing_hosted_text
                || ((self.board_tool == BoardTool::Select
                    || direct_picks
                    || image_palette::editing(self))
                    && self.text_edit.is_none()))
            && !self.shape_properties.nodes.is_empty();
        image_palette::dismiss_if_inactive(&ctx, self, live);
        if live {
            let items = live_property_strip_items(self, &self.shape_properties.nodes);
            // D13 / D11: a panel whose squircle a vertex pick took off the
            // strip commits its pending edits and closes.
            if self
                .shape_properties
                .panel
                .is_some_and(|panel| !items.contains(&StripItem::Panel(panel)))
                && !self.shape_property_points().is_empty()
            {
                self.apply_shape_preview(&ctx, true);
            }
            if let Some(bounds) = vertex_bounds.or(self.shape_properties.bounds) {
                self.shape_properties.last_chrome = Some(LastChrome {
                    bounds,
                    items,
                    blister: property_strip::rectangular_blister(&self.shape_properties.nodes),
                    image: image_palette::handles(self, true),
                });
            }
        }
        let fade = chrome::strip_fade(&ctx, Id::new("selection_property_strip"), live);
        if fade <= 0.0 {
            if !live {
                self.shape_properties.last_chrome = None;
            }
            return false;
        }
        let z = xf.z;
        if canvas_scale::too_small(12.0 * z) {
            // The strip and its panel are not painted, but screen-constant
            // grips still take presses, so the D11 commit still runs. The
            // press keeps its board meaning.
            let overlay_open =
                self.shape_properties.panel.is_some() || self.shape_properties.number.is_some();
            if let (true, true, Some(grip)) =
                (live, overlay_open, self.property_dismiss_press(&ctx))
            {
                self.apply_shape_preview(&ctx, !grip);
            }
            return false;
        }
        let Some(chrome_state) = self.shape_properties.last_chrome.clone() else {
            return false;
        };
        // The image owner dismisses its own blister. Never fall back to the
        // generic strip while its selection is fading out.
        if chrome_state.image && !live {
            return false;
        }
        let theme = self.palette();
        let bounds = xf.rect_w2s(chrome_state.bounds);
        let enabled =
            live && !self.tab().read_only && self.shape_properties.nodes.iter().all(|n| !n.locked);
        if live && !enabled {
            self.shape_properties.preview.clear();
            self.shape_properties.edits.clear();
            self.shape_properties.number = None;
        }
        let items = chrome_state.items;
        if image_palette::handles(self, live) {
            let dimensions_capture = if image_palette::dimensions_visible(&ctx, self) {
                self.paint_property_dimensions(ui, xf, live, enabled)
            } else {
                self.shape_properties.number = None;
                false
            };
            return image_palette::show(self, ui, xf, bounds, enabled) || dimensions_capture;
        }
        let blister = chrome_state.blister;
        let strip = if blister {
            chrome::blister_tab_rect(bounds, z, items.len())
        } else if items.is_empty() {
            Rect::from_center_size(
                Pos2::new(bounds.center().x, bounds.top() - chrome::OBJECT_GAP * z),
                Vec2::ZERO,
            )
        } else {
            chrome::strip_rect(
                Pos2::new(bounds.center().x, bounds.top()),
                items.len(),
                z,
                fade,
            )
        };
        let mut requested_panel = None;
        let mut requested_frame = None;
        let mut requested_agent = None;
        let mut requested_measure = false;
        let mut requested_screenshot = false;
        let mut captures = false;
        if blister {
            let painter = ui.painter().clone().with_layer_id(egui::LayerId::new(
                chrome::CANVAS_CHROME_ORDER,
                Id::new("portal_property_blister"),
            ));
            let open = self.shape_properties.panel.is_some();
            chrome::paint_blister_tab(
                &painter,
                strip,
                z,
                theme,
                false,
                fade * (1.0 - chrome::strip_fade(&ctx, Id::new("portal_blister_clear"), open)),
            );
        }
        // P1.curve.grips: a painted grip under a button takes the press. The
        // board hit-tests a drag at its press origin, so a held press is
        // judged there too.
        let grip_probe = Self::property_pointer_probe(&ctx);
        let grip_under =
            live && grip_probe.is_some_and(|p| strip.contains(p) && self.property_grip_under(p));
        for (index, item) in items.iter().enumerate() {
            let r = if blister {
                chrome::blister_icon_rect(strip, index, items.len())
            } else {
                chrome::strip_button_rect(strip, index, z)
            };
            let (label, icon, active) = self.property_strip_button_info(item);
            let button_live = live && !grip_under;
            let response = if blister {
                let response = ui
                    .interact(
                        r,
                        Id::new(("shape_property", index)),
                        if button_live {
                            egui::Sense::click()
                        } else {
                            egui::Sense::hover()
                        },
                    )
                    .on_hover_text(label);
                let ink = if active {
                    theme.accent
                } else if response.hovered() {
                    theme.ink
                } else {
                    theme.sub
                };
                let painter = ui.painter().clone().with_layer_id(egui::LayerId::new(
                    chrome::CANVAS_CHROME_ORDER,
                    Id::new("portal_property_blister"),
                ));
                atlas_shell::icons::paint(&painter, r, icon, ink.gamma_multiply(fade));
                response
            } else {
                chrome::button(
                    ui,
                    r,
                    Id::new(("shape_property", index)),
                    label,
                    icon,
                    active,
                    z,
                    theme,
                    fade,
                    button_live,
                )
            };
            if button_live && response.clicked() {
                match item {
                    StripItem::Panel(panel) => requested_panel = Some(*panel),
                    StripItem::Frame(action) => requested_frame = Some(*action),
                    StripItem::Agent(expand) => requested_agent = Some(*expand),
                    StripItem::ModelMeasure => requested_measure = true,
                    StripItem::ModelScreenshot => requested_screenshot = true,
                }
            }
            captures |= button_live && ctx.pointer_latest_pos().is_some_and(|p| r.contains(p));
            if live {
                self.shape_properties.chrome_hits.push(r);
            }
        }
        if let Some(expand) = requested_agent {
            self.dispatch(
                &ctx,
                CommandId(if expand {
                    "portal.agent.expand_chat"
                } else {
                    "portal.agent.bundle_chat"
                }),
                None,
            );
        }
        if let Some(panel) = requested_panel {
            let was = self.shape_properties.panel;
            self.apply_shape_preview(&ctx, true);
            self.shape_properties.number = None;
            if was != Some(panel) {
                self.shape_properties.panel = Some(panel);
                if panel == Panel::Pages {
                    if let Some(item) = self
                        .shape_properties
                        .ids
                        .first()
                        .and_then(|id| self.node_pdf_item(*id))
                    {
                        self.shape_properties.pages_focus = item.pdf_page;
                    }
                }
            }
        }
        if let Some(action) = requested_frame {
            self.apply_shape_preview(&ctx, true);
            self.shape_properties.number = None;
            self.run_frame_strip_action(&ctx, action);
        }
        if requested_measure {
            self.apply_shape_preview(&ctx, true);
            self.shape_properties.number = None;
            if let Some(id) = self.shape_properties.ids.first().copied() {
                self.dispatch(
                    &ctx,
                    CommandId("board.model_measure"),
                    Some(id.0.to_string()),
                );
            }
        }
        if requested_screenshot {
            self.apply_shape_preview(&ctx, true);
            self.shape_properties.number = None;
            if let (Some(id), Some(p)) = (
                self.shape_properties.ids.first().copied(),
                ctx.pointer_latest_pos(),
            ) {
                self.open_model_screenshot_menu(id, p);
            }
        }
        captures |= self.paint_property_dimensions(ui, xf, live, enabled);
        if self.shape_properties.panel != Some(Panel::Filter) {
            self.shape_properties.filter_aim = None;
        }
        if live {
            if let Some(panel) = self.shape_properties.panel {
                if panel == Panel::Pages {
                    if let Some(node_id) = self.shape_properties.ids.first().copied() {
                        if let Some(node) = self.doc().scene.node(node_id).cloned() {
                            if let Some(path) =
                                self.node_pdf_item(node_id).map(|item| item.path.clone())
                            {
                                self.documents.request(&path);
                            }
                            let host = xf.rect_w2s(node.rect.rotated_bounds(node.rotation_deg));
                            let album = self.paint_pages_album(
                                ui,
                                node_id,
                                host,
                                z,
                                theme,
                                self.shape_properties.pages_focus,
                            );
                            self.shape_properties.pages_focus = album.focus;
                            captures |= album.hover;
                            if album.commit_page {
                                if let Some(item) = self.node_pdf_item(node_id) {
                                    self.dispatch(
                                        &ctx,
                                        CommandId("board.media.page"),
                                        Some(format!("{}:{}", item.id.0, album.focus)),
                                    );
                                }
                            }
                            if album.unbundle {
                                self.dispatch(
                                    &ctx,
                                    CommandId("board.media.unbundle"),
                                    Some(format!("{}:{}", node_id.0, album.focus)),
                                );
                            }
                        }
                    }
                } else {
                    let height = match panel {
                        Panel::Fill => chrome::FILL_HEIGHT,
                        Panel::Stroke => chrome::STROKE_HEIGHT,
                        Panel::Corners => chrome::CORNER_HEIGHT,
                        Panel::Wire => chrome::WIRE_HEIGHT,
                        Panel::Filter => chrome::FILTER_HEIGHT,
                        Panel::Brush | Panel::Layers => chrome::FILTER_HEIGHT,
                        Panel::Pages => 0.0,
                        Panel::AtlasFormat => chrome::ATLAS_FORMAT_HEIGHT,
                        Panel::Text => chrome::TEXT_HEIGHT,
                        Panel::Agent => chrome::AGENT_HEIGHT,
                        Panel::Bumper => chrome::CORNER_HEIGHT,
                        Panel::ModelDisplay => chrome::FILTER_CHIPS_HEIGHT,
                    };
                    if panel != Panel::Text {
                        self.shape_properties.text_family_open = false;
                        self.shape_properties.text_size_open = false;
                    }
                    let canvas = self.canvas_rect;
                    let kind = match panel {
                        Panel::Fill | Panel::Stroke | Panel::Corners | Panel::Filter => {
                            image_palette::panel_kind(Some(panel))
                        }
                        _ => chrome::ImagePanelKind::Custom {
                            main_height: height,
                        },
                    };
                    let layout =
                        blister.then(|| chrome::image_presentation_layout(bounds, z, kind));
                    let rect = layout.map_or_else(
                        || chrome::editor_placement(strip, bounds, height, z, canvas),
                        |layout| layout.editor_rect(z),
                    );
                    let area_rect = layout.map_or(rect, |layout| layout.overall);
                    let mut sample = None;
                    chrome::popup_area(&ctx, Id::new("shape_property_editor"), ui.layer_id())
                        .fixed_pos(area_rect.min)
                        .constrain(false)
                        .movable(false)
                        .fade_in(false)
                        .show(&ctx, |ui| {
                            ui.set_clip_rect(canvas);
                            ui.set_min_size(area_rect.size());
                            if let Some(layout) = &layout {
                                chrome::paint_blister_surface(
                                    ui.painter(),
                                    layout.overall,
                                    strip,
                                    z,
                                    theme,
                                    0.0,
                                );
                                if let Some(host) = self.shape_properties.nodes.first() {
                                    let (chamfer, radius) =
                                        scene::resolved_corner_effective(host, None);
                                    if host.rotation_deg.abs() < 0.001 {
                                        chrome::paint_blister_corner_backing(
                                            ui.painter(),
                                            Id::new(("portal_blister_corners", host.id.0)),
                                            bounds,
                                            radius * z,
                                            chamfer,
                                            theme,
                                        );
                                    }
                                }
                            }
                            ui.add_enabled_ui(enabled, |ui| {
                                sample =
                                    self.shape_property_body(ui, rect, panel, z, layout.as_ref());
                            });
                        });
                    captures |= ctx.pointer_latest_pos().is_some_and(|p| {
                        layout.map_or(rect, |layout| layout.overall).contains(p)
                            && canvas.contains(p)
                            || !grip_under
                                && self
                                    .shape_properties
                                    .chrome_hits
                                    .iter()
                                    .any(|hit| hit.contains(p))
                    });
                    if live {
                        self.shape_properties
                            .chrome_hits
                            .push(layout.map_or(rect, |layout| layout.overall));
                    }
                    if let Some(gesture) = sample {
                        self.start_property_desktop_sample(panel, gesture);
                    }
                }
            }
            let overlay_open =
                self.shape_properties.panel.is_some() || self.shape_properties.number.is_some();
            let press = self.property_dismiss_press(&ctx);
            let on_host = blister && ctx.pointer_latest_pos().is_some_and(|p| bounds.contains(p));
            if overlay_open && !captures && !on_host && press == Some(true) {
                self.apply_shape_preview(&ctx, false);
            } else if overlay_open && !captures && !on_host && press.is_some() {
                self.apply_shape_preview(&ctx, true);
                if let Some(p) = ctx.pointer_latest_pos() {
                    if self.canvas_rect.contains(p) {
                        let world = xf.s2w(p);
                        match self.board_pick_node(world.x, world.y) {
                            Some(id) => {
                                self.board_sel =
                                    crate::app::board_flags::expand_selection_to_groups(
                                        &self.doc().scene,
                                        &[id],
                                    )
                                    .into_iter()
                                    .collect();
                            }
                            None => self.board_sel.clear(),
                        }
                    }
                }
                captures = true;
            }
        }
        if captures && ctx.input(|i| i.pointer.any_pressed()) {
            self.board_align_eat_press = true;
        }
        captures
    }
}
