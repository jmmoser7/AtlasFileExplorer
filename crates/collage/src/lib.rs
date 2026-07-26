//! # collage
//!
//! Pure collage arithmetic: a set of aspect ratios and a rectangle in, a
//! rectangle per tile out. No renderer, no document model, no image decoding —
//! `std` only, so the whole crate is testable on any platform.
//!
//! Three layouts, and nothing else (Article III): [`Layout::JustifiedRows`] is
//! what the collage command uses, [`Layout::Grid`] and [`Layout::Masonry`] are
//! the two arrangements the same selection is asked for often enough to name.
//!
//! Two properties hold everywhere in this crate:
//!
//! - **Aspect ratios are sacred.** A tile is scaled, never cropped and never
//!   distorted. Where a layout cannot honour a tile's aspect and fill its cell
//!   at the same time, the cell keeps space free — the image does not stretch.
//! - **The solver is deterministic.** Identical inputs produce identical
//!   output, in input order, with no hashing and no randomness anywhere.
//!
//! ## Quick start
//!
//! ```
//! use collage::{solve, extent, Layout, Options, Rect, Tile};
//!
//! let tiles = [
//!     Tile { key: 1, aspect: 1.5 },
//!     Tile { key: 2, aspect: 0.75 },
//!     Tile { key: 3, aspect: 1.0 },
//! ];
//! let opts = Options {
//!     area: Rect { x: 0.0, y: 0.0, w: 1200.0, h: 800.0 },
//!     ..Options::default()
//! };
//!
//! let placed = solve(Layout::JustifiedRows, &tiles, &opts).unwrap();
//! assert_eq!(placed.len(), 3);
//! assert_eq!(placed[0].key, 1);
//! assert!(extent(&placed).w <= 1200.0);
//! ```

mod grid;
mod justified;
mod masonry;

/// One input tile. `aspect` is width / height, finite and > 0.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tile {
    pub key: u64,
    pub aspect: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// Rows of uniform height, each row scaled to fill the width exactly.
    JustifiedRows,
    /// Uniform cells; every tile is aspect-fit inside its cell, centred.
    Grid,
    /// Fixed column count; tiles stack in the currently shortest column.
    Masonry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LastRow {
    /// Scale it like every other row.
    Justify,
    /// Keep the target height and left-align (the honest default).
    Natural,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Options {
    /// The rectangle to lay out in. Only its width constrains the solution:
    /// the solver fills width and grows downwards as far as it needs to.
    /// [`Options::default`] leaves this empty, so the caller must set it.
    pub area: Rect,
    pub gutter: f32,
    pub padding: f32,
    /// JustifiedRows.
    pub target_row_height: f32,
    /// Grid / Masonry; 0 = solver picks `ceil(sqrt(n))`.
    pub columns: u16,
    pub last_row: LastRow,
    /// Reserved: never enlarge a tile beyond this multiple of its natural
    /// size, `0` = unbounded. [`Tile`] carries an aspect ratio and no natural
    /// size, so no caller can supply one yet and this bound has no effect on
    /// any layout. It stays in the struct so the option survives the version
    /// of `Tile` that does carry pixel dimensions.
    pub max_scale: f32,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            area: Rect {
                x: 0.0,
                y: 0.0,
                w: 0.0,
                h: 0.0,
            },
            gutter: 16.0,
            padding: 0.0,
            target_row_height: 240.0,
            columns: 0,
            last_row: LastRow::Natural,
            max_scale: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub key: u64,
    pub rect: Rect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolveError {
    /// No tiles were supplied.
    EmptyInput,
    /// The content box is unusable: `area`, `gutter`, `padding`, or
    /// `target_row_height` is not finite or not positive, or the gutters and
    /// padding leave no width for the tiles.
    DegenerateArea,
    /// A tile's `aspect` is not a finite number greater than zero.
    InvalidAspect,
}

/// Lay `tiles` out inside `opts.area`.
///
/// Deterministic: identical inputs always produce identical output, in input
/// order. Never allocates inside a loop over tiles more than once per row.
///
/// The returned rectangles fill the content width and may extend below
/// `opts.area` — height is a result, not a constraint. Use [`extent`] for the
/// box the solution actually occupies.
pub fn solve(layout: Layout, tiles: &[Tile], opts: &Options) -> Result<Vec<Placement>, SolveError> {
    if tiles.is_empty() {
        return Err(SolveError::EmptyInput);
    }
    if tiles
        .iter()
        .any(|t| !t.aspect.is_finite() || t.aspect <= 0.0)
    {
        return Err(SolveError::InvalidAspect);
    }

    let content = content_rect(opts)?;
    match layout {
        Layout::JustifiedRows => justified::solve(tiles, opts, content),
        Layout::Grid => grid::solve(tiles, opts, content),
        Layout::Masonry => masonry::solve(tiles, opts, content),
    }
}

/// The bounding box the solution actually occupies (may be shorter than
/// `opts.area` — the solver fills width, not height).
pub fn extent(placements: &[Placement]) -> Rect {
    let Some(first) = placements.first() else {
        return Rect {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
        };
    };

    let mut min_x = first.rect.x;
    let mut min_y = first.rect.y;
    let mut max_x = first.rect.x + first.rect.w;
    let mut max_y = first.rect.y + first.rect.h;
    for p in &placements[1..] {
        min_x = min_x.min(p.rect.x);
        min_y = min_y.min(p.rect.y);
        max_x = max_x.max(p.rect.x + p.rect.w);
        max_y = max_y.max(p.rect.y + p.rect.h);
    }

    Rect {
        x: min_x,
        y: min_y,
        w: max_x - min_x,
        h: max_y - min_y,
    }
}

/// `area` inset by `padding`, after rejecting metrics that cannot describe a
/// content box.
fn content_rect(opts: &Options) -> Result<Rect, SolveError> {
    let finite = opts.area.x.is_finite()
        && opts.area.y.is_finite()
        && opts.area.w.is_finite()
        && opts.area.h.is_finite()
        && opts.gutter.is_finite()
        && opts.padding.is_finite();
    if !finite || opts.gutter < 0.0 || opts.padding < 0.0 {
        return Err(SolveError::DegenerateArea);
    }

    let w = opts.area.w - 2.0 * opts.padding;
    if w <= 0.0 {
        return Err(SolveError::DegenerateArea);
    }

    Ok(Rect {
        x: opts.area.x + opts.padding,
        y: opts.area.y + opts.padding,
        w,
        h: opts.area.h - 2.0 * opts.padding,
    })
}

/// Column count for the track layouts: the caller's, or `ceil(sqrt(n))`.
///
/// A request larger than `n` is honoured, not clamped — the caller asked for a
/// track count, and trailing empty tracks are the honest answer.
fn track_count(requested: u16, n: usize) -> usize {
    if requested > 0 {
        return requested as usize;
    }
    ((n as f64).sqrt().ceil() as usize).max(1)
}

/// Width of one of `tracks` equal tracks spanning `content_w` with `gutter`
/// between them.
fn track_width(content_w: f32, gutter: f32, tracks: usize) -> Result<f32, SolveError> {
    let w = (content_w - gutter * (tracks - 1) as f32) / tracks as f32;
    if w <= 0.0 {
        return Err(SolveError::DegenerateArea);
    }
    Ok(w)
}
