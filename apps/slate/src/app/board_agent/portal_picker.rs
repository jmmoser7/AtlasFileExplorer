//! Project and chat pick list.

use super::*;

impl SlateApp {
    #[allow(clippy::too_many_arguments)] // Existing portal paint adapter.
    pub(super) fn paint_agent_pick_list(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
        layout: &super::super::board_portal_chrome::PortalChromeLayout,
        node: &Node,
        portal: &PortalNode,
        _maximized: bool,
    ) {
        let projects = self.agents.project_picker == Some(node.id);
        let provider = portal
            .agent
            .as_ref()
            .map(|a| a.provider.as_str())
            .unwrap_or_default();
        let projects_list = self
            .agents
            .provider_recents
            .get(provider)
            .unwrap_or(&self.agents.recents);
        let chats = if projects {
            projects_list
                .iter()
                .map(|e| CursorChat {
                    id: e.path.to_string_lossy().into_owned(),
                    title: e.title.clone(),
                    updated_at: e.opened_at,
                })
                .collect()
        } else {
            self.agents
                .chat_picker
                .as_ref()
                .map(|p| p.chats.clone())
                .unwrap_or_default()
        };
        let visible = chats.len().min(6) as f32;
        // Capsules are 70% of the previous 40px row. Chrome shrinks with the button.
        let desired_h = 156.0 + visible * 34.0 + if projects { 34.0 } else { 0.0 };
        if !self.tab().read_only
            && self.board_drag.is_none()
            && ((node.rect.h - desired_h).abs() > 1.0 || node.rect.w < 380.0)
        {
            self.patch_nodes(&[node.id], |n| {
                n.rect.h = desired_h;
                n.rect.w = n.rect.w.max(380.0);
            });
        }
        let z = xf.z.max(0.01);
        let body = layout.body;
        let palette = self.palette();
        let interactive = !self.tab().read_only;
        let title_px = canvas_text::authored_px(15.0, z);
        let row_px = canvas_text::authored_px(13.5, z);
        let pad = canvas_scale::px(14.0, z);
        let row_h = canvas_scale::px(28.0, z);
        let gap = canvas_scale::px(6.0, z);
        let button_h = canvas_scale::px(28.0, z);
        let radius = canvas_scale::px(8.0, z);
        let div_y = body.top() + canvas_scale::px(34.0, z);
        painter.line_segment(
            [
                Pos2::new(body.left() + pad, div_y),
                Pos2::new(body.right() - pad, div_y),
            ],
            egui::Stroke::new((1.0 * z).max(1.0), palette.line.gamma_multiply(0.7)),
        );
        let heading = Rect::from_min_max(
            Pos2::new(body.left() + pad, div_y + canvas_scale::px(10.0, z)),
            Pos2::new(body.right() - pad, div_y + canvas_scale::px(36.0, z)),
        );
        let stack_bottom = body.bottom() - pad;
        let action_h = if projects {
            button_h * 2.0 + gap
        } else {
            button_h
        };
        let choose_button = Rect::from_min_max(
            Pos2::new(body.left() + pad, stack_bottom - action_h),
            Pos2::new(body.right() - pad, stack_bottom - action_h + button_h),
        );
        let just_build_button = projects.then(|| {
            Rect::from_min_max(
                Pos2::new(body.left() + pad, stack_bottom - button_h),
                Pos2::new(body.right() - pad, stack_bottom),
            )
        });
        let buttons_max = just_build_button
            .map(|r| r.max)
            .unwrap_or(choose_button.max);
        let list = Rect::from_min_max(
            Pos2::new(heading.left(), heading.bottom() + canvas_scale::px(4.0, z)),
            Pos2::new(
                heading.right(),
                choose_button.top() - canvas_scale::px(10.0, z),
            ),
        );
        if list.height() < 8.0 {
            return;
        }
        let heading_label = if projects {
            "Choose project"
        } else {
            "Choose conversation"
        };
        let action = if projects {
            "Choose another folder"
        } else {
            "New conversation"
        };
        let mut chosen: Option<Option<String>> = None;
        let mut just_build = false;
        let mut scrolled = None;
        if interactive && canvas_text::legible(row_px) {
            egui::Area::new(Id::new(("agent-pick-list", node.id.0)))
                .fixed_pos(heading.min)
                .constrain(false)
                .order(egui::Order::Middle)
                .show(ui.ctx(), |ui| {
                    let stack = Rect::from_min_max(heading.min, buttons_max);
                    ui.set_min_size(stack.size());
                    ui.set_max_size(stack.size());
                    ui.set_clip_rect(stack.intersect(ui.ctx().screen_rect()));
                    ui.style_mut().visuals = palette.visuals();
                    ui.spacing_mut().item_spacing.y = 0.0;
                    if canvas_text::legible(title_px) {
                        let galley = tracked_galley(
                            ui,
                            heading_label,
                            title_px,
                            palette.ink.gamma_multiply(0.78),
                            canvas_scale::px(0.4, z),
                            heading.width(),
                        );
                        let pos = Pos2::new(
                            heading.left(),
                            heading.center().y - galley.size().y * 0.5,
                        );
                        ui.painter().galley(pos, galley, palette.ink);
                    }
                    ui.add_space(heading.height());
                    let row_fill = palette.ink.gamma_multiply(if palette.dark_mode {
                        0.06
                    } else {
                        0.05
                    });
                    let row_hover = palette.ink.gamma_multiply(if palette.dark_mode {
                        0.11
                    } else {
                        0.09
                    });
                    let bar = canvas_scale::px(PICK_BAR_RESERVE, z);
                    let scroll = &mut ui.spacing_mut().scroll;
                    scroll.floating = true;
                    scroll.floating_allocated_width = bar;
                    scroll.bar_width = bar * 0.75;
                    scroll.floating_width = bar * 0.25;
                    scroll.bar_inner_margin = 0.0;
                    scroll.bar_outer_margin = bar * 0.125;
                    let offset = self.agents.pick_scroll.get(&node.id).copied().unwrap_or(0.0);
                    let out = egui::ScrollArea::vertical()
                        .id_salt(("agent-pick-scroll", node.id.0))
                        .vertical_scroll_offset(offset * z)
                        .max_height(list.height())
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = gap;
                            if let Some(error) = &self.agents.catalog_error {
                                ui.label(
                                    egui::RichText::new(error)
                                        .size(row_px)
                                        .color(palette.sub),
                                );
                            }
                            if chats.is_empty() {
                                let empty = if provider == "cursor" {
                                    "No agents for this folder yet. Chats in the Cursor window are a separate list."
                                } else if projects {
                                    "No recent projects yet."
                                } else {
                                    "No saved conversations for this project yet."
                                };
                                ui.label(
                                    egui::RichText::new(empty)
                                        .size(canvas_text::authored_px(12.5, z))
                                        .color(palette.sub),
                                );
                            }
                            let cols = agent_pick_columns(projects, chats.len());
                            let width = list.width() - bar;
                            let col_w =
                                ((width - gap * (cols - 1) as f32) / cols as f32).max(0.0);
                            let rows = chats.len().div_ceil(cols);
                            let grid_h = (rows as f32 * (row_h + gap) - gap).max(0.0);
                            let (grid, _) =
                                ui.allocate_exact_size(egui::vec2(width, grid_h), Sense::hover());
                            for (i, chat) in chats.iter().enumerate() {
                                let rect = Rect::from_min_size(
                                    grid.min
                                        + egui::vec2(
                                            (i % cols) as f32 * (col_w + gap),
                                            (i / cols) as f32 * (row_h + gap),
                                        ),
                                    egui::vec2(col_w, row_h),
                                );
                                let resp = ui.interact(
                                    rect,
                                    Id::new(("agent-pick-row", node.id.0, i)),
                                    Sense::click(),
                                );
                                ui.painter().rect_filled(
                                    rect,
                                    radius,
                                    if resp.hovered() { row_hover } else { row_fill },
                                );
                                let text_w = (rect.width() - canvas_scale::px(44.0, z)).max(8.0);
                                let galley = tracked_galley(
                                    ui,
                                    &chat.title,
                                    row_px,
                                    palette.ink,
                                    canvas_scale::px(0.08, z),
                                    text_w,
                                );
                                let text_pos = Pos2::new(
                                    rect.left() + canvas_scale::px(14.0, z),
                                    rect.center().y - galley.size().y * 0.5,
                                );
                                ui.painter().galley(text_pos, galley, palette.ink);
                                let chev = Rect::from_center_size(
                                    Pos2::new(
                                        rect.right() - canvas_scale::px(16.0, z),
                                        rect.center().y,
                                    ),
                                    egui::vec2(canvas_scale::px(11.0, z), canvas_scale::px(11.0, z)),
                                );
                                atlas_shell::icons::paint(
                                    ui.painter(),
                                    chev,
                                    atlas_shell::icons::Icon::ChevronRight,
                                    palette.sub,
                                );
                                if resp.clicked() {
                                    chosen = Some(Some(chat.id.clone()));
                                }
                            }
                        });
                    scrolled = Some(out.state.offset.y / z);
                    if paint_pick_button(
                        ui,
                        choose_button,
                        &format!("+  {action}"),
                        z,
                        radius,
                        Id::new(("agent-pick-new", node.id.0)),
                        &palette,
                    ) {
                        chosen = Some(None);
                    }
                    if let Some(build) = just_build_button {
                        if paint_pick_button(
                            ui,
                            build,
                            "Just build",
                            z,
                            radius,
                            Id::new(("agent-just-build", node.id.0)),
                            &palette,
                        ) {
                            just_build = true;
                        }
                    }
                });
        } else if canvas_text::legible(row_px) {
            let preview = if chats.is_empty() {
                format!("{} — double-click to start", portal.title)
            } else {
                format!("{} agents — double-click to choose", chats.len())
            };
            canvas_text::text(
                painter,
                list.left_top(),
                Align2::LEFT_TOP,
                &preview,
                FontId::proportional(row_px),
                palette.sub,
            );
        }
        if let Some(offset) = scrolled {
            self.agents.pick_scroll.insert(node.id, offset);
        }
        if just_build {
            self.agent_just_build(node.id);
        } else if let Some(channel) = chosen {
            if projects {
                if let Some(path) = channel {
                    self.bind_agent_project(node.id, PathBuf::from(path));
                } else {
                    self.pick_agent_project(node.id);
                }
            } else {
                self.set_agent_channel(node.id, channel);
                self.agents.chat_picker = None;
            }
        }
    }
}
