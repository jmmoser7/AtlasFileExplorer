# Floating canvas docks

Both File Atlas and Slate host a **single floating dock** of squircle icons
over the canvas. Dock chrome lives in `atlas-shell::dock`; apps supply items
and panel bodies only. Cross-app interaction notes: **`TOOLBARS.md`**.

## Ownership split

| Concern | Owner |
|---------|-------|
| Squircle geometry, icon painting, popover frame, stack layout, partition, tracers | `crates/atlas-shell/src/dock.rs` |
| Soft AA partition ribbon | `crates/atlas-shell/src/taper.rs` — see `PAINT.md` |
| Adjustable sizes/colors | `[dock]` in `ui-tokens.toml` |
| Which icons exist, labels, descriptions, icons, panel contents | Each app's `ui/tools.rs` |
| Dock edge preference (left vs bottom) | Preferences menu → `ChromePrefs` |

## Placement (user preference)

Preferences → **Dock · left edge** / **Dock · bottom edge**. Persisted per app
as `{app}-chrome.json` next to the index DB (`atlas_shell::prefs::ChromePrefs`).

| Default | App |
|---------|-----|
| Left edge, vertically centered | File Atlas |
| Bottom edge, horizontally centered | Slate |

Popovers open **rightward** from a left dock and **upward** from a bottom dock.

## Icon kinds & gestures

| Kind | Hover | Single click | Double click |
|------|-------|--------------|--------------|
| **Tool** / **Dashboard** | Title chip only | Volatile body (on-icon) | Pin → centered stack |
| **Action** | Title chip | Fire action | — |

- **Minimize** dismisses a volatile body or unpins a pinned one back
  to its icon. Icon-strip chrome is a **vertical four-dot column**
  (top → bottom): Minimize / Close, layout toggle, Advanced, Drop to
  canvas. Stacked captions use a **horizontal three-dot ellipsis** at
  the top-right (Minimize / Close, layout toggle, Advanced) — Drop
  stays on the strip column. Hover text for the group appears in one
  place, centered above the dots. Subsection folds still use ─;
  collapsing one must **not** dismiss the panel.
  Bottom-anchored popovers shrink upward, so hit-testing unions this
  frame's panel rect with last frame's — otherwise the click that
  collapsed a fold lands outside the new rect and is read as an
  outside dismiss.
- Hover never joins the pinned stack. Volatile bodies retire after
  `close_delay` when abandoned, or on Escape / outside click.
- Title chips are suppressed on pin/click until the pointer leaves, and never
  shown for icons that already have a pinned or volatile body open.
- Title chips appear **only** while the pointer is on the icon itself. An
  open flyout — volatile in front of the strip, or pinned above it — owns
  hover: chips from icons behind or beneath it clear immediately. Close-delay
  keeps a volatile *body* alive, not a leftover name chip. Chips paint on
  the Tooltip layer *after* panels so a primary-icon name sits in front of
  a pinned toolbar, never behind it.
- Every body (tool or dashboard) shares one dock-wide layout: stacked
  list or free-space icon strip. The **second dot** on any palette
  (stacked caption or strip cluster) switches **all** pinned palettes.
  Label is **Icon strip** in the list and **Stacked view** in the strip.
  Icon-strip mode is **fieldset groups** (thin rounded frame, label
  sitting in the top border) of secondary circular icons at
  `flyout_icon_scale` (65% of the dock). Tertiary toggle capsules stack
  two-high in the same vertical space as one secondary icon, sharing that
  datum. Hover text for the dots appears in **one place**,
  centered above the group — wide in X on the strip column, wide in Y
  on the caption ellipsis. Advanced opens a framed list of every tool
  in that palette with an on-strip tag (hidden tools stay off the main
  strip; `ChromePrefs.panel_strip_hidden`). Drop to canvas journals a
  `DockStrip` node — a copy that does not pin, unpin, or replace the
  baseline dock; there is no limit on how many copies exist.
  A click on a canvas-copy icon **arms** the command (same as the
  baseline dock); click-hold-drag anywhere on the node, including an
  icon, **moves** the copy. Instant actions (join, grid, snaps, color
  swap) still fire on click. Selection / hover chrome follows the
  painted fillet (`P1.dock-strip`, `node_screen_outline`). Hovering
  anywhere in a strip's vicinity holds the leader back to that
  palette's primary icon so scanning between squircles does not flicker.
  Strip icons use the same name + linger-description chip as the primary
  dock. Stacked-list tool rows use the Document settings language:
  sliding toggle pills (`sidebar_icon_row` / `flyout_list`), white
  section headings, muted subsection labels, choice chips, segmented
  REACH, and subtle dividers.
- Hover / selected icon fills are a subtle mix, not a full-opacity swap.
- Pins persist across sessions via `ChromePrefs.pinned_panels` where wired.

### Grouping rule (no visible separator)

List icons so **Tools are neighbors** and **Dashboards are neighbors**. Order
alone carries the grouping. Recommended: Tools → Actions → Dashboards.

## Sizing

Bodies size to their content: height grows with open fold sections up to the
canvas budget (no always-on `ScrollArea` — that freezes height; see
`TOOLBARS.md`), then width up to a fraction of the canvas while the open
subsections would still overflow (`popover_width` is the minimum, not a fixed
width). When a scrollbar is present it uses `drag_to_scroll(false)` so
dual-handle timelines and thin sliders keep pointer ownership. Large panel
bodies should fold subsections closed by default (`sidebar_fold_region`).

## Multi-panel stacking

Only **pinned** ids participate. Open panels pack along the dock's secondary
axis, then the group is translated so it stays **centered** on that canvas edge.

## Partition line & tracers

Soft AA tapered ribbon (`PAINT.md`). Border-hover on a **pinned** popover paints
an orthogonal tracer back to the icon.

## Extension

```rust
DockItem {
    id: "my.tool",
    label: "My tool",
    description: "Shown after prolonged Dashboard hover (faded in).",
    icon: DockIcon::Custom(icon_frame),
    kind: DockItemKind::Tool,
    active: false,
    visible: true,
    gap_before: false,
}
```

## Verification

```powershell
cargo test -p atlas-shell
cargo build --release -p native-file-atlas -p slate
```
