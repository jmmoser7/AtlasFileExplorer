use super::{fill_triangles, fill_triangles_with_boundaries, signed_area, FillBoundary};
use crate::trim::point_in_mesh;

fn triangle_area(vertices: &[[f32; 2]], indices: &[u32]) -> f32 {
    indices
        .chunks_exact(3)
        .map(|triangle| {
            let a = vertices[triangle[0] as usize];
            let b = vertices[triangle[1] as usize];
            let c = vertices[triangle[2] as usize];
            ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs() * 0.5
        })
        .sum()
}

fn boundary_area(vertices: &[[f32; 2]], boundary: &FillBoundary) -> f32 {
    let points: Vec<[f32; 2]> = boundary
        .indices
        .iter()
        .map(|&index| vertices[index as usize])
        .collect();
    signed_area(&points)
}

#[test]
fn concave_containment_reports_reordered_cleaned_ccw_boundaries_and_keeps_fill_parity() {
    let mut exterior = vec![
        [0.0, 0.0],
        [5.0, 0.0],
        [5.0, 1.0],
        [1.0, 1.0],
        [1.0, 5.0],
        [0.0, 5.0],
    ];
    let mut inset = vec![
        [0.2, 0.2],
        [4.8, 0.2],
        [4.8, 0.8],
        [0.8, 0.8],
        [0.8, 4.8],
        [0.2, 4.8],
    ];
    exterior.reverse();
    inset.reverse();
    exterior.push(exterior[0]);
    inset.push(inset[0]);
    let mut island = vec![[20.0, 0.0], [23.0, 0.0], [23.0, 3.0], [20.0, 3.0]];
    let mut island_hole = vec![[21.0, 1.0], [22.0, 1.0], [22.0, 2.0], [21.0, 2.0]];
    island.reverse();
    island_hole.reverse();
    let polygon = vec![
        vec![[-1.0, -1.0], [-2.0, -2.0]],
        inset,
        island,
        exterior,
        island_hole,
    ];

    let (vertices, indices, boundaries) = fill_triangles_with_boundaries(&polygon);
    let (plain_vertices, plain_indices) = fill_triangles(&polygon);

    assert_eq!(vertices, plain_vertices);
    assert_eq!(indices, plain_indices);
    assert!((triangle_area(&vertices, &indices) - 11.84).abs() < 1e-4);
    assert!(point_in_mesh(&vertices, &indices, [4.9, 0.5]));
    assert!(point_in_mesh(&vertices, &indices, [0.5, 4.9]));
    assert!(!point_in_mesh(&vertices, &indices, [0.5, 0.5]));
    assert!(!point_in_mesh(&vertices, &indices, [3.0, 3.0]));
    assert!(point_in_mesh(&vertices, &indices, [20.5, 0.5]));
    assert!(!point_in_mesh(&vertices, &indices, [21.5, 1.5]));

    assert_eq!(boundaries.len(), 4);
    assert_eq!(boundaries[0].source_contour, 2);
    assert!(!boundaries[0].hole);
    assert_eq!(boundaries[1].source_contour, 4);
    assert!(boundaries[1].hole);
    assert_eq!(boundaries[2].source_contour, 3);
    assert!(!boundaries[2].hole);
    assert_eq!(boundaries[3].source_contour, 1);
    assert!(boundaries[3].hole);
    for boundary in &boundaries {
        assert_eq!(
            boundary.indices.len(),
            match boundary.source_contour {
                1 | 3 => 6,
                2 | 4 => 4,
                source => panic!("unexpected source contour {source}"),
            }
        );
        assert!(boundary_area(&vertices, boundary) > 0.0);
    }
}
