use super::*;

fn compare(cached: &CachedInkMesh, base: Color32, opacity: f32, z: f32) {
    let xf = BoardXf {
        center: Pos2::new(311.5, 197.25),
        offset: Vec2::new(-19.75, 42.5),
        z,
    };
    legacy_mesh_colors_for_test(true);
    let previous = ink_mesh_to_epaint(cached, &xf, base, |c| c.gamma_multiply(opacity));
    legacy_mesh_colors_for_test(false);
    let current = ink_mesh_to_epaint(cached, &xf, base, |c| c.gamma_multiply(opacity));
    assert_eq!(
        current, previous,
        "opacity={opacity}, zoom={z}, base={base:?}"
    );
}

#[test]
fn endpoint_reuse_preserves_every_channel_geometry_and_opacity_rounding() {
    // Include additive premultiplied colors, fractional AA and near-endpoints.
    let alphas = vec![0.0, -0.0, 1.0, 0.37, 0.5, 0.999_999_94, 1.000_000_1];
    let mut cached = CachedInkMesh {
        vertices: (0..alphas.len())
            .map(|i| [i as f32 * 1.25, -3.75])
            .collect(),
        alphas,
        indices: vec![0, 1, 2, 2, 3, 4, 4, 5, 6],
        colors: vec![],
    };
    for channel in 0..=255 {
        let base = Color32::from_rgba_premultiplied(channel, 255 - channel, channel / 2, channel);
        for opacity in [0.0, 0.1, 0.5, 0.999, 1.0] {
            for z in [0.125, 1.0, 2.0] {
                compare(&cached, base, opacity, z);
                cached.colors = vec![base; cached.vertices.len()];
                compare(&cached, Color32::WHITE, opacity, z);
                cached.colors.clear();
            }
        }
    }
}

#[test]
fn distinct_per_vertex_tints_and_empty_mesh_remain_exact() {
    let cached = CachedInkMesh {
        vertices: vec![[1.0, 2.0], [-4.0, 8.0], [0.0, 0.0]],
        alphas: vec![0.0, 1.0, 0.37],
        indices: vec![0, 1, 2],
        colors: vec![
            Color32::RED,
            Color32::from_rgba_premultiplied(9, 127, 255, 0),
            Color32::BLUE,
        ],
    };
    for opacity in [0.0, 0.1, 0.5, 1.0] {
        compare(&cached, Color32::GREEN, opacity, 0.5);
        compare(&CachedInkMesh::default(), Color32::GREEN, opacity, 2.0);
    }
}
