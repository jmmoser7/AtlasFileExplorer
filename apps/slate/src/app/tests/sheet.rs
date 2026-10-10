//! Sheet tests.

use super::*;

#[test]
fn sheet_enter_keeps_the_typed_number_until_save() {
    let mut h = Harness::new("sheet_enter");
    let path = h.base.join("rows.csv");
    std::fs::write(&path, "A,B\n1,2\n").unwrap();
    let item = h
        .app
        .doc_mut()
        .add_item(path.clone(), "rows.csv", 8, 0, "rows");
    h.app
        .sheets
        .insert(item, atlas_core::table::read_sheet_card(&path));
    h.app.sheet_edit = Some(board::SheetEdit {
        node: NodeId(1),
        item,
        row: 1,
        col: 1,
        buf: "9".into(),
        origin: "2".into(),
        fresh: false,
        screen: egui::Rect::NOTHING,
        font_px: 12.0,
    });
    h.app.commit_sheet_edit();
    let shown = h
        .app
        .sheets
        .get(&item)
        .and_then(|g| g.as_ref())
        .and_then(|rows| rows.get(1))
        .and_then(|row| row.get(1))
        .map(|cell| cell.text.as_str());
    assert_eq!(shown, Some("9"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "A,B\n1,2\n");
    assert!(h.app.sheet_dirty);
}
