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
| Palette body: fieldset frames, caption row, dot cluster | `[dock.palette]` in `ui-tokens.toml` |
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
  to its icon. Icon-strip chrome is a **vertical dot column** on the
  right of the cluster; stacked captions carry the same dots as a
  **horizontal ellipsis** at the top-right. Both presentations offer
  the same actions in the same order — Minimize / Close, [layout
  toggle], Advanced, Drop to canvas — because they are one palette
  wearing two faces, and a view switch that silently removes a power
  is a trapdoor.
  **Minimize, Advanced, and Drop act on this palette, so every palette
  carries them (three dots). The layout toggle switches every pinned
  palette at once, so exactly one palette carries it (four dots): the
  last one along the dock — rightmost on a bottom dock, bottom-most on
  a left dock. When nothing is pinned, the volatile body carries it.**
  Hover text for the group appears in one place, centered above the
  dots. Subsection folds still use ─; collapsing one must **not**
  dismiss the panel.
  Bottom-anchored popovers shrink upward, so hit-testing unions this
  frame's panel rect with last frame's — otherwise the click that
  collapsed a fold lands outside the new rect and is read as an
  outside dismiss.
- A **pinned** primary icon keeps a denser outline than an undeployed
  one (tune **Pinned icon outline**). Hovering a pinned or volatile
  primary icon lights its hosted palettes, and hovering a palette
  lights the host icon — denser fill, heavier stroke, gray lighter in
  dark mode and darker in light mode. Tune under **Host ↔ palette
  hover**. Single click on a pinned icon collapses that palette
  (unpin); double click still pins.
- Hover **beside or below** the primary icon bar (arrow appears) and
  click to collapse the bar into a **blister** on the readout. Pinned
  palettes stay and drop down to fill the space. Hover the blister
  (up-arrow) and click to redeploy.
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
  list or free-space icon strip. The **toggle dot** switches **all**
  pinned palettes, which is why only one palette shows it.
  Label is **Icon strip** in the list and **Stacked view** in the strip.
  Icon-strip mode is **fieldset groups** of secondary circular icons at
  `flyout_icon_scale` (65% of the dock). Each group's **pallet** name
  (curves, ink, object snaps) sits in the top border of that frame.
  One **horizontal rule** runs under every pallet in the flyout, the
  width of those boxes, with the **category** (Shapes, Document
  settings) centered in the line — not a pallet name, and not once
  per box. When the pinned band no longer fits, icons wrap **inside**
  a pallet as an accordion: a sideways row first, overflow stepping
  up. Every open category stacks one column in the same round so
  none overlap while another is still a single row. Pallets sit on
  the category rule (the basedatum). Not a hex zigzag and not a
  fair-share tower of one-icon boxes. Tertiary toggle
  capsules stack two-high in the same vertical space as one secondary
  icon, sharing that datum.   Pallet type (`group_label_size` / `pallet_label_lift`) and category
  type (`category_label_size` / `group_label_lift`) tune separately;
  their fills (`title` / `category`) are independent of the rule
  stroke. The box stroke leaves a gap around the pallet name
  (`rule_text_gap`).
  Labeled icons (Object Snaps, grid…) use
  `labeled_text_size`. Hover chips lift by `hover_chip_gap`. Hover text for the dots appears in **one place**,
  centered above the group — wide in X on the strip column, wide in Y
  on the caption ellipsis.   Advanced opens a **fullscreen catalog canvas** — the
  same camera paradigm as a Slate board (pan, zoom, select), not a
  stacked list. It is window chrome, not a journaled workbook scene:
  a cool color cast and grid fill the screen edge-to-edge (no bezel
  or action bar). An × in the upper-right dismisses it; a green-dot
  legend at the lower-left names the on-toolbar badge. Tools sit as
  cards inside group frames. Camera matches the other infinite
  canvases: wheel zooms, Shift+wheel pans, right-drag / middle-drag /
  Space+left-drag pans, Ctrl+right-drag turbo-pans, left-drag on
  empty (or Shift+left-drag) marquees, arrows pan, +/− zoom, F fits.
  Right-click or hover-linger a card (or a selected set) for Add to
  toolbar / Remove from toolbar / Copy to clipboard / Duplicate /
  Use — linger is suppressed while a context menu is open or the
  pointer is on it. Copy to clipboard writes id + name;
  Duplicate (Slate) writes a new kit tool seeded from the card so a
  later edit can customize that portal type. Double-click still arms
  the tool. Hidden tools stay off the main strip
  (`ChromePrefs.panel_strip_hidden`). Drop to canvas journals a
  `DockStrip` node — a copy that does not pin, unpin, or replace the
  baseline dock; there is no limit on how many copies exist. A canvas
  copy is always an icon strip, so **both** presentations record their
  visible tool ids (`last_strip_tools`); dropping from stacked view
  must not place an empty node.
  A click on a canvas-copy icon **arms** the command (same as the
  baseline dock); click-hold-drag anywhere on the node, including an
  icon, **moves** the copy. Instant actions (join, grid, snaps, color
  swap) still fire on click. A canvas copy is the same fieldset strip
  as the docked flyout (`measure_icon_strip` / `paint_icon_strip_card`):
  fieldset groups and one category rule, no second card around them.
  It is a contain-scaled poster of the docked intrinsic size
  — resizing the node does not reflow or shrink icons relative to each
  other. Selection / hover chrome follows the fieldset fillet
  (`P1.dock-strip`, `node_screen_outline`). Hovering
  anywhere in a strip's vicinity holds the leader back to that
  palette's primary icon so scanning between squircles does not flicker.
  Strip icons use the same name + linger-description chip as the primary
  dock. Stacked-list **toggles** use sliding pills (`sidebar_icon_row`);
  tools stay circular glyphs (`sidebar_tool_row`). White
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
