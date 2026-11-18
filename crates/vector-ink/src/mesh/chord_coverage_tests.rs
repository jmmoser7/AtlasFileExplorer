#[test]
fn compact_uniform_chords_keep_dense_feather_coverage_through_caps_and_turns() {
    use crate::{Cap, Join, StrokeStyle};
    let points = [[0., 0.], [300., 0.], [300., 13.], [100., 42.]];
    let style = StrokeStyle {
        width: 4.0,
        cap: Cap::Round,
        join: Join::Round,
        taper: None,
        dash: None,
    };
    let build = |widths: Option<&[f32]>| {
        let mut mesh = InkMesh::default();
        crate::mesh::tessellate_run(
            &mut mesh,
            &points,
            widths,
            None,
            &style,
            [Cap::Round; 2],
            4.,
            false,
            0.15,
        );
        mesh
    };
    // Explicit per-point widths retain all stations in the dense reference.
    let dense = build(Some(&[4.; 4]));
    let compact = build(None);
    assert!(compact.vertices.len() * 5 < dense.vertices.len());
    for x in (-5..307).step_by(3) {
        for y in -5..47 {
            let point = [x as f32 + 0.31, y as f32 + 0.27];
            let alpha = |m| sample(m, point).map_or(0., |p| p.0);
            assert!(
                (alpha(&dense) - alpha(&compact)).abs() < 0.0005,
                "coverage at {point:?}"
            );
        }
    }
}
