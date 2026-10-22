//! Shared wire-port pointer reach and overlap eligibility.
use super::*;
use atlas_shell::selection_tools::hit_regions::{Domain, Field};

impl SlateApp {
    pub(super) fn wire_host_overlaps_lower(&self, node: &Node, xf: &BoardXf) -> bool {
        let scene = &self.doc().scene;
        let Some(index) = scene.index_of(node.id) else {
            return false;
        };
        self.doc()
            .outline_queries()
            .outline_overlaps(node.id, 0.25 / xf.z.max(0.001))
            .iter()
            .any(|id| {
                scene.index_of(*id).is_some_and(|other| {
                    other < index
                        && !scene.nodes[other].hidden
                        && !scene.nodes[other].is_frame()
                        && !matches!(scene.nodes[other].kind, NodeKind::Connector(_))
                })
            })
    }

    pub(super) fn wire_cursor_inside(&self, node: &Node, world: Pos2, xf: &BoardXf) -> bool {
        let (cx, cy) = node.rect.center();
        let (x, y) =
            slate_doc::geom::world_to_local_about(world.x, world.y, cx, cy, node.rotation_deg);
        x > node.rect.x + 1e-6
            && x < node.rect.x + node.rect.w - 1e-6
            && y > node.rect.y + 1e-6
            && y < node.rect.y + node.rect.h - 1e-6
            && self.doc().outline_queries().node_outline_contains(
                node.id,
                [world.x, world.y],
                0.25 / xf.z.max(0.001),
            )
    }

    pub(super) fn semantic_wire_pick(
        &self,
        node: &Node,
        screen: Pos2,
        xf: &BoardXf,
        point: [f32; 2],
        outward: [f32; 2],
    ) -> bool {
        let shortest = node.rect.w.abs().min(node.rect.h.abs()) * xf.z;
        let Some(reach) = Domain::Wire.reach(shortest, GRIP_HIT_PX) else {
            return false;
        };
        let a = xf.w2s(Pos2::new(point[0], point[1]));
        let field = Field::Segment {
            a,
            b: a + Vec2::new(outward[0], outward[1])
                * reach
                * atlas_shell::selection_tools::hit_regions::WIRE_EXTENSION,
        };
        let Some(distance) = field.pick(screen, screen, reach) else {
            return false;
        };
        let geom = super::super::board_handles::selection_geom(xf, node.rect, node.rotation_deg);
        let chrome = super::super::board_handles::chrome_pick(
            screen,
            &geom,
            Self::node_allows_rotation(node),
        );
        chrome.is_none_or(|(_, score)| 0.02 + 0.6 * distance < score)
    }
    /// The enlarged port reach yields to transform chrome; the port's real
    /// hit disc remains available at its midpoint.
    pub(super) fn wire_extension_yields_to_transform(
        &self,
        node: &Node,
        point: Pos2,
        xf: &BoardXf,
    ) -> bool {
        let chrome = self.node_interaction_geom(node, xf);
        self.node_offers_bbox_transform(node)
            && super::super::board_handles::hit_test_chrome(
                point,
                &chrome,
                Self::node_allows_rotation(node),
            )
            .is_some()
    }
}

/// Existing selected-connector endpoint edit radius, independent of host ports.
/// Inner press radius in screen px. At zoom ≥ 1 this is [`GRIP_HIT_PX`].
/// Zooming out grows it as `8 * zoom^-0.35`, capped at 14. Pointer hit
/// area, not painted geometry.
pub fn grip_hit_radius(zoom: f32) -> f32 {
    const CAP: f32 = 14.0;
    let zoom = zoom.max(1.0e-3);
    if zoom >= 1.0 {
        GRIP_HIT_PX
    } else {
        (GRIP_HIT_PX * zoom.powf(-0.35)).min(CAP)
    }
}

/// Leave the body and its transform grips usable when port hit disks overlap.
pub(crate) fn wire_ports_fit(rect: WorldRect, zoom: f32) -> bool {
    Domain::Wire
        .reach(rect.w.abs().min(rect.h.abs()) * zoom, GRIP_HIT_PX)
        .is_some()
}
