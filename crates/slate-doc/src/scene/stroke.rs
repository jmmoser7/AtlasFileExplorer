//! Durable stroke style, shared by board and SVG interpreters.
use super::*;

/// Outline stroke. `width == 0` means no stroke.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    pub width: f32,
    pub color: Rgba,
    pub dash: Dash,
    #[serde(default)]
    pub cap: StrokeCap,
    #[serde(default)]
    pub join: StrokeJoin,
    #[serde(default)]
    pub profile: WidthProfile,
    /// Edge falloff, 0 = hard. Above zero, both painters stamp a radial
    /// bitmap instead of an offset stroke. `blur_sigma` remains the vector
    /// fallback for a stroke that is not stamped.
    #[serde(default, skip_serializing_if = "softness_zero")]
    pub softness: f32,
    /// Brush commits set this so a hard stroke is still the radial bitmap.
    /// Softness above zero stamps even when this is false.
    #[serde(default, skip_serializing_if = "is_false")]
    pub stamp: bool,
    /// Legacy two-tip tween from before per-vertex tips. Read only: new
    /// strokes store [`PathData::tips`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tween_from: Option<StrokeSpan>,
    /// Whole-stroke Gaussian blur (SVG `feGaussianBlur` / CSS `blur`), in
    /// world units as stdDeviation. Distinct from edge softness.
    #[serde(default, skip_serializing_if = "gaussian_blur_zero")]
    pub gaussian_blur: f32,
    /// An arrowhead at the path's last point (open curves).
    #[serde(default, skip_serializing_if = "is_false")]
    pub arrow_end: bool,
    /// An arrowhead at the path's first point (open curves).
    #[serde(default, skip_serializing_if = "is_false")]
    pub arrow_start: bool,
    /// Optional authored [length, full width] per arrowhead, in world units.
    #[serde(default, skip_serializing_if = "arrow_sizes_default")]
    pub arrow_sizes: [Option<[f32; 2]>; 2],
    /// The cap at the first / last point of an open curve where it differs
    /// from `cap`, which still caps dash ends in between.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cap_start: Option<StrokeCap>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cap_end: Option<StrokeCap>,
    /// Grain of a stamped brush stroke. Vector strokes ignore it.
    #[serde(default, skip_serializing_if = "texture_smooth")]
    pub texture: BrushTexture,
}

fn gaussian_blur_zero(v: &f32) -> bool {
    *v <= 0.0
}

fn arrow_sizes_default(sizes: &[Option<[f32; 2]>; 2]) -> bool {
    sizes.iter().all(Option::is_none)
}

/// One brush tip on a path vertex.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StrokeSpan {
    pub width: f32,
    pub softness: f32,
    pub color: Rgba,
    #[serde(default, skip_serializing_if = "texture_smooth")]
    pub texture: BrushTexture,
}

impl StrokeSpan {
    pub fn of(stroke: &Stroke) -> StrokeSpan {
        StrokeSpan {
            width: stroke.width,
            softness: stroke.softness,
            color: stroke.color,
            texture: stroke.texture,
        }
    }

    /// The tip both stamp interpreters paint for this span.
    pub fn stamp_style(self) -> vector_ink::StampStyle {
        vector_ink::StampStyle {
            diameter: self.width.max(0.0),
            softness: self.softness,
            rgba: self.color.0,
            grain: self.texture.grain(),
        }
    }
}

fn softness_zero(value: &f32) -> bool {
    *value == 0.0
}

impl Default for Stroke {
    fn default() -> Self {
        Self {
            width: 0.0,
            color: Rgba::BLACK,
            dash: Dash::Solid,
            cap: StrokeCap::default(),
            join: StrokeJoin::default(),
            profile: WidthProfile::default(),
            softness: 0.0,
            stamp: false,
            tween_from: None,
            gaussian_blur: 0.0,
            arrow_end: false,
            arrow_start: false,
            arrow_sizes: [None; 2],
            cap_start: None,
            cap_end: None,
            texture: Default::default(),
        }
    }
}

impl Stroke {
    pub fn arrow_size(&self, end: usize, width: f32) -> [f32; 2] {
        let length = crate::geom::arrow_len(width);
        self.arrow_sizes[end.min(1)]
            .filter(|s| s.iter().all(|v| v.is_finite() && *v > 0.0))
            .unwrap_or([length, length * 0.8])
    }

    /// Conservative broad-phase extent; exact picking uses the head triangle.
    pub fn arrow_reach(&self) -> f32 {
        (0..2)
            .filter(|i| self.arrows()[*i])
            .map(|i| {
                let [length, width] = self.arrow_size(i, self.width);
                length.max(width * 0.5)
            })
            .fold(0.0_f32, f32::max)
    }
    /// The caps an open curve paints at its first and last point.
    pub fn end_caps(&self) -> [StrokeCap; 2] {
        [
            self.cap_start.unwrap_or(self.cap),
            self.cap_end.unwrap_or(self.cap),
        ]
    }

    /// Arrowheads at the first and last point of an open curve.
    pub fn arrows(&self) -> [bool; 2] {
        [self.arrow_start, self.arrow_end]
    }

    /// The condition at end `i` (0 = first point, 1 = last).
    pub fn end(&self, i: usize) -> StrokeEnd {
        let i = i.min(1);
        StrokeEnd {
            cap: self.end_caps()[i],
            arrow: self.arrows()[i],
            narrow: self.profile.narrow_ends()[i],
        }
    }

    /// Set the condition at end `i` (0 = first point, 1 = last); the other
    /// end keeps its own.
    pub fn set_end(&mut self, i: usize, end: StrokeEnd) {
        let i = i.min(1);
        let mut arrows = self.arrows();
        arrows[i] = end.arrow;
        [self.arrow_start, self.arrow_end] = arrows;
        let mut narrow = self.profile.narrow_ends();
        narrow[i] = end.narrow;
        self.profile = self.profile.with_narrow_ends(narrow);
        let mut caps = self.end_caps();
        caps[i] = end.cap;
        self.cap_start = (caps[0] != self.cap).then_some(caps[0]);
        self.cap_end = (caps[1] != self.cap).then_some(caps[1]);
    }

    /// A piece cut from this stroke keeps the condition of each original
    /// end it still owns (`keep`); a new cut end takes the plain base cap,
    /// with no arrowhead and no narrowing.
    pub fn keep_ends(&mut self, keep: [bool; 2]) {
        let plain = StrokeEnd {
            cap: self.cap,
            arrow: false,
            narrow: false,
        };
        for (i, kept) in keep.into_iter().enumerate() {
            if !kept {
                self.set_end(i, plain);
            }
        }
    }

    /// `width` is the outer diameter, matching the on-canvas tip. The fade
    /// sits inside that diameter: opaque out to `(1 - softness)` of the
    /// radius, clear at the rim. Returns `(stroke width, feather)` for
    /// `stroke_mesh`, where the mesh's outer edge is `width / 2`.
    pub fn paint_profile(self) -> (f32, f32) {
        let softness = self.softness.clamp(0.0, 1.0);
        let width = self.width.max(0.0);
        if softness <= 0.0 {
            return (width, 0.0);
        }
        let outer = width * 0.5;
        let solid = outer * (1.0 - softness);
        let half = (solid + outer) * 0.5;
        let feather = (outer - solid).max(0.0);
        (half * 2.0, feather)
    }

    pub fn blur_sigma(self) -> f32 {
        let (_, feather) = self.paint_profile();
        self.gaussian_blur.max(feather * 0.5)
    }

    /// Radial bitmap of the centerline. Brush strokes set [`Self::stamp`];
    /// any softness above zero uses the same picture so older soft ink
    /// matches the tip.
    pub fn paints_as_stamp(self) -> bool {
        self.stamp || self.softness > 0.001
    }

    /// The same stroke as a hard vector line: no edge softness, no stamp,
    /// no tween, no blur. Pen, line, arc, polyline, and Bézier strokes are
    /// always hard; only the normal anti-aliased edge remains.
    #[must_use]
    pub fn hard_vector(self) -> Stroke {
        Stroke {
            softness: 0.0,
            stamp: false,
            tween_from: None,
            gaussian_blur: 0.0,
            ..self
        }
    }

    pub fn none() -> Stroke {
        Stroke::default()
    }

    pub fn is_none(&self) -> bool {
        self.width <= 0.0 || self.color.0[3] == 0
    }
}
