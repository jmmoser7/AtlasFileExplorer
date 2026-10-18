use crate::*;
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct InputSnapshot {
    pub revision: String,
    pub context: Vec<ContextItem>,
    pub wired: Vec<ContextItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextItem {
    pub node: u64,
    pub text: String,
    /// Linked asset locators; ordered, never copied into the workbook.
    pub images: Vec<String>,
    #[serde(default)]
    pub outputs: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub active: Option<String>,
    /// Exact depth render of a wired 3D model, near = white. `images[0]` is
    /// the shaded view from the same camera.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<String>,
    /// The input port this value arrived on. Absent on wires drawn before
    /// ports existed; those read by what they carry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<InputSlot>,
}

/// A generator's or text block's input port. The board draws them on the left
/// edge in this order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputSlot {
    /// The picture or 3D model a run starts from.
    Media,
    /// Any text: notes, sketches with words, another block's reply.
    Prompt,
    /// A second picture that guides the look of the result.
    Style,
    /// A saved Slate view wired from a screenshot into a 3D viewport.
    View,
}

impl InputSlot {
    pub fn id(self) -> &'static str {
        match self {
            Self::Media => "media",
            Self::Prompt => "prompt",
            Self::Style => "style",
            Self::View => "view",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        [Self::Media, Self::Prompt, Self::Style, Self::View]
            .into_iter()
            .find(|slot| slot.id() == id)
    }

    /// Prompt reads text; Media, Style, and View read pictures.
    pub fn takes_text(self) -> bool {
        self == Self::Prompt
    }
}

impl ContextItem {
    /// The port this value feeds. A wire drawn before ports existed reads by
    /// what it carries: pictures and models are Media, words are Prompt.
    pub fn port(&self) -> InputSlot {
        self.slot
            .unwrap_or(if self.images.is_empty() && self.depth.is_none() {
                InputSlot::Prompt
            } else {
                InputSlot::Media
            })
    }
}

impl InputSnapshot {
    /// Wired values on one port, in wire order.
    pub fn on(&self, slot: InputSlot) -> impl Iterator<Item = &ContextItem> {
        self.wired.iter().filter(move |item| item.port() == slot)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ImageBundle {
    pub images: Vec<ImageOutput>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageOutput {
    /// Stable within the bundle, independent of its current ordering.
    pub id: String,
    pub source: String,
    pub request: String,
    pub prompt: String,
    /// Checkpoint or other engine model name. Empty on older bundles.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub model: String,
    /// `generate`, `vary`, or `render`. Empty on older bundles.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub task: String,
    /// Live run that produced this frame. Only the newest frame of the current
    /// run is replaceable; every other entry is kept.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub live: String,
}
