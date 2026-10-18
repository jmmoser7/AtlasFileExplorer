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
        }
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
