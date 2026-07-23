//! Anchor-model path editing for Direct Selection (A) and Join (Ctrl+J).
//!
//! Pure geometry on `kurbo` types — no renderer deps (Constitution Art. I).
//! The app layer converts its stored path form (e.g. `slate-doc` `PathData`)
//! to a `kurbo::BezPath`, lifts it into a `Vec<Anchor>` with
//! [`anchors_from_bezpath`], applies edits, lowers it back with
//! [`bezpath_from_anchors`], and journals the result as a patch.
//!
//! Only the first subpath of a `BezPath` is read; Slate paths are
//! single-subpath by construction.

use kurbo::{BezPath, ParamCurveNearest, PathEl, Point, Vec2};

/// Tolerance for degenerate lengths (zero-length handles, coincident points).
const EDIT_EPS: f64 = 1e-9;
/// Coincidence tolerance when merging the duplicated seam anchor of a closed
/// path whose closing segment was written explicitly before `ClosePath`.
const SEAM_EPS: f64 = 1e-6;
/// Sine-of-angle tolerance for classifying collinear handles as smooth.
const SMOOTH_SIN_EPS: f64 = 1e-3;

/// How an anchor's two direction handles relate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnchorKind {
    /// No handles, or independent handles (corner-with-handles after an
    /// Alt-drag symmetry break).
    #[default]
    Corner,
    /// Handles are collinear (continuous tangent); lengths may differ.
    Smooth,
}

/// Which handle of an anchor: `In` points back along the incoming segment,
/// `Out` points forward along the outgoing segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandleEnd {
    In,
    Out,
}

/// One editable point of a cubic path. Handles are absolute positions
/// (control points of the adjacent cubic segments), not offsets. A segment
/// between anchors `i` and `i+1` is straight iff `anchors[i].handle_out` and
/// `anchors[i+1].handle_in` are both `None`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Anchor {
    pub point: Point,
    /// Control point toward the previous anchor (`None` = straight side).
    pub handle_in: Option<Point>,
    /// Control point toward the next anchor (`None` = straight side).
    pub handle_out: Option<Point>,
    pub kind: AnchorKind,
}

impl Anchor {
    /// A handle-less corner anchor.
    pub fn corner(point: Point) -> Self {
        Self {
            point,
            handle_in: None,
            handle_out: None,
            kind: AnchorKind::Corner,
        }
    }
}

/// Lift the first subpath of `path` into an anchor list plus a `closed` flag.
///
/// Lines become anchors with `None` handles on that side; quads are elevated
/// to cubics (geometry-preserving degree elevation). If a closed path draws
/// its closing segment explicitly back to the start before `ClosePath`, the
/// duplicated seam anchor is merged into anchor 0. `kind` is inferred:
/// anchors with two collinear same-direction handles are `Smooth`.
///
/// Roundtrip through [`bezpath_from_anchors`] is lossless for line+cubic
/// paths (quads come back as their elevated cubics).
pub fn anchors_from_bezpath(path: &BezPath) -> (Vec<Anchor>, bool) {
    let mut anchors: Vec<Anchor> = Vec::new();
    let mut closed = false;
    for el in path.elements() {
        match *el {
            PathEl::MoveTo(p) => {
                if !anchors.is_empty() {
                    break; // second subpath — stop
                }
                anchors.push(Anchor::corner(p));
            }
            PathEl::LineTo(p) => {
                if anchors.is_empty() {
                    break;
                }
                anchors.push(Anchor::corner(p));
            }
            PathEl::QuadTo(c, p) => {
                let Some(prev) = anchors.last_mut() else {
                    break;
                };
                // Degree elevation: cubic controls at 2/3 toward the quad ctrl.
                prev.handle_out = Some(prev.point + (c - prev.point) * (2.0 / 3.0));
                let mut next = Anchor::corner(p);
                next.handle_in = Some(p + (c - p) * (2.0 / 3.0));
                anchors.push(next);
            }
            PathEl::CurveTo(c1, c2, p) => {
                let Some(prev) = anchors.last_mut() else {
                    break;
                };
                prev.handle_out = Some(c1);
                let mut next = Anchor::corner(p);
                next.handle_in = Some(c2);
                anchors.push(next);
            }
            PathEl::ClosePath => {
                closed = true;
                break;
            }
        }
    }

    if closed && anchors.len() >= 2 {
        let first_pt = anchors[0].point;
        let last = anchors[anchors.len() - 1];
        if (last.point - first_pt).hypot() <= SEAM_EPS {
            anchors[0].handle_in = last.handle_in;
            anchors.pop();
        }
    }

    for a in &mut anchors {
        a.kind = classify_kind(a);
    }
    (anchors, closed)
}

/// Lower an anchor list back to a single-subpath `BezPath`.
///
/// A segment is emitted as `LineTo` when both facing handles are `None`,
/// otherwise as `CurveTo` (a missing handle degenerates to the anchor point).
/// When `closed`, a straight seam uses `ClosePath` alone; a curved seam emits
/// the closing `CurveTo` back to anchor 0 followed by `ClosePath`.
pub fn bezpath_from_anchors(anchors: &[Anchor], closed: bool) -> BezPath {
    let mut path = BezPath::new();
    let Some(first) = anchors.first() else {
        return path;
    };
    path.move_to(first.point);
    for w in anchors.windows(2) {
        emit_segment(&mut path, &w[0], &w[1]);
    }
    if closed && anchors.len() >= 2 {
        let last = &anchors[anchors.len() - 1];
        if last.handle_out.is_some() || first.handle_in.is_some() {
            path.curve_to(
                last.handle_out.unwrap_or(last.point),
                first.handle_in.unwrap_or(first.point),
                first.point,
            );
        }
        path.close_path();
    }
    path
}

fn emit_segment(path: &mut BezPath, a: &Anchor, b: &Anchor) {
    if a.handle_out.is_none() && b.handle_in.is_none() {
        path.line_to(b.point);
    } else {
        path.curve_to(
            a.handle_out.unwrap_or(a.point),
            b.handle_in.unwrap_or(b.point),
            b.point,
        );
    }
}

/// Translate anchor `idx` by `delta`; both handles move with the point so
/// local curvature is preserved. Out-of-range `idx` is a no-op.
pub fn move_anchor(anchors: &mut [Anchor], idx: usize, delta: Vec2) {
    let Some(a) = anchors.get_mut(idx) else {
        return;
    };
    a.point += delta;
    if let Some(h) = &mut a.handle_in {
        *h += delta;
    }
    if let Some(h) = &mut a.handle_out {
        *h += delta;
    }
}

/// Move one handle of anchor `idx` to `new_pos`.
///
/// On a `Smooth` anchor the opposite handle stays collinear — it is rotated
/// to the mirrored angle while preserving its own length — unless
/// `break_symmetry` (Alt) is set, which moves only the dragged handle and
/// converts the anchor to corner-with-handles.
pub fn move_handle(
    anchors: &mut [Anchor],
    idx: usize,
    which: HandleEnd,
    new_pos: Point,
    break_symmetry: bool,
) {
    let Some(a) = anchors.get_mut(idx) else {
        return;
    };
    match which {
        HandleEnd::In => a.handle_in = Some(new_pos),
        HandleEnd::Out => a.handle_out = Some(new_pos),
    }
    if break_symmetry {
        a.kind = AnchorKind::Corner;
        return;
    }
    if a.kind != AnchorKind::Smooth {
        return;
    }
    // Opposite handle: same axis through the anchor, opposite side, own length.
    let v = new_pos - a.point;
    let len = v.hypot();
    if len <= EDIT_EPS {
        return;
    }
    let mirror_dir = v * (-1.0 / len);
    let opposite = match which {
        HandleEnd::In => &mut a.handle_out,
        HandleEnd::Out => &mut a.handle_in,
    };
    if let Some(op) = opposite {
        let op_len = (*op - a.point).hypot();
        *op = a.point + mirror_dir * op_len;
    }
}

/// Drag segment `seg_idx` (joining anchors `seg_idx` and `seg_idx + 1`,
/// wrapping to 0 for the closing segment of a closed path) by `delta`.
///
/// Straight segment: both endpoint anchors translate. Curved segment:
/// endpoints stay put and the two inner control points reshape with their
/// **angles preserved** (Illustrator "Constrain Path Dragging on Segment
/// Reshape", default ON): `delta` is projected onto each handle's existing
/// direction and only its length changes (clamped at zero).
pub fn translate_segment(anchors: &mut [Anchor], closed: bool, seg_idx: usize, delta: Vec2) {
    let n = anchors.len();
    if n < 2 {
        return;
    }
    let seg_count = if closed { n } else { n - 1 };
    if seg_idx >= seg_count {
        return;
    }
    let ia = seg_idx;
    let ib = (seg_idx + 1) % n;
    let straight = anchors[ia].handle_out.is_none() && anchors[ib].handle_in.is_none();
    if straight {
        move_anchor(anchors, ia, delta);
        move_anchor(anchors, ib, delta);
    } else {
        reshape_handle_along_angle(&mut anchors[ia], HandleEnd::Out, delta);
        reshape_handle_along_angle(&mut anchors[ib], HandleEnd::In, delta);
    }
}

/// Project `delta` onto the handle's direction and adjust its length only.
/// Zero-length or absent handles have no angle to preserve and are left alone.
fn reshape_handle_along_angle(a: &mut Anchor, which: HandleEnd, delta: Vec2) {
    let h = match which {
        HandleEnd::In => &mut a.handle_in,
        HandleEnd::Out => &mut a.handle_out,
    };
    let Some(pos) = *h else {
        return;
    };
    let v = pos - a.point;
    let len = v.hypot();
    if len <= EDIT_EPS {
        return;
    }
    let unit = v / len;
    let new_len = (len + delta.dot(unit)).max(0.0);
    *h = Some(a.point + unit * new_len);
}

/// Toggle anchor `idx` between corner and smooth (Direct Selection
/// double-click).
///
/// Corner → Smooth: collinear handles are created along the chord tangent
/// (previous neighbor → next neighbor), each 1/3 of the distance to its
/// neighbor anchor. Open-path endpoints get only their one interior handle.
/// Smooth → Corner: both handles are removed.
pub fn toggle_anchor_kind(anchors: &mut [Anchor], closed: bool, idx: usize) {
    let n = anchors.len();
    if idx >= n {
        return;
    }
    match anchors[idx].kind {
        AnchorKind::Smooth => {
            let a = &mut anchors[idx];
            a.handle_in = None;
            a.handle_out = None;
            a.kind = AnchorKind::Corner;
        }
        AnchorKind::Corner => {
            let prev = if idx > 0 {
                Some(idx - 1)
            } else if closed && n > 1 {
                Some(n - 1)
            } else {
                None
            };
            let next = if idx + 1 < n {
                Some(idx + 1)
            } else if closed && n > 1 {
                Some(0)
            } else {
                None
            };
            let p = anchors[idx].point;
            let chord = match (prev, next) {
                (Some(i), Some(j)) => anchors[j].point - anchors[i].point,
                (Some(i), None) => p - anchors[i].point,
                (None, Some(j)) => anchors[j].point - p,
                (None, None) => return,
            };
            let chord_len = chord.hypot();
            if chord_len <= EDIT_EPS {
                return;
            }
            let dir = chord / chord_len;
            if let Some(i) = prev {
                let d = (p - anchors[i].point).hypot();
                anchors[idx].handle_in = Some(p - dir * (d / 3.0));
            }
            if let Some(j) = next {
                let d = (anchors[j].point - p).hypot();
                anchors[idx].handle_out = Some(p + dir * (d / 3.0));
            }
            anchors[idx].kind = AnchorKind::Smooth;
        }
    }
}

/// Join open endpoints (Ctrl+J). Returns the joined anchor list plus its
/// `closed` flag, or `None` when the inputs cannot be joined (fewer than two
/// anchors, or an empty second list). Anchors at the join become `Corner`
/// (Illustrator joins corner-first; smooth conversion is a follow-up edit).
///
/// - `second == None`, endpoints within `radius`: the two endpoints **merge**
///   into one anchor at their average position (interior handles follow their
///   endpoints) and the path closes.
/// - `second == None`, endpoints farther apart: the path closes with a
///   straight seam segment.
/// - `second == Some(b)`: the **nearest endpoint pair** between the two open
///   lists is bridged, producing one open list that starts with `first`'s
///   anchors (possibly reversed) — the caller keeps the first path's style.
///   A pair coincident within `radius` merges instead of bridging (spec §6
///   auto-average); the result is still open.
pub fn join_endpoints(
    first: &[Anchor],
    second: Option<&[Anchor]>,
    radius: f64,
) -> Option<(Vec<Anchor>, bool)> {
    match second {
        None => {
            if first.len() < 2 {
                return None;
            }
            let mut a = first.to_vec();
            let last_idx = a.len() - 1;
            let gap = (a[last_idx].point - a[0].point).hypot();
            if gap <= radius && a.len() >= 3 {
                let last = a.pop().expect("len >= 3");
                let merged = merge_pair(&last, &a[0]);
                a[0] = merged;
            } else {
                // Straight seam: endpoints keep their interior handles only.
                a[last_idx].handle_out = None;
                a[last_idx].kind = AnchorKind::Corner;
                a[0].handle_in = None;
                a[0].kind = AnchorKind::Corner;
            }
            Some((a, true))
        }
        Some(second) => {
            if first.is_empty() || second.is_empty() {
                return None;
            }
            let mut a = first.to_vec();
            let mut b = second.to_vec();
            // Nearest of the four endpoint pairings; orient so the seam is
            // a.last -> b.first.
            let d = |p: &[Anchor], q: &[Anchor], pi: usize, qi: usize| {
                (p[pi].point - q[qi].point).hypot()
            };
            let al = a.len() - 1;
            let bl = b.len() - 1;
            let pairs = [
                (false, false, d(&a, &b, al, 0)),
                (false, true, d(&a, &b, al, bl)),
                (true, false, d(&a, &b, 0, 0)),
                (true, true, d(&a, &b, 0, bl)),
            ];
            let &(rev_a, rev_b, gap) = pairs
                .iter()
                .min_by(|x, y| x.2.total_cmp(&y.2))
                .expect("non-empty");
            if rev_a {
                reverse_anchors(&mut a);
            }
            if rev_b {
                reverse_anchors(&mut b);
            }
            if gap <= radius && (a.len() > 1 || b.len() > 1) {
                let seam_a = a.pop().expect("non-empty");
                let seam_b = b.remove(0);
                a.push(merge_pair(&seam_a, &seam_b));
            } else {
                let al = a.len() - 1;
                a[al].handle_out = None;
                a[al].kind = AnchorKind::Corner;
                b[0].handle_in = None;
                b[0].kind = AnchorKind::Corner;
            }
            a.append(&mut b);
            Some((a, false))
        }
    }
}

/// Merge two coincident-ish endpoint anchors: average position, incoming
/// handle from `tail`, outgoing handle from `head`, each translated with its
/// endpoint so the interior curves keep their shape. Result is `Corner`.
fn merge_pair(tail: &Anchor, head: &Anchor) -> Anchor {
    let avg = tail.point.midpoint(head.point);
    Anchor {
        point: avg,
        handle_in: tail.handle_in.map(|h| h + (avg - tail.point)),
        handle_out: head.handle_out.map(|h| h + (avg - head.point)),
        kind: AnchorKind::Corner,
    }
}

/// Reverse anchor order in place, swapping each anchor's in/out handles.
fn reverse_anchors(anchors: &mut [Anchor]) {
    anchors.reverse();
    for a in anchors {
        core::mem::swap(&mut a.handle_in, &mut a.handle_out);
    }
}

/// Nearest anchor within `radius` of `pos`, if any.
pub fn anchor_hit(anchors: &[Anchor], pos: Point, radius: f64) -> Option<usize> {
    if !pos.is_finite() || !radius.is_finite() || radius < 0.0 {
        return None;
    }
    let mut best: Option<(usize, f64)> = None;
    for (i, a) in anchors.iter().enumerate() {
        let d = (a.point - pos).hypot();
        if d <= radius && best.is_none_or(|(_, bd)| d < bd) {
            best = Some((i, d));
        }
    }
    best.map(|(i, _)| i)
}

/// Nearest path segment within `radius` of `pos`, if any. Indices follow
/// `BezPath::segments()` order, which for paths built by
/// [`bezpath_from_anchors`] means segment `i` joins anchors `i` and `i + 1`
/// (the closing seam is the last index). Degenerate zero-length segments
/// (e.g. the `ClosePath` after an explicit closing curve) never match.
pub fn segment_hit(path: &BezPath, pos: Point, radius: f64) -> Option<usize> {
    if !pos.is_finite() || !radius.is_finite() || radius < 0.0 {
        return None;
    }
    let mut best: Option<(usize, f64)> = None;
    for (i, seg) in path.segments().enumerate() {
        if let kurbo::PathSeg::Line(line) = seg {
            if (line.p1 - line.p0).hypot() <= EDIT_EPS {
                continue;
            }
        }
        let d = seg.nearest(pos, 1e-6).distance_sq.sqrt();
        if d <= radius && best.is_none_or(|(_, bd)| d < bd) {
            best = Some((i, d));
        }
    }
    best.map(|(i, _)| i)
}

fn classify_kind(a: &Anchor) -> AnchorKind {
    let (Some(hi), Some(ho)) = (a.handle_in, a.handle_out) else {
        return AnchorKind::Corner;
    };
    let vin = a.point - hi;
    let vout = ho - a.point;
    let (li, lo) = (vin.hypot(), vout.hypot());
    if li <= EDIT_EPS || lo <= EDIT_EPS {
        return AnchorKind::Corner;
    }
    if vin.dot(vout) > 0.0 && vin.cross(vout).abs() <= SMOOTH_SIN_EPS * li * lo {
        AnchorKind::Smooth
    } else {
        AnchorKind::Corner
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(x: f64, y: f64) -> Point {
        Point::new(x, y)
    }

    fn assert_pt_eq(a: Point, b: Point) {
        assert!((a - b).hypot() < 1e-9, "{a:?} != {b:?}");
    }

    /// Open mixed path: straight then curved segment.
    fn mixed_path() -> BezPath {
        let mut p = BezPath::new();
        p.move_to((0.0, 0.0));
        p.line_to((10.0, 0.0));
        p.curve_to((14.0, 4.0), (16.0, 4.0), (20.0, 0.0));
        p
    }

    #[test]
    fn anchors_bezpath_roundtrip_mixed() {
        let path = mixed_path();
        let (anchors, closed) = anchors_from_bezpath(&path);
        assert!(!closed);
        assert_eq!(anchors.len(), 3);
        assert_eq!(anchors[0].handle_in, None);
        assert_eq!(anchors[0].handle_out, None);
        assert_eq!(anchors[1].handle_in, None);
        assert_pt_eq(anchors[1].handle_out.unwrap(), pt(14.0, 4.0));
        assert_pt_eq(anchors[2].handle_in.unwrap(), pt(16.0, 4.0));
        assert_eq!(anchors[2].handle_out, None);

        let back = bezpath_from_anchors(&anchors, closed);
        assert_eq!(back.elements(), path.elements());
    }

    #[test]
    fn closed_curved_seam_roundtrip() {
        let mut p = BezPath::new();
        p.move_to((0.0, 0.0));
        p.line_to((10.0, 0.0));
        p.curve_to((12.0, 5.0), (2.0, 5.0), (0.0, 0.0));
        p.close_path();
        let (anchors, closed) = anchors_from_bezpath(&p);
        assert!(closed);
        // Seam anchor merged back into anchor 0.
        assert_eq!(anchors.len(), 2);
        assert_pt_eq(anchors[0].handle_in.unwrap(), pt(2.0, 5.0));
        let back = bezpath_from_anchors(&anchors, closed);
        assert_eq!(back.elements(), p.elements());
    }

    #[test]
    fn quad_elevation_preserves_geometry() {
        let mut p = BezPath::new();
        p.move_to((0.0, 0.0));
        p.quad_to((5.0, 10.0), (10.0, 0.0));
        let (anchors, _) = anchors_from_bezpath(&p);
        // Elevated controls: p0 + 2/3(c - p0), p1 + 2/3(c - p1).
        assert_pt_eq(anchors[0].handle_out.unwrap(), pt(10.0 / 3.0, 20.0 / 3.0));
        assert_pt_eq(anchors[1].handle_in.unwrap(), pt(20.0 / 3.0, 20.0 / 3.0));
    }

    #[test]
    fn smooth_kind_inferred() {
        let mut p = BezPath::new();
        p.move_to((0.0, 0.0));
        p.curve_to((3.0, 3.0), (7.0, 3.0), (10.0, 3.0));
        p.curve_to((13.0, 3.0), (17.0, 3.0), (20.0, 0.0));
        let (anchors, _) = anchors_from_bezpath(&p);
        // Middle anchor: handles (7,3) and (13,3) collinear through (10,3).
        assert_eq!(anchors[1].kind, AnchorKind::Smooth);
        assert_eq!(anchors[0].kind, AnchorKind::Corner);
    }

    #[test]
    fn move_anchor_carries_handles() {
        let (mut anchors, _) = anchors_from_bezpath(&mixed_path());
        move_anchor(&mut anchors, 2, Vec2::new(1.0, 2.0));
        assert_pt_eq(anchors[2].point, pt(21.0, 2.0));
        assert_pt_eq(anchors[2].handle_in.unwrap(), pt(17.0, 6.0));
    }

    #[test]
    fn move_handle_smooth_keeps_opposite_collinear() {
        let mut anchors = vec![Anchor {
            point: pt(10.0, 0.0),
            handle_in: Some(pt(7.0, 0.0)),
            handle_out: Some(pt(15.0, 0.0)),
            kind: AnchorKind::Smooth,
        }];
        // Drag the out handle upward; in handle must mirror the angle but
        // keep its own length (3).
        move_handle(&mut anchors, 0, HandleEnd::Out, pt(10.0, 5.0), false);
        let hin = anchors[0].handle_in.unwrap();
        assert_pt_eq(hin, pt(10.0, -3.0));
        assert_eq!(anchors[0].kind, AnchorKind::Smooth);
    }

    #[test]
    fn move_handle_break_symmetry_makes_corner() {
        let mut anchors = vec![Anchor {
            point: pt(10.0, 0.0),
            handle_in: Some(pt(7.0, 0.0)),
            handle_out: Some(pt(15.0, 0.0)),
            kind: AnchorKind::Smooth,
        }];
        move_handle(&mut anchors, 0, HandleEnd::Out, pt(10.0, 5.0), true);
        // Opposite handle untouched; anchor demoted to corner-with-handles.
        assert_pt_eq(anchors[0].handle_in.unwrap(), pt(7.0, 0.0));
        assert_eq!(anchors[0].kind, AnchorKind::Corner);
    }

    #[test]
    fn straight_segment_translates_endpoints() {
        let (mut anchors, _) = anchors_from_bezpath(&mixed_path());
        translate_segment(&mut anchors, false, 0, Vec2::new(0.0, 3.0));
        assert_pt_eq(anchors[0].point, pt(0.0, 3.0));
        assert_pt_eq(anchors[1].point, pt(10.0, 3.0));
        // The curved segment's handle moved with its anchor.
        assert_pt_eq(anchors[1].handle_out.unwrap(), pt(14.0, 7.0));
        assert_pt_eq(anchors[2].point, pt(20.0, 0.0));
    }

    #[test]
    fn curved_segment_translate_preserves_handle_angles() {
        let (mut anchors, _) = anchors_from_bezpath(&mixed_path());
        let dir_before = |a: &Anchor, h: Point| {
            let v = h - a.point;
            v / v.hypot()
        };
        let d_out = dir_before(&anchors[1], anchors[1].handle_out.unwrap());
        let d_in = dir_before(&anchors[2], anchors[2].handle_in.unwrap());
        let delta = Vec2::new(1.0, 4.0);
        let len_out = (anchors[1].handle_out.unwrap() - anchors[1].point).hypot();
        let len_in = (anchors[2].handle_in.unwrap() - anchors[2].point).hypot();

        translate_segment(&mut anchors, false, 1, delta);

        // Endpoints stayed.
        assert_pt_eq(anchors[1].point, pt(10.0, 0.0));
        assert_pt_eq(anchors[2].point, pt(20.0, 0.0));
        // Angles preserved, lengths adjusted by the projection of delta.
        let v_out = anchors[1].handle_out.unwrap() - anchors[1].point;
        let v_in = anchors[2].handle_in.unwrap() - anchors[2].point;
        assert!(v_out.cross(d_out).abs() < 1e-9);
        assert!(v_out.dot(d_out) > 0.0);
        assert!(v_in.cross(d_in).abs() < 1e-9);
        assert!(v_in.dot(d_in) > 0.0);
        assert!((v_out.hypot() - (len_out + delta.dot(d_out))).abs() < 1e-9);
        assert!((v_in.hypot() - (len_in + delta.dot(d_in))).abs() < 1e-9);
    }

    #[test]
    fn corner_smooth_toggle_roundtrip() {
        let (mut anchors, _) = anchors_from_bezpath(&mixed_path());
        assert_eq!(anchors[1].kind, AnchorKind::Corner);

        toggle_anchor_kind(&mut anchors, false, 1);
        assert_eq!(anchors[1].kind, AnchorKind::Smooth);
        // Handles collinear along the chord (0,0)->(20,0), at 1/3 of each
        // neighbor distance (both neighbors 10 away).
        let hin = anchors[1].handle_in.unwrap();
        let hout = anchors[1].handle_out.unwrap();
        assert_pt_eq(hin, pt(10.0 - 10.0 / 3.0, 0.0));
        assert_pt_eq(hout, pt(10.0 + 10.0 / 3.0, 0.0));

        toggle_anchor_kind(&mut anchors, false, 1);
        assert_eq!(anchors[1].kind, AnchorKind::Corner);
        assert_eq!(anchors[1].handle_in, None);
        assert_eq!(anchors[1].handle_out, None);
    }

    fn open_l(offset: Vec2) -> Vec<Anchor> {
        vec![
            Anchor::corner(pt(0.0, 0.0) + offset),
            Anchor::corner(pt(10.0, 0.0) + offset),
            Anchor::corner(pt(10.0, 10.0) + offset),
        ]
    }

    #[test]
    fn join_coincident_endpoints_merge() {
        // Endpoints 1 apart, radius 2 -> merge at the average, path closes.
        let mut anchors = open_l(Vec2::ZERO);
        anchors.push(Anchor::corner(pt(0.0, 1.0)));
        let (joined, closed) = join_endpoints(&anchors, None, 2.0).unwrap();
        assert!(closed);
        assert_eq!(joined.len(), 3);
        assert_pt_eq(joined[0].point, pt(0.0, 0.5));
        assert_eq!(joined[0].kind, AnchorKind::Corner);
    }

    #[test]
    fn join_far_endpoints_close_with_straight_segment() {
        let anchors = open_l(Vec2::ZERO);
        let (joined, closed) = join_endpoints(&anchors, None, 2.0).unwrap();
        assert!(closed);
        assert_eq!(joined.len(), 3);
        assert_pt_eq(joined[0].point, pt(0.0, 0.0));
        assert_eq!(joined[0].kind, AnchorKind::Corner);
        assert_eq!(joined[2].kind, AnchorKind::Corner);
        // Lowered path closes with a plain ClosePath (straight seam).
        let path = bezpath_from_anchors(&joined, closed);
        assert_eq!(path.elements().last(), Some(&PathEl::ClosePath));
    }

    #[test]
    fn join_two_paths_bridges_nearest_pair() {
        let a = open_l(Vec2::ZERO); // ends at (10, 10)
        let b = open_l(Vec2::new(30.0, 0.0)); // nearest endpoint: its start (30, 0)...
        let (joined, closed) = join_endpoints(&a, Some(&b), 2.0).unwrap();
        assert!(!closed);
        assert_eq!(joined.len(), 6);
        // First path's anchors come first (caller keeps first style).
        assert_pt_eq(joined[0].point, pt(0.0, 0.0));
        // Nearest pair is a.last (10,10) <-> b.first (30,0)? Distances:
        // |(10,10)-(30,0)| ~ 22.36, |(10,10)-(40,10)| = 30,
        // |(0,0)-(30,0)| = 30, |(0,0)-(40,10)| ~ 41.2 -> seam at indices 2,3.
        assert_pt_eq(joined[2].point, pt(10.0, 10.0));
        assert_pt_eq(joined[3].point, pt(30.0, 0.0));
        assert_eq!(joined[2].kind, AnchorKind::Corner);
        assert_eq!(joined[3].kind, AnchorKind::Corner);
    }

    #[test]
    fn join_two_paths_reverses_to_nearest_pair() {
        // b's END (0,20) sits nearest to a's END (10,10) -> b gets reversed
        // so the bridge runs a.last -> b.first.
        let a = open_l(Vec2::ZERO); // ends at (10, 10)
        let mut b = open_l(Vec2::new(0.0, 20.0)); // (0,20), (10,20), (10,30)
        b.reverse(); // (10,30), (10,20), (0,20): its LAST is now nearest
        let (joined, _) = join_endpoints(&a, Some(&b), 2.0).unwrap();
        assert_pt_eq(joined[2].point, pt(10.0, 10.0));
        assert_pt_eq(joined[3].point, pt(0.0, 20.0));
        assert_pt_eq(joined[5].point, pt(10.0, 30.0));
    }

    #[test]
    fn anchor_hit_nearest_within_radius() {
        let anchors = open_l(Vec2::ZERO);
        assert_eq!(anchor_hit(&anchors, pt(9.0, 0.5), 3.0), Some(1));
        assert_eq!(anchor_hit(&anchors, pt(50.0, 50.0), 3.0), None);
    }

    #[test]
    fn segment_hit_indices_match_anchor_segments() {
        let path = mixed_path();
        // Near the straight segment.
        assert_eq!(segment_hit(&path, pt(5.0, 0.5), 1.0), Some(0));
        // Near the curved segment.
        assert_eq!(segment_hit(&path, pt(15.0, 3.5), 2.0), Some(1));
        assert_eq!(segment_hit(&path, pt(15.0, 30.0), 2.0), None);
    }
}
