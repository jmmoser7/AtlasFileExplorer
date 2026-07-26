# collage

Pure layout arithmetic for arranging a selection of images into a collage.
Aspect ratios and a rectangle in, one rectangle per tile out.

Zero dependencies — `std` only. No renderer, no `slate-doc`, no image decoding
(Constitution Article I): the crate never learns what a tile *is*, only how
wide it is relative to its height. That is what makes it testable everywhere
and reusable by the board, by an export path, or by an agent command.

```rust
use collage::{solve, extent, Layout, LastRow, Options, Rect, Tile};

let tiles = [
    Tile { key: 1, aspect: 1.5 },
    Tile { key: 2, aspect: 0.75 },
    Tile { key: 3, aspect: 1.0 },
];
let opts = Options {
    area: Rect { x: 0.0, y: 0.0, w: 1200.0, h: 800.0 },
    gutter: 16.0,
    target_row_height: 240.0,
    last_row: LastRow::Natural,
    ..Options::default()
};

let placed = solve(Layout::JustifiedRows, &tiles, &opts).unwrap();
let box_used = extent(&placed);
```

`key` is opaque to the solver and comes back untouched — the caller maps it to
whatever a tile really is (a `NodeId`, an item index, a path hash).

## The three layouts

### `JustifiedRows`

Rows of uniform height, each scaled to span the content width exactly. This is
the collage layout; the other two are the arrangements the same selection gets
asked for often enough to name.

```
+--------+ +----+ +-----------+
|        | |    | |           |
+--------+ +----+ +-----------+
+-----+ +------------+ +----+
|     | |            | |    |
+-----+ +------------+ +----+
+------+ +---+                     <- last row, LastRow::Natural
|      | |   |
+------+ +---+
```

The algorithm is greedy and single-pass: append tiles to the open row while the
height the row would have at full width stays above `target_row_height`, then
close the row at the tile that brought it to or below the target. A
dynamic-programming linear partition would minimise total deviation from the
target height instead, at O(n·k); it is deliberately not implemented (greedy is
what Flickr and Google Photos ship, and Article III says build the fraction that
is used).

The trailing row never reached the target height, so justifying it would make it
taller than every row above. `LastRow::Natural` keeps it at
`target_row_height` and left-aligns it; `LastRow::Justify` stretches it like any
other row.

### `Grid`

`columns` (or `ceil(sqrt(n))`) identical square cells filling the content width.
Each tile is aspect-fit and centred in its cell, so a portrait leaves slack left
and right and a landscape leaves it above and below.

```
+-------+ +-------+ +-------+
| +---+ | |[=====]| | +---+ |
| |   | | |       | | |   | |
| +---+ | |[=====]| | +---+ |
+-------+ +-------+ +-------+
   cell      cell      cell
```

### `Masonry`

Fixed columns, every tile scaled to the column width, each appended to the
currently shortest column. Ties go to the lowest column index.

```
+------+ +------+ +------+
|      | |      | |      |
+------+ |      | +------+
+------+ +------+ +------+
|      | +------+ |      |
|      | |      | +------+
+------+ +------+ +------+
```

## Invariants

Asserted in `tests/layout.rs` across all three layouts, to `1e-3`:

1. **Aspect is preserved** for every placement, within `1e-3` relative error.
2. **No two placements overlap**, allowing `1e-3` of floating-point slop.
3. **Every placement lies within `area` horizontally**, inset by `padding`.
   Vertically the solution grows as far as it needs to — the solver fills
   width, not height, and `extent` reports what it used.
4. **Gaps between neighbours equal `gutter`.** In `Grid` this is the gap
   between *cells*: a fitted tile leaves slack inside its cell, so the gap
   between two rectangles is the gutter plus both tiles' slack.
5. **Output order matches input order**, always. No hashing, no sorting, no
   randomness: the same input produces bit-identical output.

## What this deliberately does not do

- **No cropping.** A tile's aspect ratio is sacred; a cell the tile does not
  fill keeps the space free rather than filling it with a cropped image.
- **No distortion.** Nothing is scaled non-uniformly, ever.
- No text flow, captions, labels, or any notion of a caption's height.
- No face detection, saliency, colour analysis, or content-aware anything.
- No packing that reorders the input to fit better.
- No randomness, no jitter, no "organic" scatter.
- No rotation.
- No knowledge of pixels: `Tile` carries an aspect ratio, so `Options::max_scale`
  (which bounds enlargement relative to a tile's *natural* size) has no effect
  until a future `Tile` carries pixel dimensions.
