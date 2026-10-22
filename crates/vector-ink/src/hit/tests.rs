use super::*;
use crate::{Cap, Join, StrokeStyle};
use kurbo::BezPath;

#[test]
fn variable_width_pick_follows_outline_and_cap() {
    let style = StrokeStyle {
        width: 100.0,
        cap: Cap::Butt,
        join: Join::Miter,
        taper: None,
        dash: None,
    };
    let hit = |p| {
        hit_stroke_outline(
            &hline(),
            &style,
            [Cap::Butt; 2],
            Some(&[2.0, 100.0]),
            TipEase::Linear,
            p,
            4.0,
        )
    };
    assert!(hit([5.0, 4.0]));
    assert!(
        !hit([5.0, 30.0]),
        "the widest tip must not expand the narrow end"
    );
    assert!(!hit([-8.0, 0.0]), "flat cap has only screen slop");
    assert!(hit([95.0, 40.0]));
}

fn hline() -> BezPath {
    let mut p = BezPath::new();
    p.move_to((0.0, 0.0));
    p.line_to((100.0, 0.0));
    p
}

#[test]
fn on_line_hits_off_line_misses() {
    let path = hline();
    let style = StrokeStyle {
        width: 4.0,
        cap: Cap::Butt,
        join: Join::Miter,
        taper: None,
        dash: None,
    };
    assert!(hit_stroke(&path, &style, [50.0, 0.0], 0.0));
    assert!(!hit_stroke(&path, &style, [50.0, 8.0], 0.0));
}

#[test]
fn cached_pick_reuses_the_same_hit_boundary_for_many_points() {
    let path = hline();
    let style = style();
    let cached = CachedStrokeHit::new(&path, &style).unwrap();
    for (point, expected) in [
        ([0.0, 0.0], true),
        ([50.0, 1.0], true),
        ([50.0, 8.0], false),
        ([100.0, 0.0], true),
    ] {
        assert_eq!(cached.hit(point, 0.0), expected);
        assert_eq!(
            cached.hit(point, 0.0),
            hit_stroke(&path, &style, point, 0.0)
        );
    }
}

#[test]
fn taper_thin_end_misses_where_thick_would_hit() {
    let path = hline();
    let style = StrokeStyle {
        width: 10.0,
        cap: Cap::Butt,
        join: Join::Miter,
        taper: Some(crate::Taper::Linear(1.0, 0.0)),
        dash: None,
    };
    assert!(hit_stroke(&path, &style, [5.0, 0.0], 0.0));
    // On the centerline the tapered half-width is still > 0; use lateral offset.
    assert!(hit_stroke(&path, &style, [5.0, 2.0], 0.0));
    assert!(!hit_stroke(&path, &style, [99.0, 2.0], 0.0));
}

fn style() -> StrokeStyle {
    StrokeStyle {
        width: 2.0,
        cap: Cap::Butt,
        join: Join::Miter,
        taper: None,
        dash: None,
    }
}

#[test]
fn closed_triangle_hits_the_seam_not_the_origin() {
    let mut path = BezPath::new();
    path.move_to((400.0, 300.0));
    path.line_to((480.0, 300.0));
    path.line_to((400.0, 380.0));
    path.close_path();
    let s = style();
    assert!(hit_stroke(&path, &s, [400.0, 340.0], 0.0), "closing seam");
    assert!(!hit_stroke(&path, &s, [0.0, 0.0], 0.0), "world origin");
    assert!(
        !hit_stroke(&path, &s, [200.0, 150.0], 0.0),
        "along the line from the first vertex toward the origin"
    );
    assert!(
        !hit_stroke(&path, &s, [200.0, 300.0], 0.0),
        "infinite extension of the base, outside the AABB"
    );
}

#[test]
fn disjoint_contours_do_not_invent_a_joining_segment() {
    let mut path = BezPath::new();
    path.move_to((400.0, 300.0));
    path.line_to((480.0, 300.0));
    path.close_path();
    path.move_to((0.0, 0.0));
    path.line_to((10.0, 0.0));
    let s = style();
    // Midpoint of the ghost line from (400,300) to (0,0).
    assert!(
        !hit_stroke(&path, &s, [200.0, 150.0], 0.0),
        "concatenated contours must not hit between subpaths"
    );
    assert!(hit_stroke(&path, &s, [440.0, 300.0], 0.0));
    assert!(hit_stroke(&path, &s, [5.0, 0.0], 0.0));
}

#[test]
fn outline_broad_phase_preserves_caps_miters_tips_tapers_and_curves() {
    let paths = [
        "M0 0L1 0",
        "M0 0L100 0L8 40",
        "M0 0C-100 300 300 -200 100 100M400 400L500 500",
        "M100000003 100000003L100000103 100000003",
    ];
    let mut samples = 0;
    for svg in paths {
        let path = BezPath::from_svg(svg).unwrap();
        let vertices = path
            .elements()
            .iter()
            .filter(|p| !matches!(p, kurbo::PathEl::ClosePath))
            .count();
        let widths: Vec<_> = (0..vertices)
            .map(|i| if i % 2 == 0 { 2.0 } else { 100.0 })
            .collect();
        for cap in [Cap::Butt, Cap::Square, Cap::Round] {
            for join in [Join::Miter, Join::Bevel, Join::Round] {
                for taper in [
                    None,
                    Some(crate::Taper::Linear(2.0, 0.1)),
                    Some(crate::Taper::Ends(0.2)),
                ] {
                    for dash in [None, Some((vec![12.0, 8.0], 0.0))] {
                        let style = StrokeStyle {
                            cap,
                            join,
                            taper,
                            dash,
                            ..style()
                        };
                        for tips in [None, Some(widths.as_slice())] {
                            let ends = [Cap::Round, cap];
                            let outline = crate::stroke_outline_ends(
                                &path,
                                &style,
                                ends,
                                tips,
                                TipEase::Smooth,
                                0.25,
                            );
                            for p in outline
                                .elements()
                                .iter()
                                .filter_map(|p| p.end_point())
                                .step_by(5)
                            {
                                let point = [p.x as f32, p.y as f32];
                                assert!(
                                    outline_might_hit(&path, &style, ends, tips, point, 0.5),
                                    "clipped {svg} {style:?} at {point:?}"
                                );
                                samples += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(samples > 1_000, "sampled {samples} outline vertices");
}

#[test]
fn bounded_outline_pick_matches_exact_pick_near_and_far() {
    let path = BezPath::from_svg("M0 0C0 100 100 -100 100 0L101 1").unwrap();
    let widths = [2.0, 100.0, 4.0];
    for cap in [Cap::Butt, Cap::Round, Cap::Square] {
        let style = StrokeStyle { cap, ..style() };
        for tips in [None, Some(widths.as_slice()), Some(&[f32::NAN][..])] {
            for point in [
                [-900.0, 0.0],
                [-10.0, 1.0],
                [50.0, 0.0],
                [100.0, 5.0],
                [900.0, 900.0],
            ] {
                assert_eq!(
                    hit_stroke_outline(&path, &style, [cap; 2], tips, TipEase::Linear, point, 4.0),
                    hit_stroke_outline_exact(
                        &path,
                        &style,
                        [cap; 2],
                        tips,
                        TipEase::Linear,
                        point,
                        4.0
                    ),
                    "{cap:?} {point:?}"
                );
            }
        }
    }
}

#[test]
fn zero_slop_picks_keep_float_rounded_wide_caps_on_short_paths() {
    let style = StrokeStyle {
        cap: Cap::Round,
        join: Join::Round,
        ..style()
    };
    for svg in ["M0 0L1 0", "M0 0L0.001 0.001", "M-3 7L-2 8"] {
        let path = BezPath::from_svg(svg).unwrap();
        for widths in [[100.0, 0.0], [0.0, 100.0], [1000.0, 1.0], [1.0, 1000.0]] {
            for ease in [TipEase::Linear, TipEase::Smooth] {
                let ends = [Cap::Round; 2];
                let outline =
                    crate::stroke_outline_ends(&path, &style, ends, Some(&widths), ease, 0.25);
                for p in outline.elements().iter().filter_map(|p| p.end_point()) {
                    let point = [p.x as f32, p.y as f32];
                    assert_eq!(
                        hit_stroke_outline(&path, &style, ends, Some(&widths), ease, point, 0.0),
                        hit_stroke_outline_exact(
                            &path,
                            &style,
                            ends,
                            Some(&widths),
                            ease,
                            point,
                            0.0
                        ),
                        "clipped {svg} widths={widths:?} ease={ease:?} at {point:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn rounded_cap_pick_outside_ideal_f64_reach_remains_a_hit() {
    let path = BezPath::from_svg("M0 0L1 0").unwrap();
    let style = StrokeStyle {
        cap: Cap::Round,
        join: Join::Round,
        ..style()
    };
    let widths = [100.0, 0.0];
    let ends = [Cap::Round; 2];
    let mut outside_hits = 0;
    for ease in [TipEase::Linear, TipEase::Smooth] {
        let outline = crate::stroke_outline_ends(&path, &style, ends, Some(&widths), ease, 0.25);
        for p in outline.elements().iter().filter_map(|p| p.end_point()) {
            let point = [p.x as f32, p.y as f32];
            if p.x == -803.1129150390625
                && hit_stroke_outline_exact(&path, &style, ends, Some(&widths), ease, point, 0.0)
            {
                assert!(p.x < -803.1128878917646, "outside the ideal f64 cap bound");
                assert!(hit_stroke_outline(
                    &path,
                    &style,
                    ends,
                    Some(&widths),
                    ease,
                    point,
                    0.0
                ));
                outside_hits += 1;
            }
        }
    }
    assert!(
        outside_hits > 0,
        "the known float-rounded cap vertex must reproduce an exact hit"
    );
}

#[test]
#[ignore = "manual before/after outline-pick microbenchmark"]
fn distant_outline_pick_bench() {
    use std::{hint::black_box, time::Instant};
    let style = StrokeStyle {
        cap: Cap::Round,
        join: Join::Round,
        ..style()
    };
    for segments in [64, 512, 4096] {
        let mut path = BezPath::new();
        path.move_to((0.0, 0.0));
        for i in 1..=segments {
            path.line_to((i as f64, (i as f64 * 0.3).sin() * 8.0));
        }
        let point = [-10_000.0, -10_000.0];
        let repeats = 200;
        let start = Instant::now();
        for _ in 0..repeats {
            assert!(!black_box(hit_stroke_outline_exact(
                black_box(&path),
                &style,
                [Cap::Round; 2],
                None,
                TipEase::Linear,
                point,
                4.0
            )));
        }
        let exact = start.elapsed().as_secs_f64() * 1000.0;
        let start = Instant::now();
        for _ in 0..repeats {
            assert!(!black_box(hit_stroke_outline(
                black_box(&path),
                &style,
                [Cap::Round; 2],
                None,
                TipEase::Linear,
                point,
                4.0
            )));
        }
        let bounded = start.elapsed().as_secs_f64() * 1000.0;
        eprintln!("outline-pick segments={segments} repeats={repeats} exact_ms={exact:.3} bounded_ms={bounded:.3} speedup={:.1}x", exact / bounded);
    }
}
