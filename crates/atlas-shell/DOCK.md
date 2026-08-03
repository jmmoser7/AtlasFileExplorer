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

- **Minimize (─)** in the upper-right of any open body dismisses a volatile
  panel or unpins a pinned one back to its icon.
- Hover never joins the pinned stack. Volatile bodies retire after
  `close_delay` when abandoned, or on Escape / outside click.
- Title chips are suppressed on pin/click until the pointer leaves, and never
  shown for icons that already have a pinned or volatile body open.
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
