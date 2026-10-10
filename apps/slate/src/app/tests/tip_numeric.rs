//! User item "ffor alt ctrl and shift hud allow for fast click release"
//! (28 September 2026): an Alt, Ctrl or Shift + right press released without
//! travel opens that tip HUD's numeric entry. Driven by real frames.

use super::*;
use board::BoardTool;
use eframe::egui::{Key, Modifiers, PointerButton};

const ALT: Modifiers = Modifiers {
    alt: true,
    ctrl: false,
    shift: false,
    mac_cmd: false,
    command: false,
};
const CTRL: Modifiers = Modifiers {
    alt: false,
    ctrl: true,
    shift: false,
    mac_cmd: false,
    command: true,
};
const SHIFT: Modifiers = Modifiers {
    alt: false,
    ctrl: false,
    shift: true,
    mac_cmd: false,
    command: false,
};

fn board(tag: &str, tool: BoardTool) -> Harness {
    let mut h = Harness::new(tag);
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h.app.tab_mut().cam.z = 1.0;
    h.app.board_osnap.enabled = false;
    h.app.board_smart_guides = false;
    h.app.board_snap_grid = false;
    h.app.board_ortho = false;
    h.frame();
    h.frame();
    h.app.set_board_tool(tool);
    h.frame();
    h
}

/// One frame, 100 ms after the last.
fn events(h: &mut Harness, mods: Modifiers, events: Vec<egui::Event>) {
    let t = h.ctx.input(|i| i.time) + 0.1;
    h.frame_with(|i| {
        i.time = Some(t);
        i.modifiers = mods;
        i.events = events;
    });
}

fn button(pos: Pos2, button: PointerButton, pressed: bool, m: Modifiers) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button,
        pressed,
        modifiers: m,
    }
}

fn screen(h: &Harness, world: Pos2) -> Pos2 {
    h.app.board_xf().w2s(world)
}

fn click(h: &mut Harness, world: Pos2) {
    let p = screen(h, world);
    let left = PointerButton::Primary;
    events(h, Modifiers::NONE, vec![egui::Event::PointerMoved(p)]);
    events(
        h,
        Modifiers::NONE,
        vec![
            button(p, left, true, Modifiers::NONE),
            button(p, left, false, Modifiers::NONE),
        ],
    );
}

/// Modifier + right press and release at `at` (screen), no travel; then
/// the modifier comes up.
fn quick_click(h: &mut Harness, m: Modifiers, at: Pos2) {
    let right = PointerButton::Secondary;
    events(h, m, vec![egui::Event::PointerMoved(at)]);
    events(h, m, vec![button(at, right, true, m)]);
    events(h, m, vec![button(at, right, false, m)]);
    events(h, Modifiers::NONE, vec![]);
}

/// Modifier + right-drag from `at` by `offsets`, then release.
fn drag(h: &mut Harness, m: Modifiers, at: Pos2, offsets: &[EVec2]) {
    let right = PointerButton::Secondary;
    events(h, m, vec![egui::Event::PointerMoved(at)]);
    events(h, m, vec![button(at, right, true, m)]);
    let mut last = at;
    for o in offsets {
        last = at + *o;
        events(h, m, vec![egui::Event::PointerMoved(last)]);
    }
    events(h, m, vec![button(last, right, false, m)]);
    events(h, Modifiers::NONE, vec![]);
}

fn type_text(h: &mut Harness, text: &str) {
    events(h, Modifiers::NONE, vec![egui::Event::Text(text.into())]);
}

fn key_with(h: &mut Harness, key: Key, m: Modifiers) {
    events(
        h,
        m,
        vec![egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: m,
        }],
    );
}

fn key(h: &mut Harness, key: Key) {
    key_with(h, key, Modifiers::NONE);
}

fn assert_no_menu_no_pan(h: &Harness, offset: EVec2) {
    assert!(h.app.board_menu.is_none(), "no node context menu");
    assert!(h.app.board_empty_menu.is_none(), "no board context menu");
    assert_eq!(h.app.tab().cam.offset, offset, "no pan");
}

fn undo_depth(h: &Harness) -> usize {
    h.app.tab().journal.undo_depth()
}

#[test]
fn a_quick_alt_right_click_types_the_brush_size_and_esc_restores() {
    let mut h = board("numeric_brush", BoardTool::Brush);
    h.app.brush_width = 8.0;
    h.app.brush_softness = 0.25;
    h.app.brush_opacity = 1.0;
    let at = Pos2::new(700.0, 450.0);
    let offset = h.app.tab().cam.offset;
    let undo = h.app.brush_setting_undo.len();

    quick_click(&mut h, ALT, at);
    assert!(
        matches!(h.app.brush_hud, Some(board_color::BrushHud::Size { .. })),
        "a quick Alt+right click keeps the size HUD open for typing, got {:?}",
        h.app.brush_hud
    );
    assert_no_menu_no_pan(&h, offset);
    type_text(&mut h, "30");
    assert!(
        !h.app.palette_state.open,
        "digits go to the HUD, not type-to-command"
    );
    assert_eq!(h.app.board_tool, BoardTool::Brush);
    key(&mut h, Key::Enter);
    assert!(h.app.brush_hud.is_none(), "Enter closes the HUD");
    assert_eq!(h.app.brush_width, 30.0, "the typed size, exactly");
    assert_eq!(h.app.brush_softness, 0.25, "softness kept");
    assert_eq!(h.app.brush_setting_undo.len(), undo + 1, "one undo step");
    assert_no_menu_no_pan(&h, offset);
    key_with(&mut h, Key::Z, CTRL);
    assert_eq!(h.app.brush_width, 8.0, "Ctrl+Z restores the size");

    // Tab applies the size live and moves to softness; Esc restores both.
    quick_click(&mut h, ALT, at);
    type_text(&mut h, "50");
    key(&mut h, Key::Tab);
    assert_eq!(h.app.brush_width, 50.0, "Tab applies the field it leaves");
    type_text(&mut h, "80");
    key(&mut h, Key::Escape);
    assert!(h.app.brush_hud.is_none(), "Esc closes the HUD");
    assert_eq!(h.app.brush_width, 8.0, "Esc restores the size");
    assert_eq!(h.app.brush_softness, 0.25, "and the softness");
    assert_eq!(
        h.app.board_tool,
        BoardTool::Brush,
        "Esc closes the HUD first"
    );
    assert_eq!(h.app.brush_setting_undo.len(), undo, "Esc journals nothing");

    // Shift: opacity in percent.
    quick_click(&mut h, SHIFT, at);
    assert!(matches!(
        h.app.brush_hud,
        Some(board_color::BrushHud::Opacity { .. })
    ));
    type_text(&mut h, "40");
    key(&mut h, Key::Enter);
    assert!(
        (h.app.brush_opacity - 0.4).abs() < 1e-6,
        "{}",
        h.app.brush_opacity
    );

    // Ctrl: hue, saturation, value; the hue field comes first.
    h.app.board_colors.fg = slate_doc::scene::Rgba([255, 0, 0, 255]);
    quick_click(&mut h, CTRL, at);
    assert!(matches!(
        h.app.brush_hud,
        Some(board_color::BrushHud::Wheel { .. })
    ));
    type_text(&mut h, "120");
    key(&mut h, Key::Enter);
    assert_eq!(h.app.board_colors.fg.0, [0, 255, 0, 255]);
    assert_no_menu_no_pan(&h, offset);

    // A real right-drag still scrubs from the first pixel and closes.
    let w0 = h.app.brush_width;
    drag(
        &mut h,
        ALT,
        at,
        &[EVec2::new(20.0, 0.0), EVec2::new(40.0, 0.0)],
    );
    assert!(h.app.brush_hud.is_none(), "a drag closes on release");
    assert!(h.app.brush_width > w0 + 40.0, "{}", h.app.brush_width);
    assert_no_menu_no_pan(&h, offset);
}

#[test]
fn a_quick_alt_right_click_types_a_lines_width_as_one_undo_step() {
    let mut h = board("numeric_line", BoardTool::Line);
    click(&mut h, Pos2::new(0.0, 0.0));
    click(&mut h, Pos2::new(200.0, 0.0));
    assert_eq!(h.app.doc().scene.nodes.len(), 1, "one committed line");
    let id = h.app.doc().scene.nodes[0].id;
    let width = |h: &Harness| match &h.app.doc().scene.node(id).unwrap().kind {
        slate_doc::NodeKind::Shape(s) => {
            slate_doc::vertex_style::curve_tip(s.path.as_deref(), &s.stroke).width
        }
        _ => panic!("a shape"),
    };
    let w0 = width(&h);
    h.app.set_board_tool(BoardTool::DirectSelect);
    h.frame();
    click(&mut h, Pos2::new(100.0, 0.0));
    assert_eq!(h.app.direct.node, Some(id), "the click targets the line");
    let depth = undo_depth(&h);
    let offset = h.app.tab().cam.offset;
    let away = screen(&h, Pos2::new(100.0, 200.0));

    quick_click(&mut h, ALT, away);
    assert!(h.app.brush_hud.is_some(), "numeric entry on the line");
    assert_no_menu_no_pan(&h, offset);
    type_text(&mut h, "12");
    key(&mut h, Key::Enter);
    assert!(h.app.brush_hud.is_none());
    assert_eq!(width(&h), 12.0, "the typed width, exactly");
    assert_eq!(undo_depth(&h), depth + 1, "one undo step");
    key_with(&mut h, Key::Z, CTRL);
    assert_eq!(width(&h), w0, "one Ctrl+Z restores the width");

    // Esc restores the line and journals nothing.
    quick_click(&mut h, ALT, away);
    type_text(&mut h, "20");
    key(&mut h, Key::Tab);
    key(&mut h, Key::Escape);
    assert_eq!(width(&h), w0, "Esc restores");
    assert_eq!(undo_depth(&h), depth, "Esc journals nothing");

    // A real right-drag still scrubs the line.
    drag(
        &mut h,
        ALT,
        away,
        &[EVec2::new(20.0, 0.0), EVec2::new(40.0, 0.0)],
    );
    assert!(h.app.brush_hud.is_none());
    assert!(width(&h) > w0 + 40.0, "{}", width(&h));
    assert_eq!(undo_depth(&h), depth + 1);
    assert_no_menu_no_pan(&h, offset);
}

#[test]
fn a_quick_click_on_a_line_tool_sets_its_create_width_and_the_eraser_has_no_color_entry() {
    let mut h = board("numeric_line_tool", BoardTool::Line);
    click(&mut h, Pos2::new(0.0, 0.0));
    assert!(h.app.line_draft.is_some(), "mid-draft (tr2)");
    let at = Pos2::new(700.0, 450.0);
    quick_click(&mut h, ALT, at);
    type_text(&mut h, "9");
    key(&mut h, Key::Enter);
    assert_eq!(h.app.stroke_for_new_curve().width, 9.0);
    assert!(
        h.app.line_draft.is_some(),
        "Enter went to the HUD, not the draft"
    );

    // A click on the canvas away from the panel commits it and is eaten.
    quick_click(&mut h, ALT, at);
    type_text(&mut h, "14");
    let p = Pos2::new(200.0, 200.0);
    events(&mut h, Modifiers::NONE, vec![egui::Event::PointerMoved(p)]);
    events(
        &mut h,
        Modifiers::NONE,
        vec![
            button(p, PointerButton::Primary, true, Modifiers::NONE),
            button(p, PointerButton::Primary, false, Modifiers::NONE),
        ],
    );
    events(&mut h, Modifiers::NONE, vec![]);
    assert!(h.app.brush_hud.is_none(), "a click away closes the HUD");
    assert_eq!(h.app.stroke_for_new_curve().width, 14.0, "and commits it");
    assert!(
        h.app.doc().scene.nodes.is_empty(),
        "the dismissing click places no point"
    );
    assert!(h.app.line_draft.is_some(), "and leaves the draft alone");

    let mut h = board("numeric_eraser", BoardTool::Eraser);
    quick_click(&mut h, CTRL, at);
    assert!(
        h.app.brush_hud.is_none(),
        "the eraser has no color wheel (tx4)"
    );
    type_text(&mut h, "5");
    assert!(h.app.brush_hud.is_none());
}

/// Review round 8: a quick Alt+right click away from the grips of a
/// curve selected with the Select tool (nothing picked or hovered) types
/// the whole curve's size. The target freezes at the press, so the
/// typed size scales every tip by one factor, in one undo step.
#[test]
fn a_quick_alt_right_click_types_a_whole_selected_curves_size() {
    let (mut h, id, _, away) = whole_curve_board("numeric_select_whole");
    let before = painted_vertex_tips(&h, id);
    assert_eq!(before[1].0, 8.0, "the widest tip is the curve's size");
    let depth = undo_depth(&h);
    let offset = h.app.tab().cam.offset;

    quick_click(&mut h, ALT, away);
    assert!(
        matches!(h.app.brush_hud, Some(board_color::BrushHud::Size { .. })),
        "numeric entry on the whole curve, got {:?}",
        h.app.brush_hud
    );
    assert_no_menu_no_pan(&h, offset);
    type_text(&mut h, "16");
    key(&mut h, Key::Enter);
    assert!(h.app.brush_hud.is_none(), "Enter closes the HUD");
    let tips = painted_vertex_tips(&h, id);
    for (k, (tip, was)) in tips.iter().zip(&before).enumerate() {
        assert!(
            (tip.0 - was.0 * 2.0).abs() < 1e-3,
            "vertex {k} doubles with the curve: {tips:?}"
        );
        assert_eq!(tip.1, was.1, "vertex {k} keeps its color");
    }
    assert!(h.app.board_sel.contains(&id), "the curve stays selected");
    assert_eq!(h.app.picked_vertices(), None, "nothing got picked");
    assert_eq!(undo_depth(&h), depth + 1, "one undo step");
    key_with(&mut h, Key::Z, CTRL);
    assert_eq!(painted_vertex_tips(&h, id), before, "one Ctrl+Z restores");
}
