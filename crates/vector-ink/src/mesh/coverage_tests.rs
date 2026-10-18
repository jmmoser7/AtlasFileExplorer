use super::max_coverage_mesh;
use crate::{InkMesh, InkVertex};

fn build_mesh(
    triangles: &[[[f32; 2]; 3]],
    alphas: &[[f32; 3]],
    colors: Option<&[[[f32; 4]; 3]]>,
) -> InkMesh {
    let mut mesh = InkMesh::default();
    for (triangle_index, triangle) in triangles.iter().enumerate() {
        let base = mesh.vertices.len() as u32;
        for vertex_index in 0..3 {
            mesh.vertices.push(InkVertex {
                pos: triangle[vertex_index],
                alpha: alphas[triangle_index][vertex_index],
            });
            if let Some(colors) = colors {
                mesh.colors.push(colors[triangle_index][vertex_index]);
            }
        }
        mesh.indices.extend([base, base + 1, base + 2]);
    }
    mesh
}

fn sample(mesh: &InkMesh, point: [f32; 2]) -> Option<(f32, Option<[f32; 4]>)> {
    for triangle in mesh.indices.chunks_exact(3) {
        let a = mesh.vertices[triangle[0] as usize];
        let b = mesh.vertices[triangle[1] as usize];
        let c = mesh.vertices[triangle[2] as usize];
        let denominator = cross(sub(b.pos, a.pos), sub(c.pos, a.pos));
        let wb = cross(sub(point, a.pos), sub(c.pos, a.pos)) / denominator;
        let wc = cross(sub(b.pos, a.pos), sub(point, a.pos)) / denominator;
        let wa = 1.0 - wb - wc;
        if wa >= 0.0 && wb >= 0.0 && wc >= 0.0 {
            let alpha = wa * a.alpha + wb * b.alpha + wc * c.alpha;
            let color = (!mesh.colors.is_empty()).then(|| {
                let ca = mesh.colors[triangle[0] as usize];
                let cb = mesh.colors[triangle[1] as usize];
                let cc = mesh.colors[triangle[2] as usize];
                std::array::from_fn(|i| wa * ca[i] + wb * cb[i] + wc * cc[i])
            });
            return Some((alpha, color));
        }
    }
    None
}

fn strict_coverage(mesh: &InkMesh, point: [f32; 2]) -> (usize, Option<f32>) {
    let mut count = 0;
    let mut alpha = None;
    for triangle in mesh.indices.chunks_exact(3) {
        let a = mesh.vertices[triangle[0] as usize];
        let b = mesh.vertices[triangle[1] as usize];
        let c = mesh.vertices[triangle[2] as usize];
        let denominator = cross(sub(b.pos, a.pos), sub(c.pos, a.pos));
        let wb = cross(sub(point, a.pos), sub(c.pos, a.pos)) / denominator;
        let wc = cross(sub(b.pos, a.pos), sub(point, a.pos)) / denominator;
        let wa = 1.0 - wb - wc;
        if wa > 1e-5 && wb > 1e-5 && wc > 1e-5 {
            count += 1;
            alpha = Some(wa * a.alpha + wb * b.alpha + wc * c.alpha);
        }
    }
    (count, alpha)
}

fn sub(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn cross(a: [f32; 2], b: [f32; 2]) -> f32 {
    a[0] * b[1] - a[1] * b[0]
}

fn area(mesh: &InkMesh) -> f64 {
    mesh.indices
        .chunks_exact(3)
        .map(|triangle| {
            let a = mesh.vertices[triangle[0] as usize].pos.map(f64::from);
            let b = mesh.vertices[triangle[1] as usize].pos.map(f64::from);
            let c = mesh.vertices[triangle[2] as usize].pos.map(f64::from);
            ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs() * 0.5
        })
        .sum()
}

fn sorted_vertex_bits(mesh: &InkMesh, triangle: &[u32]) -> [(u32, u32, u32); 3] {
    let mut bits = std::array::from_fn(|index| {
        let vertex = mesh.vertices[triangle[index] as usize];
        (
            vertex.pos[0].to_bits(),
            vertex.pos[1].to_bits(),
            vertex.alpha.to_bits(),
        )
    });
    bits.sort_unstable();
    bits
}

fn triangle_count(mesh: &InkMesh) -> usize {
    mesh.indices.len() / 3
}

fn right_triangle(offset: [f32; 2], size: f32) -> [[f32; 2]; 3] {
    [
        offset,
        [offset[0] + size, offset[1]],
        [offset[0], offset[1] + size],
    ]
}

#[test]
fn coincident_constant_fields_keep_the_greater_alpha_and_empty_colors() {
    let triangle = right_triangle([0.0, 0.0], 1.0);
    let mesh = build_mesh(&[triangle, triangle], &[[0.25; 3], [0.8; 3]], None);

    let output = max_coverage_mesh(&mesh);

    assert!((area(&output) - 0.5).abs() < 1e-6);
    assert!((sample(&output, [0.2, 0.2]).unwrap().0 - 0.8).abs() < 1e-6);
    assert!(output.colors.is_empty());
}

#[test]
fn coincident_equal_fields_give_ties_to_the_earlier_source_triangle() {
    let triangle = right_triangle([0.0, 0.0], 1.0);
    let mesh = build_mesh(
        &[triangle, triangle],
        &[[0.75; 3], [0.75; 3]],
        Some(&[[[1.0, 0.0, 0.0, 1.0]; 3], [[0.0, 1.0, 0.0, 1.0]; 3]]),
    );

    let output = max_coverage_mesh(&mesh);

    assert!((area(&output) - 0.5).abs() < 1e-6);
    assert_eq!(
        sample(&output, [0.2, 0.2]).unwrap().1.unwrap(),
        [1.0, 0.0, 0.0, 1.0]
    );
}

#[test]
fn fractional_coincident_affine_fields_preserve_analytic_attributes_once() {
    let triangle = [[0.25, 0.125], [2.25, 0.125], [0.25, 2.125]];
    let alpha = triangle.map(|[x, y]| 0.2 + 0.1 * x + 0.2 * y);
    let colors =
        triangle.map(|[x, y]| [0.1 + 0.2 * x, 0.3 + 0.1 * y, 0.5 - 0.05 * x + 0.05 * y, 1.0]);
    let mesh = build_mesh(
        &[triangle, triangle],
        &[alpha, alpha],
        Some(&[colors, [[0.0, 1.0, 0.0, 1.0]; 3]]),
    );

    let output = max_coverage_mesh(&mesh);

    assert_eq!(triangle_count(&output), 1);
    for row in 1..8 {
        for column in 1..(9 - row) {
            let u = column as f32 / 9.0;
            let v = row as f32 / 9.0;
            let point = [0.25 + 2.0 * u, 0.125 + 2.0 * v];
            let (count, actual_alpha) = strict_coverage(&output, point);
            assert_eq!(count, 1, "coverage count at {point:?}");
            let expected_alpha = 0.2 + 0.1 * point[0] + 0.2 * point[1];
            assert!((actual_alpha.unwrap() - expected_alpha).abs() < 2e-5);
            let (sampled_alpha, actual_color) = sample(&output, point).unwrap();
            assert!((sampled_alpha - expected_alpha).abs() < 2e-5);
            let expected_color = [
                0.1 + 0.2 * point[0],
                0.3 + 0.1 * point[1],
                0.5 - 0.05 * point[0] + 0.05 * point[1],
                1.0,
            ];
            for (actual, expected) in actual_color.unwrap().into_iter().zip(expected_color) {
                assert!((actual - expected).abs() < 2e-5, "color at {point:?}");
            }
        }
    }
}

#[test]
fn diagnosed_reciprocal_rounding_fixture_has_single_coverage_and_first_colors() {
    let triangle = [
        [19.35456657409668, 6.841848850250244],
        [1.788248896598816, 14.564129829406738],
        [11.305647850036621, 19.844606399536133],
    ];
    let alpha = [0.09474056214094162, 0.2760789096355438, 0.4839041829109192];
    let first_colors = [
        [0.12, 0.25, 0.75, 1.0],
        [0.82, 0.10, 0.15, 1.0],
        [0.30, 0.90, 0.05, 1.0],
    ];
    let mesh = build_mesh(
        &[triangle, triangle],
        &[alpha, alpha],
        Some(&[first_colors, [[0.0, 1.0, 0.0, 1.0]; 3]]),
    );

    let output = max_coverage_mesh(&mesh);

    assert_eq!(triangle_count(&output), 1);
    let [a, b, c] = triangle;
    for row in 1..9 {
        for column in 1..(10 - row) {
            let u = column as f32 / 10.0;
            let v = row as f32 / 10.0;
            let w = 1.0 - u - v;
            let point = [
                w * a[0] + u * b[0] + v * c[0],
                w * a[1] + u * b[1] + v * c[1],
            ];
            let (count, sampled_alpha) = strict_coverage(&output, point);
            assert_eq!(count, 1, "coverage count at {point:?}");
            let expected_alpha = w * alpha[0] + u * alpha[1] + v * alpha[2];
            assert!((sampled_alpha.unwrap() - expected_alpha).abs() < 2e-5);
            let (sampled_alpha, sampled_color) = sample(&output, point).unwrap();
            assert!((sampled_alpha - expected_alpha).abs() < 2e-5);
            let expected_color: [f32; 4] = std::array::from_fn(|channel| {
                w * first_colors[0][channel]
                    + u * first_colors[1][channel]
                    + v * first_colors[2][channel]
            });
            for (actual, expected) in sampled_color.unwrap().into_iter().zip(expected_color) {
                assert!((actual - expected).abs() < 2e-5, "color at {point:?}");
            }
        }
    }
}

#[test]
fn crossing_affine_alpha_fields_choose_the_pointwise_max_and_interpolate_color() {
    let triangle = right_triangle([0.0, 0.0], 1.0);
    let mesh = build_mesh(
        &[triangle, triangle],
        &[[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        Some(&[
            [
                [1.0, 0.0, 0.0, 1.0],
                [0.0, 1.0, 0.0, 1.0],
                [0.0, 0.0, 1.0, 1.0],
            ],
            [[0.0, 0.0, 1.0, 1.0]; 3],
        ]),
    );

    let output = max_coverage_mesh(&mesh);
    let (first_alpha, first_color) = sample(&output, [0.2, 0.2]).unwrap();
    let (second_alpha, second_color) = sample(&output, [0.7, 0.1]).unwrap();

    assert!((first_alpha - 0.6).abs() < 1e-5);
    for (actual, expected) in first_color.unwrap().into_iter().zip([0.6, 0.2, 0.2, 1.0]) {
        assert!((actual - expected).abs() < 1e-6);
    }
    assert!((second_alpha - 0.7).abs() < 1e-5);
    assert_eq!(second_color.unwrap(), [0.0, 0.0, 1.0, 1.0]);
    assert!((area(&output) - 0.5).abs() < 1e-6);

    for point in [
        [0.137, 0.119],
        [0.283, 0.199],
        [0.431, 0.173],
        [0.617, 0.107],
        [0.219, 0.533],
        [0.391, 0.407],
    ] {
        let (count, alpha) = strict_coverage(&output, point);
        assert_eq!(count, 1, "coverage count at {point:?}");
        let expected = (1.0 - point[0] - point[1]).max(point[0]);
        assert!(
            (alpha.unwrap() - expected).abs() < 1e-5,
            "alpha at {point:?}"
        );
    }
}

#[test]
fn earlier_foreground_triangle_excludes_the_later_undercoat() {
    let foreground = right_triangle([0.25, 0.25], 0.5);
    let background = right_triangle([0.0, 0.0], 2.0);
    let mesh = build_mesh(&[foreground, background], &[[1.0; 3], [0.5; 3]], None);

    let output = max_coverage_mesh(&mesh);

    assert!((area(&output) - 2.0).abs() < 1e-6);
    let (foreground_count, foreground_alpha) = strict_coverage(&output, [0.35, 0.35]);
    assert_eq!(foreground_count, 1);
    assert_eq!(foreground_alpha, Some(1.0));
    let (background_count, background_alpha) = strict_coverage(&output, [1.2, 0.2]);
    assert_eq!(background_count, 1);
    assert_eq!(background_alpha, Some(0.5));
}

#[test]
fn mixed_winding_and_disjoint_triangles_keep_their_coverage() {
    let ccw = right_triangle([0.0, 0.0], 1.0);
    let cw = [ccw[0], ccw[2], ccw[1]];
    let island = right_triangle([3.0, 0.0], 1.0);
    let mesh = build_mesh(&[cw, ccw, island], &[[0.2; 3], [0.9; 3], [0.4; 3]], None);

    let output = max_coverage_mesh(&mesh);

    assert!((area(&output) - 1.0).abs() < 1e-6);
    assert!((sample(&output, [0.2, 0.2]).unwrap().0 - 0.9).abs() < 1e-6);
    assert!((sample(&output, [3.2, 0.2]).unwrap().0 - 0.4).abs() < 1e-6);
    assert_eq!(strict_coverage(&output, [0.2, 0.2]).0, 1);
    assert_eq!(strict_coverage(&output, [3.2, 0.2]).0, 1);
    for triangle in output.indices.chunks_exact(3) {
        let a = output.vertices[triangle[0] as usize].pos;
        let b = output.vertices[triangle[1] as usize].pos;
        let c = output.vertices[triangle[2] as usize].pos;
        assert!(cross(sub(b, a), sub(c, a)) > 0.0);
    }
}

#[test]
fn bbox_overlapping_but_polygon_disjoint_triangles_are_not_split() {
    let lower = right_triangle([0.0, 0.0], 2.0);
    let upper = right_triangle([1.5, 1.5], 1.3);
    let upper_cw = [upper[0], upper[2], upper[1]];
    let mesh = build_mesh(&[lower, upper_cw], &[[0.25; 3], [0.75; 3]], None);

    let output = max_coverage_mesh(&mesh);

    assert_eq!(triangle_count(&output), 2);
    assert!((area(&output) - (2.0 + 1.3 * 1.3 / 2.0)).abs() < 1e-5);
    assert_eq!(strict_coverage(&output, [0.2, 0.2]).0, 1);
    assert_eq!(strict_coverage(&output, [1.7, 1.7]).0, 1);
}

#[test]
fn many_bbox_overlapping_skinny_islands_keep_bounded_coverage() {
    let mut triangles = Vec::new();
    let mut alphas = Vec::new();
    for index in 0..16 {
        let offset = index as f32 * 0.2;
        let triangle = [[0.0, offset], [10.0, 10.0 + offset], [10.0, 10.01 + offset]];
        triangles.push(if index % 2 == 0 {
            triangle
        } else {
            [triangle[0], triangle[2], triangle[1]]
        });
        alphas.push([0.2 + index as f32 * 0.01; 3]);
    }
    let mesh = build_mesh(&triangles, &alphas, None);
    let expected_area = area(&mesh);

    let output = max_coverage_mesh(&mesh);

    assert_eq!(triangle_count(&output), triangles.len());
    for (source_triangle, output_triangle) in mesh
        .indices
        .chunks_exact(3)
        .zip(output.indices.chunks_exact(3))
    {
        assert_eq!(
            sorted_vertex_bits(&mesh, source_triangle),
            sorted_vertex_bits(&output, output_triangle)
        );
    }
    let actual_area = area(&output);
    let error = (actual_area - expected_area).abs();
    println!(
        "skinny-island area: actual={actual_area:.17}, expected={expected_area:.17}, abs_error={error:.17}"
    );
    assert!(error < 1e-5);
    for (index, triangle) in triangles.iter().enumerate() {
        let point = [
            (triangle[0][0] + triangle[1][0] + triangle[2][0]) / 3.0,
            (triangle[0][1] + triangle[1][1] + triangle[2][1]) / 3.0,
        ];
        let (count, alpha) = strict_coverage(&output, point);
        assert_eq!(count, 1, "coverage count at island {index}");
        assert!((alpha.unwrap() - alphas[index][0]).abs() < 1e-6);
    }
}

#[test]
fn edge_touching_triangles_with_overlapping_bounds_remain_unsplit() {
    let lower_left = [[0.0, 0.0], [2.0, 0.0], [0.0, 2.0]];
    let upper_right = [[2.0, 0.0], [2.0, 2.0], [0.0, 2.0]];
    let mesh = build_mesh(&[lower_left, upper_right], &[[0.25; 3], [0.75; 3]], None);

    let output = max_coverage_mesh(&mesh);

    assert_eq!(triangle_count(&output), 2);
    assert!((area(&output) - 4.0).abs() < 1e-6);
    assert_eq!(strict_coverage(&output, [0.5, 0.5]).0, 1);
    assert_eq!(strict_coverage(&output, [1.5, 1.5]).0, 1);
}

#[test]
fn a_backward_clip_edge_cannot_hide_an_opaque_foreground_triangle() {
    // Actual Agent 36px input triangles 308, 309, 315 and 2025. Sequential
    // cuts create a one-ulp backward edge that is not a supporting boundary.
    let triangles = [
        [
            [0x40acc2da, 0x41a0c298],
            [0x40cd38f9, 0x4197e7a9],
            [0x40c0615b, 0x4194b1c2],
        ],
        [
            [0x40acc2da, 0x41a0c298],
            [0x40c0615b, 0x4194b1c2],
            [0x409cf5a1, 0x419ccf4a],
        ],
        [
            [0x409cf5a1, 0x419ccf4a],
            [0x40c0615b, 0x4194b1c2],
            [0x409a474b, 0x419c02ed],
        ],
        [
            [0x40993031, 0x419dc0a6],
            [0x40c47af8, 0x4193d571],
            [0x4095e957, 0x419cc6df],
        ],
    ]
    .map(|triangle| triangle.map(|point| point.map(f32::from_bits)));
    let foreground = [[1.0, 0.0, 0.0, 1.0]; 3];
    let background = [[0.0, 0.0, 0.0, 1.0]; 3];
    let mesh = build_mesh(
        &triangles,
        &[[1.0; 3]; 4],
        Some(&[foreground, foreground, foreground, background]),
    );
    let output = max_coverage_mesh(&mesh);
    let point = [f32::from_bits(0x40a40000), f32::from_bits(0x419a8000)];
    let (count, alpha) = strict_coverage(&output, point);
    assert_eq!(
        count, 1,
        "the opaque undercoat must not survive below foreground"
    );
    assert!((alpha.unwrap() - 1.0).abs() < 1e-5);
    let (_, color) = sample(&output, point).unwrap();
    assert!(
        color
            .unwrap()
            .iter()
            .zip([1.0, 0.0, 0.0, 1.0])
            .all(|(actual, expected)| (*actual - expected).abs() < 1e-5)
    );
}

#[test]
fn narrow_positive_area_remainders_are_not_filtered() {
    let outer = right_triangle([0.0, 0.0], 2.0);
    let inset = right_triangle([0.01, 0.01], 1.98);
    let mesh = build_mesh(&[outer, inset], &[[0.5; 3], [1.0; 3]], None);

    let output = max_coverage_mesh(&mesh);

    assert!((area(&output) - 2.0).abs() < 1e-5);
    assert!((sample(&output, [0.003, 0.003]).unwrap().0 - 0.5).abs() < 1e-6);
    assert!((sample(&output, [0.1, 0.1]).unwrap().0 - 1.0).abs() < 1e-6);
}
