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

/// Keep egui's first native label tooltip from registering a 400px ghost hit.
pub fn object_hover_text(response: egui::Response, label: &str) -> egui::Response {
    let ctx = response.ctx.clone();
    let tooltip_id = egui::containers::next_tooltip_id(&ctx, response.id);
    let unmeasured = egui::containers::AreaState::load(&ctx, tooltip_id)
        .and_then(|area| area.size)
        .is_none();
    if !response.enabled() || !response.hovered() || !unmeasured {
        return response.on_hover_text(label);
    }
    let original = ctx.style();
    let margin = egui::Frame::popup(&original).total_margin().sum();
    let width = original.spacing.tooltip_width.max(1.0);
    let galley = ctx.fonts(|fonts| {
        fonts.layout(
            label.to_owned(),
            egui::FontSelection::Default.resolve(&original),
            original.visuals.text_color(),
            width,
        )
    });
    let mut measured = (*original).clone();
    // Native tooltip placement/timing/content stay with egui. Its Area only
    // specifies default_width; bound the initial height before hit registration.
    measured.spacing.default_area_size.y = (galley.size().y + margin.y).ceil();
    ctx.set_style(measured);
    let response = response.on_hover_text(label);
    ctx.set_style(original);
    response
}
