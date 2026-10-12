//! One intake and choice owner for image and closed-shape drops.

use super::*;

/// Drop one image onto another: Replace base vs add as a layer child.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageDropChoice {
    Replace,
    AddLayer,
    FillShape,
}

#[derive(Clone, Debug)]
pub enum ImageDropSource {
    Node(NodeId),
    /// The shown output path survives rewind of a staged Alt copy.
    GeneratedNode {
        node: NodeId,
        path: std::path::PathBuf,
    },
    Item(slate_doc::ItemId),
    /// Snapshot before the staged Alt copy is removed at release.
    ShapeNode {
        node: NodeId,
        image: Box<ImageNode>,
        aspect: f32,
    },
}

#[derive(Clone, Debug)]
pub struct ImageDropOffer {
    pub target: NodeId,
    pub source: ImageDropSource,
    pub highlight: Option<ImageDropChoice>,
    /// Screen strip from the anchor through the last capsule, as last
    /// painted. A pointer crossing it on the way to a choice keeps the offer.
    pub row: Option<egui::Rect>,
}

impl SlateApp {
    /// A staged Alt duplicate borrows its original's runtime output identity.
    pub(crate) fn image_drop_source_owner(&self, source: NodeId) -> NodeId {
        self.staged_dup_sel
            .as_ref()
            .filter(|selection| selection.len() == 1 && !selection.contains(&source))
            .and_then(|selection| selection.iter().next().copied())
            .unwrap_or(source)
    }

    pub(crate) fn update_image_drop_offer(&mut self, world: Pos2, ids: &[NodeId]) {
        let screen = self.board_xf().w2s(world);
        let keep = self.image_drop.as_ref().is_some_and(|offer| {
            matches!(offer.source, ImageDropSource::Node(id) | ImageDropSource::GeneratedNode { node: id, .. } if ids == [id])
                && offer.row.is_some_and(|row| row.contains(screen))
        });
        if keep {
            return;
        }
        self.image_drop = None;
        if ids.len() != 1 || self.board_tool != BoardTool::Select {
            return;
        };
        let source_id = ids[0];
        let Some(source) = self.doc().scene.node(source_id) else {
            return;
        };
        let NodeKind::Image(source_image) = &source.kind else {
            return;
        };
        // An Alt copy has no session cache of its own. Its original still
        // owns the displayed runtime output until the drop is committed.
        let output_owner = self.image_drop_source_owner(source_id);
        if !self.raster_drop_source(output_owner) {
            return;
        };
        let target = self.image_under_point(world, ids);
        let Some(target) = target else {
            return;
        };
        let shape = self.is_shape_fill_target(target);
        let source = if shape {
            let Some((image, aspect)) = self.shape_fill_source_with_output(source_id, output_owner)
            else {
                return;
            };
            ImageDropSource::ShapeNode {
                node: source_id,
                image: Box::new(image),
                aspect,
            }
        } else if source_image.agent.is_some() {
            let Some(path) = self.image_item_path(output_owner) else {
                return;
            };
            ImageDropSource::GeneratedNode {
                node: source_id,
                path,
            }
        } else {
            ImageDropSource::Node(source_id)
        };
        self.image_drop = Some(ImageDropOffer {
            target,
            source,
            highlight: shape.then_some(ImageDropChoice::FillShape),
            row: None,
        });
    }

    pub(crate) fn image_drop_target_at(&self, world: Pos2) -> Option<NodeId> {
        self.image_under_point(world, &[])
    }

    pub(crate) fn offer_image_file_drop(
        &mut self,
        target: NodeId,
        item: slate_doc::ItemId,
        screen: Pos2,
    ) {
        if self.is_shape_fill_target(target) {
            let size = self.image_natural_size(item);
            self.fill_shape_with_image(
                target,
                ImageNode::new(item),
                size.0 / size.1.max(1e-6),
                None,
            );
            self.image_drop_screen = None;
            return;
        }
        self.image_drop = Some(ImageDropOffer {
            target,
            source: ImageDropSource::Item(item),
            highlight: None,
            row: None,
        });
        self.image_drop_screen = Some(screen);
    }

    /// The topmost node under `world` other than `skip`, when it is a raster
    /// image. A dragged image sits under the pointer itself, so it is skipped.
    pub(super) fn image_under_point(&self, world: Pos2, skip: &[NodeId]) -> Option<NodeId> {
        let node = self.doc().scene.nodes.iter().rev().find(|n| {
            !n.hidden
                && !n.locked
                && !n.is_frame()
                && !skip.contains(&n.id)
                && !matches!(n.kind, NodeKind::Connector(_))
                && n.rect.contains_rotated(world.x, world.y, n.rotation_deg)
        })?;
        let pick = node.id;
        if self.is_shape_fill_target(pick) {
            return slate_doc::geom::point_in_node_outline(node, world.x, world.y, 0.05)
                .then_some(pick);
        }
        let NodeKind::Image(_) = &node.kind else {
            return None;
        };
        if !Self::supports_image_paint(pick, self) {
            return None;
        }
        Some(pick)
    }

    pub(crate) fn paint_image_drop_capsules(&mut self, ui: &egui::Ui, painter: &egui::Painter) {
        if self.image_drop.is_none() {
            return;
        }
        if self
            .image_drop
            .as_ref()
            .is_some_and(|offer| offer.highlight == Some(ImageDropChoice::FillShape))
        {
            return; // Closed shapes fill directly; image choices keep their capsules.
        }
        let pointer = ui.ctx().pointer_latest_pos().or(self.image_drop_screen);
        let Some(pointer) = pointer else {
            return;
        };
        if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.image_drop = None;
            self.image_drop_screen = None;
            return;
        }
        let z = self.tab().cam.z;
        let labels = [
            ("Replace", ImageDropChoice::Replace),
            ("Add as layer", ImageDropChoice::AddLayer),
        ];
        let pad = atlas_shell::canvas_scale::px(10.0, z);
        let h = atlas_shell::canvas_scale::px(22.0, z);
        let gap = atlas_shell::canvas_scale::px(6.0, z);
        let palette = self.palette();
        let target = self
            .image_drop
            .as_ref()
            .and_then(|offer| self.doc().scene.node(offer.target));
        let anchor = if let Some(screen) = self.image_drop_screen {
            screen
        } else if let Some(target) = target {
            let edge = target.rect.rotate_point(
                [target.rect.x + target.rect.w, target.rect.center().1],
                target.rotation_deg,
            );
            self.board_xf().w2s(Pos2::new(edge[0], edge[1]))
        } else {
            pointer
        };
        let mut rects = Vec::new();
        let mut x = anchor.x + pad;
        let y = anchor.y;
        let mut highlight = None;
        for (label, choice) in labels {
            let galley = atlas_shell::canvas_text::layout_no_wrap(
                painter,
                label.to_string(),
                atlas_shell::canvas_scale::font(12.0, z),
                palette.ink,
            );
            let w = galley.size().x + pad * 2.0;
            let rect = egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(w, h));
            if rect.contains(pointer) {
                highlight = Some(choice);
            }
            rects.push((rect, label, choice));
            x += w + gap;
        }
        let row = rects.iter().fold(
            egui::Rect::from_min_size(egui::pos2(anchor.x, y), egui::vec2(0.0, h)),
            |row, (rect, _, _)| row.union(*rect),
        );
        let file_drop = self.image_drop_screen.is_some();
        // egui keeps the pointer's last position from before an OS drag, so
        // only real movement away from the target and row dismisses a drop.
        let moved = ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO);
        if file_drop && moved && !row.contains(pointer) {
            let over_target = target.is_some_and(|target| {
                let local = slate_doc::geom::world_to_local(
                    self.board_xf().s2w(pointer).x,
                    self.board_xf().s2w(pointer).y,
                    target.rect,
                    target.rotation_deg,
                );
                (0.0..=1.0).contains(&local.0) && (0.0..=1.0).contains(&local.1)
            });
            if !over_target {
                self.image_drop = None;
                self.image_drop_screen = None;
                return;
            }
        }
        // The capsules are their own foreground area: a press on one is that
        // choice and never starts a board gesture on the image beneath.
        let mut chosen = None;
        let canvas = self.canvas_rect;
        egui::Area::new(egui::Id::new("image_drop_capsules"))
            .order(egui::Order::Foreground)
            .fixed_pos(row.min)
            .constrain(false)
            .movable(false)
            .fade_in(false)
            .show(ui.ctx(), |ui| {
                ui.set_clip_rect(canvas);
                ui.set_min_size(row.size());
                let sense = if file_drop {
                    egui::Sense::click_and_drag()
                } else {
                    egui::Sense::hover()
                };
                for (rect, label, choice) in &rects {
                    let response = ui.interact(*rect, ui.id().with(label), sense);
                    let hot = highlight == Some(*choice);
                    if file_drop && (response.clicked() || (response.drag_stopped() && hot)) {
                        chosen = Some(*choice);
                    }
                    atlas_shell::selection_tools::paint_capsule_row(
                        ui.painter(),
                        *rect,
                        &atlas_shell::selection_tools::Capsule {
                            label,
                            chip: None,
                            badge: None,
                            selected: hot,
                            spawned: false,
                            disabled: false,
                            dim: false,
                        },
                        hot,
                        false,
                        z,
                        palette,
                    );
                }
            });
        if let Some(d) = &mut self.image_drop {
            d.highlight = chosen.or(highlight);
            d.row = Some(row);
        }
        if chosen.is_some() {
            let item = self.image_drop_item();
            self.commit_image_drop(item, false);
        }
    }

    /// A drop target is highlighted: releasing now commits a drop.
    pub(crate) fn image_drop_armed(&self) -> bool {
        self.image_drop
            .as_ref()
            .is_some_and(|offer| offer.highlight.is_some())
    }

    /// The picture the armed drop places. A dragged node's picture must be
    /// read before the drag rewinds, while a staged copy still exists.
    pub(crate) fn image_drop_item(&self) -> Option<slate_doc::ItemId> {
        let offer = self.image_drop.as_ref()?;
        let item = match offer.source {
            ImageDropSource::Node(id) => match &self.doc().scene.node(id)?.kind {
                NodeKind::Image(img) => img.item,
                _ => return None,
            },
            ImageDropSource::Item(item) => item,
            ImageDropSource::ShapeNode { ref image, .. } => image.item,
            ImageDropSource::GeneratedNode { .. } => return None,
        };
        (!item.is_none()).then_some(item)
    }

    /// Commit the armed drop of `item` as one journal group (P0.2). The
    /// caller has already rewound any live drag. A dragged source node is
    /// consumed unless `keep_source` (an Alt copy leaves its original).
    pub(crate) fn commit_image_drop(
        &mut self,
        item: Option<slate_doc::ItemId>,
        keep_source: bool,
    ) -> bool {
        let Some(offer) = self.image_drop.take() else {
            return false;
        };
        if let ImageDropSource::ShapeNode {
            node,
            image,
            aspect,
        } = offer.source
        {
            let remove = (!keep_source).then_some(node);
            return self.fill_shape_with_image(offer.target, *image, aspect, remove);
        }
        let item = match &offer.source {
            ImageDropSource::GeneratedNode { path, .. } => self.item_for_path(path),
            _ => item,
        };
        let (Some(choice), Some(item)) = (offer.highlight, item) else {
            return false;
        };
        let source_node = match offer.source {
            ImageDropSource::Node(id) | ImageDropSource::GeneratedNode { node: id, .. }
                if !keep_source =>
            {
                Some(id)
            }
            _ => None,
        };
        let ok = match choice {
            ImageDropChoice::Replace => {
                self.replace_image_item_preserve_layers(offer.target, item, source_node)
            }
            ImageDropChoice::AddLayer => {
                self.add_dropped_image_as_layer(offer.target, item, source_node)
            }
            ImageDropChoice::FillShape => false,
        };
        if ok {
            self.image_drop_screen = None;
            return true;
        }
        false
    }
}
