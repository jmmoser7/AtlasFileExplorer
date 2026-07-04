//! Canvas: camera math, input handling, hit-testing, and all world-space
//! painting (branches, directory nodes, portals, file cards, thumbnails).
//!
//! Everything here is `impl AtlasApp` — the canvas reads and mutates the
//! active tab's workspace directly. Screen/world conversion goes through
//! `w2s`/`s2w`; painting decisions key off the level-of-detail thresholds
//! `LOD_FULL`/`LOD_MID` in `app/mod.rs`.

use super::*;

impl AtlasApp {
    // ---------- camera ----------

    fn w2s(&self, p: Pos2) -> Pos2 {
        Pos2::new(p.x * self.cam.z, p.y * self.cam.z) + self.cam.offset
    }

    fn s2w(&self, p: Pos2) -> Pos2 {
        Pos2::new(
            (p.x - self.cam.offset.x) / self.cam.z,
            (p.y - self.cam.offset.y) / self.cam.z,
        )
    }

    fn w2s_rect(&self, r: Rect) -> Rect {
        Rect::from_min_max(self.w2s(r.min), self.w2s(r.max))
    }

    pub(in crate::app) fn zoom_at(&mut self, screen: Pos2, factor: f32) {
        self.anim = None;
        let nz = (self.cam.z * factor).clamp(ZOOM_MIN, ZOOM_MAX);
        let k = nz / self.cam.z;
        self.cam.offset.x = screen.x - (screen.x - self.cam.offset.x) * k;
        self.cam.offset.y = screen.y - (screen.y - self.cam.offset.y) * k;
        self.cam.z = nz;
    }

    fn cam_for_bounds(&self, b: Rect, max_z: f32) -> Camera {
        let avail = self.canvas_rect.shrink(40.0);
        let z = ((avail.width() / (b.width() + 70.0)).min(avail.height() / (b.height() + 70.0)))
            .clamp(ZOOM_MIN, max_z);
        Camera {
            offset: Vec2::new(
                avail.min.x + (avail.width() - b.width() * z) / 2.0 - b.min.x * z,
                avail.min.y + (avail.height() - b.height() * z) / 2.0 - b.min.y * z,
            ),
            z,
        }
    }

    fn fly_to(&mut self, to: Camera) {
        self.anim = Some(CamAnim {
            t0: Instant::now(),
            dur: 0.43,
            from: self.cam,
            to,
        });
    }

    fn apply_view_cmd(&mut self, cmd: ViewCmd) {
        let Some(t) = &self.tree else { return };
        match cmd {
            ViewCmd::Fit => {
                self.cam = self.cam_for_bounds(t.root_bounds(), 1.2);
            }
            ViewCmd::Home => {
                // Opening view: root readable, thumbnails already visible.
                let root = &t.dirs[0];
                let z = 0.9;
                let r = self.canvas_rect;
                self.cam = match self.orient {
                    Orient::V => Camera {
                        offset: Vec2::new(r.min.x + 60.0 - root.x * z, r.center().y - root.y * z),
                        z,
                    },
                    Orient::H => Camera {
                        offset: Vec2::new(
                            r.center().x - (root.x + root.w / 2.0) * z,
                            r.min.y + 50.0 - (root.y - root.h / 2.0) * z,
                        ),
                        z,
                    },
                };
                // Small trees: just fit.
                if t.dirs[0].desc_files <= 60 {
                    self.cam = self.cam_for_bounds(t.root_bounds(), 1.2);
                }
            }
            ViewCmd::FlyToBounds(b) => {
                let to = self.cam_for_bounds(b, 1.1);
                self.fly_to(to);
            }
        }
    }

    // ---------- canvas ----------

    pub(in crate::app) fn canvas(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette();
        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        self.canvas_rect = rect;

        if let Some(cmd) = self.pending_view.take() {
            self.apply_view_cmd(cmd);
        }

        let painter = ui.painter().with_clip_rect(rect);
        painter.rect_filled(rect, CornerRadius::ZERO, palette.bg);
        self.draw_dot_grid(&painter, rect);

        let pointer = ui.ctx().pointer_latest_pos();
        let shift = ui.input(|i| i.modifiers.shift);

        // --- input: zoom (wheel & pinch) ---
        if resp.hovered() {
            let (scroll, zoom_delta) = ui.input(|i| (i.raw_scroll_delta, i.zoom_delta()));
            if let Some(p) = pointer {
                if scroll.y.abs() > 0.0 && !shift {
                    self.zoom_at(p, (scroll.y * 0.0021).exp());
                } else if shift && (scroll.y.abs() > 0.0 || scroll.x.abs() > 0.0) {
                    self.cam.offset.x -= scroll.y + scroll.x;
                }
                if zoom_delta != 1.0 {
                    self.zoom_at(p, zoom_delta);
                }
            }
        }

        // --- input: pan / rubber band / turbo pan ---
        if resp.drag_started() {
            if shift {
                self.rubber_origin = pointer;
            }
            self.anim = None;
        }
        let turbo_pan_active = self
            .turbo_pan
            .step(ui.ctx(), rect, pointer, &mut self.cam.offset);
        if turbo_pan_active {
            self.anim = None;
        }
        if resp.dragged() && self.rubber_origin.is_none() && !turbo_pan_active {
            self.cam.offset += resp.drag_delta();
        }

        // --- hover ---
        self.hovered_file = None;
        self.hovered_dir = None;
        self.hovered_dir_grip = None;
        if let (Some(p), Some(t)) = (pointer, &self.tree) {
            if rect.contains(p) && self.rubber_origin.is_none() {
                // Files take priority: the global grip search has a generous
                // radius and must never steal hover from a thumbnail under
                // the cursor (it made ctrl-click selection intermittent).
                match t.hit_test(self.s2w(p)) {
                    Some(Hit::File(f)) => self.hovered_file = Some(f),
                    Some(Hit::Dir(d)) => {
                        let visible = self
                            .tree
                            .as_ref()
                            .and_then(|t| t.dirs.get(d as usize))
                            .map(|dir| {
                                self.structure_only
                                    || self.filter_mode != FilterMode::Hide
                                    || !self.any_filter
                                    || d == 0
                                    || dir.desc_matches > 0
                            })
                            .unwrap_or(false);
                        if visible {
                            self.hovered_dir = Some(d);
                            self.hovered_dir_grip = self.dir_grip_at(d, p);
                        }
                    }
                    None => {
                        if let Some((d, grip)) = self.grip_hit_test(p) {
                            self.hovered_dir = Some(d);
                            self.hovered_dir_grip = Some(grip);
                        }
                    }
                }
            }
        }

        // --- draw the tree ---
        let world_view = Rect::from_min_max(self.s2w(rect.min), self.s2w(rect.max));
        let z = self.cam.z;
        let lod = if z < LOD_MID {
            0
        } else if z < LOD_FULL {
            1
        } else {
            2
        };
        let mut requests: Vec<ThumbRequest> = Vec::new();
        let mut color_budget: i32 = 14;
        if self.tree.is_some() {
            let tree = self.tree.take().unwrap();
            self.draw_branch(
                &painter,
                &tree,
                0,
                world_view,
                lod,
                &mut requests,
                &mut color_budget,
            );
            self.tree = Some(tree);
        }
        for r in requests {
            self.thumbs_pending += 1;
            self.thumbs.request(r);
        }

        // Dev harness: ATLAS_HITDEBUG=1 paints hit-test results across the
        // viewport â€” green dot = file hit, blue = dir, nothing = miss.
        if std::env::var("ATLAS_HITDEBUG").is_ok() {
            if let Some(t) = &self.tree {
                let mut y = rect.min.y;
                while y < rect.max.y {
                    let mut x = rect.min.x;
                    while x < rect.max.x {
                        let p = Pos2::new(x, y);
                        match t.hit_test(self.s2w(p)) {
                            Some(Hit::File(_)) => {
                                painter.circle_filled(p, 2.0, Color32::from_rgb(0, 220, 90));
                            }
                            Some(Hit::Dir(_)) => {
                                painter.circle_filled(p, 2.0, Color32::from_rgb(70, 130, 255));
                            }
                            None => {}
                        }
                        x += 12.0;
                    }
                    y += 12.0;
                }
            }
        }

        // --- rubber band ---
        if let (Some(a), Some(p)) = (self.rubber_origin, pointer) {
            let r = Rect::from_two_pos(a, p);
            painter.rect_filled(
                r,
                CornerRadius::ZERO,
                Color32::from_rgba_unmultiplied(0x4f, 0x9c, 0xf0, 28),
            );
            painter.rect_stroke(
                r,
                CornerRadius::ZERO,
                Stroke::new(1.0, palette.select),
                StrokeKind::Inside,
            );
        }

        // --- clicks ---
        // Mutations deferred until after hit-testing borrows end.
        type Deferred = Box<dyn FnOnce(&mut AtlasApp)>;
        let mut deferred: Vec<Deferred> = Vec::new();

        if resp.drag_stopped() {
            if let (Some(a), Some(p)) = (self.rubber_origin, pointer) {
                let world = Rect::from_min_max(self.s2w(a.min(p)), self.s2w(a.max(p)));
                let additive = ui.input(|i| i.modifiers.ctrl);
                deferred.push(Box::new(move |app| {
                    if !additive {
                        app.selection.clear();
                    }
                    let mut hits = Vec::new();
                    if let Some(t) = &app.tree {
                        t.files_in_rect(world, &mut hits);
                    }
                    for f in hits {
                        let alive = app
                            .entries
                            .get(f as usize)
                            .map(|e| !e.dead)
                            .unwrap_or(false);
                        if alive && app.file_match.get(f as usize).copied().unwrap_or(false) {
                            app.selection.insert(f);
                        }
                    }
                }));
            }
            self.rubber_origin = None;
        }

        if resp.clicked() {
            let (ctrl, shift) = ui.input(|i| (i.modifiers.ctrl, i.modifiers.shift));
            match (self.hovered_file, self.hovered_dir) {
                (Some(f), _) => {
                    deferred.push(Box::new(move |app| {
                        if shift {
                            if !ctrl {
                                app.selection.clear();
                            }
                            app.select_range_to(f);
                        } else if ctrl {
                            if !app.selection.remove(&f) {
                                app.selection.insert(f);
                            }
                            app.last_selected_file = Some(f);
                        } else {
                            app.selection.clear();
                            app.selection.insert(f);
                            app.last_selected_file = Some(f);
                        }
                    }));
                }
                (None, Some(d)) => {
                    let grip = self.hovered_dir_grip.unwrap_or(DirGrip::Incremental);
                    deferred.push(Box::new(move |app| app.toggle_dir(d, grip)));
                }
                (None, None) => {
                    if !ctrl {
                        deferred.push(Box::new(|app| app.selection.clear()));
                    }
                }
            }
        }
        if resp.double_clicked() {
            match self.hovered_file {
                Some(f) => {
                    if let Some(e) = self.entries.get(f as usize) {
                        platform::open_path(&e.path);
                    }
                }
                None => {
                    if self.hovered_dir.is_none() {
                        if let Some(p) = pointer {
                            self.zoom_at(p, 1.7);
                        }
                    }
                }
            }
        }
        if resp.secondary_clicked() && !self.turbo_pan.should_suppress_context_menu() {
            if let (Some(f), Some(p)) = (self.hovered_file, pointer) {
                if !self.selection.contains(&f) {
                    self.selection.clear();
                    self.selection.insert(f);
                }
                self.menu_at = Some((f, p));
            } else if let (Some(d), Some(p)) = (self.hovered_dir, pointer) {
                let ids = self.subtree_file_ids(d);
                if let Some(&first) = ids.first() {
                    self.selection.clear();
                    self.selection.extend(ids);
                    self.last_selected_file = Some(first);
                    self.menu_at = Some((first, p));
                }
            }
        }
        self.turbo_pan.acknowledge_context_menu();

        // --- chip drop ---
        if self.drag_chip.is_some() {
            let released = ui.input(|i| i.pointer.any_released());
            if released {
                if let Some(f) = self.hovered_file {
                    let chipv = self.drag_chip.clone().unwrap();
                    deferred.push(Box::new(move |app| {
                        let rels = app.target_rels(Some(f));
                        match chipv {
                            DragChip::Tag(t) => app.add_tag(&rels, &t),
                            DragChip::Dest(d) => {
                                let n = rels.len();
                                app.set_assign(
                                    &rels,
                                    Some((d.clone(), None)),
                                    format!("Assign {n} file(s) â†’ {d}"),
                                );
                            }
                        }
                    }));
                }
                self.drag_chip = None;
            }
        }

        for f in deferred {
            f(self);
        }

        // Zoom controls overlay (bottom-right of canvas).
        self.zoom_controls(ui, rect);

        // Cursor feedback.
        if turbo_pan_active {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        } else if self.hovered_file.is_some() || self.hovered_dir.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        } else if resp.dragged() && self.rubber_origin.is_none() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        }
    }

    /// Screen positions of the (incremental, full) expand grips for a
    /// collapsed dir. Single source of truth for drawing and hit testing.
    fn grip_positions(&self, sr: Rect) -> (Pos2, Pos2) {
        let z = self.cam.z.max(0.4);
        match self.orient {
            Orient::V => (
                Pos2::new(sr.max.x + 10.0 * z, sr.center().y - 11.0 * z),
                Pos2::new(sr.max.x + 10.0 * z, sr.center().y + 11.0 * z),
            ),
            Orient::H => (
                Pos2::new(sr.center().x - 11.0 * z, sr.max.y + 10.0 * z),
                Pos2::new(sr.center().x + 11.0 * z, sr.max.y + 10.0 * z),
            ),
        }
    }

    fn dir_grip_at(&self, di: u32, screen: Pos2) -> Option<DirGrip> {
        let t = self.tree.as_ref()?;
        let d = t.dirs.get(di as usize)?;
        if !d.collapsed {
            return None;
        }
        let sr = self.w2s_rect(d.rect());
        let z = self.cam.z.max(0.4);
        let r = (9.0 * z).clamp(6.0, 12.0);
        let (inc, full) = self.grip_positions(sr);
        // The two grips can sit close together at low zoom; always resolve
        // to the nearest one so both remain clickable.
        let d_inc = screen.distance(inc);
        let d_full = screen.distance(full);
        if d_inc.min(d_full) > r {
            None
        } else if d_inc <= d_full {
            Some(DirGrip::Incremental)
        } else {
            Some(DirGrip::Full)
        }
    }

    fn grip_hit_test(&self, screen: Pos2) -> Option<(u32, DirGrip)> {
        let t = self.tree.as_ref()?;
        let mut best: Option<(f32, u32, DirGrip)> = None;
        for (di, d) in t.dirs.iter().enumerate() {
            if !d.collapsed {
                continue;
            }
            if !self.structure_only
                && self.filter_mode == FilterMode::Hide
                && self.any_filter
                && di != 0
                && d.desc_matches == 0
            {
                continue;
            }
            if let Some(grip) = self.dir_grip_at(di as u32, screen) {
                let sr = self.w2s_rect(d.rect());
                let (inc, full) = self.grip_positions(sr);
                let dist = match grip {
                    DirGrip::Incremental => screen.distance(inc),
                    DirGrip::Full => screen.distance(full),
                };
                if best.is_none_or(|(bd, _, _)| dist < bd) {
                    best = Some((dist, di as u32, grip));
                }
            }
        }
        best.map(|(_, di, grip)| (di, grip))
    }

    fn toggle_dir(&mut self, di: u32, grip: DirGrip) {
        let Some(t) = &mut self.tree else { return };
        let di = di as usize;
        if di >= t.dirs.len() {
            return;
        }
        let was_portal = t.dirs[di].is_portal(t.cfg);
        let before = Pos2::new(t.dirs[di].x, t.dirs[di].y);
        let threshold = t.cfg.normalized().portal_threshold;
        match grip {
            DirGrip::Incremental => {
                let expanding = t.dirs[di].collapsed;
                t.dirs[di].collapsed = !t.dirs[di].collapsed;
                if expanding {
                    // Incremental means exactly one level: re-collapse the
                    // children so a previous full expand doesn't bleed back.
                    let children = t.dirs[di].child_dirs.clone();
                    for c in children {
                        t.dirs[c as usize].collapsed = true;
                    }
                }
            }
            DirGrip::Full => {
                // "Fully expanded" ignores portal-sized folders, which full
                // expand deliberately leaves as thumbnail previews.
                let fully_expanded = t.dirs[di].child_dirs.iter().all(|&c| {
                    let cd = &t.dirs[c as usize];
                    !cd.collapsed || cd.child_dirs.len() + cd.files.len() > threshold
                });
                let collapse = !t.dirs[di].collapsed && fully_expanded;
                set_subtree_collapsed(t, di, collapse);
                t.dirs[di].collapsed = collapse;
            }
        }
        t.layout_filtered(
            self.orient,
            self.filter_mode == FilterMode::Hide && self.any_filter,
            &self.file_match,
            self.structure_only,
        );

        if was_portal && !t.dirs[di].collapsed && grip == DirGrip::Incremental {
            // Entering the portal: fly to the folder's contents.
            let b = t.dirs[di].grid_bounds.unwrap_or(t.dirs[di].bounds);
            let own = t.dirs[di].rect();
            self.pending_view = Some(ViewCmd::FlyToBounds(b.union(own)));
        } else {
            // Keep the clicked node visually stable.
            let after = Pos2::new(t.dirs[di].x, t.dirs[di].y);
            self.cam.offset += (before - after) * self.cam.z;
        }
        self.filter_dirty = true; // match counts move around
    }

    fn zoom_controls(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let pos = rect.right_bottom() + Vec2::new(-14.0, -14.0);
        egui::Area::new(egui::Id::new("zoomctl"))
            .fixed_pos(pos)
            .pivot(Align2::RIGHT_BOTTOM)
            .order(egui::Order::Middle)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("âˆ’").clicked() {
                            self.zoom_at(rect.center(), 1.0 / 1.3);
                        }
                        if ui
                            .button(format!("{:.0}%", self.cam.z * 100.0))
                            .on_hover_text("Reset to 100%")
                            .clicked()
                        {
                            let f = 1.0 / self.cam.z;
                            self.zoom_at(rect.center(), f);
                        }
                        if ui.button("+").clicked() {
                            self.zoom_at(rect.center(), 1.3);
                        }
                        if ui.button("Fit").clicked() {
                            self.pending_view = Some(ViewCmd::Fit);
                        }
                    });
                });
            });
    }

    fn draw_dot_grid(&self, painter: &egui::Painter, rect: Rect) {
        let p = self.palette();
        let z = self.cam.z;
        if z < 0.05 {
            return;
        }
        let s = 96.0 * z;
        if s < 8.0 {
            return;
        }
        let ox = rect.min.x + ((self.cam.offset.x - rect.min.x) % s + s) % s;
        let oy = rect.min.y + ((self.cam.offset.y - rect.min.y) % s + s) % s;
        let r = (1.1 * z).max(0.8);
        let mut x = ox - s;
        while x < rect.max.x {
            let mut y = oy - s;
            while y < rect.max.y {
                painter.rect_filled(
                    Rect::from_center_size(Pos2::new(x, y), Vec2::splat(r)),
                    CornerRadius::ZERO,
                    p.grid_dot,
                );
                y += s;
            }
            x += s;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_branch(
        &mut self,
        painter: &egui::Painter,
        t: &Tree,
        di: usize,
        view: Rect,
        lod: u8,
        requests: &mut Vec<ThumbRequest>,
        color_budget: &mut i32,
    ) {
        let p = self.palette();
        let d = &t.dirs[di];
        if !self.structure_only
            && self.filter_mode == FilterMode::Hide
            && self.any_filter
            && di != 0
            && d.desc_matches == 0
        {
            return;
        }
        if !d.bounds.expand(40.0).intersects(view) {
            return;
        }
        let z = self.cam.z;
        let dimming = self.any_filter;

        if !d.collapsed {
            // Edges to child dirs + branch files + grid trunk.
            let v = self.orient == Orient::V;
            let px = if v { d.x + d.w } else { d.x + d.w / 2.0 };
            let py = if v { d.y } else { d.y + d.h / 2.0 };
            let stroke_w = if lod == 0 { 1.6 } else { (1.3 * z).max(1.0) };

            // Edges route root -> leaf, terminating at the center of the
            // target's near edge. Collect every outgoing wire first, then
            // assign nested rails: because all same-side wires share the
            // port endpoint, their spans are strictly nested by run length,
            // so ranking by |breadth delta| and stacking rails outward-in
            // is provably crossing-free â€” no collision detection needed.
            // Left and right of the port get mirrored, independent stacks.
            let port = Pos2::new(px, py);
            let mut targets: Vec<Pos2> = Vec::new();
            for &c in d.child_dirs.iter() {
                let cd = &t.dirs[c as usize];
                if !self.structure_only
                    && self.filter_mode == FilterMode::Hide
                    && self.any_filter
                    && cd.desc_matches == 0
                {
                    continue;
                }
                targets.push(if v {
                    Pos2::new(cd.x, cd.y)
                } else {
                    Pos2::new(cd.x + cd.w / 2.0, cd.y - cd.h / 2.0)
                });
            }
            if let Some(gb) = d.grid_bounds {
                targets.push(if v {
                    Pos2::new(gb.min.x, gb.center().y)
                } else {
                    Pos2::new(gb.center().x, gb.min.y)
                });
            }

            // (exit breadth, rail depth) per wire; None = straight run.
            let mut routes: Vec<Option<(f32, f32)>> = vec![None; targets.len()];
            let (p_b, p_d) = if v { (py, px) } else { (px, py) };
            let breadth = |t: &Pos2| if v { t.y } else { t.x };
            let depth_of = |t: &Pos2| if v { t.x } else { t.y };
            let mut neg: Vec<(f32, usize)> = Vec::new();
            let mut pos: Vec<(f32, usize)> = Vec::new();
            for (i, tp) in targets.iter().enumerate() {
                let db = breadth(tp) - p_b;
                if db > 0.5 {
                    pos.push((db, i));
                } else if db < -0.5 {
                    neg.push((-db, i));
                }
            }
            // Exits fan out along the node edge; keep them inside the node.
            let exit_limit = ((if v { d.h } else { d.w }) / 2.0 - 8.0).max(2.0);
            for (mut list, sign) in [(neg, -1.0f32), (pos, 1.0f32)] {
                if list.is_empty() {
                    continue;
                }
                list.sort_by(|a, b| b.0.total_cmp(&a.0)); // longest run first
                let n = list.len() as f32;
                let exit_gap = 4.0f32.min(exit_limit / n);
                let min_td = list
                    .iter()
                    .map(|&(_, i)| depth_of(&targets[i]))
                    .fold(f32::INFINITY, f32::min);
                let avail = (min_td - p_d - 16.0 - 14.0).max(0.0);
                let rail_gap = if n > 1.0 {
                    8.0f32.min(avail / (n - 1.0))
                } else {
                    8.0
                };
                for (r, &(_, i)) in list.iter().enumerate() {
                    let exit = p_b + sign * (n - r as f32) * exit_gap;
                    let rail = (p_d + 16.0 + r as f32 * rail_gap)
                        .min(depth_of(&targets[i]) - 12.0)
                        .max(p_d + 4.0);
                    routes[i] = Some((exit, rail));
                }
            }

            for (i, tgt) in targets.iter().enumerate() {
                let edge_extent = Rect::from_two_pos(port, *tgt);
                if !edge_extent.expand(60.0).intersects(view) {
                    continue;
                }
                self.route_edge(painter, port, *tgt, routes[i], v, stroke_w);
            }

            if let Some(gb) = d.grid_bounds {
                if gb.expand(40.0).intersects(view) && lod > 0 {
                    // Dashed group outline.
                    let sr = self.w2s_rect(gb);
                    let dash = 7.0 * z.max(0.15);
                    let gap = 6.0 * z.max(0.15);
                    let pts = [
                        sr.min,
                        Pos2::new(sr.max.x, sr.min.y),
                        sr.max,
                        Pos2::new(sr.min.x, sr.max.y),
                        sr.min,
                    ];
                    for w in pts.windows(2) {
                        painter.add(egui::Shape::dashed_line(
                            w,
                            Stroke::new(1.0, p.border_strong),
                            dash,
                            gap,
                        ));
                    }
                }
            }

            // Files.
            for &f in &d.files {
                let fp = &t.file_pos[f as usize];
                if fp.place == FilePlace::Hidden {
                    continue;
                }
                let fr = fp.rect();
                if !fr.intersects(view) {
                    continue;
                }
                self.draw_file_card(painter, t, f, fr, lod, dimming, requests, color_budget);
            }
        }

        self.draw_dir_node(painter, t, di, lod, dimming, requests);

        if !d.collapsed {
            for &c in &d.child_dirs {
                if !self.structure_only
                    && self.filter_mode == FilterMode::Hide
                    && self.any_filter
                    && t.dirs[c as usize].desc_matches == 0
                {
                    continue;
                }
                self.draw_branch(painter, t, c as usize, view, lod, requests, color_budget);
            }
        }
    }

    /// Draws one wire from a node's port to a target, all in world coords.
    /// `route` = (exit breadth, rail depth): the wire leaves the node edge at
    /// `exit`, runs to its nested rail, travels along it, then descends to
    /// the target center. `None` means a straight run.
    #[allow(clippy::too_many_arguments)]
    fn route_edge(
        &self,
        painter: &egui::Painter,
        port: Pos2,
        tgt: Pos2,
        route: Option<(f32, f32)>,
        v: bool,
        stroke_w: f32,
    ) {
        let pal = self.palette();
        let stroke = Stroke::new(stroke_w, pal.line);
        let Some((exit, rail)) = route else {
            painter.line_segment([self.w2s(port), self.w2s(tgt)], stroke);
            return;
        };
        let start = if v {
            Pos2::new(port.x, exit)
        } else {
            Pos2::new(exit, port.y)
        };
        let (m1, m2) = if v {
            (Pos2::new(rail, exit), Pos2::new(rail, tgt.y))
        } else {
            (Pos2::new(exit, rail), Pos2::new(tgt.x, rail))
        };
        if self.leader_style == LeaderStyle::Orthogonal {
            let pts = [self.w2s(start), self.w2s(m1), self.w2s(m2), self.w2s(tgt)];
            rounded_route(painter, &pts, (9.0 * self.cam.z).clamp(2.0, 11.0), stroke);
            return;
        }
        // Bezier: control points sit on the same nested rail, so curved
        // wires fan out without crossing either.
        painter.add(egui::Shape::CubicBezier(
            egui::epaint::CubicBezierShape::from_points_stroke(
                [self.w2s(start), self.w2s(m1), self.w2s(m2), self.w2s(tgt)],
                false,
                Color32::TRANSPARENT,
                stroke,
            ),
        ));
    }

    fn draw_dir_node(
        &mut self,
        painter: &egui::Painter,
        t: &Tree,
        di: usize,
        lod: u8,
        _dimming: bool,
        requests: &mut Vec<ThumbRequest>,
    ) {
        let p = self.palette();
        let d = &t.dirs[di];
        let z = self.cam.z;
        let sr = self.w2s_rect(d.rect());
        let hovered = self.hovered_dir == Some(di as u32);

        if lod == 0 {
            painter.rect_filled(
                sr,
                CornerRadius::ZERO,
                if d.is_portal(t.cfg) {
                    p.portal.gamma_multiply(0.85)
                } else {
                    p.accent.gamma_multiply(0.75)
                },
            );
            return;
        }

        if d.is_portal(t.cfg) {
            self.draw_portal(painter, t, di, sr, lod, requests);
            return;
        }

        let cr = CornerRadius::same((10.0 * z).clamp(2.0, 10.0) as u8);
        painter.rect_filled(sr, cr, if hovered { p.card_hover } else { p.card });
        painter.rect_stroke(
            sr,
            cr,
            Stroke::new(
                if hovered { 1.6 } else { 1.1 },
                if hovered { p.border_strong } else { p.border },
            ),
            StrokeKind::Inside,
        );

        // Open/closed ring indicator.
        let ring_c = self.w2s(Pos2::new(d.x + 20.0, d.y));
        let ring_r = 6.5 * z;
        if ring_r > 1.5 {
            painter.circle_stroke(ring_c, ring_r, Stroke::new((1.8 * z).max(1.0), p.accent));
            if !d.collapsed {
                painter.circle_filled(ring_c, 2.4 * z, p.accent);
            }
        }

        if d.collapsed && (hovered || lod == 2) {
            let (inc, full) = self.grip_positions(sr);
            let grip_r = (4.5 * z).clamp(3.0, 6.0);
            let inc_hover = self.hovered_dir == Some(di as u32)
                && self.hovered_dir_grip == Some(DirGrip::Incremental);
            let full_hover =
                self.hovered_dir == Some(di as u32) && self.hovered_dir_grip == Some(DirGrip::Full);
            painter.circle_filled(
                inc,
                grip_r + if inc_hover { 2.0 } else { 0.0 },
                if inc_hover { p.accent } else { p.border_strong },
            );
            painter.circle_stroke(
                full,
                grip_r + if full_hover { 2.0 } else { 0.0 },
                Stroke::new(
                    1.5,
                    if full_hover {
                        p.portal
                    } else {
                        p.border_strong
                    },
                ),
            );
            painter.circle_stroke(
                full,
                (grip_r * 0.55).max(2.0),
                Stroke::new(
                    1.2,
                    if full_hover {
                        p.portal
                    } else {
                        p.border_strong
                    },
                ),
            );
        }

        let name_px = (13.0 * z).min(15.0);
        if name_px >= 6.0 {
            let text_pos = self.w2s(Pos2::new(d.x + 34.0, d.y));
            if lod == 2 {
                painter.text(
                    text_pos - Vec2::new(0.0, 7.0 * z),
                    Align2::LEFT_CENTER,
                    trunc(&d.name, 15),
                    FontId::proportional(name_px),
                    p.ink,
                );
                let sub_px = (10.5 * z).min(12.0);
                if sub_px >= 6.0 {
                    painter.text(
                        text_pos + Vec2::new(0.0, 9.0 * z),
                        Align2::LEFT_CENTER,
                        format!(
                            "{} files Â· {}{}",
                            group_digits(d.desc_files as u64),
                            human_size(d.desc_bytes),
                            if d.collapsed { "  â–¸" } else { "" }
                        ),
                        FontId::proportional(sub_px),
                        p.sub,
                    );
                }
                if self.any_filter && d.desc_matches > 0 && d.collapsed {
                    painter.text(
                        self.w2s(Pos2::new(d.x + d.w + 10.0, d.y)),
                        Align2::LEFT_CENTER,
                        format!("{} match", group_digits(d.desc_matches as u64)),
                        FontId::proportional(sub_px.max(8.0)),
                        p.accent,
                    );
                }
            } else {
                painter.text(
                    text_pos,
                    Align2::LEFT_CENTER,
                    trunc(&d.name, 13),
                    FontId::proportional(name_px),
                    p.ink,
                );
            }
        }
    }

    fn draw_portal(
        &mut self,
        painter: &egui::Painter,
        t: &Tree,
        di: usize,
        sr: Rect,
        lod: u8,
        requests: &mut Vec<ThumbRequest>,
    ) {
        let p = self.palette();
        let d = &t.dirs[di];
        let z = self.cam.z;
        let hovered = self.hovered_dir == Some(di as u32);
        let cr = CornerRadius::same((12.0 * z).clamp(2.0, 12.0) as u8);
        painter.rect_filled(sr, cr, if hovered { p.card_hover } else { p.card });
        painter.rect_stroke(
            sr,
            cr,
            Stroke::new(if hovered { 1.8 } else { 1.4 }, p.portal),
            StrokeKind::Inside,
        );

        let pad = 9.0 * z;
        let mos_h = sr.height() - 62.0 * z;
        let mos = Rect::from_min_size(
            sr.min + Vec2::splat(pad),
            Vec2::new(sr.width() - pad * 2.0, mos_h.max(0.0)),
        );
        // Structure-only map: keep the portal card but skip the thumbnail
        // mosaic (no previews, no thumbnail requests).
        if mos.height() > 2.0 && !self.structure_only {
            let mp = painter.with_clip_rect(mos);
            mp.rect_filled(mos, CornerRadius::ZERO, p.thumb_bg);
            let gp = 3.0 * z;
            let cw = (mos.width() - gp * 2.0) / 3.0;
            let ch = (mos.height() - gp * 2.0) / 3.0;
            for i in 0..9usize {
                let sample = d.portal_samples.get(i).copied();
                let cell = Rect::from_min_size(
                    mos.min + Vec2::new((i % 3) as f32 * (cw + gp), (i / 3) as f32 * (ch + gp)),
                    Vec2::new(cw, ch),
                );
                match sample {
                    Some(f) => {
                        if lod == 2 {
                            self.maybe_request_full(t, f, requests);
                        }
                        if let Some((tex, last)) = self.textures.get_mut(&f) {
                            *last = self.frame_no;
                            let uv = cover_uv(tex.size_vec2(), cell.size());
                            mp.image(tex.id(), cell, uv, Color32::WHITE);
                        } else {
                            let e = &self.entries[f as usize];
                            let c = self.avg_color[f as usize]
                                .map(|[r, g, b]| Color32::from_rgb(r, g, b))
                                .unwrap_or(e.family.color().gamma_multiply(0.16));
                            mp.rect_filled(cell, CornerRadius::ZERO, c);
                        }
                    }
                    None => {
                        mp.rect_filled(cell, CornerRadius::ZERO, p.thumb_bg.gamma_multiply(1.4));
                    }
                }
            }
        }

        if lod == 2 {
            let name_px = (13.0 * z).min(14.0);
            if name_px >= 6.0 {
                painter.text(
                    Pos2::new(sr.min.x + pad + 2.0 * z, sr.max.y - 33.0 * z),
                    Align2::LEFT_CENTER,
                    trunc(&d.name, 24),
                    FontId::proportional(name_px),
                    p.ink,
                );
                painter.text(
                    Pos2::new(sr.min.x + pad + 2.0 * z, sr.max.y - 18.0 * z),
                    Align2::LEFT_CENTER,
                    format!(
                        "{} items Â· {}",
                        group_digits((d.child_dirs.len() + d.files.len()) as u64),
                        human_size(d.desc_bytes)
                    ),
                    FontId::proportional((10.5 * z).clamp(6.0, 12.0)),
                    p.sub,
                );
                painter.text(
                    Pos2::new(sr.max.x - pad - 2.0 * z, sr.max.y - 18.0 * z),
                    Align2::RIGHT_CENTER,
                    "Enter â¤¢",
                    FontId::proportional((10.5 * z).clamp(6.0, 12.0)),
                    p.portal,
                );
            }
        }
    }

    /// Cache keys are project-root-relative when a shared project cache is
    /// active, so all machines agree on them.
    pub(in crate::app) fn entry_key(&self, e: &FileEntry) -> String {
        if self.key_prefix.is_empty() {
            cache_key(&e.rel, e.size, e.mtime)
        } else {
            cache_key(&format!("{}{}", self.key_prefix, e.rel), e.size, e.mtime)
        }
    }

    fn maybe_request_full(&mut self, _t: &Tree, f: u32, requests: &mut Vec<ThumbRequest>) {
        let i = f as usize;
        let e = &self.entries[i];
        if !wants_thumb(e.family) || e.dead {
            return;
        }
        let key = self.entry_key(e);
        let e = &self.entries[i];
        match self.thumb_state[i] {
            ThumbState::NotAsked | ThumbState::HasColor => {
                self.thumb_state[i] = ThumbState::AskedFull;
                requests.push(ThumbRequest {
                    id: f,
                    generation: self.generation,
                    path: e.path.clone(),
                    key,
                    color_only: false,
                    shared_dir: self.shared_cache.clone(),
                    src_bytes: e.size,
                });
            }
            ThumbState::Loaded if !self.textures.contains_key(&f) => {
                // Evicted â€” ask again (disk cache makes this cheap).
                self.thumb_state[i] = ThumbState::AskedFull;
                requests.push(ThumbRequest {
                    id: f,
                    generation: self.generation,
                    path: e.path.clone(),
                    key,
                    color_only: false,
                    shared_dir: self.shared_cache.clone(),
                    src_bytes: e.size,
                });
            }
            _ => {}
        }
    }

    fn maybe_request_color(
        &mut self,
        f: u32,
        requests: &mut Vec<ThumbRequest>,
        color_budget: &mut i32,
    ) {
        let i = f as usize;
        if *color_budget <= 0 || self.thumb_state[i] != ThumbState::NotAsked {
            return;
        }
        let e = &self.entries[i];
        if !wants_thumb(e.family) || e.dead {
            return;
        }
        *color_budget -= 1;
        let key = self.entry_key(e);
        let e = &self.entries[i];
        self.thumb_state[i] = ThumbState::AskedColor;
        requests.push(ThumbRequest {
            id: f,
            generation: self.generation,
            path: e.path.clone(),
            key,
            color_only: true,
            shared_dir: self.shared_cache.clone(),
            src_bytes: e.size,
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_file_card(
        &mut self,
        painter: &egui::Painter,
        t: &Tree,
        f: u32,
        world: Rect,
        lod: u8,
        dimming: bool,
        requests: &mut Vec<ThumbRequest>,
        color_budget: &mut i32,
    ) {
        let p = self.palette();
        let i = f as usize;
        let e = self.entries[i].clone();
        let z = self.cam.z;
        let sr = self.w2s_rect(world);
        let matched = self.file_match.get(i).copied().unwrap_or(true);
        let alpha = if dimming && !matched { 0.15 } else { 1.0 };
        let fam_color = e.family.color();
        let selected = self.selection.contains(&f);
        let hovered = self.hovered_file == Some(f);

        if lod == 0 {
            // Overview: true-to-scale blocks in the file's own average color.
            self.maybe_request_color(f, requests, color_budget);
            let c = self.avg_color[i]
                .map(|[r, g, b]| Color32::from_rgb(r, g, b))
                .unwrap_or(fam_color.gamma_multiply(0.5));
            painter.rect_filled(sr, CornerRadius::ZERO, c.gamma_multiply(alpha));
            if selected {
                painter.rect_stroke(
                    sr,
                    CornerRadius::ZERO,
                    Stroke::new(1.0, p.select),
                    StrokeKind::Inside,
                );
            }
            return;
        }

        let cr = CornerRadius::same((9.0 * z).clamp(2.0, 9.0) as u8);
        let card_fill = if hovered || selected {
            p.card_hover
        } else {
            p.card
        };
        painter.rect_filled(sr, cr, card_fill.gamma_multiply(alpha));
        let border = if selected {
            Stroke::new(2.0, p.select)
        } else if matched && dimming {
            Stroke::new(1.2, p.accent.gamma_multiply(0.65))
        } else if hovered {
            Stroke::new(1.4, p.border_strong)
        } else {
            Stroke::new(1.0, p.border.gamma_multiply(alpha))
        };
        painter.rect_stroke(sr, cr, border, StrokeKind::Inside);

        if lod == 2 {
            // Thumb area.
            let pad = 6.0 * z;
            let thumb = Rect::from_min_size(
                sr.min + Vec2::splat(pad),
                Vec2::new(sr.width() - pad * 2.0, tree::THUMB_H * z),
            );
            let tp = painter.with_clip_rect(thumb);
            tp.rect_filled(thumb, CornerRadius::ZERO, p.thumb_bg.gamma_multiply(alpha));
            self.maybe_request_full(t, f, requests);
            let mut drew = false;
            if let Some((tex, last)) = self.textures.get_mut(&f) {
                *last = self.frame_no;
                let uv = cover_uv(tex.size_vec2(), thumb.size());
                tp.image(tex.id(), thumb, uv, Color32::WHITE.gamma_multiply(alpha));
                drew = true;
                if e.family == Family::Video {
                    let c = thumb.max - Vec2::splat(14.0 * z);
                    let r = 9.0 * z;
                    if r > 2.0 {
                        tp.circle_filled(
                            Pos2::new(c.x, c.y),
                            r,
                            Color32::from_rgba_unmultiplied(255, 255, 255, 230),
                        );
                        tp.text(
                            Pos2::new(c.x + r * 0.08, c.y),
                            Align2::CENTER_CENTER,
                            "â–¶",
                            FontId::proportional(r),
                            Color32::from_rgb(0x1b, 0x1e, 0x22),
                        );
                    }
                }
            }
            if !drew {
                let c = self.avg_color[i]
                    .map(|[r, g, b]| Color32::from_rgb(r, g, b))
                    .unwrap_or(fam_color.gamma_multiply(0.14));
                tp.rect_filled(thumb, CornerRadius::ZERO, c.gamma_multiply(alpha));
                let glyph_px = 14.0 * z;
                if glyph_px >= 6.0 {
                    tp.text(
                        thumb.center(),
                        Align2::CENTER_CENTER,
                        format!(".{}", if e.ext.is_empty() { "?" } else { &e.ext }),
                        FontId::monospace(glyph_px),
                        if self.avg_color[i].is_some() {
                            Color32::from_rgba_unmultiplied(255, 255, 255, 217)
                        } else {
                            fam_color
                        },
                    );
                }
            }

            // Type tick + name + size.
            let name_px = 11.0 * z;
            if name_px >= 6.0 {
                painter.rect_filled(
                    Rect::from_min_size(
                        self.w2s(Pos2::new(world.min.x + 6.0, world.max.y - 25.0)),
                        Vec2::new(3.0 * z, 11.0 * z),
                    ),
                    CornerRadius::ZERO,
                    fam_color.gamma_multiply(alpha),
                );
                painter.text(
                    self.w2s(Pos2::new(world.min.x + 14.0, world.max.y - 19.0)),
                    Align2::LEFT_CENTER,
                    trunc(&e.name, 20),
                    FontId::proportional(name_px),
                    p.ink.gamma_multiply(alpha),
                );
                painter.text(
                    self.w2s(Pos2::new(world.min.x + 14.0, world.max.y - 8.0)),
                    Align2::LEFT_CENTER,
                    format!("{} Â· {}", human_size(e.size), age_string(e.mtime)),
                    FontId::proportional((9.5 * z).max(6.0)),
                    p.sub.gamma_multiply(alpha),
                );
            }

            // Tag chips (top-left) and staged underline.
            if let Some(tags) = self.tag_state.tags.get(&e.rel) {
                let chip_px = 9.0 * z;
                if chip_px >= 5.0 && !tags.is_empty() {
                    let mut x = sr.min.x + 4.0 * z;
                    let y = sr.min.y + 4.0 * z;
                    for tg in tags.iter().take(3) {
                        let text: String = tg.chars().take(10).collect();
                        let galley = painter.layout_no_wrap(
                            text,
                            FontId::proportional(chip_px),
                            Color32::WHITE,
                        );
                        let w = galley.size().x + 8.0 * z;
                        let chip_rect =
                            Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, 13.0 * z));
                        if chip_rect.max.x > sr.max.x - 8.0 {
                            break;
                        }
                        painter.rect_filled(
                            chip_rect,
                            CornerRadius::same((6.0 * z).clamp(1.0, 6.0) as u8),
                            Color32::from_rgba_unmultiplied(0x2b, 0x4a, 0x63, 220),
                        );
                        painter.galley(Pos2::new(x + 4.0 * z, y + 2.0 * z), galley, Color32::WHITE);
                        x += w + 3.0 * z;
                    }
                }
            }
            if self.tag_state.assigns.contains_key(&e.rel) {
                painter.rect_filled(
                    Rect::from_min_size(
                        Pos2::new(sr.min.x, sr.max.y - 2.0),
                        Vec2::new(sr.width(), 2.0),
                    ),
                    CornerRadius::ZERO,
                    p.staged.gamma_multiply(alpha),
                );
            }
        } else {
            // Mid LOD: simplified color slab.
            self.maybe_request_color(f, requests, color_budget);
            let c = self.avg_color[i]
                .map(|[r, g, b]| Color32::from_rgb(r, g, b))
                .unwrap_or(fam_color.gamma_multiply(0.28));
            let inner = sr.shrink(5.0 * z);
            painter.rect_filled(
                inner,
                CornerRadius::same((6.0 * z).clamp(1.0, 6.0) as u8),
                c.gamma_multiply(alpha),
            );
            if self.tag_state.assigns.contains_key(&e.rel) {
                painter.rect_filled(
                    Rect::from_min_size(
                        Pos2::new(sr.min.x, sr.max.y - 2.0),
                        Vec2::new(sr.width(), 2.0),
                    ),
                    CornerRadius::ZERO,
                    p.staged.gamma_multiply(alpha),
                );
            }
        }
    }
}

/// UV rect that crops a `tex_size` texture to cover `cell` (aspect-fill).
fn cover_uv(tex_size: Vec2, cell: Vec2) -> Rect {
    if tex_size.x <= 0.0 || tex_size.y <= 0.0 || cell.x <= 0.0 || cell.y <= 0.0 {
        return Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
    }
    let tex_aspect = tex_size.x / tex_size.y;
    let cell_aspect = cell.x / cell.y;
    if tex_aspect > cell_aspect {
        // Texture is wider: crop left/right.
        let frac = cell_aspect / tex_aspect;
        let x0 = (1.0 - frac) / 2.0;
        Rect::from_min_max(Pos2::new(x0, 0.0), Pos2::new(x0 + frac, 1.0))
    } else {
        let frac = tex_aspect / cell_aspect;
        let y0 = (1.0 - frac) / 2.0;
        Rect::from_min_max(Pos2::new(0.0, y0), Pos2::new(1.0, y0 + frac))
    }
}

/// Draws an axis-aligned wire route with rounded corners (PCB trace style).
fn rounded_route(painter: &egui::Painter, pts: &[Pos2], radius: f32, stroke: Stroke) {
    if pts.len() < 2 {
        return;
    }
    let mut cursor = pts[0];
    for i in 1..pts.len() {
        let cur = pts[i];
        if i + 1 < pts.len() {
            let next = pts[i + 1];
            let in_v = cur - cursor;
            let out_v = next - cur;
            let in_len = in_v.length();
            let out_len = out_v.length();
            let r = radius.min(in_len * 0.5).min(out_len * 0.5);
            if r < 0.5 || in_len < 0.5 || out_len < 0.5 {
                if in_len >= 0.5 {
                    painter.line_segment([cursor, cur], stroke);
                }
                cursor = cur;
                continue;
            }
            let a = cur - in_v.normalized() * r;
            let b = cur + out_v.normalized() * r;
            painter.line_segment([cursor, a], stroke);
            painter.add(egui::Shape::CubicBezier(
                egui::epaint::CubicBezierShape::from_points_stroke(
                    [a, cur, cur, b],
                    false,
                    Color32::TRANSPARENT,
                    stroke,
                ),
            ));
            cursor = b;
        } else {
            painter.line_segment([cursor, cur], stroke);
        }
    }
}

fn set_subtree_collapsed(t: &mut Tree, di: usize, collapsed: bool) {
    let threshold = t.cfg.normalized().portal_threshold;
    let children = t.dirs[di].child_dirs.clone();
    for c in children {
        let c = c as usize;
        // Full expand stops at large folders: they stay in thumbnail/portal
        // mode until the user explicitly clicks into them.
        if !collapsed && t.dirs[c].child_dirs.len() + t.dirs[c].files.len() > threshold {
            t.dirs[c].collapsed = true;
            continue;
        }
        set_subtree_collapsed(t, c, collapsed);
    }
    if di != 0 {
        t.dirs[di].collapsed = collapsed;
    }
}
