//! Justified rows — the layout the collage command uses.
//!
//! Greedy, single pass, O(n): append tiles to the open row while the height
//! the row would have at full content width stays above `target_row_height`,
//! then close the row at the tile that brought it to or below the target and
//! scale it to span the content width exactly.
//!
//! A dynamic-programming linear partition (Knuth–Plass over row heights) would
//! minimise total deviation from the target height instead of taking the first
//! acceptable break, at O(n·k). It is deliberately not implemented: greedy is
//! what Flickr and Google Photos ship, the difference shows up only on the odd
//! row, and Article III says build the fraction that is used.

use crate::{LastRow, Options, Placement, Rect, SolveError, Tile};

pub(crate) fn solve(
    tiles: &[Tile],
    opts: &Options,
    content: Rect,
) -> Result<Vec<Placement>, SolveError> {
    if !opts.target_row_height.is_finite() || opts.target_row_height <= 0.0 {
        return Err(SolveError::DegenerateArea);
    }

    let mut out = Vec::with_capacity(tiles.len());
    let mut y = content.y;
    let mut start = 0usize;
    let mut aspect_sum = 0.0f32;

    for (i, tile) in tiles.iter().enumerate() {
        aspect_sum += tile.aspect;
        let height = full_width_height(content.w, opts.gutter, i + 1 - start, aspect_sum)?;
        if height <= opts.target_row_height {
            place_row(
                &mut out,
                &tiles[start..=i],
                content.x,
                y,
                opts.gutter,
                height,
            );
            y += height + opts.gutter;
            start = i + 1;
            aspect_sum = 0.0;
        }
    }

    // The trailing row never reached the target height, so justifying it would
    // enlarge it past every row above; `LastRow` decides whether that is what
    // the caller wants.
    if start < tiles.len() {
        let rest = &tiles[start..];
        let height = match opts.last_row {
            LastRow::Justify => full_width_height(content.w, opts.gutter, rest.len(), aspect_sum)?,
            LastRow::Natural => opts.target_row_height,
        };
        place_row(&mut out, rest, content.x, y, opts.gutter, height);
    }

    Ok(out)
}

/// Height of a row of `count` tiles whose aspects sum to `aspect_sum` when the
/// row spans `content_w` with `gutter` between neighbours.
fn full_width_height(
    content_w: f32,
    gutter: f32,
    count: usize,
    aspect_sum: f32,
) -> Result<f32, SolveError> {
    let available = content_w - gutter * (count - 1) as f32;
    if available <= 0.0 {
        return Err(SolveError::DegenerateArea);
    }
    Ok(available / aspect_sum)
}

fn place_row(out: &mut Vec<Placement>, row: &[Tile], x0: f32, y: f32, gutter: f32, height: f32) {
    let mut x = x0;
    for tile in row {
        let w = tile.aspect * height;
        out.push(Placement {
            key: tile.key,
            rect: Rect { x, y, w, h: height },
        });
        x += w + gutter;
    }
}
