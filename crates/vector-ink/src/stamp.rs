//! Radial brush stamp. A soft tip is a distance field around the centerline,
//! not an offset of that curve: offset joins cusp, and stacked fringe bands
//! paint brighter than one dab.
//!
//! This is Photoshop's default brush: blend mode Normal, flow 100%. Coverage
//! is the tip (opaque out to `(1 - softness)` of the radius, then a smooth
//! fade to the rim). Inside one stamp, overlaps keep the maximum, so a stroke
//! never exceeds its own opacity where it crosses itself or turns a corner.
//! The board and the HTML artifact then draw the finished bitmap over earlier
//! ink with source-over, so a second stroke covers the first.

/// Tip description in the same units as the polyline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StampStyle {
    /// Outer diameter of the tip.
    pub diameter: f32,
    /// 0 keeps a hard edge. 1 fades from the center to the rim.
    pub softness: f32,
    /// Straight RGBA. Alpha is the paint opacity.
    pub rgba: [u8; 4],
}

/// One polyline vertex and the tip painted there. Segments lerp between
/// their two vertex tips.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TipPoint {
    pub pos: [f32; 2],
    pub tip: StampStyle,
}

/// Raster of one stroke. `origin` is the top-left corner of pixel (0, 0)
/// in the polyline's coordinate space. `pixel` is the length of one pixel
/// in that space.
#[derive(Clone, Debug)]
pub struct StampImage {
    pub width: u32,
    pub height: u32,
    pub origin: [f32; 2],
    pub pixel: f32,
    pub rgba: Vec<u8>,
}

const MAX_SIDE: f32 = 4096.0;
const MAX_PIXELS: f32 = 4_000_000.0;

/// Coverage of a round tip at `dist` from its center. The single falloff
/// every brush surface uses: the cursor tip, the live preview, the
/// committed stamp, and the HTML export.
pub fn tip_coverage(dist: f32, radius: f32, softness: f32) -> f32 {
    if radius <= 0.0 || radius.is_nan() || dist >= radius {
        return 0.0;
    }
    let softness = softness.clamp(0.0, 1.0);
    if softness <= 0.001 {
        // One pixel of edge antialiasing, in the caller's units.
        return (radius - dist).clamp(0.0, 1.0);
    }
    let core = radius * (1.0 - softness);
    if dist <= core {
        return 1.0;
    }
    let t = ((dist - core) / (radius - core).max(1.0e-4)).clamp(0.0, 1.0);
    1.0 - t * t * (3.0 - 2.0 * t)
}

/// Pick the pixel size for a stamp: `pixel` (world units per pixel) at best,
/// coarsened until the bitmap fits the size limits.
pub fn stamp_pixel(bounds_w: f32, bounds_h: f32, pixel: f32) -> f32 {
    let mut pixel = pixel.max(1.0e-3);
    pixel = pixel
        .max(bounds_w / MAX_SIDE)
        .max(bounds_h / MAX_SIDE)
        .max(((bounds_w * bounds_h) / MAX_PIXELS).sqrt());
    pixel
}

/// Stamp every contour of tipped vertices at `pixel` world units per pixel
/// (coarsened if the bitmap would be too large). A one-point contour is a
/// single dab.
pub fn stamp_tipped(contours: &[Vec<TipPoint>], pixel: f32) -> Option<StampImage> {
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    let mut reach = 0.0_f32;
    for p in contours.iter().flatten() {
        if !p.pos[0].is_finite() || !p.pos[1].is_finite() {
            continue;
        }
        let r = p.tip.diameter.max(0.0) * 0.5;
        reach = reach.max(r);
        min_x = min_x.min(p.pos[0] - r);
        min_y = min_y.min(p.pos[1] - r);
        max_x = max_x.max(p.pos[0] + r);
        max_y = max_y.max(p.pos[1] + r);
    }
    if reach <= 0.0 || reach.is_nan() || !min_x.is_finite() {
        return None;
    }
    let world_w = max_x - min_x;
    let world_h = max_y - min_y;
    let pixel = stamp_pixel(world_w + 2.0 * pixel, world_h + 2.0 * pixel, pixel);
    let x0 = min_x - pixel;
    let y0 = min_y - pixel;
    let w = ((world_w / pixel).ceil() as u32 + 2).max(1);
    let h = ((world_h / pixel).ceil() as u32 + 2).max(1);
    if (w as u64) * (h as u64) > (MAX_PIXELS as u64) * 2 {
        return None;
    }
    let mut img = StampImage {
        width: w,
        height: h,
        origin: [x0, y0],
        pixel,
        rgba: vec![0u8; (w as usize) * (h as usize) * 4],
    };
    for contour in contours {
        let pts: Vec<TipPoint> = contour
            .iter()
            .copied()
            .filter(|p| p.pos[0].is_finite() && p.pos[1].is_finite())
            .collect();
        stamp_polyline(&mut img, &pts);
    }
    Some(img)
}

/// Stamp one segment into `img`, keeping the maximum coverage per pixel.
/// Rows only visit the span the capsule can reach, so a long diagonal costs
/// its own area and not its bounding box.
pub fn stamp_segment(img: &mut StampImage, a: TipPoint, b: TipPoint) {
    let px = img.pixel;
    let to_px = |p: [f32; 2]| [(p[0] - img.origin[0]) / px, (p[1] - img.origin[1]) / px];
    let pa = to_px(a.pos);
    let pb = to_px(b.pos);
    let ra = a.tip.diameter.max(0.0) * 0.5 / px;
    let rb = b.tip.diameter.max(0.0) * 0.5 / px;
    let reach = ra.max(rb);
    if reach <= 0.0 || reach.is_nan() {
        return;
    }
    let abx = pb[0] - pa[0];
    let aby = pb[1] - pa[1];
    let len2 = abx * abx + aby * aby;
    let w = img.width as i64;
    let h = img.height as i64;
    let y_lo = ((pa[1].min(pb[1]) - reach).floor() as i64).max(0);
    let y_hi = ((pa[1].max(pb[1]) + reach).ceil() as i64).min(h - 1);
    let same = a.tip == b.tip;
    for py in y_lo..=y_hi {
        let qy = py as f32 + 0.5;
        // X extent of the capsule on this row: the segment clipped to
        // |y - qy| <= reach, widened by reach.
        let (x_min, x_max) = if aby.abs() <= 1.0e-6 {
            (pa[0].min(pb[0]), pa[0].max(pb[0]))
        } else {
            let t0 = ((qy - reach - pa[1]) / aby).clamp(0.0, 1.0);
            let t1 = ((qy + reach - pa[1]) / aby).clamp(0.0, 1.0);
            let xa = pa[0] + abx * t0;
            let xb = pa[0] + abx * t1;
            (xa.min(xb), xa.max(xb))
        };
        let x_lo = ((x_min - reach).floor() as i64).max(0);
        let x_hi = ((x_max + reach).ceil() as i64).min(w - 1);
        for pxl in x_lo..=x_hi {
            let qx = pxl as f32 + 0.5;
            let t = if len2 <= f32::EPSILON {
                0.0
            } else {
                (((qx - pa[0]) * abx + (qy - pa[1]) * aby) / len2).clamp(0.0, 1.0)
            };
            let cx = pa[0] + abx * t;
            let cy = pa[1] + aby * t;
            let dist = (qx - cx).hypot(qy - cy);
            let r = ra + (rb - ra) * t;
            if dist >= r {
                continue;
            }
            let (soft, color) = if same {
                (a.tip.softness, a.tip.rgba)
            } else {
                (
                    a.tip.softness + (b.tip.softness - a.tip.softness) * t,
                    lerp_rgba(a.tip.rgba, b.tip.rgba, t),
                )
            };
            let cover = tip_coverage(dist, r, soft);
            write_max(
                &mut img.rgba,
                img.width,
                pxl as u32,
                py as u32,
                color,
                cover,
            );
        }
    }
}

/// Subtract eraser passes from a finished stamp, oldest first. Each pass is
/// stamped into a mask with the tip falloff and max compositing (tip alpha =
/// strength), then ink alpha scales by `1 - mask`. So one pass never erases
/// more than its strength however often it crosses itself, while separate
/// passes compound like separate strokes.
pub fn apply_erase(img: &mut StampImage, marks: &[Vec<TipPoint>]) {
    let mut mask = StampImage {
        width: img.width,
        height: img.height,
        origin: img.origin,
        pixel: img.pixel,
        rgba: Vec::new(),
    };
    for mark in marks {
        let Some(region) = polyline_box(img, mark) else {
            continue;
        };
        if mask.rgba.is_empty() {
            mask.rgba = vec![0u8; img.rgba.len()];
        }
        stamp_polyline(&mut mask, mark);
        multiply_by_mask(&mut img.rgba, &mask.rgba, img.width, Some(region));
        let stride = img.width as usize * 4;
        let [x0, y0, x1, y1] = region;
        for y in y0 as usize..y1 as usize {
            mask.rgba[y * stride + x0 as usize * 4..y * stride + x1 as usize * 4].fill(0);
        }
    }
}

/// Pixel box `[x0, y0, x1, y1)` a tipped polyline can touch in `img`.
fn polyline_box(img: &StampImage, pts: &[TipPoint]) -> Option<[u32; 4]> {
    let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for p in pts {
        let r = p.tip.diameter * 0.5 / img.pixel + 2.0;
        let x = (p.pos[0] - img.origin[0]) / img.pixel;
        let y = (p.pos[1] - img.origin[1]) / img.pixel;
        b = [
            b[0].min(x - r),
            b[1].min(y - r),
            b[2].max(x + r),
            b[3].max(y + r),
        ];
    }
    let x0 = b[0].floor().max(0.0);
    let y0 = b[1].floor().max(0.0);
    let x1 = b[2].ceil().min(img.width as f32);
    let y1 = b[3].ceil().min(img.height as f32);
    (x1 > x0 && y1 > y0).then_some([x0 as u32, y0 as u32, x1 as u32, y1 as u32])
}

/// Stamp one tipped polyline (a single point is a dab) into `img`.
pub fn stamp_polyline(img: &mut StampImage, pts: &[TipPoint]) {
    match pts {
        [] => {}
        [only] => stamp_segment(img, *only, *only),
        _ => {
            for s in pts.windows(2) {
                stamp_segment(img, s[0], s[1]);
            }
        }
    }
}

/// `ink.a *= 1 - mask.a` over `region` (the whole image when `None`).
pub fn multiply_by_mask(ink: &mut [u8], mask: &[u8], width: u32, region: Option<[u32; 4]>) {
    let height = (ink.len() / 4) as u32 / width.max(1);
    let [x0, y0, x1, y1] = region.unwrap_or([0, 0, width, height]);
    for y in y0..y1.min(height) {
        for x in x0..x1.min(width) {
            let i = ((y * width + x) * 4 + 3) as usize;
            let m = mask[i];
            if m == 0 {
                continue;
            }
            ink[i] = ((ink[i] as u32 * (255 - m as u32) + 127) / 255) as u8;
        }
    }
}

/// Erase strength at `p` from `marks` (0 = untouched, 1 = fully erased).
/// Separate passes compound, matching [`apply_erase`].
pub fn erase_coverage_at(p: [f32; 2], marks: &[Vec<TipPoint>]) -> f32 {
    let mut remain = 1.0_f32;
    for mark in marks {
        let mut best = 0.0_f32;
        let segs: Vec<(TipPoint, TipPoint)> = match mark.as_slice() {
            [] => continue,
            [only] => vec![(*only, *only)],
            pts => pts.windows(2).map(|s| (s[0], s[1])).collect(),
        };
        for (a, b) in segs {
            let abx = b.pos[0] - a.pos[0];
            let aby = b.pos[1] - a.pos[1];
            let len2 = abx * abx + aby * aby;
            let t = if len2 <= f32::EPSILON {
                0.0
            } else {
                (((p[0] - a.pos[0]) * abx + (p[1] - a.pos[1]) * aby) / len2).clamp(0.0, 1.0)
            };
            let dist = (p[0] - a.pos[0] - abx * t).hypot(p[1] - a.pos[1] - aby * t);
            let tip = lerp_tip(a.tip, b.tip, t);
            let cover =
                tip_coverage(dist, tip.diameter * 0.5, tip.softness) * tip.rgba[3] as f32 / 255.0;
            best = best.max(cover);
        }
        remain *= 1.0 - best;
    }
    1.0 - remain
}

/// Constant tip along every contour.
pub fn stamp_contours(contours: &[Vec<[f32; 2]>], style: StampStyle) -> Option<StampImage> {
    stamp_contours_at(contours, style, default_pixel(style.diameter))
}

/// [`stamp_contours`] at an explicit resolution.
pub fn stamp_contours_at(
    contours: &[Vec<[f32; 2]>],
    style: StampStyle,
    pixel: f32,
) -> Option<StampImage> {
    if style.rgba[3] == 0 {
        return None;
    }
    let tipped: Vec<Vec<TipPoint>> = contours
        .iter()
        .map(|c| c.iter().map(|&pos| TipPoint { pos, tip: style }).collect())
        .collect();
    stamp_tipped(&tipped, pixel)
}

/// Straight stroke whose tip lerps from `start` to `end`.
pub fn stamp_line(
    a: [f32; 2],
    b: [f32; 2],
    start: StampStyle,
    end: StampStyle,
    pixel: f32,
) -> Option<StampImage> {
    stamp_tipped(
        &[vec![
            TipPoint { pos: a, tip: start },
            TipPoint { pos: b, tip: end },
        ]],
        pixel,
    )
}

/// One world unit per pixel, coarser only for very large tips.
pub fn default_pixel(diameter: f32) -> f32 {
    (diameter * 0.5 / 192.0).max(1.0)
}

/// Flatten `path` to tipped contours. `tips` holds one tip per vertex in
/// path order (the move-to, then the end of every segment); segments lerp
/// between their end tips by arc length. Missing tips fall back to `base`.
pub fn tipped_contours(
    path: &kurbo::BezPath,
    tips: &[StampStyle],
    base: StampStyle,
    tolerance: f64,
) -> Vec<Vec<TipPoint>> {
    use kurbo::{PathEl, PathSeg as KSeg};
    let tip_at = |i: usize| tips.get(i).copied().unwrap_or(base);
    let mut out: Vec<Vec<TipPoint>> = Vec::new();
    let mut vertex = 0usize;
    let mut last = kurbo::Point::ZERO;
    let mut start = kurbo::Point::ZERO;
    for el in path.elements() {
        let seg = match *el {
            PathEl::MoveTo(p) => {
                if vertex > 0 || !out.is_empty() {
                    vertex += 1;
                }
                out.push(vec![TipPoint {
                    pos: [p.x as f32, p.y as f32],
                    tip: tip_at(vertex),
                }]);
                last = p;
                start = p;
                continue;
            }
            PathEl::LineTo(p) => KSeg::Line(kurbo::Line::new(last, p)),
            PathEl::QuadTo(c, p) => KSeg::Quad(kurbo::QuadBez::new(last, c, p)),
            PathEl::CurveTo(c1, c2, p) => KSeg::Cubic(kurbo::CubicBez::new(last, c1, c2, p)),
            PathEl::ClosePath => {
                if last == start {
                    continue;
                }
                KSeg::Line(kurbo::Line::new(last, start))
            }
        };
        if out.is_empty() {
            out.push(vec![TipPoint {
                pos: [last.x as f32, last.y as f32],
                tip: tip_at(0),
            }]);
        }
        let from = tip_at(vertex);
        vertex += 1;
        let to = tip_at(vertex);
        let mut pts: Vec<kurbo::Point> = Vec::new();
        kurbo::flatten([PathEl::MoveTo(last), seg.as_path_el()], tolerance, |e| {
            if let PathEl::LineTo(p) = e {
                pts.push(p);
            }
        });
        let mut lengths = Vec::with_capacity(pts.len());
        let mut acc = 0.0_f64;
        let mut prev = last;
        for p in &pts {
            acc += prev.distance(*p);
            lengths.push(acc);
            prev = *p;
        }
        let total = acc.max(1.0e-9);
        let contour = out.last_mut().expect("contour started");
        for (p, len) in pts.iter().zip(lengths) {
            let t = (len / total) as f32;
            contour.push(TipPoint {
                pos: [p.x as f32, p.y as f32],
                tip: lerp_tip(from, to, t),
            });
        }
        last = match seg {
            KSeg::Line(l) => l.p1,
            KSeg::Quad(q) => q.p2,
            KSeg::Cubic(c) => c.p3,
        };
    }
    out
}

fn lerp_tip(a: StampStyle, b: StampStyle, t: f32) -> StampStyle {
    if a == b {
        return a;
    }
    StampStyle {
        diameter: a.diameter + (b.diameter - a.diameter) * t,
        softness: a.softness + (b.softness - a.softness) * t,
        rgba: lerp_rgba(a.rgba, b.rgba, t),
    }
}

fn write_max(rgba: &mut [u8], w: u32, px: u32, py: u32, color: [u8; 4], cover: f32) {
    if cover <= 0.0 {
        return;
    }
    let i = ((py as usize) * (w as usize) + px as usize) * 4;
    let na = (cover * color[3] as f32).round().clamp(0.0, 255.0) as u8;
    if na > rgba[i + 3] {
        rgba[i] = color[0];
        rgba[i + 1] = color[1];
        rgba[i + 2] = color[2];
        rgba[i + 3] = na;
    }
}

fn lerp_rgba(a: [u8; 4], b: [u8; 4], t: f32) -> [u8; 4] {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    [
        mix(a[0], b[0]),
        mix(a[1], b[1]),
        mix(a[2], b[2]),
        mix(a[3], b[3]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha_at(img: &StampImage, x: f32, y: f32) -> u8 {
        let px = ((x - img.origin[0]) / img.pixel - 0.5).round() as i32;
        let py = ((y - img.origin[1]) / img.pixel - 0.5).round() as i32;
        if px < 0 || py < 0 || px >= img.width as i32 || py >= img.height as i32 {
            return 0;
        }
        img.rgba[((py as u32 * img.width + px as u32) * 4 + 3) as usize]
    }

    fn max_alpha(img: &StampImage) -> u8 {
        img.rgba
            .iter()
            .skip(3)
            .step_by(4)
            .copied()
            .max()
            .unwrap_or(0)
    }

    fn dist_to_polyline(p: [f32; 2], pts: &[[f32; 2]]) -> f32 {
        pts.windows(2)
            .map(|s| {
                let (a, b) = (s[0], s[1]);
                let abx = b[0] - a[0];
                let aby = b[1] - a[1];
                let len2 = abx * abx + aby * aby;
                let t = if len2 <= f32::EPSILON {
                    0.0
                } else {
                    (((p[0] - a[0]) * abx + (p[1] - a[1]) * aby) / len2).clamp(0.0, 1.0)
                };
                (p[0] - a[0] - abx * t).hypot(p[1] - a[1] - aby * t)
            })
            .fold(f32::INFINITY, f32::min)
    }

    const FAINT: StampStyle = StampStyle {
        diameter: 20.0,
        softness: 0.0,
        rgba: [255, 0, 0, 100],
    };

    #[test]
    fn coverage_matches_the_tip_contract() {
        assert_eq!(tip_coverage(0.0, 10.0, 0.5), 1.0);
        assert_eq!(tip_coverage(5.0, 10.0, 0.5), 1.0);
        let mid = tip_coverage(7.5, 10.0, 0.5);
        assert!((mid - 0.5).abs() < 1e-3, "{mid}");
        assert_eq!(tip_coverage(10.0, 10.0, 0.5), 0.0);
        assert!(tip_coverage(1.0, 10.0, 1.0) < 1.0);
    }

    #[test]
    fn a_soft_stamp_fades_inside_the_diameter() {
        let style = StampStyle {
            diameter: 20.0,
            softness: 1.0,
            rgba: [10, 20, 30, 255],
        };
        let img = stamp_contours(&[vec![[0.0, 0.0], [40.0, 0.0]]], style).unwrap();
        let center = alpha_at(&img, 20.0, 0.0);
        let mid = alpha_at(&img, 20.0, 5.0);
        let outside = alpha_at(&img, 20.0, 11.0);
        assert!(center > 240, "center {center}");
        assert!(mid < center && mid > 60, "mid {mid} center {center}");
        assert_eq!(outside, 0);
    }

    #[test]
    fn overlaps_inside_one_stamp_never_exceed_the_opacity() {
        // A polyline that doubles back over itself and turns sharp corners.
        let pts = vec![
            [0.0, 0.0],
            [120.0, 0.0],
            [10.0, 8.0],
            [60.0, 90.0],
            [70.0, -20.0],
        ];
        let img = stamp_contours(&[pts], FAINT).unwrap();
        assert_eq!(max_alpha(&img), 100);
        let twice = stamp_contours(
            &[vec![[0.0, 0.0], [40.0, 0.0]], vec![[0.0, 0.0], [40.0, 0.0]]],
            FAINT,
        )
        .unwrap();
        assert_eq!(max_alpha(&twice), 100);
    }

    #[test]
    fn a_chain_joint_has_no_notch_and_no_double_opacity() {
        // Two segments of one chain meeting at an acute angle.
        let chain = vec![vec![
            TipPoint {
                pos: [0.0, 0.0],
                tip: FAINT,
            },
            TipPoint {
                pos: [100.0, 0.0],
                tip: FAINT,
            },
            TipPoint {
                pos: [10.0, 30.0],
                tip: FAINT,
            },
        ]];
        let img = stamp_tipped(&chain, 1.0).unwrap();
        assert_eq!(max_alpha(&img), 100);
        // Every pixel well inside the ink is the full opacity: no ring or
        // notch around the joint.
        for (x, y) in [
            (100.0, 0.0),
            (97.0, 1.0),
            (94.0, 2.0),
            (99.0, -6.0),
            (90.0, 0.0),
        ] {
            assert_eq!(alpha_at(&img, x, y), 100, "at {x},{y}");
        }
    }

    #[test]
    fn a_corner_stays_inside_the_tip_radius() {
        let pts = vec![[0.0, 0.0], [0.0, 40.0], [40.0, 40.0]];
        let radius = 5.0;
        let img = stamp_contours(
            std::slice::from_ref(&pts),
            StampStyle {
                diameter: radius * 2.0,
                softness: 0.85,
                rgba: [255, 255, 255, 220],
            },
        )
        .unwrap();
        for y in 0..img.height {
            for x in 0..img.width {
                let a = img.rgba[((y * img.width + x) * 4 + 3) as usize];
                if a == 0 {
                    continue;
                }
                let wx = img.origin[0] + (x as f32 + 0.5) * img.pixel;
                let wy = img.origin[1] + (y as f32 + 0.5) * img.pixel;
                let d = dist_to_polyline([wx, wy], &pts);
                assert!(
                    d <= radius + img.pixel,
                    "pixel {wx},{wy} is {d} from the centerline"
                );
            }
        }
    }

    #[test]
    fn the_row_span_covers_a_steep_diagonal_completely() {
        // Brute force against the per-row span: every pixel within the
        // capsule must be painted.
        let a = [3.0, 2.0];
        let b = [41.0, 97.0];
        let style = StampStyle {
            diameter: 18.0,
            softness: 0.0,
            rgba: [0, 0, 0, 255],
        };
        let img = stamp_line(a, b, style, style, 1.0).unwrap();
        for y in 0..img.height {
            for x in 0..img.width {
                let wx = img.origin[0] + x as f32 + 0.5;
                let wy = img.origin[1] + y as f32 + 0.5;
                let d = dist_to_polyline([wx, wy], &[a, b]);
                if d < 8.0 {
                    let got = img.rgba[((y * img.width + x) * 4 + 3) as usize];
                    assert_eq!(got, 255, "hole at {wx},{wy}");
                }
            }
        }
    }

    #[test]
    fn a_tween_widens_from_start_to_end() {
        let thin = StampStyle {
            diameter: 4.0,
            softness: 0.0,
            rgba: [0, 0, 0, 255],
        };
        let thick = StampStyle {
            diameter: 40.0,
            ..thin
        };
        let img = stamp_line([0.0, 0.0], [200.0, 0.0], thin, thick, 1.0).unwrap();
        assert_eq!(alpha_at(&img, 10.0, 6.0), 0);
        assert_eq!(alpha_at(&img, 190.0, 15.0), 255);
    }

    #[test]
    fn tipped_contours_follow_curves() {
        let mut bez = kurbo::BezPath::new();
        bez.move_to((0.0, 0.0));
        bez.curve_to((30.0, 60.0), (70.0, -60.0), (100.0, 0.0));
        bez.quad_to((150.0, 50.0), (200.0, 0.0));
        let tip = StampStyle {
            diameter: 6.0,
            softness: 0.0,
            rgba: [0, 0, 0, 255],
        };
        let pts = tipped_contours(&bez, &[], tip, 0.25);
        let chain = &pts[0];
        assert!(chain.len() > 10, "flattened only {} points", chain.len());
        assert_eq!(chain.last().unwrap().pos, [200.0, 0.0]);
        let img = stamp_tipped(&pts, 1.0).unwrap();
        // Mid-curve ink, not just the start dab.
        use kurbo::ParamCurve;
        let mid = bez.segments().next().unwrap().eval(0.5);
        assert_eq!(alpha_at(&img, mid.x as f32, mid.y as f32), 255);
        let quad_mid = bez.segments().nth(1).unwrap().eval(0.5);
        assert_eq!(alpha_at(&img, quad_mid.x as f32, quad_mid.y as f32), 255);
    }

    #[test]
    fn an_erase_pass_cuts_a_hole_and_never_exceeds_its_strength() {
        let ink = StampStyle {
            diameter: 30.0,
            softness: 0.0,
            rgba: [200, 100, 50, 255],
        };
        let mut img = stamp_contours(&[vec![[0.0, 0.0], [200.0, 0.0]]], ink).unwrap();
        let full = StampStyle {
            diameter: 20.0,
            softness: 0.0,
            rgba: [0, 0, 0, 255],
        };
        let half = StampStyle {
            rgba: [0, 0, 0, 128],
            ..full
        };
        let at = |x: f32, y: f32, tip| TipPoint { pos: [x, y], tip };
        // A full-strength dab, and a half-strength pass that crosses itself.
        let marks = vec![
            vec![at(40.0, 0.0, full)],
            vec![
                at(120.0, -10.0, half),
                at(160.0, 10.0, half),
                at(120.0, 10.0, half),
                at(160.0, -10.0, half),
            ],
        ];
        apply_erase(&mut img, &marks);
        assert_eq!(alpha_at(&img, 40.0, 0.0), 0, "dab erases fully");
        assert_eq!(alpha_at(&img, 80.0, 0.0), 255, "untouched ink stays");
        let crossed = alpha_at(&img, 140.0, 0.0);
        assert!(
            (120..=135).contains(&crossed),
            "self-crossing pass stacked: {crossed}"
        );
        assert!(erase_coverage_at([40.0, 0.0], &marks) > 0.99);
        assert_eq!(erase_coverage_at([80.0, 0.0], &marks), 0.0);
        // A second, separate half pass compounds: about a quarter remains.
        apply_erase(&mut img, &[vec![at(140.0, 0.0, half)]]);
        let twice = alpha_at(&img, 140.0, 0.0);
        assert!((58..=70).contains(&twice), "two passes left {twice}");
    }

    #[test]
    fn tipped_contours_lerp_per_vertex() {
        let mut bez = kurbo::BezPath::new();
        bez.move_to((0.0, 0.0));
        bez.line_to((100.0, 0.0));
        bez.line_to((100.0, 100.0));
        let a = StampStyle {
            diameter: 2.0,
            softness: 0.0,
            rgba: [0, 0, 0, 255],
        };
        let b = StampStyle {
            diameter: 10.0,
            ..a
        };
        let c = StampStyle {
            diameter: 30.0,
            ..a
        };
        let pts = tipped_contours(&bez, &[a, b, c], a, 0.25);
        assert_eq!(pts.len(), 1);
        let chain = &pts[0];
        assert_eq!(chain.first().unwrap().tip.diameter, 2.0);
        let corner = chain
            .iter()
            .find(|p| p.pos == [100.0, 0.0])
            .expect("corner vertex");
        assert!((corner.tip.diameter - 10.0).abs() < 1e-3);
        assert!((chain.last().unwrap().tip.diameter - 30.0).abs() < 1e-3);
    }
}
