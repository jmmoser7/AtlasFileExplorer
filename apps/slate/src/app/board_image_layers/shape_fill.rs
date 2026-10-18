//! Closed-shape image transfer; authored clips reuse the image/crop model.

use super::*;
use slate_doc::scene::TextNode;

#[cfg(test)]
#[path = "shape_fill/tests.rs"]
mod tests;

#[path = "shape_fill/preview.rs"]
mod preview;
pub(crate) use preview::ShapeFillPreview;

impl SlateApp {
    /// Agent/command entry for the same closed-shape drop journal. IDs name
    /// existing nodes; `keep_source` has the board's Alt-drag meaning.
    pub(crate) fn shape_fill_command(&mut self, detail: Option<&str>) -> bool {
        #[derive(serde::Deserialize)]
        struct Payload {
            source: u64,
            target: u64,
            #[serde(default)]
            keep_source: bool,
        }
        let Some(payload) = detail.and_then(|detail| serde_json::from_str::<Payload>(detail).ok())
        else {
            return false;
        };
        let source = NodeId(payload.source);
        if !self.raster_drop_source(source) {
            return false;
        }
        let Some(_) = self
            .doc()
            .scene
            .node(source)
            .filter(|node| !node.hidden && !node.locked)
        else {
            return false;
        };
        let Some((mut image, aspect)) = self.shape_fill_source(source) else {
            return false;
        };
        if payload.keep_source {
            slate_doc::image_paint::fresh_layer_node_ids(&mut image, || {
                self.doc_mut()
                    .scene
                    .build_node(
                        WorldRect::new(0.0, 0.0, 1.0, 1.0),
                        NodeKind::Image(ImageNode::new(slate_doc::ItemId::NONE)),
                    )
                    .id
            });
        }
        self.fill_shape_with_image(
            NodeId(payload.target),
            image,
            aspect,
            (!payload.keep_source).then_some(source),
        )
    }

    pub(super) fn shape_fill_source(&self, source: NodeId) -> Option<(ImageNode, f32)> {
        self.shape_fill_source_with_output(source, source)
    }

    pub(crate) fn shape_fill_source_with_output(
        &self,
        source: NodeId,
        output_owner: NodeId,
    ) -> Option<(ImageNode, f32)> {
        let node = self.doc().scene.node(source)?;
        let NodeKind::Image(image) = &node.kind else {
            return None;
        };
        let content = slate_doc::geom::image_content_rect(node.rect, image.crop);
        let mut image = image.clone();
        // Session output caches are keyed by node id. Carry the shown output
        // as the durable seed before the source is moved to the target id.
        if let Some(agent) = image.agent.as_mut() {
            let active = self.agent_active_output(output_owner)?;
            agent.seed = self
                .agent_images(output_owner)
                .iter()
                .find(|output| output.id == active)
                .cloned();
        }
        Some((image, content.w / content.h.max(1e-6)))
    }

    pub(super) fn raster_drop_source(&self, id: NodeId) -> bool {
        self.image_item_path(id)
            .is_some_and(|path| slate_doc::media_kind(&path) == slate_doc::MediaKind::Image)
    }

    pub(super) fn is_shape_fill_target(&self, id: NodeId) -> bool {
        self.doc().scene.node(id).is_some_and(|node| {
            !node.hidden
                && !node.locked
                && matches!(&node.kind, NodeKind::Shape(_))
                && slate_doc::scene::supports_fill(node)
        })
    }

    /// Same target identity and geometry, a linked picture behind its shape
    /// boundary. Source layers, result identities, mirrors and filters survive.
    pub(crate) fn fill_shape_with_image(
        &mut self,
        target: NodeId,
        mut image: ImageNode,
        source_aspect: f32,
        remove_source: Option<NodeId>,
    ) -> bool {
        if self.refuse_read_only_edit() || !self.is_shape_fill_target(target) {
            return false;
        }
        let before = self.doc().scene.node(target).unwrap().clone();
        let NodeKind::Shape(shape) = &before.kind else {
            return false;
        };
        let Some(clip) = slate_doc::geom::shape_image_clip(&before) else {
            return false;
        };
        image.crop = slate_doc::geom::cover_crop(before.rect, source_aspect, image.crop);
        image.corner = slate_doc::scene::Corner::Square;
        image.stroke = shape.stroke;
        let mut after = before.clone();
        after.kind = NodeKind::Image(image);
        after.clip = Some(clip);
        let mut overlay = shape
            .text
            .as_ref()
            .filter(|text| !text.body.is_empty())
            .map(|text| {
                let mut node = self.doc_mut().scene.build_node(
                    before.rect,
                    NodeKind::Text(TextNode {
                        text: text.body.clone(),
                        style: Default::default(),
                        family: text.family,
                        size: text.size,
                        color: text.color,
                        align: text.align,
                        fill: None,
                        stroke: slate_doc::scene::Stroke::none(),
                        agent: None,
                    }),
                );
                node.rotation_deg = before.rotation_deg;
                node.opacity = before.opacity;
                node.clip = after.clip.clone();
                node
            });
        if let Some(overlay) = &mut overlay {
            let group = before
                .group
                .unwrap_or_else(|| self.doc_mut().scene.alloc_group_key());
            after.group = Some(group);
            overlay.group = Some(group);
        }
        let mut commands = vec![SceneCmd::Patch {
            before: Box::new(before),
            after: Box::new(after),
        }];
        // Keep hosted text at the shape's authored stacking position.
        let target_index = self.doc().scene.index_of(target).unwrap();
        let source = remove_source.filter(|id| *id != target).and_then(|id| {
            self.doc()
                .scene
                .index_of(id)
                .map(|index| (index, self.doc().scene.node(id).unwrap().clone()))
        });
        if let Some((index, node)) = &source {
            commands.push(SceneCmd::Remove {
                index: *index,
                node: node.clone(),
            });
        }
        if let Some(node) = overlay {
            commands.push(SceneCmd::Add {
                index: target_index + 1
                    - usize::from(
                        source
                            .as_ref()
                            .is_some_and(|(index, _)| *index < target_index),
                    ),
                node,
            });
        }
        if !self.commit_scene(commands) {
            return false;
        }
        self.shape_fill_preview = None;
        self.image_drop_screen = None;
        self.board_sel = std::iter::once(target).collect();
        self.push_history(
            atlas_commands::CommandId("board.image.shape_fill"),
            Some("fill shape".into()),
        );
        true
    }

    /// A clipped image's stroke follows every authored mask contour, using
    /// the existing cached path interpreter rather than a box border.
    pub(crate) fn paint_image_mask_stroke(
        &mut self,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &Node,
        image: &ImageNode,
        outline: &[Pos2],
        alpha: f32,
    ) {
        if let Some(clip) = &node.clip {
            let shape = slate_doc::scene::ShapeNode {
                shape: slate_doc::scene::ShapeKind::Path,
                fill: None,
                stroke: image.stroke,
                corner: slate_doc::scene::Corner::Square,
                sides: slate_doc::scene::default_regular_sides(),
                phase_deg: 0.0,
                flip: false,
                path: None,
                text: None,
            };
            super::super::board_path::paint_path_shape(
                self,
                painter,
                xf,
                node,
                &shape,
                clip,
                &|color| color.gamma_multiply(alpha),
            );
        } else {
            super::super::board::stroke_outline(painter, outline, &image.stroke, xf.z);
        }
    }
}
