use super::super::{boolean_union_all_raw, point_in_polygon, Polygon};

fn area(poly: &Polygon) -> f32 {
    poly.iter()
        .enumerate()
        .map(|(index, ring)| {
            let twice = ring
                .iter()
                .zip(ring.iter().cycle().skip(1))
                .take(ring.len())
                .map(|(a, b)| a[0] * b[1] - b[0] * a[1])
                .sum::<f32>();
            let area = twice.abs() * 0.5;
            if index == 0 {
                area
            } else {
                -area
            }
        })
        .sum()
}

fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Vec<[f32; 2]> {
    vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
}

#[test]
fn raw_union_merges_fractional_crossings_and_preserves_fractional_edges() {
    let horizontal = vec![rect(0.125, 0.25, 2.125, 2.25)];
    let vertical = vec![rect(1.125, -0.75, 3.125, 1.25)];

    let union = boolean_union_all_raw(&[horizontal, vertical]);

    assert_eq!(union.len(), 1);
    assert!((area(&union[0]) - 7.0).abs() < 1e-4);
    assert!(union[0][0]
        .iter()
        .any(|point| (point[0] - 0.125).abs() < 1e-6 || (point[1] - 0.25).abs() < 1e-6));
    for crossing in [[1.125, 0.25], [2.125, 1.25]] {
        assert!(union[0][0].iter().any(|point| {
            (point[0] - crossing[0]).abs() < 1e-6 && (point[1] - crossing[1]).abs() < 1e-6
        }));
    }
    assert!(point_in_polygon(&union[0], [2.75, 0.0]));
    assert!(!point_in_polygon(&union[0], [3.5, 0.0]));
}

#[test]
fn raw_union_handles_empty_input_and_preserves_single_holed_polygon() {
    assert!(boolean_union_all_raw(&[]).is_empty());

    let holed = vec![
        rect(0.125, 0.25, 4.125, 4.25),
        rect(1.125, 1.25, 3.125, 3.25),
    ];
    let union = boolean_union_all_raw(&[holed]);

    assert_eq!(union.len(), 1);
    assert_eq!(union[0].len(), 2);
    assert!((area(&union[0]) - 12.0).abs() < 1e-4);
    assert!(point_in_polygon(&union[0], [0.5, 0.5]));
    assert!(!point_in_polygon(&union[0], [2.0, 2.0]));
}
