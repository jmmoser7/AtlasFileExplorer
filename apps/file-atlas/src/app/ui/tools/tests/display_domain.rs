//! Actual Display range-menu keyboard ownership through Atlas dispatch.
use super::*;

fn key(key: egui::Key) -> Vec<Event> {
    [true, false]
        .into_iter()
        .map(|pressed| Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        })
        .collect()
}

fn open_range(h: &mut Harness) {
    let shot = h.settle();
    let label = shot.text_rect("row spacing");
    let rail = shot
        .rects
        .iter()
        .filter(|r| {
            r.width() > 40.0
                && r.height() < 4.0
                && r.center().y < label.top()
                && label.top() - r.center().y < 20.0
        })
        .max_by(|a, b| a.center().y.total_cmp(&b.center().y))
        .expect("row spacing rail should precede its label")
        .center();
    h.frame(vec![Event::PointerMoved(rail)]);
    for pressed in [true, false] {
        h.frame(vec![Event::PointerButton {
            pos: rail,
            button: PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }]);
    }
    h.settle()
        .assert_labels(&["Slider range", "Minimum", "Maximum", "Apply"]);
    assert!(h.ctx.is_context_menu_open());
    // Exercise menu ownership itself, independent of a TextEdit focus guard.
    h.ctx.memory_mut(|memory| {
        if let Some(focused) = memory.focused() {
            memory.surrender_focus(focused);
        }
    });
    assert!(!h.ctx.wants_keyboard_input());
}

fn layout(h: &Harness) -> (usize, usize, usize, usize) {
    (
        h.app.grid_cols,
        h.app.portal_threshold,
        h.app.row_spacing,
        h.app.display_slider_domains[2].maximum(),
    )
}

#[test]
fn escape_from_a_display_range_menu_preserves_selection_then_normal_cancel_resumes() {
    let mut h = Harness::new(DockSide::LeftCenter, "display", &["*"]);
    h.app.selection.insert(0);
    h.app.pending_view = None;
    let before_layout = layout(&h);
    let camera = (h.app.cam.offset, h.app.cam.z);
    let history = h.app.cmd_history.len();
    open_range(&mut h);

    h.frame(key(egui::Key::Escape));
    assert!(!h.ctx.is_context_menu_open());
    assert_eq!(h.app.selection.len(), 1);
    assert!(h.app.selection.contains(&0));
    assert_eq!((h.app.cam.offset, h.app.cam.z), camera);
    assert_eq!(h.app.cmd_history.len(), history);
    assert_eq!(layout(&h), before_layout);
    assert!(h.app.pending_view.is_none());
    assert!(h.app.detail.is_none());

    h.frame(key(egui::Key::Escape));
    assert!(
        h.app.selection.is_empty(),
        "normal cancel resumes after close"
    );
}

#[test]
fn an_unfocused_display_range_menu_blocks_repeat_and_arrow_pan_until_closed() {
    let mut h = Harness::new(DockSide::LeftCenter, "display", &["*"]);
    h.app.selection.insert(0);
    h.app.root = h.app.entries[0].path.parent().map(|p| p.to_path_buf());
    h.app.pending_view = None;
    // A normal repeat opens the selected file's Details, without OS actions.
    h.app.push_history("app.properties", None);
    let history = h.app.cmd_history.len();
    let before_layout = layout(&h);
    let camera = (h.app.cam.offset, h.app.cam.z);
    open_range(&mut h);
    h.app.space_press = Some((std::time::Instant::now(), false));

    h.frame(key(egui::Key::Enter));
    let mut arrow = key(egui::Key::ArrowRight);
    let released = arrow.pop().unwrap();
    h.frame(arrow); // Held for this update so the normal pan path would run.
    h.frame(vec![released]);
    assert!(h.ctx.is_context_menu_open());
    assert!(h.app.space_press.is_none());
    assert!(
        h.app.detail.is_none(),
        "Enter must not repeat Details through the menu"
    );
    assert_eq!(h.app.cmd_history.len(), history);
    assert_eq!(h.app.selection.len(), 1);
    assert!(h.app.selection.contains(&0));
    assert_eq!((h.app.cam.offset, h.app.cam.z), camera);
    assert_eq!(layout(&h), before_layout);
    assert!(h.app.pending_view.is_none());

    h.frame(key(egui::Key::Escape));
    assert!(!h.ctx.is_context_menu_open());
    h.frame(key(egui::Key::Enter));
    assert_eq!(h.app.detail, Some(0), "normal repeat resumes after close");
    assert_eq!(h.app.cmd_history.len(), history + 1);
}
