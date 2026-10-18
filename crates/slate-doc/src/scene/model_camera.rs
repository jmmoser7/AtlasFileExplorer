//! Authored 3D viewport presentation shared by rendering, posters and screenshots.

use serde::{Deserialize, Serialize};

/// How a model viewport draws. The material mask remains a segmentation pass,
/// independent of the imported-material switch used by shaded display modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelDisplay {
    #[default]
    Shaded,
    /// White surfaces with soft light and ambient occlusion.
    Arctic,
    /// Visible silhouettes and creases over flat diagrammatic surfaces.
    HiddenLine,
    /// Flat color per mesh part. Authored colors stay together; parts with
    /// no color get a stable palette color. This is the segmentation mask.
    Material,
    /// Inverse view depth: nearest geometry is white, far geometry is black.
    Depth,
}

impl ModelDisplay {
    /// Canonical order shared by the selection controls and preview swatches.
    pub const ALL: [Self; 5] = [
        Self::Shaded,
        Self::Arctic,
        Self::HiddenLine,
        Self::Material,
        Self::Depth,
    ];

    /// Stable command, XMP, and serde spelling for this display pass.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Shaded => "shaded",
            Self::Arctic => "arctic",
            Self::HiddenLine => "hidden_line",
            Self::Material => "material",
            Self::Depth => "depth",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Shaded => "Shaded",
            Self::Arctic => "Arctic",
            Self::HiddenLine => "Hidden line",
            Self::Material => "Material mask",
            Self::Depth => "Z-buffer",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "shaded" => Some(Self::Shaded),
            "arctic" => Some(Self::Arctic),
            "hidden_line" | "hidden-line" | "hiddenline" => Some(Self::HiddenLine),
            "material" => Some(Self::Material),
            "depth" => Some(Self::Depth),
            _ => None,
        }
    }

    pub fn next(self) -> Self {
        let index = Self::ALL.iter().position(|mode| *mode == self).unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }

    /// Arctic is always white; mask and depth are diagnostic passes.
    pub const fn uses_materials(self) -> bool {
        matches!(self, Self::Shaded | Self::HiddenLine)
    }
}

/// Saved viewport pose for placed 3D models. The board renders this camera live
/// while unlocked and as a poster while locked; the artifact embeds that same
/// poster. Duplicated nodes keep independent poses. Orbit follows Rhino's Z-up
/// convention: yaw spins around +Z and pitch tilts above/below the XY plane.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelCamera {
    /// Orbit target in model space. Non-finite/unset = model bounds center.
    pub target: [f32; 3],
    /// Rotation around +Z, radians.
    pub yaw: f32,
    /// Elevation above the XY plane, radians (clamped near ±π/2).
    pub pitch: f32,
    /// Eye distance from the target. `<= 0` = auto-fit to the model bounds.
    pub distance: f32,
    /// Viewport display pass. Missing on older files, which stay shaded.
    pub display: ModelDisplay,
    /// Use imported material colors in Shaded and Hidden line. Older workbooks
    /// retain their imported colors; Arctic always remains white.
    pub materials: bool,
}

impl Default for ModelCamera {
    fn default() -> Self {
        Self {
            target: [0.0, 0.0, 0.0],
            yaw: -std::f32::consts::FRAC_PI_4,
            pitch: 0.5,
            distance: 0.0,
            display: ModelDisplay::Shaded,
            materials: true,
        }
    }
}

impl ModelCamera {
    /// Stable hash identifying a poster's complete authored presentation.
    pub fn cache_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for v in self.target {
            v.to_bits().hash(&mut h);
        }
        self.yaw.to_bits().hash(&mut h);
        self.pitch.to_bits().hash(&mut h);
        self.distance.to_bits().hash(&mut h);
        self.display.hash(&mut h);
        self.materials.hash(&mut h);
        h.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_keys_and_cycle_match_canonical_order() {
        for (index, mode) in ModelDisplay::ALL.into_iter().enumerate() {
            assert_eq!(ModelDisplay::from_key(mode.key()), Some(mode));
            assert_eq!(
                serde_json::to_string(&mode).unwrap(),
                format!("\"{}\"", mode.key())
            );
            assert_eq!(
                mode.next(),
                ModelDisplay::ALL[(index + 1) % ModelDisplay::ALL.len()]
            );
        }
        assert_eq!(ModelDisplay::from_key("unknown"), None);
        assert_eq!(
            ModelDisplay::from_key("hidden-line"),
            Some(ModelDisplay::HiddenLine)
        );
    }

    #[test]
    fn camera_without_materials_field_preserves_imported_colors() {
        let old = r#"{"target":[1,2,3],"yaw":0.2,"pitch":0.4,"distance":10,"display":"shaded"}"#;
        let camera: ModelCamera = serde_json::from_str(old).unwrap();
        assert!(camera.materials);
        assert!(serde_json::from_str::<ModelCamera>("{}").unwrap().materials);
    }

    #[test]
    fn camera_cache_tracks_display_and_material_switch() {
        let camera = ModelCamera::default();
        assert!(camera.distance <= 0.0, "fresh nodes auto-fit");
        for changed in [
            ModelCamera {
                yaw: camera.yaw + 0.25,
                ..camera
            },
            ModelCamera {
                display: ModelDisplay::HiddenLine,
                ..camera
            },
            ModelCamera {
                materials: false,
                ..camera
            },
        ] {
            assert_ne!(camera.cache_hash(), changed.cache_hash());
            let encoded = serde_json::to_string(&changed).unwrap();
            assert_eq!(
                serde_json::from_str::<ModelCamera>(&encoded).unwrap(),
                changed
            );
        }
        assert_eq!(camera.cache_hash(), ModelCamera::default().cache_hash());
    }
}
