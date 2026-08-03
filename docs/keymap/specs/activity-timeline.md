# Spec — the unified activity timeline (File Atlas readout, shared chrome)

Shipped. Implementation: `crates/atlas-shell/src/timeline.rs` (paint +
interaction), `crates/atlas-core/src/timeline.rs` (pure math), tokens in
`[activity_heatmap]` of `ui-tokens.toml`. Constitution: Art. X (chrome lives in
`atlas-shell`), Art. II (tessellate/allocate once, bucket by binary search),
Art. IV (honest models — the graph is extracted from file timestamps, never
smoothed or invented).

This surface is not a canvas tool, so it has no `docs/keymap/contracts/` entry
(that framework scopes to `tool` and `portal` families). It is the shipped form
of the two **temporal controllers** named for reuse in
`../contracts/portal-lens-repository.md` § Temporal controllers — the range
window and the contribution heatmap, merged into one control.

## 1. Why one axis

The graph and the range slider used to be two stacked timelines with
independent scales, which meant a handle was never under the buckets it
selected. They now share a single mapping:

```
x(t) = axis.left + (t - view_lo) / (view_hi - view_lo) * axis.width
```

Everything — grid cells, strip buckets, per-file dashes, both handles, the
tick scale, the drag-to-select preview — is positioned by that one function, so
correspondence is structural rather than maintained by hand. A cell's **left
edge is its bucket start**, so a handle parked on a cell edge selects from that
instant.

A 7×N weekday grid cannot honour this at day resolution: one column is a week
and its days stack vertically, so horizontal handle motion can only address a
week. That conflict is what the morph resolves.

## 2. Semantic zoom: two morph curves

Both are `atlas_core::timeline::morph` (smoothstep, direction-agnostic) over
**days visible**, and zoom is always anchored at the cursor.

| Visible span | Form |
|---|---|
| ≥ `stagger_begin_days` (31) | GitHub 7×N block: x by week, y by weekday. Cells shrink to slivers as week columns compress. Below 2 px per week a column is drawn as one aggregate bar rather than seven dishonest rows. |
| 31 → `stagger_full_days` (7) | **Stagger**: x lerps from the week column to each day's true time position — Sunday left through Saturday right, evenly spaced, weekday still stepping in y. The column shears into a staircase, and a handle can now address one day. |
| — | A cell's **width is always the slot it owns**: the week column while this reads as a grid, its own day once the staircase separates them. Nothing caps it at a square — a cap could only bind once a week column outgrew `cell + gap`, which is where the stagger already begins, so it never shaped the grid form it was meant to protect and only starved the morph. The square look at rest comes from the stagger onset, not from clamping width. The gap also yields below a ~12 px pitch (`slot_fill`), because a fixed 3 px gap at a 4 px pitch is almost all gap. |
| 7 → `expand_full_days` (1) | **Expansion**: y flattens toward the bar top and cell height grows to the full bar; days far from the cursor clip off the edges; an adaptive bucket strip (6 h → hour → 15 m → minute → second) crossfades in underneath. |
| — | Everything standing for a bucket — the day cell, the finer strip taking over from it, the dashes inside that — takes its height from one shared curve (`bar_height`) **and its band from the weekday row of its own day** (`bar_top`). The layers therefore stay concentric: a finer bucket resolves out of the day cell it is carving up rather than fading in at the top of the bar and sliding into place, and the staircase carries through to the fine grain until the rows converge at full expansion. |
| ≤ `file_tick_days` (1) | One dash per file timestamp, so hours/minutes/seconds separate individual files. Skipped above 4 000 stamps in view — the buckets already carry density. |

The stagger also starts on a **pixel** condition — `week_px > cell + cell_gap`,
i.e. the moment week columns outgrow a cell — whichever comes first. Without it
a short span on a wide panel would hold the grid form far past the point where a
week column is a legible unit, and each row would become one enormous flat bar.
It is also what keeps the aspect ratio sane: at full stagger seven day slots
tile the week column exactly, so the morph conserves ink rather than inventing
or losing it. This follows the LOD
precedent in the repository-lens contract (D23): key on pixels, not on zoom
alone, and expose the thresholds as dials because large monitors hold detail
legible longer.

Grain selection is `grain_for(visible, width, min_bucket_px)`: the smallest
ladder entry whose buckets clear `min_bucket_px` (8). Every sub-day grain
divides 86 400, so buckets align to UTC midnight instead of drifting.

## 3. Selection model

Two pieces of state, in a deliberate hierarchy:

- **The window** (`date_range_lo/hi`) is the coarse selection.
- **Picks** (`atlas_core::timeline::TimePicks`, a normalized disjoint
  half-open interval set) are *exceptions inside* the window.

A file matches when it is inside the window **and**, if picks exist, inside a
pick. Defining a new window clears picks — a fresh window is a fresh
selection, so widening a handle can never be silently limited by stale holes.

The first Ctrl-click seeds picks from the current window, then toggles the
clicked bucket. That is what makes it read as *deselect this one* rather than
*select only this one*, which was the previous behaviour's actual complaint:
"clear days" cleared the day set but left the window cropped, so the cells
still looked selected.

Out-of-selection buckets are **muted** (opacity × saturation toward luminance),
never outlined — a heavy border obscured the cells it marked.

## 3a. One info row

The control gets exactly one line of text, above the axis: the heat legend, the
*clear selection* button, then `N files · <source> · <field> · <window>`. It
replaced three lines (a source caption above, a header, and a range caption
below the scale) that between them spent more height on prose than the axis
spent on data. Consequences worth keeping:

- **No caption under the rail.** The tick scale already labels the axis, and a
  row of text below it was the widest band of whitespace in the readout.
- The window readout's precision follows **the window's own length**
  (`caption_snap`), not the zoom, so it does not flicker between dates and clock
  times while the view moves underneath it.
- The legend leads the row, next to the button that clears what the muting
  expresses — those two are the same thought.
- "· window" is gone: the row now *states* the window, so a word announcing that
  one exists was noise.

Together with tighter cell and band tokens this cut the control's height by
about 40%.

## 3b. Every pad is a token

Because the control is dense — four stacked bands in ~50 px — its whitespace is
load-bearing, and the right values are found by eye at the size the user
actually runs. So no pad or offset is a literal in the painter: outer padding
(`pad_top`, `row_gap`, `pad_bottom`, and the `pad_right` inset that stops the
axis short of the panel edge, mirroring the `day_label_width` gutter on the
left), the info row's `info_text` / `info_gap` / `info_button_pad_*` /
`info_row_height` and legend swatch size/gap, `label_font` and
`weekday_label_dx`, the rail's `rail_inset`, `handle_radius`, `handle_hit`, and
`grip_min_width`, and the scale's `scale_top_gap`, `scale_tick_len`, and
`scale_label_gap` all live in `[activity_heatmap]` and appear under *Activity
timeline · Padding & positions* in the tuner. The two horizontal insets sit
next to each other there — `day_label_width` on the left, `pad_right` on the
right — because splitting them across groups made the axis look asymmetric by
accident.

The graph's vertical size is exposed as a *graph height* that solves `cell` for
the fixed seven rows (`set_grid_height`). Sizing a square is the wrong question
when the real one is how many vertical pixels the readout bar can spare.

One exception to "every dimension is a dial": `scale_height` is a **floor**, not
a height. The band is `scale_top_gap + scale_tick_len + scale_label_gap +` one
measured line of `label_font`, or `scale_height` if that is larger. A pad that
can clip the only date readout in the control is not a pad worth exposing, so
pushing the labels down grows the band instead of cutting them off. The end
labels also center on ticks at the axis ends, so half of each overhangs the
axis; the scale paints clipped to the panel, not to the axis block, so those
read whole.

The widget adds its own outer padding, which keeps that height in one place
rather than split between the token file and whatever `add_space` the host
happened to write. Hosts must not pad around the call.

## 4. Gestures

Every row below is a `SPECS` entry under the `Timeline` category in
`apps/file-atlas/src/app/commands.rs`, so the Advanced window stays complete.

The buttons divide by what they address: **left selects time, right moves the
view.** Right-drag is already how the canvas pans (`canvas.pan`), so the gesture
carries over without being relearned, and it leaves the left button free for the
thing there is no other way to do — sweeping out a window across whatever is on
screen.

| Gesture | Effect |
|---|---|
| **Left-drag** | Define a new window from the press point, snapped to the grain on screen. Works anywhere over the graph or the bare rail, including with both handles scrolled off screen. Movement past 3 px is what separates it from a click. |
| **Right-drag** | Pan the view earlier / later, from anywhere over the control. |
| Wheel | Pan earlier / later. Wheel-down is later; flip with `pan_invert`. |
| Ctrl+wheel | Zoom around the cursor. Reads `zoom_delta` when egui folds Ctrl+wheel into it, else the scroll delta. |
| Double-click | Fit the whole span. |
| Drag a handle | Move that edge; snaps to the grain on screen (low edge takes the bucket start, high edge its last second, so dragging over a day includes that day). |
| Drag between handles | Scrub the window, width preserved. A minimum grip width keeps a tight window draggable. Claims the drag only while the window is narrower than the whole span — a full-span window has nowhere to slide, so the drag defines a new one instead. |
| Click a bucket | Window := that bucket. |
| Shift+click | Extend from the anchor. |
| Ctrl+click | Toggle that bucket in or out of the window (§3). |
| Reset button, "clear selection", Esc | Whole span, no picks. |

Drag kind is chosen on press and sticks until *that button* releases, so a
gesture survives leaving the zone it started in and the two buttons cannot
interfere if both go down. Hit order on a left press is handle → grip → window,
which is why the handles stay reachable inside the band they sit on.

The scroll wheel is consumed while the timeline is hovered so the enclosing
panel cannot also move, and the horizontal `ScrollArea` the graph used to sit
in is gone: the wheel *is* the scrub.

**Esc** registers `CancelLayer::Readout`, which sits below
`CancelLayer::Selection`: a canvas selection clears first, the next press
resets the timeline. The layer was added to `atlas-commands` rather than
overloading `Chrome`, so the pop order stays honest and testable.

## 5. Performance

- One sorted `Vec<i64>` per data revision (`ActivityIndex`), rebuilt only when
  the entry set, canvas selection, or date field changes — never when the
  window moves, so scrubbing cannot collapse the graph.
- Bucket counts are two `partition_point` calls; a frame paints a few hundred
  buckets and never scans the file set.
- Ramp maxima are precomputed per grain, so colours stay stable while panning
  instead of renormalizing to whatever is on screen.
- The cell list and the dash buffer are thread-local and reused, so panning and
  deep zoom do not allocate in the paint path.
- `readouts.rs` lends the index to the widget (`take` / put back) instead of
  cloning it — the vector is one entry per file.

## 6. Hit-testing

Cells are collected during the geometry pass and hit-tested from that list, so
the pointer resolves correctly at any morph state (during the stagger a cell's
day depends on both x and y). This also replaced a per-cell `ui.interact` —
thousands of interactions per frame on a multi-year folder.

`hit_cell` searches **frontmost first** and applies two rules:

- **Legible, not dominant** (`alpha ≥ LAYER_HIT_ALPHA`, 0.25). A finer bucket
  becomes selectable and hoverable as soon as it is visible enough to aim at,
  rather than after it wins the crossfade; a layer only beginning to fade in
  still cannot steal the pointer.
- **Empty intervals are inert.** A bucket that received no files is structure
  worth painting but not worth reporting — at depth most of a day is empty, so
  hovering them would answer "nothing here" over and over and turn a slow pan
  into a stutter of tooltips. Skipping them also lets the pointer fall through
  to the coarser bucket underneath, which does have something to say.

Hover feedback is a thin ring on the resolved bucket, faded by that layer's
alpha, so the thing that will be selected is unambiguous during a crossfade.
