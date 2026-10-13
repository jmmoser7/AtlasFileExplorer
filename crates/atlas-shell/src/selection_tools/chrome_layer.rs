//! One layer owner for object chrome painting and its egui hit registration.
use super::*;

pub fn selection_chrome_layer() -> egui::LayerId {
    egui::LayerId::new(CANVAS_CHROME_ORDER, Id::new("selection_property_chrome"))
}

fn object_layer(ui: &egui::Ui) -> egui::LayerId {
    if ui.layer_id().order >= POPUP_ORDER {
        ui.layer_id()
    } else {
        selection_chrome_layer()
    }
}

/// Object controls interact on the same layer their visible chrome uses.
pub fn object_ui(ui: &mut egui::Ui, id: Id, rect: Rect) -> egui::Ui {
    let layer = object_layer(ui);
    ui.new_child(
        egui::UiBuilder::new()
            .id_salt(id)
            .layer_id(layer)
            .max_rect(rect),
    )
}

/// Keep popup controls on their own layer; board controls use canvas chrome.
pub fn object_painter(ui: &egui::Ui) -> egui::Painter {
    ui.painter().clone().with_layer_id(object_layer(ui))
}
