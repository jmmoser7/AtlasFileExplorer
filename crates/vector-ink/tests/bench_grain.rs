//! Cost of each brush texture on the committed-stamp and tile paths.
//!
//! ```powershell
//! cargo test -p vector-ink --test bench_grain -- --ignored --nocapture
//! ```
//!
//! `SLATE_GRAIN_SHEET=<dir>` also writes one PNG swatch per texture there,
//! for judging the look against its prior art.

use std::time::Instant;
use vector_ink::{
    composite_strokes_tiled, finish_grain, stamp_blurred, Grain, StampImage, StampStyle, StrokeInk, TipPoint,
};

const GRAINS: [Grain; 5] = [
    Grain::Smooth,
    Grain::Graphite,
    Grain::Pencil,
    Grain::Ink,
    Grain::Watercolor,
];

/// A freehand-like wave sampled every two world units, as the brush stores it.
fn wave(grain: Grain, diameter: f32, softness: f32) -> Vec<Vec<TipPoint>> {
    let tip = StampStyle {
        diameter,
        softness,
        rgba: [40, 60, 90, 230],
        grain,
    };
    let pts = (0..400)
        .map(|i| {
            let x = i as f32 * 2.0;
            TipPoint {
                pos: [x, 120.0 + (x * 0.02).sin() * 60.0 + (x * 0.071).sin() * 12.0],
                tip,
            }
        })
        .collect();
    vec![pts]
}

fn median_ms(mut v: Vec<f32>) -> f32 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

fn time(mut f: impl FnMut()) -> f32 {
    f();
    median_ms(
        (0..5)
            .map(|_| {
                let t = Instant::now();
                f();
                t.elapsed().as_secs_f32() * 1000.0
            })
            .collect(),
    )
}

#[test]
#[ignore]
fn bench_grain_stamp() {
    println!("texture     stamp ms (pixel 0.5)   tiles ms (pixel 0.5)   stamp ms (pixel 2)   finish ms (pixel 0.5)");
    for grain in GRAINS {
        let contours = wave(grain, 48.0, 0.3);
        let fine = time(|| {
            std::hint::black_box(stamp_blurred(&contours, &[], 0.5, 0.0));
        });
        let coarse = time(|| {
            std::hint::black_box(stamp_blurred(&contours, &[], 2.0, 0.0));
        });
        let ink = [StrokeInk {
            contours: contours.clone(),
            erase: Vec::new(),
        }];
        let tiles = time(|| {
            let mut dst = StampImage {
                width: 2048,
                height: 512,
                origin: [0.0, 0.0],
                pixel: 0.5,
                rgba: vec![0u8; 2048 * 512 * 4],
                depth: Vec::new(),
                side: Default::default(),
            };
            composite_strokes_tiled(&mut dst, &ink, 512);
            std::hint::black_box(dst);
        });
        let plain = stamp_blurred(&wave(Grain::Smooth, 48.0, 0.3), &[], 0.5, 0.0).unwrap();
        let finish = time(|| {
            let mut img = plain.clone();
            img.depth = vec![200; img.rgba.len() / 4];
            finish_grain(&mut img, grain, None);
            std::hint::black_box(img);
        });
        let name = format!("{grain:?}");
        println!("{name:<11} {fine:>10.1} {tiles:>22.1} {coarse:>20.1} {finish:>18.1}");
    }
    if let Ok(dir) = std::env::var("SLATE_GRAIN_SHEET") {
        write_sheet(std::path::Path::new(&dir));
    }
}

/// Each texture at hard and soft tips, three overlapping strokes over white,
/// so the tooth, the edge, and the build-up between strokes all show.
fn write_sheet(dir: &std::path::Path) {
    std::fs::create_dir_all(dir).expect("sheet dir");
    for grain in GRAINS {
        let (w, h) = (820u32, 300u32);
        let mut canvas = vec![255u8; (w * h * 4) as usize];
        for (row, softness) in [(0.0_f32, 0.0_f32), (150.0, 0.6)] {
            for k in 0..3 {
                let tip = StampStyle {
                    diameter: 36.0,
                    softness,
                    rgba: [30, 45, 70, 235],
                    grain,
                };
                let pts: Vec<TipPoint> = (0..200)
                    .map(|i| {
                        let x = 20.0 + i as f32 * 3.9;
                        TipPoint {
                            pos: [
                                x,
                                row + 60.0 + k as f32 * 14.0 + (x * 0.015 + k as f32).sin() * 30.0,
                            ],
                            tip,
                        }
                    })
                    .collect();
                let Some(img) = stamp_blurred(&[pts], &[], 1.0, 0.0) else {
                    continue;
                };
                over_white(&mut canvas, w, h, &img);
            }
        }
        let path = dir.join(format!("grain-{grain:?}.png").to_lowercase());
        let file = std::fs::File::create(&path).expect("sheet file");
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header()
            .and_then(|mut wr| wr.write_image_data(&canvas))
            .expect("sheet png");
        println!("wrote {}", path.display());
    }
}

fn over_white(canvas: &mut [u8], w: u32, h: u32, img: &StampImage) {
    let ox = img.origin[0].round() as i64;
    let oy = img.origin[1].round() as i64;
    for y in 0..img.height as i64 {
        for x in 0..img.width as i64 {
            let (cx, cy) = (ox + x, oy + y);
            if cx < 0 || cy < 0 || cx >= w as i64 || cy >= h as i64 {
                continue;
            }
            let s = ((y * img.width as i64 + x) * 4) as usize;
            let d = ((cy * w as i64 + cx) * 4) as usize;
            let a = img.rgba[s + 3] as f32 / 255.0;
            for c in 0..3 {
                let v = img.rgba[s + c] as f32 * a + canvas[d + c] as f32 * (1.0 - a);
                canvas[d + c] = v.round() as u8;
            }
        }
    }
}
