//! Workbook-local style memory for board create tools (P1.shape.style /
//! P1.curve.create-style). Persisted on [`crate::view::ViewState`], not in
//! the scene journal.

use serde::{Deserialize, Serialize};

use crate::scene::{Rgba, Stroke};

/// One slot of remembered stroke / fill / opacity.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StyleMemorySlot {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opacity: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke: Option<Stroke>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<Rgba>,
}

/// Closed shapes (rect, ellipse, polygon, …) vs open curves (line, polyline, …).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CreateStyleMemory {
    #[serde(default, skip_serializing_if = "StyleMemorySlot::is_empty")]
    pub closed: StyleMemorySlot,
    #[serde(default, skip_serializing_if = "StyleMemorySlot::is_empty")]
    pub open: StyleMemorySlot,
}

impl StyleMemorySlot {
    pub fn is_empty(&self) -> bool {
        self.opacity.is_none() && self.stroke.is_none() && self.fill.is_none()
    }
}

impl CreateStyleMemory {
    /// Legacy single memory → split closed/open. Open stroke width is never 0.
    pub fn from_legacy(legacy: StyleMemorySlot, min_open_stroke_width: f32) -> Self {
        let closed = legacy.clone();
        let mut open = legacy;
        if let Some(st) = open.stroke.as_mut() {
            if st.width <= 0.0 {
                st.width = min_open_stroke_width;
            }
        }
        Self { closed, open }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{Dash, StrokeCap, StrokeJoin, WidthProfile};

    #[test]
    fn legacy_migration_never_seeds_open_stroke_width_zero() {
        let legacy = StyleMemorySlot {
            opacity: Some(1.0),
            stroke: Some(Stroke {
                width: 0.0,
                color: Rgba::BLACK,
                dash: Dash::Solid,
                cap: StrokeCap::Butt,
                join: StrokeJoin::Miter,
                profile: WidthProfile::Uniform,
                softness: 0.0,
                stamp: false,
                tween_from: None,
                gaussian_blur: 0.0,
            }),
            fill: Some(Rgba::WHITE),
        };
        let mem = CreateStyleMemory::from_legacy(legacy, 2.0);
        assert_eq!(mem.closed.stroke.unwrap().width, 0.0);
        assert_eq!(mem.open.stroke.unwrap().width, 2.0);
        assert_eq!(mem.closed.fill, Some(Rgba::WHITE));
    }
}
