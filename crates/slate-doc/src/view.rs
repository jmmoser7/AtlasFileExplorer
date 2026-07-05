use serde::{Deserialize, Serialize};

/// Active canvas layout mode for the workbook view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewKind {
    #[default]
    Grid,
    Branch,
    Venn,
    /// Open-world authored canvas: frames, shapes, text, placed images.
    Board,
    /// Catch-all for forward-compatible deserialization; treated as [`ViewKind::Grid`].
    #[serde(other)]
    Unknown,
}

impl ViewKind {
    /// Returns [`ViewKind::Grid`] when this value is [`ViewKind::Unknown`].
    pub fn normalized(self) -> Self {
        match self {
            ViewKind::Unknown => ViewKind::Grid,
            other => other,
        }
    }
}

/// Camera pan/zoom and active view mode persisted with the document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ViewState {
    pub active_view: ViewKind,
    pub cam_x: f32,
    pub cam_y: f32,
    pub zoom: f32,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            active_view: ViewKind::Grid,
            cam_x: 0.0,
            cam_y: 0.0,
            zoom: 1.0,
        }
    }
}
