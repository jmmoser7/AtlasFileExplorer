//! Shared wire-port pointer reach and overlap eligibility.
use super::*;

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

/// Outward press radius: six times [`grip_hit_radius`], so the half-plane
/// stays the same proportion as the zoom ≥ 1 pair.
pub fn grip_hit_out_radius(zoom: f32) -> f32 {
    grip_hit_radius(zoom) * (GRIP_HIT_OUT_PX / GRIP_HIT_PX)
}

/// A painted port disc at `designed_radius` world units is still drawn.
pub(crate) fn port_disc_visible(designed_radius: f32, zoom: f32) -> bool {
    atlas_shell::canvas_scale::px(designed_radius, zoom) >= PORT_LOD_PX
}

/// Leave the body and its transform grips usable when port hit disks overlap.
pub(crate) fn wire_ports_fit(rect: WorldRect, zoom: f32) -> bool {
    const MIN_HIT_DIAMETERS: f32 = 2.0;
    rect.w.abs().min(rect.h.abs()) * zoom > MIN_HIT_DIAMETERS * 2.0 * grip_hit_radius(zoom)
}

/// Grip under the pointer. The inner disk is `hit_px` on every side.
/// Past that, the pointer must sit in the outward half-plane and within
/// `hit_out_px` — the enlargement does not reach into the node.
pub(crate) fn grip_hit(
    screen: Pos2,
    grip: Pos2,
    outward: Vec2,
    hit_px: f32,
    hit_out_px: f32,
) -> bool {
    let delta = screen - grip;
    let dist = delta.length();
    if dist <= hit_px {
        return true;
    }
    if dist > hit_out_px {
        return false;
    }
    let len = outward.length();
    if len <= 1e-4 {
        return false;
    }
    delta.dot(outward / len) > 0.0
}
