//! Folder-map card labels keep their breaks and ellipses at every zoom and
//! scale as one block with the card (P0.9, TX1).

use super::cam::FolderCam;
use super::input::MapHover;
use super::labels::{
    paint_dir_labels, paint_file_labels, paint_file_labels_detail, paint_portal_labels,
};
use super::paint::{MapStyle, PaintArgs};
use crate::theme::Palette;
use atlas_core::dirmeta::DirMetaMap;
use atlas_core::tree::{LayoutConfig, Tree};
use atlas_core::types::FileEntry;
use eframe::egui::{self, Painter, Pos2, Rect, Vec2};
use std::collections::HashSet;
use std::path::Path;

const SWEEP: [f32; 7] = [0.25, 0.5, 1.0, 1.5, 2.0, 3.0, 4.0];
const DIR: &str = "Quarterly reports and supporting spreadsheets for the regional offices";
const SUB: &str = "Northern territory archive of signed statements";
const FILE: &str = "Quarterly_report_final_revised_v12_for_review_by_finance_and_legal.pdf";

/// A painted text block, measured from the card's screen origin and divided
/// by the zoom, so a rigid block reads the same at every zoom.
#[derive(Debug)]
struct Block {
    rows: Vec<String>,
    origin: Vec2,
    glyph_x: Vec<Vec<f32>>,
    row_y: Vec<f32>,
}

fn blocks(out: &egui::FullOutput, card: Pos2, z: f32) -> Vec<Block> {
    let mut found = Vec::new();
    for clipped in &out.shapes {
        let egui::Shape::Text(text) = &clipped.shape else {
            continue;
        };
        let galley = &text.galley;
        // `Shape::transform` scales a text shape's galley rect and mesh but
        // not its rows or glyphs; the ratio recovers the scale it painted at.
        let bottom = galley.rows.last().map_or(0.0, |row| row.rect.max.y);
        let s = if bottom > 0.0 {
            galley.rect.max.y / bottom
        } else {
            1.0
        };
        found.push(Block {
            rows: galley.rows.iter().map(|row| row.text()).collect(),
            origin: (text.pos - card) / z,
            glyph_x: galley
                .rows
                .iter()
                .map(|row| row.glyphs.iter().map(|g| g.pos.x * s / z).collect())
                .collect(),
            row_y: galley
                .rows
                .iter()
                .map(|row| row.rect.min.y * s / z)
                .collect(),
        });
    }
    found
}

/// Paint `labels` at every zoom of the sweep and return what differs from
/// zoom 1. The camera is panned too, so a pan cannot reshape either.
fn sweep(name: &str, card: Rect, labels: impl Fn(&Painter, FolderCam)) -> Vec<String> {
    let ctx = egui::Context::default();
    let paint = |z: f32| {
        let cam = FolderCam {
            offset: Vec2::new(37.3, -21.7) * z.sqrt(),
            z,
        };
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(20_000.0))),
            ..Default::default()
        };
        let out = ctx.run(input, |ctx| {
            labels(&ctx.layer_painter(egui::LayerId::background()), cam);
        });
        blocks(&out, cam.w2s(card.min), z)
    };
    let want = paint(1.0);
    let mut failures = Vec::new();
    if want.is_empty() {
        failures.push(format!("{name}: nothing painted at zoom 1"));
    }
    if !want
        .iter()
        .any(|block| block.rows.len() > 1 || block.rows[0].ends_with('…'))
    {
        failures.push(format!(
            "{name}: no label wrapped or was cut, so nothing is tested"
        ));
    }
    for z in SWEEP {
        let got = paint(z);
        if got.len() != want.len() {
            failures.push(format!(
                "{name}: {} blocks at zoom {z}, {} at 1",
                got.len(),
                want.len()
            ));
            continue;
        }
        for (g, w) in got.iter().zip(&want) {
            if g.rows != w.rows {
                failures.push(format!(
                    "{name}: rows {:?} at zoom {z}, {:?} at 1",
                    g.rows, w.rows
                ));
                continue;
            }
            let tol = 0.5 / z;
            let off = (g.origin - w.origin).length();
            let ys = g.row_y.iter().zip(&w.row_y).map(|(a, b)| (a - b).abs());
            let xs = g.glyph_x.iter().flatten().zip(w.glyph_x.iter().flatten());
            let worst = xs.map(|(a, b)| (a - b).abs()).chain(ys).fold(off, f32::max);
            if worst > tol {
                failures.push(format!(
                    "{name}: {:?} is {:.2}px off its scaled place at zoom {z}",
                    w.rows[0],
                    worst * z
                ));
            }
        }
    }
    failures
}

fn args<'a>(
    cam: FolderCam,
    lod: u8,
    selection: &'a HashSet<u32>,
    entries: &'a [FileEntry],
) -> PaintArgs<'a> {
    PaintArgs {
        canvas: Rect::from_min_size(Pos2::ZERO, Vec2::splat(1.0e6)),
        cam,
        palette: Palette::dark(),
        style: MapStyle::default(),
        hover: MapHover::default(),
        selection,
        entries,
        file_match: &[],
        root: Path::new("/srv/share"),
        lod,
    }
}

#[test]
fn folder_map_labels_keep_their_breaks_and_scale_uniformly_at_every_zoom() {
    let root = Path::new("/srv/share");
    let rel = format!("{DIR}\\{SUB}\\{FILE}");
    let mut file = FileEntry::from_rel(
        root,
        rel,
        48_211_337,
        1_700_000_000,
        1_690_000_000,
        "jmoser".into(),
    );
    file.owner = "finance-shared-services".into();
    let entries = vec![file.clone()];
    let mut tree = Tree::build(&entries, root, LayoutConfig::default(), &DirMetaMap::new());
    let di = tree
        .dirs
        .iter()
        .position(|d| d.name == SUB)
        .expect("nested folder");
    tree.dirs[di].ctime = 1_690_000_000;
    tree.dirs[di].owner = "regional-operations".into();
    let d = &tree.dirs[di];
    let selection = HashSet::new();
    let card = Rect::from_min_size(Pos2::new(120.0, 40.0), Vec2::new(680.0, 400.0));
    let strip = Rect::from_min_size(card.min, Vec2::new(520.0, 176.0));
    let file_card = Rect::from_min_size(card.min, Vec2::new(600.0, 472.0));
    let region = Rect::from_min_max(Pos2::new(144.0, 260.0), Pos2::new(696.0, 488.0));
    let footer = Rect::from_min_size(card.min, Vec2::new(900.0, 160.0));
    let palette = Palette::dark();
    let ink = egui::Color32::WHITE;

    let failures: Vec<String> = [
        sweep("folder detail", card, |p, cam| {
            paint_dir_labels(p, d, card, &args(cam, 3, &selection, &entries))
        }),
        sweep("folder summary", strip, |p, cam| {
            paint_dir_labels(p, d, strip, &args(cam, 2, &selection, &entries))
        }),
        sweep("group footer", footer, |p, cam| {
            paint_portal_labels(p, d, footer, &args(cam, 2, &selection, &entries))
        }),
        sweep("file summary", file_card, |p, cam| {
            paint_file_labels(
                p,
                &file,
                file_card,
                &args(cam, 2, &selection, &entries),
                ink,
                1.0,
            )
        }),
        sweep("file detail", region, |p, cam| {
            let clip = cam.w2s_rect(file_card);
            paint_file_labels_detail(p, &file, region, clip, cam, ink, 1.0, &palette)
        }),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
