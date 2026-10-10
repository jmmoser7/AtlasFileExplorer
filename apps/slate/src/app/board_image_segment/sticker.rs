//! Drag a highlight off its image to place a clipped sticker node.

use super::{runtime::Grab, SlateApp};
use eframe::egui::{self, Pos2, Vec2};
use slate_doc::scene::{Corner, Crop, ImageNode, Node, NodeKind, PathData, Stroke, WorldRect};
use slate_doc::NodeId;

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
            journal_top: self.tab().journal.top_token(),
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
        let journal_top = grab.journal_top;
        let host = self.image_segments.result.as_ref().map(|r| r.host);
        if moved {
            self.image_segments.drag_delta = Some(world - origin);
        }
        let released = ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary));
        if !released {
            return true;
        }
        let outside = host.is_some_and(|id| !self.point_in_image_paint_window(id, world));
        if journal_top == self.tab().journal.top_token() {
            let gen = self.scene_gen;
            if let Some(result) = self.image_segments.result.as_mut() {
                result.gen = gen;
            }
            if let Some(hover) = self.image_segments.hover.as_mut() {
                hover.gen = gen;
            }
        }
        self.image_segments.grab = None;
        self.image_segments.drag_delta = None;
        if moved && outside {
            self.place_hover_sticker(world - origin);
        } else if !moved {
            self.open_segment_tag(pointer);
        }
        true
    }

    /// Commit the highlight as a cut-out image node, offset by `delta` from
    /// where it sits on its picture. One journaled add; Undo removes it.
    pub(crate) fn place_hover_sticker(&mut self, delta: Vec2) -> Option<NodeId> {
        let mut node = self.hover_sticker_node(delta)?;
        node.id = self
            .doc_mut()
            .scene
            .build_node(node.rect, node.kind.clone())
            .id;
        let id = *self.add_nodes(vec![node]).first()?;
        self.push_history(
            atlas_commands::CommandId("board.image.segment.commit"),
            Some("sticker".into()),
        );
        self.image_segments.clear();
        Some(id)
    }

    /// The sticker the live highlight would become, still carrying its host's
    /// id. Only pictures backed by a file can be cut out.
    fn hover_sticker_node(&self, delta: Vec2) -> Option<Node> {
        let result = self.image_segments.result.as_ref()?;
        if result.gen != self.scene_gen {
            return None;
        }
        let host = self.doc().scene.node(result.host)?;
        let NodeKind::Image(img) = &host.kind else {
            return None;
        };
        if img.item.is_none() {
            return None;
        }
        let geom = sticker_from_mask(host, img, &result.mask, delta)?;
        let mut picture = img.clone();
        picture.paint_layers.clear();
        picture.agent = None;
        picture.corner = Corner::Square;
        picture.stroke = Stroke::default();
        picture.crop = geom.crop;
        Some(Node {
            id: host.id,
            rect: geom.rect,
            rotation_deg: host.rotation_deg,
            opacity: host.opacity,
            locked: false,
            hidden: false,
            group: None,
            clip: Some(geom.clip),
            bumper: None,
            kind: NodeKind::Image(picture),
        })
    }

    /// Mid-drag the cut-out itself follows the pointer, painted by the same
    /// node painter that paints the placed sticker.
    pub(super) fn paint_sticker_ghost(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &super::super::board::BoardXf,
    ) {
        let Some(delta) = self.image_segments.drag_delta else {
            return;
        };
        if let Some(ghost) = self.hover_sticker_node(delta) {
            self.paint_board_node(ui, painter, xf, &ghost, false);
        }
    }
}

pub(super) struct StickerGeom {
    pub rect: WorldRect,
    pub crop: Crop,
    pub clip: PathData,
}

/// Pixels of `img` inside the mask, as a crop plus a node-local clip path.
/// Mask points are normalized over the full content as displayed, which is
/// the space [`Crop`] uses, so the bounding box is the crop window.
pub(super) fn sticker_from_mask(
    host: &Node,
    img: &ImageNode,
    contours: &[Vec<[f32; 2]>],
    delta: Vec2,
) -> Option<StickerGeom> {
    let rings: Vec<Vec<[f32; 2]>> = contours
        .iter()
        .filter(|c| c.len() >= 3)
        .map(|c| {
            c.iter()
                .map(|p| [p[0].clamp(0.0, 1.0), p[1].clamp(0.0, 1.0)])
                .collect()
        })
        .collect();
    let (min_x, min_y, max_x, max_y) = rings.iter().flatten().fold(
        (f32::MAX, f32::MAX, f32::MIN, f32::MIN),
        |(x0, y0, x1, y1), p| (x0.min(p[0]), y0.min(p[1]), x1.max(p[0]), y1.max(p[1])),
    );
    if rings.is_empty() || max_x - min_x < 1e-3 || max_y - min_y < 1e-3 {
        return None;
    }
    let (bw, bh) = (max_x - min_x, max_y - min_y);
    let crop = Crop {
        x: min_x,
        y: min_y,
        w: bw,
        h: bh,
    };
    let basis = slate_doc::image_paint::image_content_rect(host, img);
    let (w, h) = (bw * basis.w, bh * basis.h);
    let local = [
        basis.x + (min_x + bw * 0.5) * basis.w,
        basis.y + (min_y + bh * 0.5) * basis.h,
    ];
    let [cx, cy] = host.rect.rotate_point(local, host.rotation_deg);
    let rect = WorldRect::new(cx + delta.x - w * 0.5, cy + delta.y - h * 0.5, w, h);
    let remapped = rings
        .into_iter()
        .map(|ring| {
            ring.into_iter()
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
        if self.image_segments.result.is_none() || self.image_segments.tag_at.is_some() {
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

#[cfg(test)]
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
    use slate_doc::scene::PathFillRule;
    use slate_doc::ItemId;

    fn host(crop: Crop, rotation_deg: f32) -> (Node, ImageNode) {
        let mut img = ImageNode::new(ItemId::NONE);
        img.crop = crop;
        let node = Node {
            id: NodeId(1),
            rect: WorldRect::new(0.0, 0.0, 400.0, 200.0),
            rotation_deg,
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

    fn square_with_hole() -> Vec<Vec<[f32; 2]>> {
        vec![
            vec![[0.2, 0.2], [0.8, 0.2], [0.8, 0.7], [0.2, 0.7]],
            vec![[0.3, 0.3], [0.4, 0.3], [0.4, 0.4], [0.3, 0.4]],
        ]
    }

    #[test]
    fn sticker_crop_matches_the_mask_and_the_clip_fills_that_window() {
        let (node, img) = host(Crop::full(), 0.0);
        let geom = sticker_from_mask(&node, &img, &square_with_hole(), Vec2::ZERO).unwrap();
        assert!((geom.crop.x - 0.2).abs() < 1e-4);
        assert!((geom.crop.y - 0.2).abs() < 1e-4);
        assert!((geom.crop.w - 0.6).abs() < 1e-3);
        assert!((geom.rect.w - 240.0).abs() < 1e-2);
        assert!((geom.rect.h - 100.0).abs() < 1e-2);
        assert!((geom.rect.x - 80.0).abs() < 1e-2, "lands where it was cut");
        assert!((geom.rect.y - 40.0).abs() < 1e-2);
        assert_eq!(geom.clip.fill_rule, PathFillRule::EvenOdd);
        assert_eq!(geom.clip.extra.len(), 1, "the hole stays a second contour");
        assert!((geom.clip.start[0]).abs() < 1e-3);
        assert!((geom.clip.extra[0].start[0] - (0.1 / 0.6)).abs() < 1e-3);
    }

    #[test]
    fn a_cropped_host_cuts_from_its_full_content_not_its_window() {
        let crop = Crop {
            x: 0.25,
            y: 0.0,
            w: 0.5,
            h: 1.0,
        };
        let (node, img) = host(crop, 0.0);
        let mask = vec![vec![[0.3, 0.2], [0.6, 0.2], [0.6, 0.6], [0.3, 0.6]]];
        let geom = sticker_from_mask(&node, &img, &mask, Vec2::new(10.0, 0.0)).unwrap();
        assert!((geom.crop.x - 0.3).abs() < 1e-4, "mask space is crop space");
        assert!((geom.crop.w - 0.3).abs() < 1e-4);
        // Content is 800 wide and starts at -200; the window is 400 wide.
        assert!((geom.rect.w - 240.0).abs() < 1e-2);
        assert!((geom.rect.x - (-200.0 + 0.3 * 800.0 + 10.0)).abs() < 1e-2);
    }

    #[test]
    fn a_rotated_host_keeps_the_cut_where_it_was_drawn() {
        let (node, img) = host(Crop::full(), 90.0);
        let geom = sticker_from_mask(&node, &img, &square_with_hole(), Vec2::ZERO).unwrap();
        let local = [80.0 + 120.0, 40.0 + 50.0];
        let [cx, cy] = node.rect.rotate_point(local, 90.0);
        let c = geom.rect.center();
        assert!((c.0 - cx).abs() < 1e-2 && (c.1 - cy).abs() < 1e-2);
    }
}
