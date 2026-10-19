use super::*;

#[test]
fn witnessed_brush_eraser_keeps_its_preview_without_brush_path_panic() {
    let mut f = Fixture::new(-320.0);
    f.h.app
        .finish_freehand_brush(vec![egui::pos2(-200.0, 100.0), egui::pos2(200.0, 100.0)]);
    let target = f.h.app.doc().scene.nodes.last().unwrap().id;
    f.invoke(
        0,
        "board.eraser.stroke",
        &[target],
        json!({"points":[[-100,100],[100,100]],"radius":20}),
    );
    let mut saw_preview = false;
    for _ in 0..190 {
        let _ = f.step();
        let work = f.h.app.presence.agents[0].work.as_ref().unwrap();
        if let Some(preview) = work.previews.get(&target) {
            assert_ne!(preview, work.steps[0].0.as_ref().unwrap());
            saw_preview = true;
        }
        if !f.pending() {
            break;
        }
    }
    assert!(saw_preview, "the witnessed eraser preview remains visible");
}
