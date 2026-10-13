//! Derived saved-view drag feedback and return lifecycle.

use super::*;

impl SlateApp {
    /// The saved view held over a model this frame, if any: a dragged board
    /// picture or a single image file an OS drag is holding.
    fn view_drop_hover(&self, ctx: &egui::Context) -> Option<(ViewDropSource, NodeId, Pos2)> {
        if self.doc().view.active_view != slate_doc::ViewKind::Board || self.tab().read_only {
            return None;
        }
        let (source, pointer) = if let Some(BoardDrag::Move { ids, .. }) = &self.board_drag {
            if self.bumper.dragging() {
                return None;
            }
            let (image, _) = self.dragged_view_image(ids)?;
            (
                ViewDropSource::Node(image),
                ctx.input(|i| i.pointer.hover_pos())?,
            )
        } else {
            let (path, at) = self.external_drop.hover_file()?;
            if slate_doc::media_kind(&path) != slate_doc::MediaKind::Image {
                return None;
            }
            (ViewDropSource::File(path), at)
        };
        let model = self.model_node_at_world(self.board_xf().s2w(pointer))?;
        if self.doc().scene.node(model).is_none_or(|n| n.locked) {
            return None;
        }
        Some((source, model, pointer))
    }

    fn start_view_drop_preview(
        &self,
        source: ViewDropSource,
        model: NodeId,
        pointer: Pos2,
    ) -> ViewDropPreview {
        let path = match &source {
            ViewDropSource::File(path) => Some(path.clone()),
            ViewDropSource::Node(id) => self.image_item_path(*id),
        };
        let thumbnail = matches!(source, ViewDropSource::File(_));
        let (tx, rx) = std::sync::mpsc::channel();
        if let Some(path) = path {
            std::thread::spawn(move || {
                let msg = if atlas_core::cloud::is_dehydrated(&path) {
                    None
                } else {
                    view_meta::read_view_meta(&path)
                        .ok()
                        .flatten()
                        .map(|parsed| (parsed, thumbnail.then(|| file_thumbnail(&path)).flatten()))
                };
                let _ = tx.send(msg);
            });
        }
        let mut preview = ViewDropPreview::new(self.tab().id, model, source, pointer);
        preview.rx = Some(rx);
        preview
    }

    /// A delayed packet commits once, then uses the existing preview motion.
    pub(super) fn begin_released_view_drop(
        &mut self,
        pending: PendingViewDrop,
        parsed: view_meta::ViewMetaParsed,
        thumbnail: Option<egui::ColorImage>,
    ) {
        if self.tab().read_only
            || self
                .doc()
                .scene
                .node(pending.model)
                .is_none_or(|n| n.hidden || n.locked)
        {
            return;
        }
        self.cancel_view_drop_preview();
        let model = pending.model;
        let Some(info) = self.model_node_info(model) else {
            return;
        };
        let entered = !self.model3d.live.contains_key(&model);
        if entered {
            self.unlock_model(model);
        }
        let opened = self.model3d.live.contains_key(&model);
        let original = self.model3d.live.get(&model).map_or(info.cam, |vp| vp.cam);
        let source = pending.picture.as_ref().map_or_else(
            || ViewDropSource::File(pending.image_path),
            |picture| ViewDropSource::Node(picture.id),
        );
        let mut p = ViewDropPreview::new(pending.tab_id, model, source, pending.pointer);
        p.entered = entered && opened;
        p.original = Some(original);
        p.released = true;
        p.picture = pending.picture;
        p.thumb = thumbnail.map(|image| {
            self.documents.ui_ctx().load_texture(
                "view-drop-picture",
                image,
                egui::TextureOptions::LINEAR,
            )
        });
        let to = parsed.camera;
        self.apply_view_drop(model, parsed.clone());
        p.parsed = Some(parsed);
        if let Some(vp) = self.model3d.live.get_mut(&model) {
            vp.cam = original;
            vp.view_tween = Some(ViewTween {
                from: original,
                to,
                started: p.tick,
            });
        }
        self.view_drop_preview = Some(p);
        self.documents.ui_ctx().request_repaint();
    }

    /// Per frame: follow the drag, orient the viewport to the held view,
    /// and put everything back once it leaves.
    pub(crate) fn maintain_view_drop_preview(&mut self, ctx: &egui::Context) {
        let hover = self.view_drop_hover(ctx);
        let tab_id = self.tab().id;
        let stale = self.view_drop_preview.as_ref().is_some_and(|p| {
            p.tab_id != tab_id
                || p.released
                    && p.parsed.as_ref().is_some_and(|saved| {
                        self.model_node_info(p.model)
                            .is_none_or(|info| info.cam != saved.camera)
                    })
                || !p.released
                    && hover
                        .as_ref()
                        .is_some_and(|(source, model, _)| *model != p.model || *source != p.source)
        });
        if stale {
            self.cancel_view_drop_preview();
        }
        let Some(mut p) = self.view_drop_preview.take().or_else(|| {
            let (source, model, pointer) = hover.clone()?;
            Some(self.start_view_drop_preview(source, model, pointer))
        }) else {
            return;
        };
        if let Some(rx) = &p.rx {
            match rx.try_recv() {
                Ok(msg) => {
                    p.rx = None;
                    if let Some((parsed, thumb)) = msg {
                        p.parsed = Some(parsed);
                        p.thumb = thumb.map(|img| {
                            ctx.load_texture("view-drop-picture", img, egui::TextureOptions::LINEAR)
                        });
                    }
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => p.rx = None,
            }
        }
        let over = p.released || hover.is_some();
        if let Some((_, _, pointer)) = hover.filter(|_| !p.released) {
            p.pointer = pointer;
        }
        let now = Instant::now();
        let dt = now.duration_since(p.tick);
        p.tick = now;

        let saved = p.parsed.as_ref().filter(|_| over).map(|v| v.camera);
        if let Some(to) = saved {
            if p.original.is_none() {
                if !self.model3d.live.contains_key(&p.model) {
                    self.unlock_model(p.model);
                    p.entered = self.model3d.live.contains_key(&p.model);
                }
                if let Some(vp) = self.model3d.live.get_mut(&p.model) {
                    if p.entered {
                        vp.before = vp.cam;
                    }
                    p.original = Some(vp.cam);
                    vp.view_tween = Some(ViewTween {
                        from: vp.cam,
                        to,
                        started: now,
                    });
                }
            } else if p.returning {
                p.returning = false;
                if let Some(vp) = self.model3d.live.get_mut(&p.model) {
                    vp.view_tween = Some(ViewTween {
                        from: vp.cam,
                        to,
                        started: now,
                    });
                }
            }
        } else if !over && !p.returning {
            p.returning = true;
            if let (Some(original), Some(vp)) = (p.original, self.model3d.live.get_mut(&p.model)) {
                vp.view_tween = Some(ViewTween {
                    from: vp.cam,
                    to: original,
                    started: now,
                });
            }
        }
        if let Some(vp) = self.model3d.live.get_mut(&p.model) {
            if p.original.is_some() {
                vp.last_interact = now;
            }
        }

        p.sink = atlas_shell::drop_transfer::advance(p.sink, dt, over && p.parsed.is_some());
        let settled = self
            .model3d
            .live
            .get(&p.model)
            .is_none_or(|vp| vp.view_tween.is_none());
        if p.released && p.sink >= 1.0 && settled {
            if p.entered {
                self.lock_model(p.model);
            }
            return;
        }
        if !over && p.sink <= 0.0 && settled {
            // The return tween ended exactly on the original pose.
            if p.entered {
                self.model3d.live.remove(&p.model);
            }
            return;
        }
        let sinking = if over && p.parsed.is_some() {
            p.sink < 1.0
        } else {
            p.sink > 0.0
        };
        if p.rx.is_some() || !settled || sinking {
            ctx.request_repaint();
        }
        self.view_drop_preview = Some(p);
    }

    /// Esc while a board picture holds a saved view over a model: the drag
    /// is cancelled (egui aborts drags on Esc, so no release will come), the
    /// picture goes back unjournaled, and the viewport tweens home.
    pub(crate) fn dismiss_view_drop_preview(&mut self) -> bool {
        let tab_id = self.tab().id;
        let held = self.view_drop_preview.as_ref().is_some_and(|p| {
            p.tab_id == tab_id && p.parsed.is_some() && matches!(p.source, ViewDropSource::Node(_))
        });
        if !held || !matches!(self.board_drag, Some(BoardDrag::Move { .. })) {
            return false;
        }
        self.cancel_node_drag()
    }

    /// End the preview at once, putting the viewport back as it was.
    fn cancel_view_drop_preview(&mut self) {
        let Some(p) = self.view_drop_preview.take() else {
            return;
        };
        if p.tab_id != self.tab().id {
            return;
        }
        self.restore_view_drop_camera(&p);
        if p.entered {
            self.model3d.live.remove(&p.model);
        }
    }

    /// A viewport under preview is being frozen: it freezes at its own pose.
    pub(crate) fn release_view_drop_preview_for(&mut self, model: NodeId) {
        if self
            .view_drop_preview
            .as_ref()
            .is_some_and(|p| p.model == model && p.tab_id == self.tab().id)
        {
            if let Some(p) = self.view_drop_preview.take() {
                self.restore_view_drop_camera(&p);
            }
        }
    }

    fn restore_view_drop_camera(&mut self, p: &ViewDropPreview) {
        let camera = if p.released {
            self.model_node_info(p.model).map(|info| info.cam)
        } else {
            p.original
        };
        if let (Some(original), Some(vp)) = (camera, self.model3d.live.get_mut(&p.model)) {
            vp.cam = original;
            if p.released {
                vp.before = original;
            }
            vp.view_tween = None;
        }
    }

    /// The held view, when the release lands on the model it previews.
    pub(super) fn take_view_drop_preview(
        &mut self,
        model: NodeId,
        source: &ViewDropSource,
    ) -> Option<(view_meta::ViewMetaParsed, bool)> {
        let p = self.view_drop_preview.as_ref()?;
        if p.model != model || p.source != *source || p.tab_id != self.tab().id {
            return None;
        }
        p.parsed.as_ref()?;
        let p = self.view_drop_preview.take()?;
        self.restore_view_drop_camera(&p);
        Some((p.parsed?, p.entered))
    }

    /// Paint-clone hook: the dragged picture sinks into the viewport.
    pub(crate) fn sink_view_drop_picture(&self, nodes: &mut Vec<Node>) {
        let Some(p) = self
            .view_drop_preview
            .as_ref()
            .filter(|p| p.sink > 0.0 && p.tab_id == self.tab().id)
        else {
            return;
        };
        let ViewDropSource::Node(picture) = p.source else {
            return;
        };
        let Some(m) = self.doc().scene.node(p.model).map(|m| m.rect) else {
            return;
        };
        let into = Pos2::new(m.x + m.w * 0.5, m.y + m.h * 0.5);
        let sink = |n: &mut Node| {
            let from =
                Rect::from_min_size(Pos2::new(n.rect.x, n.rect.y), Vec2::new(n.rect.w, n.rect.h));
            let (r, alpha) = atlas_shell::drop_transfer::visual(from, into, p.sink);
            n.rect = WorldRect::new(r.min.x, r.min.y, r.width(), r.height());
            n.opacity *= alpha;
        };
        if let Some(picture) = &p.picture {
            let mut picture = picture.clone();
            sink(&mut picture);
            nodes.push(picture);
        } else if let Some(n) = nodes.iter_mut().find(|n| n.id == picture) {
            sink(n);
        }
    }

    /// A dragged file's picture leaves the pointer and sinks into the
    /// viewport.
    pub(crate) fn paint_view_drop_file(&self, painter: &egui::Painter, xf: &BoardXf) {
        let Some(p) = self
            .view_drop_preview
            .as_ref()
            .filter(|p| p.sink > 0.0 && p.tab_id == self.tab().id)
        else {
            return;
        };
        let (ViewDropSource::File(_), Some(tex)) = (&p.source, &p.thumb) else {
            return;
        };
        let Some(m) = self.doc().scene.node(p.model) else {
            return;
        };
        let size = tex.size_vec2();
        let from =
            Rect::from_center_size(p.pointer, size * (SINK_FILE_PX / size.max_elem().max(1.0)));
        let (r, alpha) =
            atlas_shell::drop_transfer::visual(from, xf.rect_w2s(m.rect).center(), p.sink);
        let alpha = alpha * (p.sink * 4.0).min(1.0);
        painter.image(
            tex.id(),
            r,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE.gamma_multiply(alpha),
        );
    }
}
