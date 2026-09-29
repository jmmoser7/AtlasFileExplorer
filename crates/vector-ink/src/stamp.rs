//! Radial brush stamp. A soft tip is a distance field around the centerline,
//! not an offset of that curve: offset joins cusp, and stacked fringe bands
//! paint brighter than one dab.
//!
//! This is Photoshop's default brush: blend mode Normal, flow 100%. Coverage
//! is the tip (opaque out to `(1 - softness)` of the radius, then a smooth
//! fade to the rim). Inside one stamp, overlaps keep the maximum, so a stroke
//! never exceeds its own opacity where it crosses itself or turns a corner.
//! Watercolor is the one exception: where the tip comes back to a pixel more
//! than a tip diameter further along the stroke, the new visit builds over
//! the old one ([`StampSide::arc`]).
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
    /// Medium texture. Grain never raises coverage above the plain tip, so
    /// the stroke's opacity stays its ceiling.
    pub grain: Grain,
}

/// Medium texture of a stamped tip, a deterministic function of world
/// position: the same paper grain under every stroke and at every tile.
///
/// A stroke is stamped as the plain tip first. The grain then scales each
/// finished pixel once ([`finish_grain`]) by a factor in `0..=1` read from
/// the pixel's depth into the stroke and baked paper fields, so it costs one
/// pass over the stroke however many dabs overlap, and a factor that peaks
/// at the rim (a wet edge) cannot ring the joints between segments.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Grain {
    #[default]
    Smooth,
    /// Fine paper tooth lifting a mid-gray share of the body; the tooth
    /// bites deeper toward the edge.
    Graphite,
    /// Harder and grainier: paper tooth breaks the edge up and speckles the
    /// body.
    Pencil,
    /// Uniform density, crisp edge, with a slightly darker wet edge.
    Ink,
    /// Translucent wash that builds across strokes and where one stroke
    /// comes back over itself, blooms, pigment granulation, and a darker wet
    /// edge on a wandering rim.
    Watercolor,
}

impl Grain {
    const ALL: [Grain; 5] = [
        Grain::Smooth,
        Grain::Graphite,
        Grain::Pencil,
        Grain::Ink,
        Grain::Watercolor,
    ];

    fn code(self) -> u8 {
        Grain::ALL.iter().position(|g| *g == self).unwrap_or(0) as u8
    }

    fn of_code(code: u8) -> Grain {
        Grain::ALL.get(code as usize).copied().unwrap_or_default()
    }
}

/// A Watercolor tip that comes back to a pixel more than this many tip
/// diameters further along the stroke builds over what it left there; a
/// nearer dab keeps the maximum coverage. The legs of a joint with interior
/// angle `a` overlap up to `cot(a / 2)` diameters of arc apart, so 3 keeps
/// every joint of 37° or wider (Shift steps by 45°) from building.
pub const WET_REVISIT_DIAMETERS: f32 = 3.0;

/// Arc gap, in pixels, between separate contours of one stroke: always a
/// revisit.
const CONTOUR_GAP_PX: f32 = 8192.0;

/// Per-pixel records of a stroke while it is being stamped, beside
/// [`StampImage::depth`]. Every buffer stays empty until a stroke needs it,
/// so a plain Smooth stroke costs nothing more. [`finish_grain`] clears them.
#[derive(Clone, Debug, Default)]
pub struct StampSide {
    /// The texture of the tip that reached deepest into each pixel (the
    /// first one on a tie), once any textured tip has been stamped. A
    /// segment takes its end tip's texture, so a Shift segment painted after
    /// a texture change keeps the texture it was painted with.
    pub grain: Vec<u8>,
    /// Arc position, in half pixels (wrapping), of the last Watercolor dab
    /// that covered each pixel.
    pub arc: Vec<u16>,
    /// The finished earlier visits of each pixel (straight RGBA), once a
    /// Watercolor stroke has come back over itself.
    pub prev: Vec<[u8; 4]>,
    /// Arc length stamped so far, in pixels.
    pub run: f32,
    /// An erase mask: one pass never builds over itself, Watercolor or not.
    pub mask: bool,
}

impl StampSide {
    /// Side records for an erase mask.
    pub fn for_mask() -> StampSide {
        StampSide {
            mask: true,
            ..Default::default()
        }
    }

    /// Empty records that continue this stroke's arc: a clear region that
    /// the next segment of the same stroke stamps into.
    pub fn continuing(&self) -> StampSide {
        StampSide {
            run: self.run,
            mask: self.mask,
            ..Default::default()
        }
    }

    /// Box `b` of records kept for an image `width` pixels wide, reusing the
    /// buffers of `into`.
    pub fn crop_into(&self, width: u32, b: [u32; 4], mut into: StampSide) -> StampSide {
        let w = width as usize;
        let rows = b[1] as usize..b[3] as usize;
        let span = |y: usize| y * w + b[0] as usize..y * w + b[2] as usize;
        into.grain.clear();
        into.arc.clear();
        into.prev.clear();
        for y in rows {
            if !self.grain.is_empty() {
                into.grain.extend_from_slice(&self.grain[span(y)]);
            }
            if !self.arc.is_empty() {
                into.arc.extend_from_slice(&self.arc[span(y)]);
            }
            if !self.prev.is_empty() {
                into.prev.extend_from_slice(&self.prev[span(y)]);
            }
        }
        into.run = self.run;
        into.mask = self.mask;
        into
    }

    /// Write `src`, the records of box `b`, into these records of an image
    /// `width` × `height`, and continue `src`'s arc.
    pub fn paste(&mut self, width: u32, height: u32, b: [u32; 4], src: &StampSide) {
        let (w, pixels) = (width as usize, width as usize * height as usize);
        let bw = (b[2] - b[0]) as usize;
        fn put<T: Copy + Default>(
            dst: &mut Vec<T>,
            src: &[T],
            pixels: usize,
            w: usize,
            bw: usize,
            b: [u32; 4],
        ) {
            if src.is_empty() && dst.is_empty() {
                return;
            }
            if dst.len() != pixels {
                *dst = vec![T::default(); pixels];
            }
            for (i, y) in (b[1] as usize..b[3] as usize).enumerate() {
                let row = &mut dst[y * w + b[0] as usize..y * w + b[2] as usize];
                match src.get(i * bw..(i + 1) * bw) {
                    Some(s) => row.copy_from_slice(s),
                    None => row.fill(T::default()),
                }
            }
        }
        put(&mut self.grain, &src.grain, pixels, w, bw, b);
        put(&mut self.arc, &src.arc, pixels, w, bw, b);
        put(&mut self.prev, &src.prev, pixels, w, bw, b);
        self.run = src.run;
    }

    fn clear_box(&mut self, width: u32, b: [u32; 4]) {
        let w = width as usize;
        for y in b[1] as usize..b[3] as usize {
            let span = y * w + b[0] as usize..y * w + b[2] as usize;
            if !self.grain.is_empty() {
                self.grain[span.clone()].fill(0);
            }
            if !self.arc.is_empty() {
                self.arc[span.clone()].fill(0);
            }
            if !self.prev.is_empty() {
                self.prev[span].fill([0; 4]);
            }
        }
    }
}

/// Clear box `b` of a stamp still being drawn: ink, depth, and side
/// records. With `whole` the box holds everything stamped so far, so the
/// next stroke starts a fresh arc.
pub fn clear_stamp_box(img: &mut StampImage, b: [u32; 4], whole: bool) {
    let w = img.width as usize;
    for y in b[1] as usize..b[3] as usize {
        img.rgba[(y * w + b[0] as usize) * 4..(y * w + b[2] as usize) * 4].fill(0);
        if !img.depth.is_empty() {
            img.depth[y * w + b[0] as usize..y * w + b[2] as usize].fill(0);
        }
    }
    img.side.clear_box(img.width, b);
    if whole {
        img.side.run = 0.0;
    }
}

/// `src` over `dst`, both straight RGBA: the source-over every finished
/// stamp is composited with.
pub fn over_px(src: [u8; 4], dst: [u8; 4]) -> [u8; 4] {
    let sa = src[3];
    if sa == 0 {
        return dst;
    }
    if sa == 255 || dst[3] == 0 {
        return src;
    }
    let da = dst[3] as u32;
    let inv = 255 - sa as u32;
    let out_a = sa as u32 + (da * inv + 127) / 255;
    if out_a == 0 {
        return [0; 4];
    }
    let mut out = [0u8; 4];
    for c in 0..3 {
        let num = src[c] as u32 * sa as u32 + (dst[c] as u32 * da * inv + 127) / 255;
        out[c] = ((num + out_a / 2) / out_a).min(255) as u8;
    }
    out[3] = out_a.min(255) as u8;
    out
}

/// How far a finished pixel lies inside its stroke: `0` at the rim, `1` a
/// quarter radius in or deeper. Stored as a byte per pixel beside the ink.
/// A pixel at full depth and full opacity skips every later overlapping
/// segment, so a narrow band keeps a textured stroke near a plain one's cost.
const DEPTH_SPAN: f32 = 0.25;

fn depth_of(dist: f32, radius: f32) -> f32 {
    ((radius - dist) / (radius * DEPTH_SPAN).max(1.0e-6)).clamp(0.0, 1.0)
}

/// Grain factor at `depth` (see [`DEPTH_SPAN`]) and world point `(wx, wy)`.
fn grain_factor(grain: Grain, depth: f32, wx: f32, wy: f32) -> f32 {
    match grain {
        Grain::Smooth => 1.0,
        Grain::Graphite => {
            let tooth = crate::grain::fields().tooth.at(wx, wy);
            let bite = 0.3 + 0.35 * (1.0 - depth);
            1.0 - bite * (1.0 - tooth)
        }
        Grain::Pencil => {
            let tooth = crate::grain::fields().pencil.at(wx, wy);
            smoothstep(0.5, 0.78, tooth + 0.45 * depth)
        }
        Grain::Ink => {
            let rim = 1.0 - smoothstep(0.0, 0.6, depth);
            0.9 + 0.1 * rim
        }
        Grain::Watercolor => {
            let f = crate::grain::fields();
            let edge = smoothstep(0.0, 0.2, depth - 0.28 * (1.0 - f.wander.at(wx, wy)));
            let wash = (0.4 + 0.16 * f.bloom.at(wx, wy)) * (0.8 + 0.2 * f.granule.at(wx, wy));
            let rim = 1.0 - smoothstep(0.1, 0.8, depth);
            edge * (wash + (0.92 - wash) * rim)
        }
    }
}

/// One dab's coverage with the medium's grain at world point `(wx, wy)`:
/// the look a lone dab finishes with. `dist` and `radius` share units; the
/// paper is sampled in world units.
pub fn grain_coverage(
    grain: Grain,
    dist: f32,
    radius: f32,
    softness: f32,
    wx: f32,
    wy: f32,
) -> f32 {
    let cover = tip_coverage(dist, radius, softness);
    if cover <= 0.0 || grain == Grain::Smooth {
        return cover;
    }
    cover * grain_factor(grain, depth_of(dist, radius), wx, wy)
}

/// Apply a stroke's grain to its finished stamp over `region` (the whole
/// image when `None`), then clear the depth and side records there and
/// start the next stroke's arc afresh. `img` must hold that one stroke,
/// stamped since the records were last cleared. `grain` is the stroke's
/// texture for pixels without a recorded one. Pixels are judged on the
/// shared grid, so tiles and whole stamps of one stroke agree.
pub fn finish_grain(img: &mut StampImage, grain: Grain, region: Option<[u32; 4]>) {
    let (w, h) = (img.width, img.height);
    img.side.run = 0.0;
    if img.depth.len() != (w as usize) * (h as usize) {
        return;
    }
    let [x0, y0, x1, y1] = region.unwrap_or([0, 0, w, h]);
    let paper = Paper::of(img);
    let plain = img.side.grain.is_empty() && img.side.prev.is_empty();
    for y in y0..y1.min(h) {
        for x in x0..x1.min(w) {
            let p = (y * w + x) as usize;
            let depth = std::mem::take(&mut img.depth[p]);
            let a = img.rgba[p * 4 + 3];
            if plain {
                if a != 0 && grain != Grain::Smooth {
                    img.rgba[p * 4 + 3] = paper.alpha(grain, a, depth, x, y);
                }
                continue;
            }
            if a == 0 && img.side.prev.get(p).is_none_or(|u| u[3] == 0) {
                continue;
            }
            let px: [u8; 4] = img.rgba[p * 4..p * 4 + 4].try_into().expect("four bytes");
            let out = paper.finish_px(img, grain, px, p, depth, x, y);
            img.rgba[p * 4..p * 4 + 4].copy_from_slice(&out);
        }
    }
    let region = [x0, y0, x1.min(w), y1.min(h)];
    img.side.clear_box(w, region);
}

/// `region` of a stamp still being drawn, as straight RGBA rows with the
/// grain applied, leaving `img` raw so later segments keep stamping into
/// it. The live brush and eraser previews upload this.
pub fn finished_region(img: &StampImage, grain: Grain, region: [u32; 4]) -> Vec<u8> {
    let [x0, y0, x1, y1] = region;
    let (x1, y1) = (x1.min(img.width), y1.min(img.height));
    let stride = img.width as usize * 4;
    let mut out = Vec::with_capacity((x1.saturating_sub(x0) * y1.saturating_sub(y0) * 4) as usize);
    for y in y0..y1 {
        let row = y as usize * stride;
        out.extend_from_slice(&img.rgba[row + x0 as usize * 4..row + x1 as usize * 4]);
    }
    if img.depth.len() * 4 != img.rgba.len() {
        return out;
    }
    let paper = Paper::of(img);
    let mut i = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            let p = (y * img.width + x) as usize;
            let px: [u8; 4] = out[i..i + 4].try_into().expect("four bytes");
            out[i..i + 4].copy_from_slice(&paper.finished(img, grain, px, p, x, y));
            i += 4;
        }
    }
    out
}

/// Where an image's pixels sit on the world paper and the shared dither grid.
struct Paper {
    origin: [f32; 2],
    pixel: f32,
    grid: [i64; 2],
}

impl Paper {
    fn of(img: &StampImage) -> Paper {
        let px = img.pixel;
        Paper {
            origin: img.origin,
            pixel: px,
            grid: [
                (img.origin[0] / px).round() as i64,
                (img.origin[1] / px).round() as i64,
            ],
        }
    }

    fn alpha(&self, grain: Grain, a: u8, depth: u8, x: u32, y: u32) -> u8 {
        let wx = self.origin[0] + (x as f32 + 0.5) * self.pixel;
        let wy = self.origin[1] + (y as f32 + 0.5) * self.pixel;
        let f = grain_factor(grain, depth as f32 / 255.0, wx, wy);
        crate::dither::quantize(
            a as f32 * f,
            self.grid[0] + x as i64,
            self.grid[1] + y as i64,
        )
    }

    /// Raw pixel `px` (index `p`, at `x`, `y`) of `img` as it finishes: its
    /// texture's grain, over the earlier visits a Watercolor stroke left.
    fn finished(
        &self,
        img: &StampImage,
        grain: Grain,
        px: [u8; 4],
        p: usize,
        x: u32,
        y: u32,
    ) -> [u8; 4] {
        self.finish_px(img, grain, px, p, img.depth[p], x, y)
    }

    /// [`Self::finished`] with the pixel's `depth` already read.
    #[allow(clippy::too_many_arguments)]
    fn finish_px(
        &self,
        img: &StampImage,
        grain: Grain,
        px: [u8; 4],
        p: usize,
        depth: u8,
        x: u32,
        y: u32,
    ) -> [u8; 4] {
        let mut out = px;
        let g = img.side.grain.get(p).map_or(grain, |c| Grain::of_code(*c));
        if out[3] != 0 && g != Grain::Smooth {
            out[3] = self.alpha(g, out[3], depth, x, y);
        }
        match img.side.prev.get(p) {
            Some(under) if under[3] != 0 => over_px(out, *under),
            _ => out,
        }
    }
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
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
#[derive(Clone, Debug, Default)]
pub struct StampImage {
    pub width: u32,
    pub height: u32,
    pub origin: [f32; 2],
    pub pixel: f32,
    pub rgba: Vec<u8>,
    /// One byte per pixel while a textured stroke is being stamped: how deep
    /// the pixel lies inside it. [`finish_grain`] reads and clears it. Empty
    /// for smooth strokes.
    pub depth: Vec<u8>,
    /// Texture and Watercolor revisit records while a stroke is stamped.
    pub side: StampSide,
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
    stamp_tipped_padded(contours, pixel, 0.0)
}

/// Heaviest blur, in pixels, a stamp is blurred at. A heavier blur coarsens
/// the pixel instead: the result is smooth, so fewer pixels lose nothing and
/// the kernel stays small.
const MAX_BLUR_PX: f32 = 4.0;

/// A committed stroke's bitmap: stamp `contours`, blur by `blur`, a standard
/// deviation in the contours' units (SVG `stdDeviation`), then subtract
/// `erase` passes. Erasing after the blur removes what is seen, halo
/// included, and keeps the hole's edge as the eraser drew it. The bitmap is
/// padded to hold the whole falloff, so a heavy blur never clips to a square.
pub fn stamp_blurred(
    contours: &[Vec<TipPoint>],
    erase: &[Vec<TipPoint>],
    pixel: f32,
    blur: f32,
) -> Option<StampImage> {
    let blur = if blur.is_finite() { blur.max(0.0) } else { 0.0 };
    let pixel = if blur > 0.0 {
        pixel.max(blur / MAX_BLUR_PX)
    } else {
        pixel
    };
    let mut img = stamp_tipped_padded(contours, pixel, 3.0 * blur)?;
    if blur > 0.0 {
        crate::blur::gaussian_blur_rgba(&mut img.rgba, img.width, img.height, blur / img.pixel);
    }
    apply_erase(&mut img, erase);
    Some(img)
}

/// [`stamp_tipped`] with `margin` more room on every side, in world units.
fn stamp_tipped_padded(contours: &[Vec<TipPoint>], pixel: f32, margin: f32) -> Option<StampImage> {
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
    let room = margin + pixel;
    let pixel = stamp_pixel(world_w + 2.0 * room, world_h + 2.0 * room, pixel);
    let pad = (margin / pixel).ceil() as u32 + 1;
    let x0 = min_x - pad as f32 * pixel;
    let y0 = min_y - pad as f32 * pixel;
    let w = ((world_w / pixel).ceil() as u32 + 2 * pad).max(1);
    let h = ((world_h / pixel).ceil() as u32 + 2 * pad).max(1);
    if (w as u64) * (h as u64) > (MAX_PIXELS as u64) * 2 {
        return None;
    }
    let mut img = StampImage {
        width: w,
        height: h,
        origin: [x0, y0],
        pixel,
        rgba: vec![0u8; (w as usize) * (h as usize) * 4],
        depth: Vec::new(),
        side: StampSide::default(),
    };
    for contour in contours {
        let pts: Vec<TipPoint> = contour
            .iter()
            .copied()
            .filter(|p| p.pos[0].is_finite() && p.pos[1].is_finite())
            .collect();
        stamp_polyline(&mut img, &pts);
    }
    finish_grain(&mut img, stroke_grain(contours), None);
    img.depth = Vec::new();
    img.side = StampSide::default();
    Some(img)
}

/// The grain of a stroke's first tip: the texture of pixels no textured
/// tip recorded (each segment takes its end tip's grain).
pub fn stroke_grain(contours: &[Vec<TipPoint>]) -> Grain {
    contours
        .iter()
        .flatten()
        .next()
        .map_or(Grain::Smooth, |p| p.tip.grain)
}

/// Stamp one segment into `img`, keeping the maximum coverage per pixel.
/// Rows only visit the span the capsule can reach, so a long diagonal costs
/// its own area and not its bounding box. A textured tip also records depth
/// and its texture; the grain lands once the stroke is finished
/// ([`finish_grain`]). The segment continues the arc of the one stamped
/// before it: a Watercolor tip builds over a pixel it last covered more
/// than [`WET_REVISIT_DIAMETERS`] back along that arc.
pub fn stamp_segment(img: &mut StampImage, a: TipPoint, b: TipPoint) {
    let px = img.pixel;
    let to_px = |p: [f32; 2]| [(p[0] - img.origin[0]) / px, (p[1] - img.origin[1]) / px];
    let pa = to_px(a.pos);
    let pb = to_px(b.pos);
    let ra = a.tip.diameter.max(0.0) * 0.5 / px;
    let rb = b.tip.diameter.max(0.0) * 0.5 / px;
    let reach = ra.max(rb);
    let abx = pb[0] - pa[0];
    let aby = pb[1] - pa[1];
    let len2 = abx * abx + aby * aby;
    let arc0 = img.side.run;
    let seg_len = len2.sqrt();
    if seg_len.is_finite() {
        img.side.run += seg_len;
    }
    if reach <= 0.0 || reach.is_nan() {
        return;
    }
    let w = img.width as i64;
    let h = img.height as i64;
    let y_lo = ((pa[1].min(pb[1]) - reach).floor() as i64).max(0);
    let y_hi = ((pa[1].max(pb[1]) + reach).ceil() as i64).min(h - 1);
    let same = a.tip == b.tip;
    // Pixel (0, 0) on the grid shared by every image at this pixel size, so
    // tiles and stamps that meet continue one dither pattern.
    let grid = [
        (img.origin[0] / px).round() as i64,
        (img.origin[1] / px).round() as i64,
    ];
    let grain = b.tip.grain;
    let code = grain.code();
    let pixels = (img.width as usize) * (img.height as usize);
    if grain != Grain::Smooth {
        if img.depth.len() != pixels {
            img.depth = vec![0u8; pixels];
        }
        if img.side.grain.len() != pixels {
            img.side.grain = vec![0u8; pixels];
        }
    }
    // Depth and texture are tracked once any textured tip is in the stroke,
    // so a smooth segment of a mixed chain keeps its own pixels smooth.
    let grained = img.depth.len() == pixels;
    let owned = img.side.grain.len() == pixels;
    let wet = grain == Grain::Watercolor && !img.side.mask;
    if wet && img.side.arc.len() != pixels {
        img.side.arc = vec![0u16; pixels];
    }
    let wet_code = Grain::Watercolor.code();
    // Half pixels, so the wrapping u16 spans 32768 px of arc.
    let revisit = WET_REVISIT_DIAMETERS * 2.0 * reach * 2.0;
    let paper = Paper::of(img);
    // A pixel already at this segment's opacity (and, for a textured tip, at
    // full depth) cannot change, so dense dabs skip their overlap. A
    // Watercolor dab still reads such a pixel's arc.
    let top = a.tip.rgba[3].max(b.tip.rgba[3]);
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
        let row = py as usize * img.width as usize;
        for pxl in x_lo..=x_hi {
            let p = row + pxl as usize;
            let full = img.rgba[p * 4 + 3] >= top && (!grained || img.depth[p] == 255);
            if full && !wet {
                continue;
            }
            let qx = pxl as f32 + 0.5;
            let t = if len2 <= f32::EPSILON {
                0.0
            } else {
                (((qx - pa[0]) * abx + (qy - pa[1]) * aby) / len2).clamp(0.0, 1.0)
            };
            let cx = pa[0] + abx * t;
            let cy = pa[1] + aby * t;
            let (dx, dy) = (qx - cx, qy - cy);
            let r = ra + (rb - ra) * t;
            if dx * dx + dy * dy >= r * r {
                continue;
            }
            let dist = (dx * dx + dy * dy).sqrt();
            let mut full = full;
            if wet {
                let at = ((arc0 + seg_len * t) * 2.0) as i64 as u16;
                if img.rgba[p * 4 + 3] != 0 && img.side.grain[p] == wet_code {
                    let gap = (at.wrapping_sub(img.side.arc[p]) as i16).unsigned_abs();
                    if gap as f32 > revisit {
                        fold_visit(img, &paper, p, pxl as u32, py as u32);
                        full = false;
                    }
                }
                img.side.arc[p] = at;
                if full {
                    continue;
                }
            }
            let (soft, color) = if same {
                (a.tip.softness, a.tip.rgba)
            } else {
                (
                    a.tip.softness + (b.tip.softness - a.tip.softness) * t,
                    lerp_rgba(a.tip.rgba, b.tip.rgba, t),
                )
            };
            if grained {
                let d = (depth_of(dist, r) * 255.0).round() as u8;
                if owned && (d > img.depth[p] || img.rgba[p * 4 + 3] == 0) {
                    img.side.grain[p] = code;
                }
                if d > img.depth[p] {
                    img.depth[p] = d;
                }
                if img.rgba[p * 4 + 3] >= top {
                    continue;
                }
            }
            let cover = tip_coverage(dist, r, soft);
            write_max(
                &mut img.rgba,
                img.width,
                (pxl as u32, py as u32),
                (grid[0] + pxl, grid[1] + py),
                color,
                cover,
            );
        }
    }
}

/// Close pixel `p`'s current Watercolor visit: finish it with its grain,
/// lay it over the visits before it, and clear the pixel for the next one.
fn fold_visit(img: &mut StampImage, paper: &Paper, p: usize, x: u32, y: u32) {
    let px: [u8; 4] = img.rgba[p * 4..p * 4 + 4].try_into().expect("four bytes");
    let finished = paper.finished(img, Grain::Watercolor, px, p, x, y);
    let pixels = img.depth.len();
    if img.side.prev.len() != pixels {
        img.side.prev = vec![[0; 4]; pixels];
    }
    img.side.prev[p] = finished;
    img.rgba[p * 4..p * 4 + 4].fill(0);
    img.depth[p] = 0;
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
        depth: Vec::new(),
        side: StampSide::for_mask(),
    };
    for mark in marks {
        let Some(region) = polyline_box(img, mark) else {
            continue;
        };
        if mask.rgba.is_empty() {
            mask.rgba = vec![0u8; img.rgba.len()];
        }
        stamp_polyline(&mut mask, mark);
        finish_grain(
            &mut mask,
            stroke_grain(std::slice::from_ref(mark)),
            Some(region),
        );
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

/// Stamp one tipped polyline (a single point is a dab) into `img`. A
/// polyline after another of the same stroke starts far along the arc, so
/// Watercolor builds where contours meet.
pub fn stamp_polyline(img: &mut StampImage, pts: &[TipPoint]) {
    img.side.run += CONTOUR_GAP_PX;
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
/// path order (the move-to, then the end of every segment); segments blend
/// between their end tips by arc length, straight on a line and by
/// smoothstep on a curve (P1.curve.tip-chord). Missing tips fall back to
/// `base`.
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
        let ease = match seg {
            KSeg::Line(_) => crate::TipEase::Linear,
            KSeg::Quad(_) | KSeg::Cubic(_) => crate::TipEase::Smooth,
        };
        let mut pts: Vec<[f32; 2]> = vec![[last.x as f32, last.y as f32]];
        kurbo::flatten([PathEl::MoveTo(last), seg.as_path_el()], tolerance, |e| {
            if let PathEl::LineTo(p) = e {
                pts.push([p.x as f32, p.y as f32]);
            }
        });
        // An eased blend needs points to bend at, as for a hard stroke.
        if ease == crate::TipEase::Smooth && from != to {
            pts = crate::stroke::densify(&pts, crate::stroke::SMOOTH_TIP_STEPS);
        }
        let lengths = crate::geom::cumulative_arclength(&pts);
        let total = lengths.last().copied().unwrap_or(0.0).max(1.0e-9);
        let contour = out.last_mut().expect("contour started");
        for (p, len) in pts.iter().zip(&lengths).skip(1) {
            contour.push(TipPoint {
                pos: *p,
                tip: lerp_tip(from, to, ease.weight(len / total)),
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

/// Size, softness, and color blend; the texture is the end tip's anywhere
/// past the start, since a paper grain is not a number to blend.
fn lerp_tip(a: StampStyle, b: StampStyle, t: f32) -> StampStyle {
    if a == b {
        return a;
    }
    StampStyle {
        diameter: a.diameter + (b.diameter - a.diameter) * t,
        softness: a.softness + (b.softness - a.softness) * t,
        rgba: lerp_rgba(a.rgba, b.rgba, t),
        grain: if t > 0.0 { b.grain } else { a.grain },
    }
}

/// Keep the larger alpha at `(px, py)`. Coverage is dithered on the shared
/// grid position `at`, so a slow falloff does not band into rings.
fn write_max(
    rgba: &mut [u8],
    w: u32,
    (px, py): (u32, u32),
    at: (i64, i64),
    color: [u8; 4],
    cover: f32,
) {
    if cover <= 0.0 {
        return;
    }
    let i = ((py as usize) * (w as usize) + px as usize) * 4;
    let na = crate::dither::quantize(cover * color[3] as f32, at.0, at.1);
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
        grain: Grain::Smooth,
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
            grain: Default::default(),
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
                grain: Default::default(),
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
            grain: Default::default(),
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
            grain: Default::default(),
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
            grain: Default::default(),
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

    /// Dry media thin out toward the rim. Wet media (Ink, Watercolor) pool
    /// pigment there: a hard tip's wet edge is denser than its body.
    #[test]
    fn dry_grains_fall_off_toward_the_rim_and_wet_ones_darken_it() {
        for g in [Grain::Graphite, Grain::Pencil] {
            for i in 0..200 {
                let (wx, wy) = (i as f32 * 1.3, i as f32 * 0.7);
                let mut last = f32::INFINITY;
                for d in 0..20 {
                    let c = grain_coverage(g, d as f32, 20.0, 0.4, wx, wy);
                    assert!(c <= last + 1e-5, "{g:?} rises at d={d}: {c} > {last}");
                    last = c;
                }
            }
        }
        for g in [Grain::Ink, Grain::Watercolor] {
            let mean = |dist: f32| {
                (0..400)
                    .map(|i| grain_coverage(g, dist, 20.0, 0.0, i as f32 * 2.3, i as f32 * 1.1))
                    .sum::<f32>()
                    / 400.0
            };
            let body = mean(0.0);
            let rim = (14..20).map(|d| mean(d as f32)).fold(0.0, f32::max);
            assert!(
                rim > body + 0.04,
                "{g:?} has no wet edge: rim {rim}, body {body}"
            );
        }
    }

    /// The grain is applied to the finished stroke, so a chain of many short
    /// segments paints exactly what one long segment paints: a wet edge or
    /// a tooth never rings the joints.
    #[test]
    fn a_textured_chain_paints_like_one_segment() {
        for grain in [
            Grain::Graphite,
            Grain::Pencil,
            Grain::Ink,
            Grain::Watercolor,
        ] {
            let tip = StampStyle {
                diameter: 24.0,
                softness: 0.0,
                rgba: [20, 30, 40, 230],
                grain,
            };
            let at = |x: f32| TipPoint { pos: [x, 0.0], tip };
            let one = stamp_tipped(&[vec![at(0.0), at(120.0)]], 1.0).unwrap();
            let many =
                stamp_tipped(&[(0..=40).map(|i| at(i as f32 * 3.0)).collect()], 1.0).unwrap();
            assert_eq!(
                (one.width, one.height, one.origin),
                (many.width, many.height, many.origin)
            );
            let worst = one
                .rgba
                .iter()
                .zip(&many.rgba)
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap();
            assert!(worst <= 2, "{grain:?}: joints differ by {worst}");
            assert!(one.depth.is_empty(), "{grain:?}: depth left on the stamp");
        }
    }

    /// `contours` stamped and finished, with Watercolor revisits on or off.
    fn finished(contours: &[Vec<TipPoint>], revisits: bool) -> StampImage {
        let mut img = StampImage {
            width: 360,
            height: 300,
            origin: [-80.0, -150.0],
            pixel: 1.0,
            rgba: vec![0u8; 360 * 300 * 4],
            depth: Vec::new(),
            side: if revisits {
                StampSide::default()
            } else {
                StampSide::for_mask()
            },
        };
        for c in contours {
            stamp_polyline(&mut img, c);
        }
        finish_grain(&mut img, stroke_grain(contours), None);
        img
    }

    fn wet_chain(points: &[(f32, f32)], alpha: u8) -> Vec<Vec<TipPoint>> {
        let tip = StampStyle {
            diameter: 24.0,
            softness: 0.3,
            rgba: [40, 60, 160, alpha],
            grain: Grain::Watercolor,
        };
        vec![points
            .iter()
            .map(|&(x, y)| TipPoint { pos: [x, y], tip })
            .collect()]
    }

    /// r7-9 keeps tr7: neighbouring dabs along the path and Shift-chain
    /// joints down to 45° keep the maximum coverage, so they paint exactly
    /// what a stroke that never builds paints.
    #[test]
    fn watercolor_joints_and_neighbouring_dabs_do_not_build() {
        let dense: Vec<(f32, f32)> = (0..=80)
            .map(|i| (i as f32 * 2.5, (i as f32 * 0.2).sin() * 30.0))
            .collect();
        for points in [
            dense,
            vec![(0.0, 0.0), (100.0, 0.0), (100.0, 100.0)],
            vec![(0.0, 0.0), (120.0, 0.0), (60.0, 60.0)],
            vec![
                (0.0, 0.0),
                (60.0, 0.0),
                (60.0, 60.0),
                (0.0, 60.0),
                (0.0, 110.0),
            ],
        ] {
            let chain = wet_chain(&points, 200);
            let wet = finished(&chain, true);
            let dry = finished(&chain, false);
            let worst = wet
                .rgba
                .iter()
                .zip(&dry.rgba)
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap();
            assert_eq!(worst, 0, "{points:?} built by {worst}");
        }
    }

    /// Erase masks never build over themselves, whatever their texture: one
    /// pass never erases more than its strength.
    #[test]
    fn a_self_crossing_watercolor_erase_pass_never_exceeds_its_strength() {
        let mut img = finished(&wet_chain(&[(0.0, 0.0), (200.0, 0.0)], 255), true);
        let solid = img.rgba.clone();
        let pass = wet_chain(
            &[(100.0, -40.0), (100.0, 40.0), (60.0, 40.0), (140.0, -40.0)],
            128,
        );
        let mut once = img.clone();
        apply_erase(&mut img, &pass);
        apply_erase(&mut once, &[pass[0][..2].to_vec()]);
        let at = |i: &StampImage| alpha_at(i, 100.0, 0.0);
        assert!(at(&img) < solid[((150 * 360 + 180) * 4 + 3) as usize]);
        assert_eq!(
            at(&img),
            at(&once),
            "the pass built where it crossed itself"
        );
    }

    /// The side records are gone once a stroke finishes, so a reused layer
    /// starts the next stroke clean.
    #[test]
    fn finishing_clears_the_side_records() {
        let path = [
            (0.0, 0.0),
            (200.0, 0.0),
            (200.0, 100.0),
            (100.0, 100.0),
            (100.0, -100.0),
        ];
        let img = finished(&wet_chain(&path, 200), true);
        assert!(img.depth.iter().all(|d| *d == 0));
        assert!(img.side.grain.iter().all(|g| *g == 0));
        assert!(img.side.arc.iter().all(|a| *a == 0));
        assert!(img.side.prev.iter().all(|p| *p == [0; 4]));
        assert!(
            img.side.prev.len() == img.depth.len(),
            "the crossing kept no earlier visit"
        );
        assert_eq!(img.side.run, 0.0);
    }

    #[test]
    fn every_grain_stays_within_the_plain_tip_and_differs_from_smooth() {
        let grains = [
            Grain::Graphite,
            Grain::Pencil,
            Grain::Ink,
            Grain::Watercolor,
        ];
        for g in grains {
            let mut differs = false;
            for i in 0..400 {
                let (wx, wy) = (i as f32 * 0.37, i as f32 * 0.91);
                let dist = (i % 20) as f32;
                let c = grain_coverage(g, dist, 20.0, 0.4, wx, wy);
                assert!((0.0..=1.0).contains(&c), "{g:?} coverage {c}");
                if (c - tip_coverage(dist, 20.0, 0.4)).abs() > 0.05 {
                    differs = true;
                }
            }
            assert!(differs, "{g:?} looks identical to Smooth");
            // Deterministic: the same world point gives the same grain.
            assert_eq!(
                grain_coverage(g, 5.0, 20.0, 0.4, 12.3, 45.6),
                grain_coverage(g, 5.0, 20.0, 0.4, 12.3, 45.6)
            );
        }
    }

    #[test]
    fn an_erase_pass_cuts_a_hole_and_never_exceeds_its_strength() {
        let ink = StampStyle {
            diameter: 30.0,
            softness: 0.0,
            rgba: [200, 100, 50, 255],
            grain: Default::default(),
        };
        let mut img = stamp_contours(&[vec![[0.0, 0.0], [200.0, 0.0]]], ink).unwrap();
        let full = StampStyle {
            diameter: 20.0,
            softness: 0.0,
            rgba: [0, 0, 0, 255],
            grain: Default::default(),
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

    /// A blurred stroke is erased as seen: the pass removes the blurred ink,
    /// halo included, with the eraser's own edge, and leaves the rest as the
    /// unerased blur drew it.
    #[test]
    fn a_blurred_stamp_is_erased_after_its_blur() {
        let tip = StampStyle {
            diameter: 20.0,
            softness: 0.0,
            rgba: [200, 100, 50, 255],
            grain: Default::default(),
        };
        let at = |x: f32, y: f32, tip| TipPoint { pos: [x, y], tip };
        let line = vec![vec![at(0.0, 0.0, tip), at(200.0, 0.0, tip)]];
        let eraser = StampStyle {
            diameter: 30.0,
            ..tip
        };
        let marks = vec![vec![at(100.0, -40.0, eraser), at(100.0, 40.0, eraser)]];
        let plain = stamp_blurred(&line, &[], 1.0, 4.0).unwrap();
        let erased = stamp_blurred(&line, &marks, 1.0, 4.0).unwrap();
        assert!(alpha_at(&plain, 100.0, 14.0) > 0, "no halo to erase");
        assert_eq!(alpha_at(&erased, 100.0, 0.0), 0, "the pass kept ink");
        assert_eq!(alpha_at(&erased, 100.0, 14.0), 0, "the pass kept the halo");
        assert_eq!(alpha_at(&erased, 110.0, 0.0), 0, "the hole's edge blurred");
        assert_eq!(
            alpha_at(&erased, 40.0, 0.0),
            alpha_at(&plain, 40.0, 0.0),
            "untouched ink changed"
        );
    }

    /// Longest run of equal values in an 8-row band average of alpha, walking
    /// right from `(cx, cy)`, over pixels whose exact value is between 5% and
    /// 95% of `peak`. An undithered gradient repeats one value per ring.
    fn band_plateau(img: &StampImage, cx: u32, cy: u32, exact: impl Fn(u32) -> f32) -> usize {
        let peak = exact(cx);
        let (mut best, mut run, mut last) = (0usize, 0usize, None);
        for x in cx..img.width {
            let e = exact(x);
            if e < peak * 0.05 || e > peak * 0.95 {
                last = None;
                run = 0;
                continue;
            }
            let sum: u32 = (cy - 4..cy + 4)
                .map(|y| img.rgba[((y * img.width + x) * 4 + 3) as usize] as u32)
                .sum();
            run = if last == Some(sum) { run + 1 } else { 1 };
            last = Some(sum);
            best = best.max(run);
        }
        best
    }

    #[test]
    fn a_wide_soft_dab_has_no_banded_rings() {
        let tip = StampStyle {
            diameter: 512.0,
            softness: 1.0,
            rgba: [30, 60, 200, 64],
            grain: Default::default(),
        };
        let img = stamp_tipped(
            &[vec![TipPoint {
                pos: [0.0, 0.0],
                tip,
            }]],
            1.0,
        )
        .unwrap();
        let cx = ((0.0 - img.origin[0]) / img.pixel) as u32;
        let cy = ((0.0 - img.origin[1]) / img.pixel) as u32;
        let exact = |x: u32| {
            let d = (x as f32 + 0.5 - (0.0 - img.origin[0]) / img.pixel).abs();
            tip_coverage(d, 256.0, 1.0) * 64.0
        };
        let run = band_plateau(&img, cx, cy, exact);
        assert!(run <= 3, "soft falloff holds one alpha for {run} px");
        // Dithering never lifts ink past the tip's opacity or outside its rim.
        assert_eq!(max_alpha(&img), 64);
        assert_eq!(alpha_at(&img, 257.5, 0.0), 0);
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
            grain: Default::default(),
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

    /// P1.curve.tip-chord: a stamped curve eases its tips by smoothstep over
    /// arc length; a straight segment stays linear.
    #[test]
    fn tipped_contours_ease_curves_by_smoothstep() {
        let a = StampStyle {
            diameter: 2.0,
            softness: 0.0,
            rgba: [0, 0, 0, 255],
            grain: Default::default(),
        };
        let b = StampStyle {
            diameter: 10.0,
            ..a
        };
        let near = |chain: &[TipPoint], x: f32| {
            chain
                .iter()
                .min_by(|p, q| (p.pos[0] - x).abs().total_cmp(&(q.pos[0] - x).abs()))
                .map(|p| (p.pos[0], p.tip.diameter))
                .unwrap()
        };
        let mut curve = kurbo::BezPath::new();
        curve.move_to((0.0, 0.0));
        curve.curve_to((30.0, 0.0), (70.0, 0.0), (100.0, 0.0));
        let chain = &tipped_contours(&curve, &[a, b], a, 0.25)[0];
        assert!(chain.len() > 4, "a curve densifies its blend");
        let (x, d) = near(chain, 25.0);
        let smooth = 2.0 + 8.0 * crate::TipEase::Smooth.weight(x / 100.0);
        assert!((d - smooth).abs() < 0.05, "{d} at {x}, want {smooth}");
        assert!(
            d < 2.0 + 8.0 * x / 100.0 - 0.3,
            "slower than linear near the start"
        );

        let mut line = kurbo::BezPath::new();
        line.move_to((0.0, 0.0));
        line.line_to((100.0, 0.0));
        let chain = &tipped_contours(&line, &[a, b], a, 0.25)[0];
        assert_eq!(chain.len(), 2, "a line blends straight between its ends");
    }
}
