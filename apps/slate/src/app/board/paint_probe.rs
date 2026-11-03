//! Opt-in test witness of the transform/selection actually passed to painting.
use super::BoardXf;
use slate_doc::NodeId;
use std::cell::{Cell, RefCell};
use std::collections::HashSet;

pub(crate) struct Painted {
    pub xf: BoardXf,
    pub selected: Vec<NodeId>,
}
thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    static LAST: RefCell<Option<Painted>> = const { RefCell::new(None) };
}
pub(crate) fn enable(enabled: bool) {
    ENABLED.with(|flag| flag.set(enabled));
    reset();
}
pub(crate) fn reset() {
    LAST.with(|last| last.borrow_mut().take());
}
pub(crate) fn take() -> Option<Painted> {
    LAST.with(|last| last.borrow_mut().take())
}
pub(super) fn record(xf: &BoardXf, selected: &HashSet<NodeId>) {
    if ENABLED.with(Cell::get) {
        LAST.with(|last| {
            *last.borrow_mut() = Some(Painted {
                xf: *xf,
                selected: selected.iter().copied().collect(),
            });
        });
    }
}
