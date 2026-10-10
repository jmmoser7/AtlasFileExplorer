//! Media page command keeps grid and Venn selection.

use super::*;

#[test]
fn media_page_command_preserves_grid_and_venn_item_selection() {
    let mut h = Harness::new("media_grid_page");
    h.app.ensure_work_tab();
    h.app.leave_home();
    let source = h.base.join("pages.pdf");
    std::fs::write(&source, b"page fixture").unwrap();
    let ids = h.app.add_paths(std::slice::from_ref(&source));
    h.app.place_items_on_board(&ids, Pos2::ZERO);
    h.app.documents.seed(
        source,
        pdf::documents::DocumentPreview {
            path: h.base.join("preview.pdf"),
            revision: "grid-pages".into(),
            pages: 3,
            bytes: 120,
        },
    );
    for (view, page) in [(ViewKind::Grid, 1), (ViewKind::Venn, 2)] {
        h.app.doc_mut().view.active_view = view;
        assert!(h.app.dispatch(
            &h.ctx,
            atlas_commands::CommandId("board.media.page"),
            Some(format!("{}:{page}", ids[0].0))
        ));
        assert_eq!(h.app.doc().item(ids[0]).unwrap().pdf_page, page);
        assert_eq!(h.app.doc().items.len(), 1);
    }
}
