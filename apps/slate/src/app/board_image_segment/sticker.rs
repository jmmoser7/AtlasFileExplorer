//! Drag a highlight off its image to place a clipped sticker node.

use super::{runtime::Grab, SlateApp};
use eframe::egui::{self, Pos2};
use slate_doc::scene::{Crop, ImageNode, Node, NodeKind, PathData, WorldRect};

const DRAG_PX: f32 = 4.0;

impl SlateApp {
    /// A press on the silhouette claims the pointer so the photo does not move.
    pub(crate) fn segment_press_claims(&mut self, screen: Pos2, world: Pos2) -> bool {
        if !self.segment_mask_hit(world) {
            return false;
        }
        self.image_segments.grab = Some(Grab {
            origin: screen,
            world,
            moved: false,
        });
        self.image_segments.drag_delta = None;
        true
    }

    /// Live drag. `true` means this frame belongs to the highlight, not hover.
    pub(super) fn segment_drag_frame(
        &mut self,
        ui: &egui::Ui,
        pointer: Option<Pos2>,
        world: Option<Pos2>,
    ) -> bool {
        if self.image_segments.grab.is_none() {
            return false;
        }
        let Some(pointer) = pointer else {
            self.image_segments.grab = None;
            self.image_segments.drag_delta = None;
            return true;
        };
        let Some(world) = world else {
            return true;
        };
        let grab = self.image_segments.grab.as_mut().unwrap();
        if pointer.distance(grab.origin) > DRAG_PX {
            grab.moved = true;
        }
        let origin = grab.world;
        let moved = grab.moved;
        let host = self.image_segments.result.as_ref().map(|r| r.host);
        if moved {
            self.image_segments.drag_delta = Some(world - origin);
        }
        let released = ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary));
        if !released {
            return true;
        }
        let outside = host.is_some_and(|id| !self.point_in_image_paint_window(id, world));
        self.image_segments.grab = None;
        self.image_segments.drag_delta = None;
        if moved && outside {
            self.place_hover_sticker(world);
        } else {
            self.open_segment_tag(pointer);
        }
        true
    }

    pub(crate) fn place_hover_sticker(&mut self, at: Pos2) -> Option<slate_doc::NodeId> {
        let result = self.image_segments.result.clone()?;
        if result.gen != self.scene_gen {
            return None;
        }
        let host = self.doc().scene.node(result.host)?.clone();
        let NodeKind::Image(img) = &host.kind else {
            return None;
        };
        let geom = sticker_from_mask(&host, img, &result.mask, at)?;
        let mut picture = img.clone();
        picture.paint_layers.clear();
        picture.crop = geom.crop;
        let mut node = self
            .doc_mut()
            .scene
            .build_node(geom.rect, NodeKind::Image(picture));
        node.clip = Some(geom.clip);
        node.rotation_deg = 0.0;
        let ids = self.add_nodes(vec![node]);
        let id = *ids.first()?;
        self.push_history(
            atlas_commands::CommandId("board.image.segment.commit"),
            Some("sticker".into()),
        );
        self.image_segments.clear();
        Some(id)
    }

    pub(super) fn paint_sticker_ghost(
        &mut self,
        painter: &egui::Painter,
        xf: &super::super::board::BoardXf,
    ) {
        let Some(delta) = self.image_segments.drag_delta else {
            return;
        };
        let Some(result) = self.image_segments.result.clone() else {
            return;
        };
        let Some(host) = self.doc().scene.node(result.host) else {
            return;
        };
        let NodeKind::Image(img) = &host.kind else {
            return;
        };
        let mut region = slate_doc::image_paint::layer_node_to_world(host, img, &result.local);
        region.rect.x += delta.x;
        region.rect.y += delta.y;
        let NodeKind::Shape(shape) = &region.kind else {
            return;
        };
        let Some(path) = &shape.path else {
            return;
        };
        super::super::board_path::paint_path_shape(self, painter, xf, &region, shape, path, &|c| c);
    }
}

pub(super) struct StickerGeom {
    pub rect: WorldRect,
    pub crop: Crop,
    pub clip: PathData,
}

/// Pixels of `img` inside the mask, as a crop plus a node-local clip path.
pub(super) fn sticker_from_mask(
    host: &Node,
    img: &ImageNode,
    contours: &[Vec<[f32; 2]>],
    drop: Pos2,
) -> Option<StickerGeom> {
    let outer = contours.iter().find(|c| c.len() >= 3)?;
    let (min_x, min_y, max_x, max_y) = outer.iter().fold(
        (f32::MAX, f32::MAX, f32::MIN, f32::MIN),
        |(x0, y0, x1, y1), p| (x0.min(p[0]), y0.min(p[1]), x1.max(p[0]), y1.max(p[1])),
    );
    let bw = (max_x - min_x).max(1e-3);
    let bh = (max_y - min_y).max(1e-3);
    let base = img.crop;
    let x = (base.x + min_x * base.w).clamp(0.0, 0.98);
    let y = (base.y + min_y * base.h).clamp(0.0, 0.98);
    let crop = Crop {
        x,
        y,
        w: (bw * base.w).clamp(0.02, 1.0 - x),
        h: (bh * base.h).clamp(0.02, 1.0 - y),
    };
    let basis = slate_doc::image_paint::image_content_rect(host, img);
    let rect = WorldRect::new(
        drop.x - bw * basis.w * 0.5,
        drop.y - bh * basis.h * 0.5,
        bw * basis.w,
        bh * basis.h,
    );
    let remapped: Vec<Vec<[f32; 2]>> = contours
        .iter()
        .filter(|c| c.len() >= 3)
        .map(|ring| {
            ring.iter()
                .map(|p| [(p[0] - min_x) / bw, (p[1] - min_y) / bh])
                .collect()
        })
        .collect();
    let clip = slate_doc::image_paint::region_path(remapped)?;
    Some(StickerGeom { rect, crop, clip })
}

impl SlateApp {
    pub(super) fn segment_mask_hit(&self, world: Pos2) -> bool {
        let Some(result) = &self.image_segments.result else {
            return false;
        };
        if result.gen != self.scene_gen {
            return false;
        }
        let Some(host) = self.doc().scene.node(result.host) else {
            return false;
        };
        let NodeKind::Image(img) = &host.kind else {
            return false;
        };
        let (x, y) = slate_doc::image_paint::world_to_host_norm(host, img, world.x, world.y);
        vector_ink::point_in_polygon(&result.mask, [x.clamp(0.0, 1.0), y.clamp(0.0, 1.0)])
    }

    pub(super) fn open_segment_tag(&mut self, screen: Pos2) {
        if self.image_segments.result.is_none() {
            return;
        }
        self.image_segments.tag_at = Some(screen);
        self.image_segments.tag_fresh = true;
    }
}

#[cfg(test)]
impl SlateApp {
    /// Stub mask for review sheets and input tests. No model.
    pub(crate) fn install_stub_highlight(&mut self, host: NodeId) -> Pos2 {
        let node = self.doc().scene.node(host).unwrap().clone();
        let NodeKind::Image(img) = &node.kind else {
            panic!("stub highlight needs an image");
        };
        let basis = slate_doc::image_paint::image_content_rect(&node, img);
        let (u, v) = (0.3, 0.55);
        let world = Pos2::new(basis.x + u * basis.w, basis.y + v * basis.h);
        let screen = self.board_xf().w2s(world);
        let mask = stub_contours();
        self.image_segments.hover = Some(super::runtime::Hover {
            tab: self.tab().id,
            host,
            gen: self.scene_gen,
            point_norm: [u, v],
            screen,
            since: std::time::Instant::now() - super::LINGER,
        });
        self.image_segments.result = Some(super::SegmentResult {
            host,
            gen: self.scene_gen,
            local: super::region_node(slate_doc::image_paint::region_path(mask.clone()).unwrap()),
            mask: std::sync::Arc::new(mask),
        });
        screen
    }
}

fn stub_contours() -> Vec<Vec<[f32; 2]>> {
    vec![
        vec![
            [0.15, 0.15],
            [0.8, 0.15],
            [0.8, 0.4],
            [0.5, 0.4],
            [0.5, 0.85],
            [0.15, 0.85],
        ],
        vec![[0.2, 0.3], [0.3, 0.3], [0.3, 0.4], [0.2, 0.4]],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use slate_doc::scene::{ImageNode, PathFillRule};
    use slate_doc::ItemId;

    fn host() -> (Node, ImageNode) {
        let img = ImageNode::new(ItemId::NONE);
        let node = Node {
            id: slate_doc::NodeId(1),
            rect: WorldRect::new(0.0, 0.0, 400.0, 200.0),
            rotation_deg: 0.0,
            opacity: 1.0,
            locked: false,
            hidden: false,
            group: None,
            clip: None,
            bumper: None,
            kind: NodeKind::Image(img.clone()),
        };
        (node, img)
    }

    #[test]
    fn sticker_crop_matches_the_mask_and_the_clip_fills_that_window() {
        let (node, img) = host();
        let contours = vec![
            vec![[0.2, 0.2], [0.8, 0.2], [0.8, 0.7], [0.2, 0.7]],
            vec![[0.3, 0.3], [0.4, 0.3], [0.4, 0.4], [0.3, 0.4]],
        ];
        let geom = sticker_from_mask(&node, &img, &contours, Pos2::new(10.0, 20.0)).unwrap();
        assert!((geom.crop.x - 0.2).abs() < 1e-4);
        assert!((geom.crop.y - 0.2).abs() < 1e-4);
        assert!((geom.crop.w - 0.6).abs() < 1e-3);
        assert!((geom.rect.w - 240.0).abs() < 1e-2);
        assert!((geom.rect.h - 100.0).abs() < 1e-2);
        assert_eq!(geom.clip.fill_rule, PathFillRule::EvenOdd);
        assert_eq!(geom.clip.extra.len(), 1, "the hole stays a second contour");
        assert!((geom.clip.start[0]).abs() < 1e-3);
        assert!((geom.clip.extra[0].start[0] - (0.1 / 0.6)).abs() < 1e-3);
    }
}
