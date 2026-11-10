use crate::*;
/// Absent fields use the engine's preset.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ImageParams {
    /// A live run repeats one seed so consecutive frames stay coherent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    /// Live run identity. A new frame of the same run replaces the previous one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live: Option<String>,
    /// Shape of a new picture. Source keeps the wired picture's shape.
    #[serde(default, skip_serializing_if = "Aspect::is_source")]
    pub aspect: Aspect,
    /// Pictures this one run produces. 0 and 1 both mean one.
    #[serde(default, skip_serializing_if = "at_most_one")]
    pub count: u8,
}

fn at_most_one(count: &u8) -> bool {
    *count <= 1
}

impl ImageParams {
    /// The pictures a run makes, 1 to 8.
    pub fn images(&self) -> u8 {
        self.count.clamp(1, 8)
    }

    /// A completed batch must account for every requested picture. Adapters
    /// retain partial outputs before reporting this error; this never retries.
    pub fn validate_output_count(&self, provider: &str, returned: usize) -> Result<(), String> {
        let requested = usize::from(self.images());
        if returned == requested {
            Ok(())
        } else {
            Err(format!(
                "{provider} returned {returned} images; {requested} were requested. Any returned images were kept. No additional generation was started."
            ))
        }
    }
}

/// The shape a generated picture takes. Each engine maps it to its own sizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Aspect {
    /// The wired picture's shape, or square when nothing is wired.
    #[default]
    Source,
    Square,
    /// 3:2.
    Landscape,
    /// 2:3.
    Portrait,
    /// 16:9.
    Wide,
    Custom {
        #[serde(deserialize_with = "custom_dimension")]
        width: u16,
        #[serde(deserialize_with = "custom_dimension")]
        height: u16,
    },
}

fn custom_dimension<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<u16, D::Error> {
    let value = u16::deserialize(deserializer)?;
    if (1..=10000).contains(&value) {
        Ok(value)
    } else {
        Err(serde::de::Error::custom(
            "Custom aspect dimensions must be 1 through 10000",
        ))
    }
}

#[cfg(test)]
mod custom_tests {
    use super::*;
    #[test]
    fn completed_image_counts_use_the_same_bounds_as_generation() {
        for (count, expected) in [(0, 1), (1, 1), (3, 3), (8, 8), (255, 8)] {
            let params = ImageParams {
                count,
                ..Default::default()
            };
            assert!(params.validate_output_count("Provider", expected).is_ok());
            let error = params
                .validate_output_count("Provider", expected - 1)
                .unwrap_err();
            assert!(error.contains(&format!("{expected} were requested")));
            assert!(params
                .validate_output_count("Provider", expected + 1)
                .is_err());
        }
    }

    #[test]
    fn legacy_presets_and_valid_custom_round_trip_with_strict_dimensions() {
        for aspect in Aspect::ALL {
            let json = serde_json::to_string(&aspect).unwrap();
            assert_eq!(serde_json::from_str::<Aspect>(&json).unwrap(), aspect);
            assert!(json.starts_with('"'));
        }
        let aspect = Aspect::Custom {
            width: 16,
            height: 9,
        };
        assert_eq!(
            serde_json::from_str::<Aspect>(r#"{"custom":{"width":16,"height":9}}"#).unwrap(),
            aspect
        );
        assert_eq!(
            serde_json::from_str::<Aspect>(&serde_json::to_string(&aspect).unwrap()).unwrap(),
            aspect
        );
        for (width, height) in [(0, 1), (1, 0), (10001, 1), (1, 10001)] {
            assert!(serde_json::from_value::<Aspect>(
                serde_json::json!({"custom":{"width":width,"height":height}})
            )
            .is_err());
            assert!(Aspect::Custom { width, height }.validate().is_err());
        }
        assert!(Aspect::Custom {
            width: 1,
            height: 10000
        }
        .validate_comfy(false)
        .is_err());
        assert!(aspect.validate_comfy(true).is_err());
        assert_eq!(aspect.ratio(), Some(16.0 / 9.0));
    }
}

impl Aspect {
    pub const ALL: [Self; 5] = [
        Self::Source,
        Self::Square,
        Self::Landscape,
        Self::Portrait,
        Self::Wide,
    ];

    pub fn is_source(&self) -> bool {
        *self == Self::Source
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Source => "Source",
            Self::Square => "1:1",
            Self::Landscape => "3:2",
            Self::Portrait => "2:3",
            Self::Wide => "16:9",
            Self::Custom { .. } => "Custom…",
        }
    }

    /// Width over height; `None` follows the source.
    pub fn ratio(self) -> Option<f32> {
        match self {
            Self::Source => None,
            Self::Square => Some(1.0),
            Self::Landscape => Some(1.5),
            Self::Portrait => Some(2.0 / 3.0),
            Self::Wide => Some(16.0 / 9.0),
            Self::Custom { width, height }
                if (1..=10000).contains(&width) && (1..=10000).contains(&height) =>
            {
                Some(f32::from(width) / f32::from(height))
            }
            Self::Custom { .. } => None,
        }
    }
    pub fn custom(self) -> Option<[u16; 2]> {
        if let Self::Custom { width, height } = self {
            Some([width, height])
        } else {
            None
        }
    }
    pub fn validate(self) -> Result<(), String> {
        if self.custom().is_some() && self.ratio().is_none() {
            Err("Custom aspect dimensions must be 1 through 10000".into())
        } else {
            Ok(())
        }
    }
    /// Current Comfy generation contract: latent dimensions are rounded to 8px.
    /// Source transformation workflows retain their source size.
    pub fn validate_comfy(self, source_transform: bool) -> Result<(), String> {
        self.validate()?;
        if self.custom().is_some() {
            if source_transform {
                return Err("Custom aspect is available for new Comfy images; source transformations keep their source shape".into());
            }
            if !self
                .ratio()
                .is_some_and(|ratio| (0.25..=4.0).contains(&ratio))
            {
                return Err("Comfy custom aspect must be between 1:4 and 4:1".into());
            }
        }
        Ok(())
    }
}

/// The newest intermediate frame of a running image generation. Derived: never
/// journaled and never written to a session manifest.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ImagePreview {
    pub request: String,
    pub step: u32,
    pub steps: u32,
    /// Encoded JPEG or PNG; empty until the engine sends a frame.
    pub image: std::sync::Arc<Vec<u8>>,
    /// Counts frames, so a viewer decodes each one once.
    pub frame: u64,
}

/// What an image engine runs, named by what is wired. The board's action and
/// the engine's graph both read this rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageTask {
    /// A prompt alone.
    Generate,
    /// A wired picture, changed within its outline.
    Vary,
    /// A wired 3D view, rendered from its depth and edges.
    Render,
}

impl ImageTask {
    pub fn for_inputs(geometry: bool, image: bool) -> Self {
        if geometry {
            Self::Render
        } else if image {
            Self::Vary
        } else {
            Self::Generate
        }
    }

    /// Recorded in [`ImageOutput::task`].
    pub fn label(self) -> &'static str {
        match self {
            Self::Generate => "generate",
            Self::Vary => "vary",
            Self::Render => "render",
        }
    }

    /// The button that starts it.
    pub fn action(self) -> &'static str {
        match self {
            Self::Generate => "Generate",
            Self::Vary => "Transform",
            Self::Render => "Render",
        }
    }

    /// What the button does, for its hover text.
    pub fn explain(self) -> &'static str {
        match self {
            Self::Generate => "Make a new picture from the prompt.",
            Self::Vary => {
                "Make a new picture from the wired picture: it keeps the composition and follows the prompt."
            }
            Self::Render => {
                "Render the wired 3D model from its current view, in the prompt's style."
            }
        }
    }

    pub fn working(self) -> &'static str {
        match self {
            Self::Generate => "Generating",
            Self::Vary => "Transforming",
            Self::Render => "Rendering",
        }
    }
}
