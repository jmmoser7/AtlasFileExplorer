use super::*;

#[test]
fn batched_ink_keeps_the_same_vertices_indices_and_triangle_order() {
    let xf = BoardXf {
        center: Pos2::new(311.5, 197.25),
        offset: Vec2::new(-19.75, 42.5),
        z: 0.03,
    };
    let cached = CachedInkMesh {
        vertices: vec![[1.0, 2.0], [-4.0, 8.0], [0.0, 0.0]],
        alphas: vec![0.0, 1.0, 0.37],
        indices: vec![0, 1, 2],
        colors: vec![],
        ..Default::default()
    };
    let mut previous = egui::Mesh::default();
    let mut batched = egui::Mesh::default();
    for base in [Color32::RED, Color32::BLUE, Color32::TRANSPARENT] {
        previous.append(ink_mesh_to_epaint(&cached, &xf, base, |c| c));
        append_ink_mesh_to_epaint(&mut batched, &cached, &xf, base, |c| c);
    }
    assert_eq!(batched, previous);
}

#[test]
fn dense_wire_reservation_preserves_geometry_and_leaves_one_wire_small() {
    let xf = BoardXf {
        center: Pos2::new(311.5, 197.25),
        offset: Vec2::new(-19.75, 42.5),
        z: 0.4,
    };
    let cached = CachedInkMesh {
        vertices: (0..1200).map(|i| [i as f32, (i % 4) as f32]).collect(),
        encoding: InkEncoding::Strip,
        ..Default::default()
    };
    let limit = 4096;
    let mut expected = egui::Mesh::default();
    let mut actual = egui::Mesh::default();
    for color in [Color32::RED, Color32::GREEN] {
        append_ink_mesh_to_epaint(&mut expected, &cached, &xf, color, |c| c);
        cached.append_to_wire_batch(&mut actual, &xf, color, limit);
        assert_eq!(actual, expected);
        if color == Color32::RED {
            assert_eq!(actual.vertices.capacity(), expected.vertices.capacity());
            assert_eq!(actual.indices.capacity(), expected.indices.capacity());
        }
    }
    assert!(actual.vertices.capacity() >= limit);
    assert!(actual.indices.capacity() >= limit * 18 / 4);
}

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
        ..Default::default()
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
        ..Default::default()
    };
    for opacity in [0.0, 0.1, 0.5, 1.0] {
        compare(&cached, Color32::GREEN, opacity, 0.5);
        compare(&CachedInkMesh::default(), Color32::GREEN, opacity, 2.0);
    }
}

#[test]
fn compact_wire_strip_preserves_all_native_vertices_and_triangles() {
    let mut path = vector_ink::kurbo::BezPath::new();
    path.move_to((100.5, 10.25));
    path.curve_to((272.0, 10.25), (265.0, 240.5), (433.0, 240.5));
    let style = vector_ink::StrokeStyle {
        width: 4.0,
        cap: vector_ink::Cap::Round,
        join: vector_ink::Join::Round,
        taper: None,
        dash: None,
    };
    let ink = vector_ink::stroke_mesh(&path, &style, 0.75, 2.4);
    let regular = CachedInkMesh::from(ink.clone());
    let compact = CachedInkMesh::from_wire(ink.clone());
    assert_eq!(compact.encoding, InkEncoding::Strip);
    assert!(compact.indices.is_empty() && compact.alphas.is_empty());
    assert_eq!(compact.vertices, regular.vertices);
    for z in [0.03, 0.125, 0.375] {
        let xf = BoardXf {
            center: Pos2::new(311.5, 197.25),
            offset: Vec2::new(-19.75, 42.5),
            z,
        };
        let mut previous = egui::Mesh::default();
        let mut batched = egui::Mesh::default();
        for color in [
            Color32::RED,
            Color32::BLUE.gamma_multiply(0.4),
            Color32::TRANSPARENT,
        ] {
            previous.append(ink_mesh_to_epaint(&regular, &xf, color, |c| c));
            append_ink_mesh_to_epaint(&mut batched, &compact, &xf, color, |c| c);
        }
        assert_eq!(batched, previous);
    }
    let mut changed_alpha = ink.clone();
    changed_alpha.vertices[0].alpha = 0.5;
    assert_eq!(
        CachedInkMesh::from_wire(changed_alpha).encoding,
        InkEncoding::Explicit
    );
    let mut changed_order = ink;
    changed_order.indices.swap(0, 1);
    assert_eq!(
        CachedInkMesh::from_wire(changed_order).encoding,
        InkEncoding::Explicit
    );
}

#[test]
fn coincident_inner_strip_drops_only_degenerate_triangles_in_a_mixed_batch() {
    use egui::epaint::Vertex;

    let mut ink = InkMesh::default();
    for [x, y] in [[0.0, 1.0], [4.0, 1.75], [9.0, 2.0]] {
        ink.vertices.extend(
            [[x, y - 0.5], [x, y], [x, y], [x, y + 0.5]]
                .into_iter()
                .zip(vector_ink::AA_STRIP_ALPHAS)
                .map(|(pos, alpha)| vector_ink::InkVertex { pos, alpha }),
        );
    }
    ink.indices
        .extend(vector_ink::aa_strip_indices(ink.vertices.len()));
    let original = CachedInkMesh::from(ink.clone());
    let collapsed = CachedInkMesh::from_wire(ink.clone());
    assert_eq!(collapsed.encoding, InkEncoding::CoincidentInnerStrip);
    assert_eq!(collapsed.vertex_count(), 9);
    assert_eq!(collapsed.index_count(), 24);
    assert!(collapsed.alphas.is_empty() && collapsed.indices.is_empty());

    let mut mismatch = ink.clone();
    mismatch.vertices[6].pos[1] += 0.25;
    let original_mismatch = CachedInkMesh::from(mismatch.clone());
    let fallback = CachedInkMesh::from_wire(mismatch);
    assert_eq!(fallback.encoding, InkEncoding::Strip);
    assert_eq!(fallback.vertex_count(), 12);
    assert_eq!(fallback.index_count(), 36);

    let mut sign_mismatch = ink.clone();
    sign_mismatch.vertices[2].pos[0] = -0.0;
    assert_eq!(
        CachedInkMesh::from_wire(sign_mismatch).encoding,
        InkEncoding::Strip
    );
    let mut tinted = ink;
    tinted
        .colors
        .resize(tinted.vertices.len(), [0.1, 0.2, 0.3, 0.4]);
    assert_eq!(
        CachedInkMesh::from_wire(tinted).encoding,
        InkEncoding::Explicit
    );

    let nonzero_triangles = |mesh: &egui::Mesh| -> Vec<[Vertex; 3]> {
        mesh.indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|indices| indices.map(|i| mesh.vertices[i as usize]))
            .filter(|[a, b, c]| a.pos != b.pos && b.pos != c.pos && a.pos != c.pos)
            .collect()
    };
    for z in [0.03, 0.125, 0.5] {
        let xf = BoardXf {
            center: Pos2::new(311.5, 197.25),
            offset: Vec2::new(-19.75, 42.5),
            z,
        };
        let mut expected = egui::Mesh::default();
        let mut actual = egui::Mesh::default();
        for (source, packed, color) in [
            (&original, &collapsed, Color32::RED),
            (
                &original_mismatch,
                &fallback,
                Color32::BLUE.gamma_multiply(0.4),
            ),
            (&original, &collapsed, Color32::GREEN),
        ] {
            append_ink_mesh_to_epaint(&mut expected, source, &xf, color, |c| c);
            append_ink_mesh_to_epaint(&mut actual, packed, &xf, color, |c| c);
        }
        assert!(actual.is_valid());
        assert_eq!(nonzero_triangles(&actual), nonzero_triangles(&expected));
        assert_eq!(actual.vertices.len() + 6, expected.vertices.len());
        assert_eq!(actual.indices.len() + 24, expected.indices.len());
    }
}
