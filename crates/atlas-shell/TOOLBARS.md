# Shared toolbar / dock interaction notes

Cross-app contract for File Atlas and Slate palette docks. Implementation lives
in `dock.rs` + panel bodies; apps must not diverge.

## Interaction model (palette icons)

| Gesture | Result |
|---------|--------|
| **Hover** | Translucent title chip (tool / dashboard / action name). No body. |
| **Linger** | After `dashboard_describe_delay`, the chip expands with `DockItem.description` (any kind that sets one). |
| **Single click** | **Volatile** body — use it, move away, it collapses after `close_delay` (or Escape / outside click). |
| **Double click** | **Pin** — body joins the centered stack and persists until minimize or icon unpin. |
| **Minimize (─)** | Upper-right of any open body: dismisses volatile, or unpins a pinned panel back to its icon. |

Hover previews must never reshuffle the pinned stack. Pins persist across
sessions via `ChromePrefs.pinned_panels` where apps wire that up.

**Hover chip rules:** a pin / click immediately suppresses the title chip
until the pointer leaves the strip (so a still-hovering cursor cannot flash
the name over the icon). Pinned icons and icons with a volatile body open
never show a hover chip — the panel caption already names them. Moving onto
a pinned / open icon also clears any prior chip immediately (the bar still
counts as "inside", so close-delay alone would leave the previous name stuck).
Chip fill uses `HOVER_CHIP_OPACITY` so the canvas stays readable underneath.

**Icon fill:** hover and selected / pinned states are a barely-perceptible
mix toward the hover/active tokens (`ICON_HOVER_MIX` / `ICON_ACTIVE_MIX` in
`dock.rs`), not a full-opacity fill swap.

## Sizing & subsections

A panel's **height** grows and shrinks with its open fold sections, up to the
canvas height less margins and chrome (`panel_body_max_height`). It is anchored
inside the canvas, so a full-height panel starts at the top margin instead of
being centered off-screen.

**Why this cannot be an always-on `ScrollArea`:** an egui `Area` locks its
`max_rect` to the previous frame's size. A `ScrollArea` then takes that stale
height as its viewport, so expanding a subsection only scrolls inside a fixed
box. Panels therefore lay the body out *unsized* while content fits the budget
(the Area grows with the folds). They wrap in a thin `ScrollArea` only after a
frame where content actually overflowed.

A panel's **width** then adapts: while the open complement of subsections still
overflows the height budget, the panel widens one step per frame (up to
`PANEL_MAX_WIDTH_FRAC` of the canvas) so wrapped rows — chip flows, label +
control rows, sliders — reflow into fewer lines. It narrows again once the
content comfortably fits. `adapt_panel_width` keeps a deliberate dead zone
between the grow and shrink thresholds; without it the two decisions oscillate
every frame. A scrollbar appears only when content still overflows at maximum
width. `popover_width` is therefore the panel's **minimum**, not its fixed
width, and hover / volatile / pinned panels share one width per panel id.

- Large toolbars (many subsections): subsections **start collapsed**. User
  expands a subsection to work in it.
- Small toolbars: full expansion is fine.
- Expand/collapse chrome uses Windows-like **minimize (─)** / **maximize (□)**
  glyphs, scaled to the control, upper-right of primary panels and subsection
  headers — not `+`/`−` text for these surfaces.

## Sliders vs scroll

Dual-handle timelines and thin sliders must own the pointer while hovered or
dragged. Parent `ScrollArea` drag-to-scroll and wheel must not bury handles
(see `timeline.rs` / thin sliders in `widgets.rs`).

## Activity timeline (File Atlas)

`atlas_shell::timeline::ActivityTimeline` — the contribution graph and the
range handles on **one** time axis. Full spec:
`docs/keymap/specs/activity-timeline.md`. Two rules that must not regress:

1. **One axis.** Cells, buckets, dashes, both handles, and the tick scale are
   all positioned by the same `x(t)`, and a cell's left edge is its bucket
   start. Never reintroduce a second time scale for the same state — that is
   what the Filters-dock date slider was, and it was removed.
2. **The graph never blanks.** It is built from the folder's files and stays
   mounted while scans populate it; moving the window rebuilds nothing.

Gestures split by what they address — **left selects time, right moves the
view**, matching the canvas, where right-drag already pans. Left-drag sweeps out
a new window even with both handles off screen; right-drag pans; wheel also pans
and Ctrl+wheel zooms at the cursor; double-click fits; handles crop; drag-between
scrubs (only once the window is narrower than the span, since a full-span window
has nowhere to slide); click / Shift+click / Ctrl+click select / extend / toggle a
bucket; and the reset button, "clear selection", and Esc all return to the whole
span. Zooming in staggers the weekday columns into per-day slots, then expands the
focused day into an adaptive bucket strip and finally per-file dashes.

Selection appearance is a mute of out-of-selection buckets (opacity +
saturation), not a heavy border. Geometry, morph thresholds, LOD, wheel feel,
**and every pad / offset** are tunable in `[activity_heatmap]`
(`ui-tokens.toml` / UI Tuner) — no magic numbers in the painter. The widget
also owns its own outer padding (`pad_top` / `row_gap` / `pad_bottom` /
`pad_right`), so a host must not wrap it in `add_space` calls to nudge it.

3. **One row of text, above the axis.** Legend, *clear selection*, then
   `N files · source · field · window`. Do not add a caption row under the
   scale or a source line above the control — that was the layout this
   replaced, and it spent more height on prose than the axis spent on data.

4. **One set of date labels: the tick scale under the rail.** There is no month
   strip over the graph. It named the same dates twice and could not agree with
   the scale — months snap to week columns, ticks snap to the scale's own step —
   so it read as a misalignment bug. Only the weekday gutter (`Mon` / `Wed` /
   `Fri`) labels the graph. The scale band auto-grows to fit its label, so date
   text is never clipped by the control's extents.

## Menu checkmarks

Visibility / toggle menus use a **checkmark prefix** (`✓`), never a filled
square checkbox, matching normal Windows menu UX (`gear_menu` / portal menus).
