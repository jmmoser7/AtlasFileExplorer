//! Keyboard behavior of the actual Chat Train AgentPicker's enabled buttons.
use super::*;

pub(super) fn primary(id: Id, index: usize) -> Id {
    id.with(("primary", index))
}

pub(super) fn clear(ctx: &egui::Context, id: Id) {
    ctx.data_mut(|data| data.remove::<Id>(id.with("keyboard-page")));
}

pub(super) fn scroll_requested(ctx: &egui::Context, id: Id, target: Id) -> bool {
    ctx.data_mut(|data| {
        let key = id.with("keyboard-scroll");
        if data.get_temp::<Id>(key) == Some(target) {
            data.remove::<Id>(key);
            true
        } else {
            false
        }
    })
}

fn targets(rows: &dyn SelectorRows, id: Id) -> impl Iterator<Item = (usize, bool, Id)> + '_ {
    (0..rows.len()).flat_map(move |index| {
        let row = rows.get(index).expect("row source length");
        [
            row.interactive
                .then_some((index, false, primary(id, index))),
            (row.interactive && row.secondary).then_some((
                index,
                true,
                id.with(("secondary", index)),
            )),
        ]
        .into_iter()
        .flatten()
    })
}

/// The Studio only installs Up/Down for Chat Train native-menu buttons.
/// Tab uses egui's real, separately registered primary and key-button targets.
pub(super) fn input(
    ui: &egui::Ui,
    id: Id,
    rows: &dyn SelectorRows,
    page: Id,
    opened: bool,
) -> Option<(usize, rows_paint::RowIntent)> {
    if ui.is_sizing_pass() || rows.presentation() != Presentation::ModelPicker {
        return None;
    }
    let reset = ui.ctx().data_mut(|data| {
        let key = id.with("keyboard-page");
        let reset = opened || data.get_temp::<Id>(key) != Some(page);
        data.insert_temp(key, page);
        reset
    });
    let focused = ui.memory(|memory| memory.focused());
    if reset {
        if let Some((_, _, target)) = targets(rows, id).next() {
            ui.memory_mut(|memory| memory.request_focus(target));
        }
        // Opening with Enter/Space must never also choose the first row.
        ui.input_mut(|input| {
            input.consume_key(egui::Modifiers::NONE, egui::Key::Enter);
            input.consume_key(egui::Modifiers::NONE, egui::Key::Space);
        });
        return None;
    }
    let current = targets(rows, id).position(|(_, _, target)| Some(target) == focused)?;
    ui.memory_mut(|memory| {
        memory.set_focus_lock_filter(
            focused.expect("enabled target"),
            egui::EventFilter {
                vertical_arrows: true,
                horizontal_arrows: true,
                escape: true,
                ..Default::default()
            },
        );
    });
    let direction = ui.input_mut(|input| {
        input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft);
        input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight);
        if input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
            Some(1)
        } else if input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
            Some(-1)
        } else {
            None
        }
    });
    if let Some(direction) = direction.filter(|_| rows.cyclic_keyboard()) {
        let count = targets(rows, id).count();
        let next = (current as isize + direction).rem_euclid(count as isize) as usize;
        let (_, _, target) = targets(rows, id).nth(next).expect("enabled focus target");
        ui.memory_mut(|memory| memory.request_focus(target));
        ui.ctx()
            .data_mut(|data| data.insert_temp(id.with("keyboard-scroll"), target));
        return None;
    }
    let activate = ui.input_mut(|input| {
        let enter = input.consume_key(egui::Modifiers::NONE, egui::Key::Enter);
        let space = input.consume_key(egui::Modifiers::NONE, egui::Key::Space);
        enter || space
    });
    if activate {
        let (index, secondary, _) = targets(rows, id).nth(current)?;
        return Some((
            index,
            if secondary {
                rows_paint::RowIntent::Secondary
            } else {
                rows_paint::RowIntent::Primary
            },
        ));
    }
    None
}
