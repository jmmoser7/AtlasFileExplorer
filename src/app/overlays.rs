//! Floating UI above the canvas: staging tray, welcome screen, context
//! menu, tag/assign editor, detail window, hover tip, drag ghost, toasts,
//! and texture eviction.
//!
//! Everything here is `impl AtlasApp`. These are per-frame egui Areas and
//! Windows — they hold no state of their own beyond the `AtlasApp` fields
//! they read (`menu_at`, `edit_open`, `detail`, `toasts`, ...).

use super::*;

impl AtlasApp {
    pub(in crate::app) fn open_edit_panel(&mut self) {
        self.edit_open = true;
        self.edit_tag_input.clear();
        let rels = self.selection_rels();
        let dests: BTreeSet<String> = rels
            .iter()
            .filter_map(|r| self.tag_state.assigns.get(r).map(|(d, _)| d.clone()))
            .collect();
        self.edit_dest_input = if dests.len() == 1 {
            dests.into_iter().next().unwrap()
        } else {
            String::new()
        };
        self.edit_rename_input = if rels.len() == 1 {
            self.tag_state
                .assigns
                .get(&rels[0])
                .and_then(|(_, n)| n.clone())
                .unwrap_or_default()
        } else {
            String::new()
        };
    }

    pub(in crate::app) fn bottom_tray(&mut self, ctx: &egui::Context) {
        let assigns = &self.tag_state.assigns;
        let has_content =
            !assigns.is_empty() || self.export_ui.is_some() || !self.selection.is_empty();
        if !has_content {
            return;
        }
        let mut groups: BTreeMap<String, usize> = BTreeMap::new();
        for (dest, _) in assigns.values() {
            *groups.entry(dest.clone()).or_insert(0) += 1;
        }

        egui::TopBottomPanel::bottom("tray").show(ctx, |ui| {
            ui.add_space(6.0);
            if let Some(exp) = &self.export_ui {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(format!(
                        "Exporting {}/{} â€” {}",
                        exp.done, exp.total, exp.current
                    ));
                });
                let frac = if exp.total > 0 {
                    exp.done as f32 / exp.total as f32
                } else {
                    0.0
                };
                ui.add(egui::ProgressBar::new(frac).desired_height(6.0));
                ui.add_space(6.0);
                return;
            }

            ui.horizontal_wrapped(|ui| {
                ui.strong("Staging:");
                if groups.is_empty() {
                    ui.label(
                        egui::RichText::new(
                            "no assignments yet â€” right-click files or drag chips",
                        )
                        .color(Color32::from_gray(120)),
                    );
                }
                let mut assign_to: Option<String> = None;
                for (dest, count) in &groups {
                    let resp = chip(
                        ui,
                        &format!("{dest} ({count})"),
                        false,
                        Color32::from_rgb(0x6b, 0x4f, 0x24),
                    );
                    if resp.drag_started() {
                        self.drag_chip = Some(DragChip::Dest(dest.clone()));
                    }
                    if resp.clicked() && !self.selection.is_empty() {
                        assign_to = Some(dest.clone());
                    }
                    resp.on_hover_text("Click: assign selection here Â· Drag onto files");
                }
                if let Some(dest) = assign_to {
                    let rels = self.selection_rels();
                    let n = rels.len();
                    self.set_assign(
                        &rels,
                        Some((dest.clone(), None)),
                        format!("Assign {n} file(s) â†’ {dest}"),
                    );
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let total_assigned: usize = groups.values().sum();
                    ui.add_enabled_ui(total_assigned > 0, |ui| {
                        if ui
                            .button(format!("Export {total_assigned} filesâ€¦"))
                            .clicked()
                        {
                            self.pick_export_dest();
                        }
                    });
                    if self.export_picker_rx.is_some() {
                        ui.spinner();
                    }
                    if !self.selection.is_empty()
                        && ui
                            .button(format!("Tag / assign {} selectedâ€¦", self.selection.len()))
                            .clicked()
                    {
                        self.open_edit_panel();
                    }
                });
            });
            ui.add_space(6.0);
        });
    }

    /// Hidden from chrome for now; re-enable via a future `ToolPanel::Journal`.
    #[allow(dead_code)]
    fn journal_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("journal")
            .resizable(true)
            .default_width(280.0)
            .show(ctx, |ui| {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.strong("Action journal");
                    ui.label(
                        egui::RichText::new("every action, reversible")
                            .small()
                            .color(Color32::from_gray(120)),
                    );
                });
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    if self.journal.entries.is_empty() {
                        ui.label(
                            egui::RichText::new("No actions yet").color(Color32::from_gray(120)),
                        );
                    }
                    let cursor = self.journal.cursor;
                    for (i, entry) in self.journal.entries.iter().enumerate().rev() {
                        let applied = i < cursor;
                        let color = if applied {
                            Color32::from_gray(220)
                        } else {
                            Color32::from_gray(100)
                        };
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(if applied { "â—" } else { "â—‹" }).color(
                                    if applied {
                                        Color32::from_rgb(0x7a, 0xc7, 0x8a)
                                    } else {
                                        Color32::from_gray(90)
                                    },
                                ),
                            );
                            ui.vertical(|ui| {
                                ui.label(egui::RichText::new(&entry.label).color(color));
                                ui.label(
                                    egui::RichText::new(date_string(entry.ts))
                                        .small()
                                        .color(Color32::from_gray(100)),
                                );
                            });
                        });
                        ui.add_space(2.0);
                    }
                });
            });
    }

    pub(in crate::app) fn welcome(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.3);
            ui.heading(egui::RichText::new("File Atlas").size(34.0));
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(
                    "Map a folder. See everything at a glance. Organize without touching a single original.",
                )
                .color(Color32::from_gray(150)),
            );
            ui.add_space(18.0);
            if ui
                .add(egui::Button::new(
                    egui::RichText::new("  Open folderâ€¦  ").size(18.0),
                ))
                .clicked()
            {
                self.open_folder_dialog();
            }
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("or drop a folder anywhere in this window Â· Ctrl+O")
                    .small()
                    .color(Color32::from_gray(120)),
            );
        });
    }

    // ---------- windows / overlays ----------

    pub(in crate::app) fn action_menu(&mut self, ctx: &egui::Context) {
        let Some((id, pos)) = self.menu_at else {
            return;
        };
        let mut close = false;
        let rels = self.target_rels(Some(id));
        let n = rels.len();
        egui::Area::new(egui::Id::new("action_menu"))
            .fixed_pos(pos)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::menu(ui.style()).show(ui, |ui| {
                    ui.set_min_width(190.0);
                    ui.label(
                        egui::RichText::new(format!("{n} file(s)"))
                            .small()
                            .color(Color32::from_gray(130)),
                    );
                    if ui.button("Tag / assignâ€¦").clicked() {
                        self.open_edit_panel();
                        close = true;
                    }
                    if ui.button("Clear assignment").clicked() {
                        self.set_assign(&rels, None, format!("Clear assignment on {n} file(s)"));
                        close = true;
                    }
                    ui.separator();
                    if n == 1 {
                        if ui.button("Open").clicked() {
                            if let Some(e) = self.entry_by_rel(&rels[0]) {
                                platform::open_path(&e.path);
                            }
                            close = true;
                        }
                        if ui.button("Show in Explorer").clicked() {
                            if let Some(e) = self.entry_by_rel(&rels[0]) {
                                platform::reveal_in_explorer(&e.path);
                            }
                            close = true;
                        }
                        if ui.button("Details").clicked() {
                            self.detail = Some(id);
                            close = true;
                        }
                    }
                });
            });
        if close
            || ctx.input(|i| {
                i.pointer.any_click()
                    && i.pointer
                        .interact_pos()
                        .map(|p| (p - pos).length() > 240.0)
                        .unwrap_or(false)
            })
        {
            self.menu_at = None;
        }
    }

    fn entry_by_rel(&self, rel: &str) -> Option<&FileEntry> {
        self.rel_to_id
            .get(rel)
            .and_then(|&i| self.entries.get(i as usize))
    }

    pub(in crate::app) fn edit_window(&mut self, ctx: &egui::Context) {
        if !self.edit_open {
            return;
        }
        let rels = self.selection_rels();
        if rels.is_empty() {
            self.edit_open = false;
            return;
        }
        let mut open = true;
        egui::Window::new(format!("Tag & assign â€” {} file(s)", rels.len()))
            .open(&mut open)
            .collapsible(false)
            .default_width(360.0)
            .show(ctx, |ui| {
                let mut common: Option<BTreeSet<String>> = None;
                for rel in &rels {
                    let set: BTreeSet<String> = self
                        .tag_state
                        .tags
                        .get(rel)
                        .map(|v| v.iter().cloned().collect())
                        .unwrap_or_default();
                    common = Some(match common {
                        None => set,
                        Some(c) => c.intersection(&set).cloned().collect(),
                    });
                }
                let common = common.unwrap_or_default();

                ui.strong("Tags");
                ui.horizontal_wrapped(|ui| {
                    let mut remove: Option<String> = None;
                    for t in &common {
                        if chip(
                            ui,
                            &format!("{t} Ã—"),
                            true,
                            Color32::from_rgb(0x37, 0x5a, 0x7a),
                        )
                        .clicked()
                        {
                            remove = Some(t.clone());
                        }
                    }
                    if let Some(t) = remove {
                        self.remove_tag(&rels, &t);
                    }
                });
                ui.horizontal(|ui| {
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.edit_tag_input)
                            .hint_text("add tagâ€¦")
                            .desired_width(180.0),
                    );
                    let submit = (resp.lost_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                        || ui.button("Add").clicked();
                    if submit && !self.edit_tag_input.trim().is_empty() {
                        let t = self.edit_tag_input.trim().to_string();
                        self.add_tag(&rels, &t);
                        self.edit_tag_input.clear();
                        resp.request_focus();
                    }
                });
                let input_lc = self.edit_tag_input.to_lowercase();
                if !input_lc.is_empty() {
                    let sugg: Vec<String> = self
                        .all_tags
                        .keys()
                        .filter(|t| t.to_lowercase().starts_with(&input_lc))
                        .take(6)
                        .cloned()
                        .collect();
                    ui.horizontal_wrapped(|ui| {
                        for s in sugg {
                            if ui.small_button(&s).clicked() {
                                self.add_tag(&rels, &s);
                                self.edit_tag_input.clear();
                            }
                        }
                    });
                }

                ui.separator();
                ui.strong("Destination folder (relative to export root)");
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.edit_dest_input)
                            .hint_text(r"e.g. Projects\Renders")
                            .desired_width(220.0),
                    );
                    if ui.button("Assign").clicked() && !self.edit_dest_input.trim().is_empty() {
                        let d = self.edit_dest_input.trim().trim_matches('\\').to_string();
                        let n = rels.len();
                        self.known_dests.insert(d.clone());
                        self.set_assign(
                            &rels,
                            Some((d.clone(), None)),
                            format!("Assign {n} file(s) â†’ {d}"),
                        );
                    }
                });
                if !self.known_dests.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            egui::RichText::new("known:")
                                .small()
                                .color(Color32::from_gray(120)),
                        );
                        let dests: Vec<String> = self.known_dests.iter().cloned().collect();
                        for d in dests {
                            if ui.small_button(&d).clicked() {
                                self.edit_dest_input = d;
                            }
                        }
                    });
                }
                if ui.button("Clear assignment").clicked() {
                    let n = rels.len();
                    self.set_assign(&rels, None, format!("Clear assignment on {n} file(s)"));
                }

                if rels.len() == 1 {
                    ui.separator();
                    ui.strong("Export name (rename on copy)");
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.edit_rename_input)
                                .hint_text("new-name.ext")
                                .desired_width(220.0),
                        );
                        if ui.button("Set").clicked() {
                            let rel = &rels[0];
                            let cur = self.tag_state.assigns.get(rel).cloned();
                            let dest = cur.map(|(d, _)| d).unwrap_or_default();
                            let nn = self.edit_rename_input.trim();
                            let nn = if nn.is_empty() {
                                None
                            } else {
                                Some(nn.to_string())
                            };
                            self.set_assign(
                                &rels,
                                Some((dest, nn.clone())),
                                match nn {
                                    Some(n) => format!("Rename on export â†’ {n}"),
                                    None => "Clear export rename".into(),
                                },
                            );
                        }
                    });
                    ui.label(
                        egui::RichText::new(
                            "Only the exported copy is renamed â€” the original is never touched.",
                        )
                        .small()
                        .color(Color32::from_gray(120)),
                    );
                }
            });
        if !open {
            self.edit_open = false;
        }
    }

    pub(in crate::app) fn detail_window(&mut self, ctx: &egui::Context) {
        let p = self.palette();
        let Some(id) = self.detail else { return };
        let Some(e) = self.entries.get(id as usize).cloned() else {
            self.detail = None;
            return;
        };
        let mut open = true;
        egui::Window::new(&e.name)
            .open(&mut open)
            .default_width(420.0)
            .show(ctx, |ui| {
                if let Some((tex, _)) = self.textures.get(&id) {
                    let size = tex.size_vec2();
                    let max_w = ui.available_width().min(400.0);
                    let scale = (max_w / size.x).min(300.0 / size.y).min(2.0);
                    ui.image((tex.id(), size * scale));
                }
                ui.add_space(4.0);
                ui.label(format!(
                    "{} Â· {}",
                    human_size(e.size),
                    date_string(e.mtime)
                ));
                ui.label(
                    egui::RichText::new(e.path.to_string_lossy())
                        .small()
                        .color(Color32::from_gray(140)),
                );
                if let Some(tags) = self.tag_state.tags.get(&e.rel) {
                    ui.horizontal_wrapped(|ui| {
                        for t in tags {
                            chip(ui, t, true, Color32::from_rgb(0x37, 0x5a, 0x7a));
                        }
                    });
                }
                if let Some((dest, nn)) = self.tag_state.assigns.get(&e.rel) {
                    ui.label(
                        egui::RichText::new(format!(
                            "staged â†’ {dest}{}",
                            nn.as_ref().map(|n| format!(" as {n}")).unwrap_or_default()
                        ))
                        .color(p.staged),
                    );
                }
                ui.horizontal(|ui| {
                    if ui.button("Open").clicked() {
                        platform::open_path(&e.path);
                    }
                    if ui.button("Show in Explorer").clicked() {
                        platform::reveal_in_explorer(&e.path);
                    }
                });
            });
        if !open {
            self.detail = None;
        }
    }

    /// Hover preview like the web app's tip: bigger thumbnail near the cursor.
    pub(in crate::app) fn hover_tip(&mut self, ctx: &egui::Context) {
        if self.drag_chip.is_some() || self.rubber_origin.is_some() {
            return;
        }
        let Some(f) = self.hovered_file else { return };
        // Only useful when cards are small on screen.
        if self.cam.z > 0.75 {
            return;
        }
        let Some((tex, _)) = self.textures.get(&f) else {
            return;
        };
        let Some(p) = ctx.pointer_latest_pos() else {
            return;
        };
        let Some(entry) = self.entries.get(f as usize) else {
            return;
        };
        let size = tex.size_vec2();
        let scale = (240.0 / size.x).min(180.0 / size.y).min(2.0);
        let name = entry.name.clone();
        let tex_id = tex.id();
        egui::Area::new(egui::Id::new("hover_tip"))
            .fixed_pos(p + Vec2::new(18.0, 18.0))
            .order(egui::Order::Tooltip)
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.image((tex_id, size * scale));
                    ui.label(egui::RichText::new(name).small());
                });
            });
    }

    pub(in crate::app) fn drag_overlay(&mut self, ctx: &egui::Context) {
        let Some(chipv) = &self.drag_chip else { return };
        if ctx.input(|i| i.pointer.any_released()) && self.hovered_file.is_none() {
            self.drag_chip = None;
            return;
        }
        let label = match chipv {
            DragChip::Tag(t) => format!("tag: {t}"),
            DragChip::Dest(d) => format!("â†’ {d}"),
        };
        if let Some(p) = ctx.pointer_latest_pos() {
            egui::Area::new(egui::Id::new("drag_overlay"))
                .fixed_pos(p + Vec2::new(14.0, 10.0))
                .order(egui::Order::Tooltip)
                .interactable(false)
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.label(label);
                    });
                });
        }
    }

    pub(in crate::app) fn draw_toasts(&mut self, ctx: &egui::Context) {
        self.toasts.retain(|(_, t)| t.elapsed().as_secs_f32() < 4.0);
        if self.toasts.is_empty() {
            return;
        }
        egui::Area::new(egui::Id::new("toasts"))
            .anchor(Align2::RIGHT_BOTTOM, Vec2::new(-16.0, -16.0))
            .order(egui::Order::Foreground)
            .interactable(false)
            .show(ctx, |ui| {
                for (msg, _) in &self.toasts {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.label(msg);
                    });
                    ui.add_space(4.0);
                }
            });
    }

    pub(in crate::app) fn evict_textures(&mut self) {
        if self.textures.len() <= TEXTURE_CAP {
            return;
        }
        let mut ages: Vec<(u32, u64)> = self.textures.iter().map(|(k, (_, f))| (*k, *f)).collect();
        ages.sort_by_key(|(_, f)| *f);
        let evict = self.textures.len() - TEXTURE_CAP + 100;
        for (k, f) in ages.into_iter().take(evict) {
            if f == self.frame_no {
                break;
            }
            self.textures.remove(&k);
            if let Some(s) = self.thumb_state.get_mut(k as usize) {
                if *s == ThumbState::Loaded {
                    // Keep the average color; only the texture is gone.
                    *s = ThumbState::HasColor;
                }
            }
        }
    }
}
