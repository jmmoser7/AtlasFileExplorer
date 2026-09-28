//! Brush texture rules the user decided on 28 September 2026 (round 7):
//! r7-8, a Shift segment keeps the texture it was painted with, and r7-9,
//! Watercolor builds where one stroke crosses itself, as a second stroke
//! does. Board tiles, whole stamps, and the HTML artifact all run this code.

use vector_ink::{
    composite_strokes, composite_strokes_tiled, stamp_tipped, Grain, StampImage, StampStyle,
    StrokeInk, TipPoint,
};

fn tip(grain: Grain) -> StampStyle {
    StampStyle {
        diameter: 24.0,
        softness: 0.0,
        rgba: [40, 60, 160, 220],
        grain,
    }
}

fn line(points: &[(f32, f32)], grain: Grain) -> Vec<TipPoint> {
    points
        .iter()
        .map(|&(x, y)| TipPoint {
            pos: [x, y],
            tip: tip(grain),
        })
        .collect()
}

fn canvas() -> StampImage {
    StampImage {
        width: 320,
        height: 320,
        origin: [-60.0, -160.0],
        pixel: 1.0,
        rgba: vec![0u8; 320 * 320 * 4],
        ..Default::default()
    }
}

/// Each stroke composited onto its own canvas.
fn paint(strokes: &[Vec<Vec<TipPoint>>]) -> StampImage {
    let mut img = canvas();
    let ink: Vec<StrokeInk> = strokes
        .iter()
        .map(|c| StrokeInk {
            contours: c.clone(),
            erase: Vec::new(),
        })
        .collect();
    composite_strokes(&mut img, &ink);
    img
}

fn alpha(img: &StampImage, x: f32, y: f32) -> u8 {
    let px = (x - img.origin[0]) as u32;
    let py = (y - img.origin[1]) as u32;
    img.rgba[((py * img.width + px) * 4 + 3) as usize]
}

/// Largest alpha difference inside the world box `[x0, y0, x1, y1]`.
fn worst_in(a: &StampImage, b: &StampImage, bx: [f32; 4]) -> u8 {
    let mut worst = 0;
    let mut y = bx[1];
    while y < bx[3] {
        let mut x = bx[0];
        while x < bx[2] {
            worst = worst.max(alpha(a, x, y).abs_diff(alpha(b, x, y)));
            x += 1.0;
        }
        y += 1.0;
    }
    worst
}

/// r7-8: "the new segment takes the currently armed texture". A Graphite
/// stroke extended by a Watercolor Shift segment paints that segment as
/// Watercolor and keeps its Graphite body.
#[test]
fn a_shift_segment_keeps_the_texture_it_was_painted_with() {
    let mut chain = line(&[(0.0, 0.0), (100.0, 0.0)], Grain::Graphite);
    chain.push(TipPoint {
        pos: [200.0, 0.0],
        tip: tip(Grain::Watercolor),
    });
    let mixed = paint(&[vec![chain]]);
    let graphite = paint(&[vec![line(&[(0.0, 0.0), (200.0, 0.0)], Grain::Graphite)]]);
    let watercolor = paint(&[vec![line(&[(0.0, 0.0), (200.0, 0.0)], Grain::Watercolor)]]);
    assert!(
        worst_in(&mixed, &watercolor, [130.0, -14.0, 214.0, 14.0]) <= 1,
        "the Shift segment is not Watercolor"
    );
    assert!(
        worst_in(&mixed, &graphite, [-14.0, -14.0, 80.0, 14.0]) <= 1,
        "the Graphite body changed"
    );
    assert!(
        worst_in(&graphite, &watercolor, [130.0, -14.0, 214.0, 14.0]) > 20,
        "fixture: the two textures look alike"
    );
}

/// r7-9: one Watercolor stroke that crosses itself builds at the crossing
/// exactly as a second, separate stroke over it would.
#[test]
fn a_watercolor_stroke_builds_where_it_crosses_itself() {
    let path = [
        (0.0, 0.0),
        (200.0, 0.0),
        (200.0, 100.0),
        (100.0, 100.0),
        (100.0, -100.0),
    ];
    let one = paint(&[vec![line(&path, Grain::Watercolor)]]);
    let first = line(&path[..4], Grain::Watercolor);
    let second = line(&path[3..], Grain::Watercolor);
    let two = paint(&[vec![first.clone()], vec![second]]);
    let under = paint(&[vec![first]]);
    let crossing = [80.0, -20.0, 120.0, 20.0];
    assert!(
        worst_in(&one, &two, crossing) <= 1,
        "the crossing does not build like a second stroke"
    );
    assert!(
        alpha(&one, 100.0, 0.0) as i32 >= alpha(&under, 100.0, 0.0) as i32 + 30,
        "the crossing did not darken: {} over {}",
        alpha(&one, 100.0, 0.0),
        alpha(&under, 100.0, 0.0)
    );
}

/// Tiles cut the same pixels out of a self-crossing Watercolor stroke and a
/// mixed-texture chain as one composite does (Art. IV).
#[test]
fn tiles_paint_self_crossings_and_mixed_chains_like_one_composite() {
    let mut chain = line(&[(-40.0, 60.0), (60.0, 60.0)], Grain::Pencil);
    chain.push(TipPoint {
        pos: [160.0, 120.0],
        tip: tip(Grain::Watercolor),
    });
    let strokes = vec![
        StrokeInk {
            contours: vec![line(
                &[
                    (0.0, 0.0),
                    (200.0, 0.0),
                    (200.0, 100.0),
                    (100.0, 100.0),
                    (100.0, -100.0),
                ],
                Grain::Watercolor,
            )],
            erase: Vec::new(),
        },
        StrokeInk {
            contours: vec![chain],
            erase: vec![line(&[(20.0, 60.0), (40.0, 70.0)], Grain::Smooth)],
        },
    ];
    // The canvas origin sits on the 32 px tile grid.
    let on_grid = || StampImage {
        origin: [-64.0, -160.0],
        ..canvas()
    };
    let mut direct = on_grid();
    composite_strokes(&mut direct, &strokes);
    let mut tiled = on_grid();
    composite_strokes_tiled(&mut tiled, &strokes, 32);
    assert_eq!(tiled.rgba, direct.rgba, "tiles drifted from one composite");
    assert!(direct.rgba.iter().any(|b| *b != 0));
}

/// A whole-stroke stamp (the HTML artifact's PNG) of a self-crossing
/// Watercolor stroke builds at the crossing too.
#[test]
fn a_whole_stamp_of_a_self_crossing_watercolor_stroke_builds() {
    let path = [
        (0.0, 0.0),
        (200.0, 0.0),
        (200.0, 100.0),
        (100.0, 100.0),
        (100.0, -100.0),
    ];
    let one = stamp_tipped(&[line(&path, Grain::Watercolor)], 1.0).unwrap();
    let under = stamp_tipped(&[line(&path[..4], Grain::Watercolor)], 1.0).unwrap();
    let at = |img: &StampImage| {
        let x = (100.0 - img.origin[0]) as u32;
        let y = (0.0 - img.origin[1]) as u32;
        img.rgba[((y * img.width + x) * 4 + 3) as usize]
    };
    assert!(
        at(&one) as i32 >= at(&under) as i32 + 30,
        "{} over {}",
        at(&one),
        at(&under)
    );
}
