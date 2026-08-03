//! Hex colors for hand-authored kit files.
//!
//! The document model stores `Rgba([u8; 4])`, which serializes as a four-number
//! array — fine inside a `.slate` file that nobody types, wrong for a file a
//! human or an agent writes by hand. `KitColor` is the same value wearing
//! `#rrggbb` / `#rrggbbaa`, and it converts both ways so no color information
//! is invented or lost.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use slate_doc::scene::Rgba;

/// An `Rgba` written as a hex string in kit files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KitColor(pub Rgba);

impl KitColor {
    #[must_use]
    pub const fn rgba(self) -> Rgba {
        self.0
    }

    /// `#rrggbb` when fully opaque, `#rrggbbaa` otherwise.
    #[must_use]
    pub fn to_hex(self) -> String {
        let [r, g, b, a] = self.0 .0;
        if a == 255 {
            format!("#{r:02x}{g:02x}{b:02x}")
        } else {
            format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
        }
    }

    /// Parse `#rgb`, `#rgba`, `#rrggbb`, or `#rrggbbaa`. The leading `#` is
    /// optional so a TOML author who forgets it still gets what they meant.
    pub fn from_hex(s: &str) -> Result<KitColor, ColorError> {
        let h = s.trim().trim_start_matches('#');
        if !h.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(ColorError(s.to_string()));
        }
        let nib =
            |i: usize| -> u8 { u8::from_str_radix(&h[i..=i], 16).expect("checked hex digit") };
        let pair =
            |i: usize| -> u8 { u8::from_str_radix(&h[i..i + 2], 16).expect("checked hex digit") };
        let v = match h.len() {
            3 => [nib(0) * 17, nib(1) * 17, nib(2) * 17, 255],
            4 => [nib(0) * 17, nib(1) * 17, nib(2) * 17, nib(3) * 17],
            6 => [pair(0), pair(2), pair(4), 255],
            8 => [pair(0), pair(2), pair(4), pair(6)],
            _ => return Err(ColorError(s.to_string())),
        };
        Ok(KitColor(Rgba(v)))
    }
}

impl From<Rgba> for KitColor {
    fn from(c: Rgba) -> Self {
        KitColor(c)
    }
}

impl From<KitColor> for Rgba {
    fn from(c: KitColor) -> Self {
        c.0
    }
}

/// An unparseable color literal, reported with the offending text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColorError(pub String);

impl std::fmt::Display for ColorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "`{}` is not a hex color (expected #rgb, #rgba, #rrggbb, or #rrggbbaa)",
            self.0
        )
    }
}

impl std::error::Error for ColorError {}

impl Serialize for KitColor {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for KitColor {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<KitColor, D::Error> {
        let s = String::deserialize(d)?;
        KitColor::from_hex(&s).map_err(serde::de::Error::custom)
    }
}

/// A color that may defer to the theme instead of naming a value.
///
/// Written as `"accent"`, `"accent@60"` (the accent at alpha 60), or any hex
/// form. Deferring matters: a kit that hardcoded its accent would look wrong in
/// the theme the author did not have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorRef {
    Accent { alpha: Option<u8> },
    Fixed(KitColor),
}

impl ColorRef {
    #[must_use]
    pub fn resolve(self, accent: Rgba) -> Rgba {
        match self {
            ColorRef::Fixed(c) => c.rgba(),
            ColorRef::Accent { alpha } => {
                let [r, g, b, a] = accent.0;
                Rgba([r, g, b, alpha.unwrap_or(a)])
            }
        }
    }

    pub fn parse(s: &str) -> Result<ColorRef, String> {
        let t = s.trim();
        if let Some(rest) = t.strip_prefix("accent") {
            if rest.is_empty() {
                return Ok(ColorRef::Accent { alpha: None });
            }
            return match rest.strip_prefix('@') {
                Some(a) => a
                    .parse::<u8>()
                    .map(|alpha| ColorRef::Accent { alpha: Some(alpha) })
                    .map_err(|_| format!("`{s}`: accent alpha must be 0-255")),
                None => Err(format!("`{s}` is not a color reference")),
            };
        }
        KitColor::from_hex(t)
            .map(ColorRef::Fixed)
            .map_err(|e| e.to_string())
    }

    #[must_use]
    pub fn to_literal(self) -> String {
        match self {
            ColorRef::Fixed(c) => c.to_hex(),
            ColorRef::Accent { alpha: None } => "accent".into(),
            ColorRef::Accent { alpha: Some(a) } => format!("accent@{a}"),
        }
    }
}

impl From<Rgba> for ColorRef {
    fn from(c: Rgba) -> Self {
        ColorRef::Fixed(KitColor(c))
    }
}

impl Serialize for ColorRef {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_literal())
    }
}

impl<'de> Deserialize<'de> for ColorRef {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<ColorRef, D::Error> {
        let s = String::deserialize(d)?;
        ColorRef::parse(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn six_and_eight_digit_forms_round_trip() {
        let opaque = KitColor(Rgba([0xe8, 0x44, 0x3a, 255]));
        assert_eq!(opaque.to_hex(), "#e8443a");
        assert_eq!(KitColor::from_hex("#e8443a"), Ok(opaque));

        let alpha = KitColor(Rgba([0x10, 0x20, 0x30, 0x40]));
        assert_eq!(alpha.to_hex(), "#10203040");
        assert_eq!(KitColor::from_hex("#10203040"), Ok(alpha));
    }

    #[test]
    fn short_forms_expand_by_nibble_doubling() {
        assert_eq!(
            KitColor::from_hex("#f0c"),
            Ok(KitColor(Rgba([0xff, 0x00, 0xcc, 255])))
        );
        assert_eq!(
            KitColor::from_hex("#f0c8"),
            Ok(KitColor(Rgba([0xff, 0x00, 0xcc, 0x88])))
        );
    }

    #[test]
    fn the_hash_is_optional_and_whitespace_is_ignored() {
        let want = KitColor(Rgba([0xab, 0xcd, 0xef, 255]));
        assert_eq!(KitColor::from_hex("abcdef"), Ok(want));
        assert_eq!(KitColor::from_hex("  #ABCDEF "), Ok(want));
    }

    #[test]
    fn bad_literals_report_the_offending_text() {
        assert!(KitColor::from_hex("#xyz").is_err());
        assert!(KitColor::from_hex("#12345").is_err());
        assert!(KitColor::from_hex("").is_err());
        let e = KitColor::from_hex("nope").unwrap_err();
        assert!(e.to_string().contains("nope"));
    }

    #[test]
    fn serde_reads_and_writes_hex_strings() {
        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        struct Holder {
            color: KitColor,
        }
        let h: Holder = toml::from_str(r##"color = "#e8443a""##).unwrap();
        assert_eq!(h.color.rgba(), Rgba([0xe8, 0x44, 0x3a, 255]));
        assert!(toml::to_string(&h).unwrap().contains("#e8443a"));
    }

    #[test]
    fn accent_references_take_the_theme_color_and_honour_alpha() {
        let accent = Rgba([10, 120, 240, 255]);
        assert_eq!(
            ColorRef::Accent { alpha: None }.resolve(accent),
            Rgba([10, 120, 240, 255])
        );
        assert_eq!(
            ColorRef::Accent { alpha: Some(60) }.resolve(accent),
            Rgba([10, 120, 240, 60])
        );
        assert_eq!(
            ColorRef::Fixed(KitColor(Rgba([1, 2, 3, 4]))).resolve(accent),
            Rgba([1, 2, 3, 4])
        );
    }

    #[test]
    fn color_references_round_trip_through_their_literals() {
        for c in [
            ColorRef::Accent { alpha: None },
            ColorRef::Accent { alpha: Some(60) },
            ColorRef::Fixed(KitColor(Rgba([1, 2, 3, 4]))),
        ] {
            let lit = c.to_literal();
            assert_eq!(ColorRef::parse(&lit), Ok(c), "{lit}");
        }
    }

    #[test]
    fn a_bad_color_reference_is_an_error_not_a_silent_black() {
        assert!(ColorRef::parse("accent@999").is_err());
        assert!(ColorRef::parse("accentish").is_err());
        assert!(ColorRef::parse("#nope").is_err());
        assert!(ColorRef::parse("").is_err());
    }

    #[test]
    fn every_byte_value_survives_a_round_trip() {
        for v in 0u8..=255 {
            let c = KitColor(Rgba([v, v.wrapping_add(7), v.wrapping_mul(3), v]));
            assert_eq!(KitColor::from_hex(&c.to_hex()), Ok(c));
        }
    }
}
