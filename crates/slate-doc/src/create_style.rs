//! Workbook-local style memory for board create tools (P1.shape.style /
//! P1.curve.create-style). Persisted on [`crate::view::ViewState`], not in
//! the scene journal. Closed shapes share one slot; each stroke tool keeps
//! its own.

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

/// The drawing tools that each remember their own stroke color and width.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StrokeTool {
    Pen,
    Line,
    Arc,
    Polyline,
    Bezier,
}

impl StrokeTool {
    pub const ALL: [StrokeTool; 5] = [
        StrokeTool::Pen,
        StrokeTool::Line,
        StrokeTool::Arc,
        StrokeTool::Polyline,
        StrokeTool::Bezier,
    ];
}

/// Closed shapes (rect, ellipse, polygon, …) share one slot. Each stroke
/// tool (pen, line, arc, polyline, Bézier) has its own.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CreateStyleMemory {
    #[serde(default, skip_serializing_if = "StyleMemorySlot::is_empty")]
    pub closed: StyleMemorySlot,
    /// The shared open-curve slot older workbooks saved. It seeds each
    /// stroke tool once ([`Self::migrate_open_to_tools`]) and is never
    /// written back.
    #[serde(default, rename = "open", skip_serializing)]
    pub legacy_open: StyleMemorySlot,
    #[serde(default, skip_serializing_if = "StyleMemorySlot::is_empty")]
    pub pen: StyleMemorySlot,
    #[serde(default, skip_serializing_if = "StyleMemorySlot::is_empty")]
    pub line: StyleMemorySlot,
    #[serde(default, skip_serializing_if = "StyleMemorySlot::is_empty")]
    pub arc: StyleMemorySlot,
    #[serde(default, skip_serializing_if = "StyleMemorySlot::is_empty")]
    pub polyline: StyleMemorySlot,
    #[serde(default, skip_serializing_if = "StyleMemorySlot::is_empty")]
    pub bezier: StyleMemorySlot,
}

impl StyleMemorySlot {
    pub fn is_empty(&self) -> bool {
        self.opacity.is_none() && self.stroke.is_none() && self.fill.is_none()
    }

    /// What a stroke tool may remember: opacity and a hard stroke that is
    /// at least `min_width` wide. Stroke tools draw no fill.
    #[must_use]
    pub fn for_stroke_tool(&self, min_width: f32) -> StyleMemorySlot {
        StyleMemorySlot {
            opacity: self.opacity,
            stroke: self.stroke.map(|s| {
                let mut s = s.hard_vector();
                if s.width.is_nan() || s.width < min_width {
                    s.width = min_width;
                }
                s
            }),
            fill: None,
        }
    }
}

impl CreateStyleMemory {
    /// Legacy single memory → closed slot plus seeded stroke tools. Stroke
    /// tool widths are never 0.
    pub fn from_legacy(legacy: StyleMemorySlot, min_open_stroke_width: f32) -> Self {
        let mut mem = Self {
            closed: legacy.clone(),
            legacy_open: legacy,
            ..Self::default()
        };
        mem.migrate_open_to_tools(min_open_stroke_width);
        mem
    }

    pub fn tool(&self, tool: StrokeTool) -> &StyleMemorySlot {
        match tool {
            StrokeTool::Pen => &self.pen,
            StrokeTool::Line => &self.line,
            StrokeTool::Arc => &self.arc,
            StrokeTool::Polyline => &self.polyline,
            StrokeTool::Bezier => &self.bezier,
        }
    }

    pub fn tool_mut(&mut self, tool: StrokeTool) -> &mut StyleMemorySlot {
        match tool {
            StrokeTool::Pen => &mut self.pen,
            StrokeTool::Line => &mut self.line,
            StrokeTool::Arc => &mut self.arc,
            StrokeTool::Polyline => &mut self.polyline,
            StrokeTool::Bezier => &mut self.bezier,
        }
    }

    /// Seed every stroke tool without its own memory from the old shared
    /// open-curve slot, then drop that slot. Seeds are hard strokes at
    /// least `min_width` wide.
    pub fn migrate_open_to_tools(&mut self, min_width: f32) {
        let open = std::mem::take(&mut self.legacy_open);
        if open.is_empty() {
            return;
        }
        let seed = open.for_stroke_tool(min_width);
        for tool in StrokeTool::ALL {
            let slot = self.tool_mut(tool);
            if slot.is_empty() {
                *slot = seed.clone();
            }
        }
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
        for tool in StrokeTool::ALL {
            assert_eq!(mem.tool(tool).stroke.unwrap().width, 2.0, "{tool:?}");
        }
        assert_eq!(mem.closed.fill, Some(Rgba::WHITE));
    }

    /// A workbook saved with the shared open-curve slot seeds every stroke
    /// tool from it once: never width 0, never soft, stamped, or blurred.
    #[test]
    fn saved_open_memory_seeds_each_stroke_tool_hard() {
        let json = r#"{
            "closed": { "opacity": 0.8 },
            "open": {
                "opacity": 0.5,
                "stroke": {
                    "width": 0.0, "color": [200, 10, 10, 255], "dash": "solid",
                    "softness": 0.6, "stamp": true, "gaussian_blur": 3.0
                }
            }
        }"#;
        let mut mem: CreateStyleMemory = serde_json::from_str(json).unwrap();
        mem.migrate_open_to_tools(2.0);
        for tool in StrokeTool::ALL {
            let slot = mem.tool(tool);
            let stroke = slot.stroke.unwrap();
            assert_eq!(stroke.width, 2.0, "{tool:?}");
            assert_eq!(stroke.color, Rgba([200, 10, 10, 255]), "{tool:?}");
            assert_eq!(stroke.softness, 0.0, "{tool:?}");
            assert!(!stroke.stamp, "{tool:?}");
            assert_eq!(stroke.gaussian_blur, 0.0, "{tool:?}");
            assert_eq!(slot.opacity, Some(0.5), "{tool:?}");
            assert_eq!(slot.fill, None, "{tool:?}");
        }
        assert_eq!(mem.closed.opacity, Some(0.8));
        let saved = serde_json::to_value(&mem).unwrap();
        assert!(
            saved.get("open").is_none(),
            "the shared slot is not written back"
        );
        let reread: CreateStyleMemory = serde_json::from_value(saved).unwrap();
        assert_eq!(reread, mem);
    }

    /// Once seeded, a tool keeps its own memory; the old slot never
    /// overwrites it again.
    #[test]
    fn migration_never_overwrites_a_tool_that_has_its_own_memory() {
        let mut mem = CreateStyleMemory::default();
        mem.tool_mut(StrokeTool::Line).stroke = Some(Stroke {
            width: 9.0,
            ..Stroke::default()
        });
        mem.legacy_open.stroke = Some(Stroke {
            width: 4.0,
            ..Stroke::default()
        });
        mem.migrate_open_to_tools(2.0);
        assert_eq!(mem.tool(StrokeTool::Line).stroke.unwrap().width, 9.0);
        assert_eq!(mem.tool(StrokeTool::Arc).stroke.unwrap().width, 4.0);
        assert!(mem.legacy_open.is_empty());
    }
}
