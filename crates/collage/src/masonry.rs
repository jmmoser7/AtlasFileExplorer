//! Masonry — fixed columns, each tile at column width, stacked in the
//! currently shortest column. Ties go to the lowest column index, which is
//! what keeps a masonry deterministic: the choice is an ordered scan, never a
//! hash lookup.

use crate::{track_count, track_width, Options, Placement, Rect, SolveError, Tile};

pub(crate) fn solve(
    tiles: &[Tile],
    opts: &Options,
    content: Rect,
) -> Result<Vec<Placement>, SolveError> {
    let columns = track_count(opts.columns, tiles.len());
    let width = track_width(content.w, opts.gutter, columns)?;

    let mut filled = vec![0.0f32; columns];
    let mut out = Vec::with_capacity(tiles.len());
    for tile in tiles {
        let column = shortest(&filled);
        let h = width / tile.aspect;

        out.push(Placement {
            key: tile.key,
            rect: Rect {
                x: content.x + column as f32 * (width + opts.gutter),
                y: content.y + filled[column],
                w: width,
                h,
            },
        });
        filled[column] += h + opts.gutter;
    }

    Ok(out)
}

fn shortest(filled: &[f32]) -> usize {
    let mut best = 0usize;
    for (i, &height) in filled.iter().enumerate().skip(1) {
        if height < filled[best] {
            best = i;
        }
    }
    best
}
