//! Kit-facing spellings of the document's stroke vocabulary.
//!
//! Why mirror at all: the document's `StrokeCap` / `StrokeJoin` serialize as
//! `"Butt"` / `"Miter"` while `Dash` serializes as `"solid"`, and a file people
//! type should not inherit that inconsistency. Changing the upstream spelling
//! would rewrite what `.slate` files contain, so the kit format carries its own
//! snake_case names instead.
//!
//! Why this is safe: every conversion is an exhaustive `match` in **both**
//! directions. If `slate-doc` gains a stroke cap, the `From<StrokeCap>` impl
//! stops compiling — drift is a build error, not a silent divergence.

use serde::{Deserialize, Serialize};
use slate_doc::scene::{Dash, Rgba, Stroke, StrokeCap, StrokeJoin, WidthProfile};

use crate::color::{ColorRef, KitColor};

/// Kit spelling of [`StrokeCap`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cap {
    #[default]
    Butt,
    Round,
    Square,
}

impl From<Cap> for StrokeCap {
    fn from(c: Cap) -> Self {
        match c {
            Cap::Butt => StrokeCap::Butt,
            Cap::Round => StrokeCap::Round,
            Cap::Square => StrokeCap::Square,
        }
    }
}

impl From<StrokeCap> for Cap {
    fn from(c: StrokeCap) -> Self {
        match c {
            StrokeCap::Butt => Cap::Butt,
            StrokeCap::Round => Cap::Round,
            StrokeCap::Square => Cap::Square,
        }
    }
}

/// Kit spelling of [`StrokeJoin`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Join {
    #[default]
    Miter,
    Round,
    Bevel,
}

impl From<Join> for StrokeJoin {
    fn from(j: Join) -> Self {
        match j {
            Join::Miter => StrokeJoin::Miter,
            Join::Round => StrokeJoin::Round,
            Join::Bevel => StrokeJoin::Bevel,
        }
    }
}

impl From<StrokeJoin> for Join {
    fn from(j: StrokeJoin) -> Self {
        match j {
            StrokeJoin::Miter => Join::Miter,
            StrokeJoin::Round => Join::Round,
            StrokeJoin::Bevel => Join::Bevel,
        }
    }
}

/// Kit spelling of [`WidthProfile`]. A taper is SVG-expressible as a filled
/// outline, which is why it is allowed to be data (Art. IV).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Profile {
    #[default]
    Uniform,
    Taper {
        start: f32,
        end: f32,
    },
}

impl From<Profile> for WidthProfile {
    fn from(p: Profile) -> Self {
        match p {
            Profile::Uniform => WidthProfile::Uniform,
            Profile::Taper { start, end } => WidthProfile::Taper { start, end },
        }
    }
}

impl From<WidthProfile> for Profile {
    fn from(p: WidthProfile) -> Self {
        match p {
            WidthProfile::Uniform => Profile::Uniform,
            WidthProfile::Taper { start, end } => Profile::Taper { start, end },
        }
    }
}

/// A stroke as a kit file writes it. Every field defaults, so
/// `stroke = { color = "#e8443a" }` is a complete 2px solid red stroke.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StrokeSpec {
    pub width: f32,
    /// May be `"accent"`, so a kit looks right in a theme its author never saw.
    pub color: ColorRef,
    pub dash: Dash,
    pub cap: Cap,
    pub join: Join,
    pub profile: Profile,
}

impl Default for StrokeSpec {
    fn default() -> Self {
        StrokeSpec {
            width: 2.0,
            color: ColorRef::Fixed(KitColor(Rgba::BLACK)),
            dash: Dash::Solid,
            cap: Cap::default(),
            join: Join::default(),
            profile: Profile::default(),
        }
    }
}

impl StrokeSpec {
    /// Bake into a document stroke, substituting the theme's accent.
    #[must_use]
    pub fn resolve(self, accent: Rgba) -> Stroke {
        Stroke {
            width: self.width,
            color: self.color.resolve(accent),
            dash: self.dash,
            cap: self.cap.into(),
            join: self.join.into(),
            profile: self.profile.into(),
        }
    }
}

impl From<Stroke> for StrokeSpec {
    fn from(s: Stroke) -> Self {
        StrokeSpec {
            width: s.width,
            color: ColorRef::Fixed(KitColor(s.color)),
            dash: s.dash,
            cap: s.cap.into(),
            join: s.join.into(),
            profile: s.profile.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caps_joins_and_profiles_round_trip_through_the_document_types() {
        for c in [Cap::Butt, Cap::Round, Cap::Square] {
            assert_eq!(Cap::from(StrokeCap::from(c)), c);
        }
        for j in [Join::Miter, Join::Round, Join::Bevel] {
            assert_eq!(Join::from(StrokeJoin::from(j)), j);
        }
        for p in [
            Profile::Uniform,
            Profile::Taper {
                start: 0.2,
                end: 1.0,
            },
        ] {
            assert_eq!(Profile::from(WidthProfile::from(p)), p);
        }
    }

    #[test]
    fn a_stroke_survives_the_trip_out_and_back() {
        let spec = StrokeSpec {
            width: 3.5,
            color: ColorRef::Fixed(KitColor(Rgba([1, 2, 3, 4]))),
            dash: Dash::Dotted,
            cap: Cap::Round,
            join: Join::Bevel,
            profile: Profile::Taper {
                start: 0.1,
                end: 0.9,
            },
        };
        assert_eq!(StrokeSpec::from(spec.resolve(Rgba::BLACK)), spec);
    }

    #[test]
    fn an_accent_stroke_takes_the_theme_color() {
        let spec: StrokeSpec = toml::from_str(r#"color = "accent""#).unwrap();
        let accent = Rgba([10, 120, 240, 255]);
        assert_eq!(spec.resolve(accent).color, accent);
    }

    #[test]
    fn kit_spellings_are_snake_case() {
        assert_eq!(
            toml::to_string(&Holder { cap: Cap::Round }).unwrap().trim(),
            "cap = \"round\""
        );
        assert_eq!(
            toml::to_string(&JoinHolder { join: Join::Bevel })
                .unwrap()
                .trim(),
            "join = \"bevel\""
        );

        #[derive(Serialize)]
        struct Holder {
            cap: Cap,
        }
        #[derive(Serialize)]
        struct JoinHolder {
            join: Join,
        }
    }

    #[test]
    fn every_field_defaults_so_a_color_alone_is_a_stroke() {
        let s: StrokeSpec = toml::from_str(r##"color = "#e8443a""##).unwrap();
        assert_eq!(s.width, 2.0);
        assert_eq!(s.dash, Dash::Solid);
        assert_eq!(s.cap, Cap::Butt);
        let stroke = s.resolve(Rgba::BLACK);
        assert_eq!(stroke.color, Rgba([0xe8, 0x44, 0x3a, 255]));
        assert_eq!(stroke.width, 2.0);
    }

    #[test]
    fn a_typo_in_a_stroke_field_is_rejected_rather_than_ignored() {
        // `deny_unknown_fields`: silently dropping `wdith` would ship a tool
        // that looks authored and behaves default.
        assert!(toml::from_str::<StrokeSpec>(r#"wdith = 4.0"#).is_err());
    }
}
