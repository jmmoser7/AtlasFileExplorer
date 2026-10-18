//! Linked media nodes and their authored presentation.

use super::*;

/// Never pixels — the pool item owns the file link.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageNode {
    pub item: ItemId,
    /// Authored words shown on a linked text card; the source file stays intact.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_override: Option<String>,
    /// Column and row sizes when this card shows a spreadsheet.
    #[serde(default, skip_serializing_if = "SheetLayout::is_empty")]
    pub sheet: SheetLayout,
    #[serde(default = "Crop::full")]
    pub crop: Crop,
    #[serde(default)]
    pub corner: Corner,
    #[serde(default)]
    pub stroke: Stroke,
    #[serde(default)]
    pub adjust: ImageAdjust,
    /// Playback settings when the linked item is a video; default otherwise.
    #[serde(default)]
    pub video: VideoOpts,
    /// Saved viewport pose when the linked item is a 3D model; default
    /// otherwise.
    #[serde(default)]
    pub model: ModelCamera,
    /// The agent generating this picture. Its results are the card's album;
    /// `item` is the result a person picked, and [`ItemId::NONE`] shows the
    /// newest (see [`crate::agent_inputs::newest_image`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<Box<AgentPortalRef>>,
    /// Trace-paper overlays (brush, pen, shapes, text) clipped to this image.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paint_layers: Vec<crate::image_paint::PaintLayer>,
    /// Explicit paint-layer stacks keyed to stable album result identities.
    /// Empty on older documents, whose layers remain in `paint_layers`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub result_layers: Vec<crate::image_paint::ImageResultLayers>,
    /// Pixels mirrored left to right inside the crop window. `crop` and the
    /// paint layers are stored as displayed; see [`crate::mirror`].
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip_x: bool,
    /// Pixels mirrored top to bottom inside the crop window.
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip_y: bool,
}

impl ImageNode {
    /// A freshly-placed item: full crop, square corners, no stroke, identity
    /// adjust, default playback.
    pub fn new(item: ItemId) -> ImageNode {
        ImageNode {
            item,
            text_override: None,
            sheet: SheetLayout::default(),
            crop: Crop::full(),
            corner: Corner::Square,
            stroke: Stroke::none(),
            adjust: ImageAdjust::default(),
            video: VideoOpts::default(),
            model: ModelCamera::default(),
            agent: None,
            paint_layers: Vec::new(),
            result_layers: Vec::new(),
            flip_x: false,
            flip_y: false,
        }
    }

    pub fn mirror(&self) -> Mirror {
        Mirror {
            x: self.flip_x,
            y: self.flip_y,
        }
    }

    /// A picture an agent will generate; it shows the newest result until a
    /// person picks one.
    pub fn generated(agent: AgentPortalRef) -> ImageNode {
        ImageNode {
            agent: Some(Box::new(agent)),
            ..ImageNode::new(ItemId::NONE)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_override_is_optional_and_round_trips_authored_empty_text() {
        let original = ImageNode::new(ItemId::NONE);
        let encoded = serde_json::to_string(&original).unwrap();
        assert!(!encoded.contains("text_override"));
        assert_eq!(
            serde_json::from_str::<ImageNode>(&encoded).unwrap(),
            original
        );
        let mut edited = original;
        edited.text_override = Some(String::new());
        let encoded = serde_json::to_string(&edited).unwrap();
        assert_eq!(serde_json::from_str::<ImageNode>(&encoded).unwrap(), edited);
    }
}
