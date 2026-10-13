//! Stable-identity command adapter for the image-layer menu and agent surface.
use super::*;
use slate_doc::image_paint::PaintLayerId;

#[derive(serde::Deserialize)]
struct LayerRequest {
    image: u64,
    layer: u64,
    #[serde(default)]
    name: String,
    before: Option<u64>,
}

impl SlateApp {
    pub(super) fn dispatch_image_layer(&mut self, command: &str, detail: Option<&str>) -> bool {
        let request = if let Some(detail) = detail {
            let Ok(request) = serde_json::from_str::<LayerRequest>(detail) else {
                return false;
            };
            request
        } else {
            if command == "board.image.layer.rename" {
                return false;
            }
            let Some(session) = self.image_paint.as_ref() else {
                return false;
            };
            let Some((layer, _)) = self
                .image_paint_layer_choices(session.image)
                .get(session.layer_index)
                .cloned()
            else {
                return false;
            };
            LayerRequest {
                image: session.image.0,
                layer,
                name: String::new(),
                before: None,
            }
        };
        let image = NodeId(request.image);
        let layer = PaintLayerId(request.layer);
        let before = request.before.map(PaintLayerId);
        match command {
            "board.image.layer.rename" => self.rename_paint_layer(image, layer, request.name),
            "board.image.layer.visibility" => self.toggle_paint_layer_visibility(image, layer),
            "board.image.layer.delete" => self.delete_paint_layer_by_id(image, layer),
            "board.image.layer.duplicate" => self
                .duplicate_paint_layer_before(image, layer, before)
                .is_some(),
            "board.image.layer.reorder" => self.reorder_paint_layer(image, layer, before),
            _ => false,
        }
    }
}
