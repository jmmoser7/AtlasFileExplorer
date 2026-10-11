//! Pump the frame loop, toasts, and workbook file copies.

use super::*;

pub(crate) fn place_linked(
    h: &mut Harness,
    name: &str,
    bytes: &[u8],
) -> (std::path::PathBuf, NodeId) {
    h.app.ensure_work_tab();
    h.app.leave_home();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    let source = h.base.join(name);
    std::fs::write(&source, bytes).unwrap();
    let ids = h.app.add_paths(std::slice::from_ref(&source));
    h.app.place_items_on_board(&ids, Pos2::new(120.0, 100.0));
    (source, h.app.doc().scene.nodes[0].id)
}

pub(crate) fn pump_until(h: &mut Harness, mut done: impl FnMut(&SlateApp) -> bool) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while std::time::Instant::now() < deadline {
        h.app.pump_document_jobs(&h.ctx);
        if done(&h.app) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    false
}

pub(crate) fn toast_text(app: &SlateApp) -> String {
    app.toasts
        .iter()
        .map(|(text, _)| text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn copy_dir(from: &std::path::Path, to: &std::path::Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let dest = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &dest);
        } else {
            std::fs::copy(entry.path(), dest).unwrap();
        }
    }
}

/// Text of every shape one frame paints, with the pointer at `pointer`.
pub(crate) fn painted(h: &mut Harness, pointer: Pos2) -> Vec<String> {
    fn walk(shape: &egui::Shape, out: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(t) => out.push(t.galley.text().to_string()),
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
            _ => {}
        }
    }
    let mut input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            Pos2::ZERO,
            egui::vec2(1440.0, 900.0),
        )),
        ..Default::default()
    };
    input.events.push(egui::Event::PointerMoved(pointer));
    let ctx = h.ctx.clone();
    let app = &mut h.app;
    let output = ctx.run(input, |c| app.update_app(c));
    let mut texts = Vec::new();
    for clipped in &output.shapes {
        walk(&clipped.shape, &mut texts);
    }
    texts
}
