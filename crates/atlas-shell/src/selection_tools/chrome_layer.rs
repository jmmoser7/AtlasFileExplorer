//! One layer owner for object chrome painting and its egui hit registration.
use super::*;

#[cfg(test)]
mod tests;

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
    let hit_rect = rect.intersect(ui.clip_rect());
    if layer != ui.layer_id() && hit_rect.is_positive() {
        // A child Ui registers widgets, but not the Area consulted by egui's
        // layer ownership. Bound that Area to the strip, behind its controls.
        egui::Area::new(layer.id)
            .order(layer.order)
            .fixed_pos(hit_rect.min)
            .default_size(hit_rect.size())
            .constrain(false)
            .fade_in(false)
            .enabled(ui.is_enabled())
            .interactable(ui.is_visible())
            .sense(Sense::hover())
            .show(ui.ctx(), |area| area.set_min_size(hit_rect.size()));
    }
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
