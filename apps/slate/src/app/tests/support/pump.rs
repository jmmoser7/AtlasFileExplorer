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
