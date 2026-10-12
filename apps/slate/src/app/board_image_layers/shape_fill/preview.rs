//! Derived drop feedback; timing and visual easing belong to atlas-shell.

use super::*;
use crate::app::board::BoardDrag;
use std::{path::PathBuf, sync::mpsc::Receiver, time::Instant};

#[derive(Clone, PartialEq)]
enum Source {
    Node(NodeId),
    File(PathBuf),
}

/// Derived feedback only; the drop's one scene command owns the result.
pub(crate) struct ShapeFillPreview {
    tab: u64,
    target: NodeId,
    source: Source,
    progress: f32,
    tick: Instant,
    pointer: Pos2,
    thumbnail: Option<egui::TextureHandle>,
    rx: Option<Receiver<Option<egui::ColorImage>>>,
}

impl SlateApp {
    fn shape_fill_hover(&self, ctx: &egui::Context) -> Option<(Source, NodeId, Pos2)> {
        if self.tab().read_only || self.at_home || self.board_tool != BoardTool::Select {
            return None;
        }
        let (source, pointer, skip) = match &self.board_drag {
            Some(BoardDrag::Move { ids, .. }) if ids.len() == 1 && !self.bumper.dragging() => {
                let id = ids[0];
                if !self.raster_drop_source(self.image_drop_source_owner(id)) {
                    return None;
                }
                (Source::Node(id), ctx.pointer_latest_pos()?, ids.as_slice())
            }
            Some(_) => return None,
            None => {
                let (path, pointer) = self.external_drop.hover_file()?;
                if slate_doc::media_kind(&path) != slate_doc::MediaKind::Image {
                    return None;
                }
                (Source::File(path), pointer, &[][..])
            }
        };
        let target = self.image_under_point(self.board_xf().s2w(pointer), skip)?;
        self.is_shape_fill_target(target)
            .then_some((source, target, pointer))
    }

    pub(crate) fn maintain_shape_fill_preview(&mut self, ctx: &egui::Context) {
        let hover = self.shape_fill_hover(ctx);
        let tab = self.tab().id;
        if self.shape_fill_preview.as_ref().is_some_and(|preview| {
            preview.tab != tab
                || hover.as_ref().is_some_and(|(source, target, _)| {
                    source != &preview.source || *target != preview.target
                })
        }) {
            self.shape_fill_preview = None;
        }
        if self.shape_fill_preview.is_none() {
            let Some((source, target, pointer)) = hover.clone() else {
                return;
            };
            let rx = if let Source::File(path) = &source {
                let path = path.clone();
                let (tx, rx) = std::sync::mpsc::channel();
                std::thread::spawn(move || {
                    let thumbnail = if atlas_core::cloud::is_dehydrated(&path) {
                        None
                    } else {
                        image::open(&path).ok().map(|image| {
                            let image = image.thumbnail(256, 256).to_rgba8();
                            egui::ColorImage::from_rgba_unmultiplied(
                                [image.width() as usize, image.height() as usize],
                                image.as_raw(),
                            )
                        })
                    };
                    let _ = tx.send(thumbnail);
                });
                Some(rx)
            } else {
                None
            };
            self.shape_fill_preview = Some(ShapeFillPreview {
                tab,
                source,
                target,
                pointer,
                progress: 0.0,
                tick: Instant::now(),
                thumbnail: None,
                rx,
            });
        }
        let preview = self.shape_fill_preview.as_mut().unwrap();
        if let Some(rx) = &preview.rx {
            match rx.try_recv() {
                Ok(image) => {
                    preview.thumbnail = image.map(|image| {
                        ctx.load_texture("shape-fill-file", image, egui::TextureOptions::LINEAR)
                    });
                    preview.rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => preview.rx = None,
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some((_, _, pointer)) = hover {
            preview.pointer = pointer;
        }
        let now = Instant::now();
        let over = hover.is_some();
        preview.progress = atlas_shell::drop_transfer::advance(
            preview.progress,
            now.duration_since(preview.tick),
            over,
        );
        preview.tick = now;
        if !over && preview.progress <= 0.0 {
            self.shape_fill_preview = None;
        } else if preview.rx.is_some()
            || (over && preview.progress < 1.0)
            || (!over && preview.progress > 0.0)
        {
            ctx.request_repaint();
        }
    }

    pub(crate) fn sink_shape_fill_picture(&self, nodes: &mut [Node]) {
        let Some(preview) = &self.shape_fill_preview else {
            return;
        };
        let Source::Node(source) = preview.source else {
            return;
        };
        let Some(target) = self.doc().scene.node(preview.target) else {
            return;
        };
        let Some(node) = nodes.iter_mut().find(|node| node.id == source) else {
            return;
        };
        let from = egui::Rect::from_min_size(
            Pos2::new(node.rect.x, node.rect.y),
            egui::vec2(node.rect.w, node.rect.h),
        );
        let (x, y) = target.rect.center();
        let (rect, alpha) =
            atlas_shell::drop_transfer::visual(from, Pos2::new(x, y), preview.progress);
        node.rect = WorldRect::new(rect.min.x, rect.min.y, rect.width(), rect.height());
        node.opacity *= alpha;
    }

    pub(crate) fn paint_shape_fill_file(&self, painter: &egui::Painter, xf: &BoardXf) {
        let Some(preview) = &self.shape_fill_preview else {
            return;
        };
        let (Source::File(_), Some(texture)) = (&preview.source, &preview.thumbnail) else {
            return;
        };
        let Some(target) = self.doc().scene.node(preview.target) else {
            return;
        };
        let size = texture.size_vec2();
        let from = egui::Rect::from_center_size(
            preview.pointer,
            size * (120.0 / size.max_elem().max(1.0)),
        );
        let (rect, alpha) = atlas_shell::drop_transfer::visual(
            from,
            xf.rect_w2s(target.rect).center(),
            preview.progress,
        );
        painter.image(
            texture.id(),
            rect,
            egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE.gamma_multiply(alpha),
        );
    }
}
