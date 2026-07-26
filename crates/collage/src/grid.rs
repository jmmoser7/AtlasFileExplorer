//! Uniform grid — every tile aspect-fits and centres inside an identical cell.
//!
//! Cells are square. The cell width follows from the column count and the
//! content width; nothing in [`Options`] describes a cell height, and squaring
//! the cell is the only shape that keeps every cell identical without
//! inventing one. A tile therefore leaves slack on two sides of its cell
//! rather than being cropped or stretched to fill it.

use crate::{track_count, track_width, Options, Placement, Rect, SolveError, Tile};

pub(crate) fn solve(
    tiles: &[Tile],
    opts: &Options,
    content: Rect,
) -> Result<Vec<Placement>, SolveError> {
    let columns = track_count(opts.columns, tiles.len());
    let cell = track_width(content.w, opts.gutter, columns)?;

    let mut out = Vec::with_capacity(tiles.len());
    for (i, tile) in tiles.iter().enumerate() {
        let cell_x = content.x + (i % columns) as f32 * (cell + opts.gutter);
        let cell_y = content.y + (i / columns) as f32 * (cell + opts.gutter);

        let (w, h) = if tile.aspect >= 1.0 {
            (cell, cell / tile.aspect)
        } else {
            (cell * tile.aspect, cell)
        };

        out.push(Placement {
            key: tile.key,
            rect: Rect {
                x: cell_x + 0.5 * (cell - w),
                y: cell_y + 0.5 * (cell - h),
                w,
                h,
            },
        });
    }

    Ok(out)
}
