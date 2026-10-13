//! Shared selection-strip placement in the host coordinate system.
use super::*;
pub fn strip_size(count: usize) -> Vec2 {
    let n = count.max(1) as f32;
    Vec2::new(
        n * BUTTON_SIZE + count.saturating_sub(1) as f32 * BUTTON_GAP,
        BUTTON_SIZE,
    )
}

/// Icon strip above the host. `fade` 0 = collapsed, 1 = full size.
pub fn strip_rect(object_top_center: Pos2, count: usize, zoom: f32, fade: f32) -> Rect {
    let rest = strip_size(count) * zoom;
    let center = Pos2::new(
        object_top_center.x,
        object_top_center.y - (OBJECT_GAP + BUTTON_SIZE * 0.5) * zoom,
    );
    let t = STRIP_COLLAPSE + (1.0 - STRIP_COLLAPSE) * fade.clamp(0.0, 1.0);
    Rect::from_center_size(center, rest * t)
}

pub fn strip_button_rect(strip: Rect, index: usize, zoom: f32) -> Rect {
    let t = strip.height() / (BUTTON_SIZE * zoom).max(f32::EPSILON);
    let size = BUTTON_SIZE * zoom * t;
    let gap = BUTTON_GAP * zoom * t;
    Rect::from_min_size(
        strip.min + Vec2::new(index as f32 * (size + gap), 0.0),
        Vec2::splat(size),
    )
}

/// The same strip placed below the host's lower silhouette.
pub fn strip_rect_below(bottom: Pos2, count: usize, zoom: f32, fade: f32) -> Rect {
    strip_rect(bottom, count, zoom, fade)
        .translate(Vec2::new(0.0, (2.0 * OBJECT_GAP + BUTTON_SIZE) * zoom))
}

/// Lower silhouette across the strip's whole width, including concave notches.
pub fn strip_bottom(bounds: Rect, outlines: &[Vec<[f32; 2]>], width: f32) -> Pos2 {
    let x = bounds.center().x;
    let (left, right) = (x - width * 0.5, x + width * 0.5);
    let mut bottom = f32::NEG_INFINITY;
    for outline in outlines {
        for i in 0..outline.len() {
            let (a, b) = (outline[i], outline[(i + 1) % outline.len()]);
            let lo = a[0].min(b[0]).max(left);
            let hi = a[0].max(b[0]).min(right);
            if lo > hi {
                continue;
            }
            if (b[0] - a[0]).abs() < f32::EPSILON {
                bottom = bottom.max(a[1]).max(b[1]);
            } else {
                for at in [lo, hi] {
                    bottom = bottom.max(a[1] + (b[1] - a[1]) * (at - a[0]) / (b[0] - a[0]));
                }
            }
        }
    }
    Pos2::new(
        x,
        if bottom.is_finite() {
            bottom
        } else {
            bounds.bottom()
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bottom_strip_nests_in_a_notch_without_overlapping_either_shoulder() {
        let bounds = Rect::from_min_size(Pos2::ZERO, Vec2::splat(100.0));
        let outline = vec![
            [0.0, 0.0],
            [100.0, 0.0],
            [100.0, 100.0],
            [50.0, 50.0],
            [0.0, 100.0],
        ];
        let anchor = strip_bottom(bounds, &[outline], strip_size(1).x);
        assert_eq!(anchor, Pos2::new(50.0, 65.0));
        for zoom in [0.5, 1.0, 2.0] {
            let strip = strip_rect_below(anchor * zoom, 1, zoom, 1.0);
            assert!((strip.top() - (anchor.y + OBJECT_GAP) * zoom).abs() < 0.001);
        }
    }
}
