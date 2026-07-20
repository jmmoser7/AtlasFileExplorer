# Floating canvas docks

Both File Atlas and Slate host a **single floating dock** of squircle icons
over the canvas. Dock chrome (shape, placement, hover/pin, multi-panel stack,
partition line, tracers) lives in `atlas-shell::dock`; apps supply items and
panel bodies only.

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

## Icon kinds

| Kind | Role | Hover | Click |
|------|------|-------|-------|
| **Tool** | Sub-tool flyouts (shapes, curves…) | Preview panel anchored on the icon; ease-in | Pin → joins centered stack |
| **Dashboard** | Settings bodies (tags, filters…) | Label chip above icon; description fades in; preview on icon | Pin → joins centered stack |
| **Action** | Toggles (grid, text tool…) | Same label chip as Dashboard (short name only) | Fires the action; no pin |

### Grouping rule (no visible separator)

List icons so **Tools are neighbors** and **Dashboards are neighbors**. Do
**not** draw a divider between groups — order alone carries the grouping.
`gap_before` exists for rare spacing needs; prefer contiguous kind blocks.

Recommended order in a mixed dock: Tools → Actions → Dashboards.

### Critical hover split

- **Hover previews never join the centered stack** — only **pinned** panels do.
- **Label chips** never reshuffle pinned panels; they sit above the hovered icon.
- **Action** and **Dashboard** share one chip implementation (no legacy tooltips).

## Interaction rules

- Click a Tool / Dashboard icon → toggle pin; click again to unpin.
- Multiple pinned panels stay open together (centered stack).
- Escape clears all open state (pins + hovers). Outside click clears hover
  first, then pins.
- Docks float over the canvas and must never reserve layout space.

## Multi-panel stacking

Only **pinned** ids participate. Open panels pack along the dock's secondary
axis, then the group is translated so it stays **centered** on that canvas edge.
Panel open uses a short ease-out (`panel_open_duration`).

## Partition line

A soft anti-aliased tapered ribbon sits between the icon strip and the canvas
(`taper::paint_tapered_ribbon` — see `PAINT.md`). Tunable under **Dock · Partition & tracers**.

## Hover tracers

Hovering the **border** of a **pinned** popover paints a faint orthogonal tracer
back to the initiating icon.

## Tokens and tuning

All geometry/colors live under `[dock]` in `crates/atlas-shell/ui-tokens.toml`
and in the `ui-tuner` dashboard. Key motion tokens: `describe_fade_duration`,
`panel_open_duration`, `hover_chip_gap`, `dashboard_describe_delay`.

Dev harness: set `ATLAS_DOCK_OPEN=<item id>` to force a panel open.

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

Adding a tool = one `DockItem` + one arm in the app's body/click match.

## Verification

```powershell
cargo test -p atlas-shell
cargo build --release -p native-file-atlas -p slate
```
