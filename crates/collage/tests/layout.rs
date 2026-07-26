//! The invariants from the card, asserted across all three layouts.

use std::time::Instant;

use collage::{extent, solve, LastRow, Layout, Options, Placement, Rect, SolveError, Tile};

/// The card's tolerance: 1e-3, relative for aspect, absolute for geometry.
const TOL: f32 = 1e-3;

const LAYOUTS: [Layout; 3] = [Layout::JustifiedRows, Layout::Grid, Layout::Masonry];

fn tiles(aspects: &[f32]) -> Vec<Tile> {
    aspects
        .iter()
        .enumerate()
        .map(|(i, &aspect)| Tile {
            key: i as u64 + 1,
            aspect,
        })
        .collect()
}

/// Portrait, square, landscape, and one panorama — the mix a real selection
/// has, and the one that breaks naive row packing.
fn assorted() -> Vec<Tile> {
    tiles(&[
        1.5, 0.75, 1.0, 1.777, 0.66, 2.4, 1.0, 1.333, 0.8, 3.2, 1.0, 0.5, 1.5, 1.2, 0.9,
    ])
}

fn base_opts() -> Options {
    Options {
        area: Rect {
            x: 40.0,
            y: 24.0,
            w: 1200.0,
            h: 900.0,
        },
        padding: 20.0,
        ..Options::default()
    }
}

fn content(opts: &Options) -> Rect {
    Rect {
        x: opts.area.x + opts.padding,
        y: opts.area.y + opts.padding,
        w: opts.area.w - 2.0 * opts.padding,
        h: opts.area.h - 2.0 * opts.padding,
    }
}

fn assert_close(actual: f32, expected: f32, what: &str) {
    assert!(
        (actual - expected).abs() <= TOL,
        "{what}: expected {expected}, got {actual}"
    );
}

fn assert_aspects_preserved(tiles: &[Tile], placed: &[Placement]) {
    for (tile, p) in tiles.iter().zip(placed) {
        let got = p.rect.w / p.rect.h;
        let error = (got - tile.aspect).abs() / tile.aspect;
        assert!(
            error <= TOL,
            "tile {} aspect {} came out {got} ({}x{})",
            tile.key,
            tile.aspect,
            p.rect.w,
            p.rect.h
        );
    }
}

fn assert_no_overlaps(placed: &[Placement]) {
    for (i, a) in placed.iter().enumerate() {
        for b in &placed[i + 1..] {
            let overlap_w = (a.rect.x + a.rect.w).min(b.rect.x + b.rect.w) - a.rect.x.max(b.rect.x);
            let overlap_h = (a.rect.y + a.rect.h).min(b.rect.y + b.rect.h) - a.rect.y.max(b.rect.y);
            assert!(
                overlap_w <= TOL || overlap_h <= TOL,
                "tiles {} and {} overlap by {overlap_w} x {overlap_h}",
                a.key,
                b.key
            );
        }
    }
}

/// Placements in the order they were emitted, split wherever `y` changes.
fn rows(placed: &[Placement]) -> Vec<Vec<Placement>> {
    let mut out: Vec<Vec<Placement>> = Vec::new();
    for p in placed {
        match out.last_mut() {
            Some(row) if (row[0].rect.y - p.rect.y).abs() <= TOL => row.push(*p),
            _ => out.push(vec![*p]),
        }
    }
    out
}

#[test]
fn justified_rows_preserves_aspect() {
    let tiles = assorted();
    for last_row in [LastRow::Natural, LastRow::Justify] {
        let opts = Options {
            last_row,
            ..base_opts()
        };
        let placed = solve(Layout::JustifiedRows, &tiles, &opts).unwrap();
        assert_aspects_preserved(&tiles, &placed);
    }
}

#[test]
fn justified_rows_fills_width() {
    let tiles = assorted();
    let opts = Options {
        last_row: LastRow::Justify,
        ..base_opts()
    };
    let content = content(&opts);

    let placed = solve(Layout::JustifiedRows, &tiles, &opts).unwrap();
    let rows = rows(&placed);
    assert!(rows.len() > 1, "the sample should wrap into several rows");

    for row in &rows {
        let first = row.first().unwrap().rect;
        let last = row.last().unwrap().rect;
        assert_close(first.x, content.x, "row starts at the content edge");
        assert_close(
            last.x + last.w,
            content.x + content.w,
            "row ends at the content edge",
        );
        for p in row {
            assert_close(p.rect.h, first.h, "row height is uniform");
        }
    }
}

#[test]
fn justified_rows_natural_last_row_is_not_stretched() {
    // Two tiles that cannot fill the width at the target height: the row stays
    // open and `Natural` must leave it at its natural size, left-aligned.
    let tiles = tiles(&[1.5, 0.8]);
    let opts = Options {
        last_row: LastRow::Natural,
        ..base_opts()
    };
    let content = content(&opts);

    let natural = solve(Layout::JustifiedRows, &tiles, &opts).unwrap();
    assert_eq!(natural.len(), 2);
    for p in &natural {
        assert_close(p.rect.h, opts.target_row_height, "natural row height");
    }
    assert_close(natural[0].rect.x, content.x, "natural row is left-aligned");
    let natural_right = natural[1].rect.x + natural[1].rect.w;
    assert!(
        natural_right < content.x + content.w - 1.0,
        "the natural row should fall short of the width, ended at {natural_right}"
    );

    let justified = solve(
        Layout::JustifiedRows,
        &tiles,
        &Options {
            last_row: LastRow::Justify,
            ..opts
        },
    )
    .unwrap();
    assert_close(
        justified[1].rect.x + justified[1].rect.w,
        content.x + content.w,
        "justified last row fills the width",
    );
    assert!(
        justified[0].rect.h > natural[0].rect.h,
        "justifying the last row should have enlarged it"
    );
    assert_aspects_preserved(&tiles, &natural);
    assert_aspects_preserved(&tiles, &justified);
}

#[test]
fn grid_cells_are_uniform_and_tiles_are_fitted() {
    let tiles = assorted();
    let columns = 4usize;
    let opts = Options {
        columns: columns as u16,
        ..base_opts()
    };
    let content = content(&opts);
    let cell = (content.w - opts.gutter * (columns - 1) as f32) / columns as f32;

    let placed = solve(Layout::Grid, &tiles, &opts).unwrap();
    for (i, p) in placed.iter().enumerate() {
        let cell_x = content.x + (i % columns) as f32 * (cell + opts.gutter);
        let cell_y = content.y + (i / columns) as f32 * (cell + opts.gutter);

        assert!(
            p.rect.w <= cell + TOL && p.rect.h <= cell + TOL,
            "tile {} ({}x{}) overflows its {cell} cell",
            p.key,
            p.rect.w,
            p.rect.h
        );
        let filled = p.rect.w.max(p.rect.h);
        assert_close(filled, cell, "the fitted tile touches the cell on one axis");
        assert_close(p.rect.x, cell_x + 0.5 * (cell - p.rect.w), "centred in x");
        assert_close(p.rect.y, cell_y + 0.5 * (cell - p.rect.h), "centred in y");
    }
    assert_aspects_preserved(&tiles, &placed);
}

#[test]
fn masonry_balances_column_heights() {
    let opts = Options {
        columns: 4,
        ..base_opts()
    };
    let content = content(&opts);
    let width = (content.w - opts.gutter * 3.0) / 4.0;

    // Equal tiles distribute round-robin: every column takes the same count.
    let uniform = tiles(&[1.0; 12]);
    let placed = solve(Layout::Masonry, &uniform, &opts).unwrap();
    for column in 0..4usize {
        let x = content.x + column as f32 * (width + opts.gutter);
        let count = placed
            .iter()
            .filter(|p| (p.rect.x - x).abs() <= TOL)
            .count();
        assert_eq!(count, 3, "column {column} should hold 3 of 12 equal tiles");
    }

    // Mixed tiles: no column may end more than one tile-height taller than the
    // shortest, or the shortest-column rule was not applied.
    let mixed = assorted();
    let placed = solve(Layout::Masonry, &mixed, &opts).unwrap();
    let mut bottoms = [content.y; 4];
    for p in &placed {
        let column = ((p.rect.x - content.x) / (width + opts.gutter)).round() as usize;
        bottoms[column] = bottoms[column].max(p.rect.y + p.rect.h);
    }
    let tallest_tile = placed.iter().fold(0.0f32, |acc, p| acc.max(p.rect.h));
    let spread = bottoms.iter().cloned().fold(f32::MIN, f32::max)
        - bottoms.iter().cloned().fold(f32::MAX, f32::min);
    assert!(
        spread <= tallest_tile + opts.gutter + TOL,
        "column heights spread by {spread}, taller than the tallest tile {tallest_tile}"
    );
}

#[test]
fn no_overlaps_in_any_layout() {
    let tiles = assorted();
    for layout in LAYOUTS {
        for last_row in [LastRow::Natural, LastRow::Justify] {
            for columns in [0u16, 1, 3, 7] {
                let opts = Options {
                    columns,
                    last_row,
                    ..base_opts()
                };
                let placed = solve(layout, &tiles, &opts).unwrap();
                assert_eq!(placed.len(), tiles.len(), "{layout:?} dropped a tile");
                assert_no_overlaps(&placed);
            }
        }
    }
}

#[test]
fn aspect_is_preserved_in_any_layout() {
    let tiles = assorted();
    for layout in LAYOUTS {
        for columns in [0u16, 2, 5] {
            let opts = Options {
                columns,
                ..base_opts()
            };
            let placed = solve(layout, &tiles, &opts).unwrap();
            assert_aspects_preserved(&tiles, &placed);
        }
    }
}

#[test]
fn placements_stay_within_the_padded_area() {
    let tiles = assorted();
    for layout in LAYOUTS {
        for columns in [0u16, 2, 5] {
            let opts = Options {
                columns,
                ..base_opts()
            };
            let content = content(&opts);
            let placed = solve(layout, &tiles, &opts).unwrap();
            for p in &placed {
                assert!(
                    p.rect.x >= content.x - TOL
                        && p.rect.x + p.rect.w <= content.x + content.w + TOL,
                    "{layout:?}: tile {} spans {}..{}, outside {}..{}",
                    p.key,
                    p.rect.x,
                    p.rect.x + p.rect.w,
                    content.x,
                    content.x + content.w
                );
                assert!(
                    p.rect.y >= content.y - TOL,
                    "{layout:?}: tile above the area"
                );
            }
            let extent = extent(&placed);
            assert!(extent.x >= content.x - TOL && extent.w <= content.w + TOL);
        }
    }
}

#[test]
fn gutters_separate_neighbours_exactly() {
    let tiles = assorted();
    let opts = Options {
        gutter: 24.0,
        columns: 4,
        last_row: LastRow::Justify,
        ..base_opts()
    };

    let placed = solve(Layout::JustifiedRows, &tiles, &opts).unwrap();
    let rows = rows(&placed);
    for row in &rows {
        for pair in row.windows(2) {
            assert_close(
                pair[1].rect.x - (pair[0].rect.x + pair[0].rect.w),
                opts.gutter,
                "gutter between tiles in a row",
            );
        }
    }
    for pair in rows.windows(2) {
        let above = pair[0][0].rect;
        assert_close(
            pair[1][0].rect.y - (above.y + above.h),
            opts.gutter,
            "gutter between rows",
        );
    }

    // Masonry: the gutter is between tiles down a column and between columns.
    let content = content(&opts);
    let width = (content.w - opts.gutter * 3.0) / 4.0;
    let placed = solve(Layout::Masonry, &tiles, &opts).unwrap();
    for column in 0..4usize {
        let x = content.x + column as f32 * (width + opts.gutter);
        let stack: Vec<_> = placed
            .iter()
            .filter(|p| (p.rect.x - x).abs() <= TOL)
            .collect();
        for pair in stack.windows(2) {
            assert_close(
                pair[1].rect.y - (pair[0].rect.y + pair[0].rect.h),
                opts.gutter,
                "gutter down a masonry column",
            );
        }
    }
    // Grid cell pitch is asserted in `grid_cells_are_uniform_and_tiles_are_fitted`:
    // its tiles are fitted inside their cells, so the gap between two rects is
    // the gutter plus each tile's slack.
}

#[test]
fn output_order_matches_input_order() {
    let tiles = assorted();
    for layout in LAYOUTS {
        let placed = solve(layout, &tiles, &base_opts()).unwrap();
        let keys: Vec<u64> = placed.iter().map(|p| p.key).collect();
        let expected: Vec<u64> = tiles.iter().map(|t| t.key).collect();
        assert_eq!(keys, expected, "{layout:?} reordered its output");
    }
}

#[test]
fn solve_is_deterministic() {
    fn bits(placed: &[Placement]) -> Vec<(u64, u32, u32, u32, u32)> {
        placed
            .iter()
            .map(|p| {
                (
                    p.key,
                    p.rect.x.to_bits(),
                    p.rect.y.to_bits(),
                    p.rect.w.to_bits(),
                    p.rect.h.to_bits(),
                )
            })
            .collect()
    }

    let tiles = assorted();
    for layout in LAYOUTS {
        let opts = base_opts();
        let first = solve(layout, &tiles, &opts).unwrap();
        let second = solve(layout, &tiles, &opts).unwrap();
        assert_eq!(
            bits(&first),
            bits(&second),
            "{layout:?} is not deterministic"
        );
    }
}

#[test]
fn empty_input_is_an_error() {
    for layout in LAYOUTS {
        assert_eq!(
            solve(layout, &[], &base_opts()),
            Err(SolveError::EmptyInput)
        );
    }
}

#[test]
fn degenerate_aspect_is_an_error() {
    for bad in [0.0, -1.5, f32::NAN, f32::INFINITY] {
        let tiles = tiles(&[1.5, bad, 0.9]);
        for layout in LAYOUTS {
            assert_eq!(
                solve(layout, &tiles, &base_opts()),
                Err(SolveError::InvalidAspect),
                "{layout:?} accepted aspect {bad}"
            );
        }
    }
}

#[test]
fn degenerate_area_is_an_error() {
    let tiles = assorted();
    let bad = [
        Options {
            area: Rect {
                x: 0.0,
                y: 0.0,
                w: 0.0,
                h: 500.0,
            },
            ..Options::default()
        },
        Options {
            padding: 600.0,
            ..base_opts()
        },
        Options {
            gutter: -4.0,
            ..base_opts()
        },
        Options {
            target_row_height: 0.0,
            ..base_opts()
        },
        Options {
            area: Rect {
                x: 0.0,
                y: 0.0,
                w: f32::NAN,
                h: 500.0,
            },
            ..base_opts()
        },
    ];
    for opts in bad {
        assert_eq!(
            solve(Layout::JustifiedRows, &tiles, &opts),
            Err(SolveError::DegenerateArea),
            "accepted {opts:?}"
        );
    }
    // `target_row_height` belongs to justified rows only; the others survive it.
    let opts = Options {
        target_row_height: 0.0,
        ..base_opts()
    };
    assert!(solve(Layout::Grid, &tiles, &opts).is_ok());
    assert!(solve(Layout::Masonry, &tiles, &opts).is_ok());
}

#[test]
fn single_tile_centres_naturally() {
    let tiles = tiles(&[1.5]);
    let opts = base_opts();
    let content = content(&opts);

    // Grid: one cell, and the tile sits centred inside it with the slack split.
    let grid = solve(Layout::Grid, &tiles, &opts).unwrap();
    assert_eq!(grid.len(), 1);
    let cell = content.w;
    assert_close(grid[0].rect.w, cell, "the tile fills the cell's wide axis");
    assert_close(
        grid[0].rect.h,
        cell / 1.5,
        "and is not stretched to fill it",
    );
    assert_close(
        grid[0].rect.x,
        content.x,
        "the wide axis starts at the cell",
    );
    assert_close(
        grid[0].rect.y - content.y,
        (cell - grid[0].rect.h) / 2.0,
        "centred in y",
    );

    // Justified rows: a single tile is the last row, so `Natural` keeps it at
    // the target height rather than blowing it up to the full width.
    let justified = solve(Layout::JustifiedRows, &tiles, &opts).unwrap();
    assert_close(
        justified[0].rect.h,
        opts.target_row_height,
        "natural height",
    );
    assert_close(
        justified[0].rect.w,
        opts.target_row_height * 1.5,
        "natural width",
    );
    assert_close(justified[0].rect.x, content.x, "left-aligned");

    assert_aspects_preserved(&tiles, &grid);
    assert_aspects_preserved(&tiles, &justified);
}

#[test]
fn extent_bounds_every_placement() {
    assert_eq!(
        extent(&[]),
        Rect {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0
        }
    );

    let tiles = assorted();
    for layout in LAYOUTS {
        let placed = solve(layout, &tiles, &base_opts()).unwrap();
        let e = extent(&placed);
        for p in &placed {
            assert!(
                p.rect.x >= e.x - TOL
                    && p.rect.y >= e.y - TOL
                    && p.rect.x + p.rect.w <= e.x + e.w + TOL
                    && p.rect.y + p.rect.h <= e.y + e.h + TOL,
                "{layout:?}: tile {} escapes the extent",
                p.key
            );
        }
    }
}

#[test]
fn three_hundred_tiles_under_five_ms() {
    let aspects: Vec<f32> = (0..300)
        .map(|i| 0.4 + ((i * 37) % 23) as f32 / 10.0)
        .collect();
    let tiles = tiles(&aspects);
    let opts = Options {
        area: Rect {
            x: 0.0,
            y: 0.0,
            w: 2400.0,
            h: 1600.0,
        },
        columns: 6,
        ..Options::default()
    };

    for layout in LAYOUTS {
        let started = Instant::now();
        let placed = solve(layout, &tiles, &opts).unwrap();
        let elapsed = started.elapsed();
        assert_eq!(placed.len(), 300);
        println!("{layout:?}: 300 tiles in {elapsed:?}");
        assert!(
            elapsed.as_secs_f32() < 0.005,
            "{layout:?} took {elapsed:?} for 300 tiles"
        );
    }
}
