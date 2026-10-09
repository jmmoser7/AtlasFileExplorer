//! A bound File Atlas lens and a point outside it.

use super::*;

pub(crate) fn atlas_screen_outside(h: &Harness, id: slate_doc::NodeId) -> Pos2 {
    let xf = h.app.board_xf();
    let node = h.app.doc().scene.node(id).unwrap();
    let frame = xf.rect_w2s(node.rect);
    if frame.left() > 24.0 {
        Pos2::new(frame.left() - 12.0, frame.center().y)
    } else {
        Pos2::new(frame.right() + 12.0, frame.center().y)
    }
}

pub(crate) fn bound_atlas_lens(tag: &str) -> (Harness, slate_doc::NodeId, PathBuf) {
    let mut h = kit_board(tag, board::BoardTool::Select);
    let folder = h.base.join("shots");
    std::fs::create_dir_all(&folder).unwrap();
    let file = folder.join("a.png");
    std::fs::write(&file, [0u8; 8]).unwrap();
    h.app.apply_folder_drop(
        super::super::super::board_atlas::FolderDropKind::AtlasLens,
        folder,
        Pos2::ZERO,
    );
    let id = h.app.doc().scene.nodes[0].id;
    for _ in 0..24 {
        h.frame();
        if h.app.atlas_file_count(id) > 0 {
            break;
        }
    }
    (h, id, file)
}

// ---------- Tool arming preview (contracts/tool-arming.md GP1–GP6) ----------
