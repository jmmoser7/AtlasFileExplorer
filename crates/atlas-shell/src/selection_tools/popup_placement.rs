use super::*;

/// Default editor placement: centered above the strip, in the host's camera.
pub fn editor_rect(strip: Rect, height: f32, zoom: f32) -> Rect {
    popup_above(Vec2::new(EDITOR_WIDTH, height) * zoom, strip, 8.0 * zoom)
}

/// The active editor's rect: [`editor_rect`] unless [`place_popup`] finds
/// that the viewport clips it.
pub fn editor_placement(
    strip: Rect,
    selection: Rect,
    height: f32,
    zoom: f32,
    viewport: Rect,
) -> Rect {
    place_popup(
        Vec2::new(EDITOR_WIDTH, height) * zoom,
        strip,
        selection,
        8.0 * zoom,
        viewport,
    )
}

/// Screen margin a popup keeps from the viewport edge when it is placed.
pub const POPUP_MARGIN: f32 = 8.0;

/// Fit an anchored popup to its viewport without changing its natural size.
/// Oversized dimensions keep their leading edge visible for scrolling.
pub fn fit_popup(size: Vec2, at: Pos2, viewport: Rect) -> Rect {
    Rect::from_min_size(
        Pos2::new(
            at.x.clamp(
                viewport.left(),
                (viewport.right() - size.x).max(viewport.left()),
            ),
            at.y.clamp(
                viewport.top(),
                (viewport.bottom() - size.y).max(viewport.top()),
            ),
        ),
        size,
    )
}

fn popup_above(size: Vec2, anchor: Rect, gap: f32) -> Rect {
    Rect::from_min_size(
        Pos2::new(
            anchor.center().x - size.x * 0.5,
            anchor.top() - (size.y + gap),
        ),
        size,
    )
}

/// Where a popup attached to `anchor` (the strip or its button) opens over
/// `host` (the edited selection). Size never changes; only the side does.
///
/// Bottom strips prefer below the anchor; other anchors prefer above it.
/// Remaining candidates are below, right of, and
/// left of `host`; below, right of, and left of `anchor`. The first that
/// fits `viewport` (less [`POPUP_MARGIN`]) clear of both `anchor` and `host`
/// wins. Otherwise the first that fits and leaves `anchor` clear. Otherwise
/// the default.
pub fn place_popup(size: Vec2, anchor: Rect, host: Rect, gap: f32, viewport: Rect) -> Rect {
    let seen = host.intersect(viewport);
    let mid = if seen.is_positive() {
        seen.center()
    } else {
        host.center()
    };
    let at = |x: f32, y: f32| Rect::from_min_size(Pos2::new(x, y), size);
    let below_x = anchor.center().x - size.x * 0.5;
    let beside_y = mid.y - size.y * 0.5;
    let preferred = if anchor.center().y >= host.bottom() {
        at(below_x, anchor.bottom() + gap)
    } else {
        popup_above(size, anchor, gap)
    };
    let candidates = [
        preferred,
        at(mid.x - size.x * 0.5, host.bottom() + gap),
        at(host.right() + gap, beside_y),
        at(host.left() - gap - size.x, beside_y),
        at(below_x, anchor.bottom() + gap),
        at(anchor.right() + gap, anchor.top()),
        at(anchor.left() - gap - size.x, anchor.top()),
    ];
    let room = viewport.shrink(POPUP_MARGIN);
    let open = |r: &&Rect| room.contains_rect(**r) && !r.intersects(anchor);
    candidates
        .iter()
        .filter(open)
        .find(|r| !r.intersects(host))
        .or_else(|| {
            (anchor.center().y < host.bottom())
                .then(|| candidates.iter().find(open))
                .flatten()
        })
        .copied()
        .unwrap_or(candidates[0])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_bottom_strip_opens_below_its_selection_without_covering_it() {
        let host = Rect::from_min_size(Pos2::new(100.0, 100.0), Vec2::new(500.0, 200.0));
        let strip = Rect::from_min_size(Pos2::new(180.0, 320.0), Vec2::new(340.0, 30.0));
        let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(1000.0, 900.0));
        let popup = place_popup(Vec2::new(420.0, 174.0), strip, host, 8.0, viewport);
        assert_eq!(popup.top(), strip.bottom() + 8.0);
        assert!(!popup.intersects(host));
        let tiny = Rect::from_min_size(Pos2::ZERO, Vec2::new(620.0, 370.0));
        let overflow = place_popup(Vec2::new(420.0, 174.0), strip, host, 8.0, tiny);
        assert!(
            !overflow.intersects(host),
            "clipping is preferable to concealing the edited object"
        );
    }
}
