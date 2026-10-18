//! Owner-produced poses for browser design previews. Motion remains a preview;
//! the shipping album's input, projection, and physics stay in `home`.

use super::{rack_angle, rack_x, rack_z, CoverFlowTuning};
use serde::Serialize;

/// CSS uses positive Z toward the viewer, opposite the native rack's depth.
/// Apply perspective, translate3d, then rotateY to a centered card, in that order.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CssPose {
    pub offset: f32,
    pub x: f32,
    pub yaw_deg: f32,
    pub z: f32,
    pub focal: f32,
}

pub fn css_pose(cover: f32, offset: f32) -> CssPose {
    let tuning = CoverFlowTuning::from_tokens(&crate::tokens::current().home, cover);
    CssPose {
        offset,
        x: rack_x(offset, &tuning),
        yaw_deg: -rack_angle(offset, &tuning).to_degrees(),
        z: -rack_z(offset, &tuning),
        focal: tuning.focal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exported_css_pose_matches_native_projection() {
        let cover = 78.0;
        let tuning = CoverFlowTuning::from_tokens(&crate::tokens::current().home, cover);
        for offset in [-4.0, -1.5, -0.05, 0.0, 0.05, 1.5, 4.0] {
            let pose = css_pose(cover, offset);
            let (sin, cos) = pose.yaw_deg.to_radians().sin_cos();
            for (x, y) in [(-39.0, -39.0), (39.0, -39.0), (39.0, 39.0), (-39.0, 39.0)] {
                let depth = pose.focal - (pose.z - x * sin);
                let css = super::super::Pos2::new(
                    pose.focal * (pose.x + x * cos) / depth,
                    pose.focal * y / depth,
                );
                let native =
                    super::super::project_point(super::super::Pos2::ZERO, x, y, offset, &tuning);
                assert!(
                    css.distance(native) < 0.0001,
                    "offset {offset}: {css:?} / {native:?}"
                );
            }
        }
    }
}
