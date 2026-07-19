# Floating canvas dock contract

File Atlas and Slate use floating squircle icon docks for canvas tools
instead of permanent rails or framed toolbars. There is exactly **one dock
per app**; never render a second toolbar system alongside it.

| App | Placement | Contents |
|-----|-----------|----------|
| File Atlas | Left edge, vertically centered | Filters, Display, Workflow, AI |
| Slate | Bottom edge, horizontally centered | Board tools (Board view), grid/snap/align, Tags, Selection, View, Lens (Lens view) |

Contextual overlays (Slate frame toolbar, 3D viewport tools) stay attached to
their objects and are not dock content.

## Ownership split

The shell (`crates/atlas-shell/src/dock.rs`) owns **all chrome**:

- squircle shape (true superellipse, tunable exponent), fills, borders;
- icon layout, spacing, grouping gaps, and flyout-direction markers;
- hover-open, click-pin, Escape/outside-click close, hover grace period;
- popover frame, header (item label + pinned indicator), scroll bounds;
- anchoring: popovers open **upward** from bottom docks and **rightward**
  from left docks, aligned to the hovered icon.

Apps own only **data and panel bodies**:

```rust
DockItem {
    id: "tool.frame",          // stable id: click result + body dispatch
    label: "Frame",            // popover header / tooltip — rename here
    icon: DockIcon::Custom(icon_frame), // or a built-in DockIcon variant
    kind: DockItemKind::Panel, // Panel = popover; Action = click-only
    active: tool == BoardTool::Frame,
    visible: board,            // hide per view / per state
    gap_before: false,         // visual grouping
}
```

`floating_dock(...)` returns the clicked item id (Panel items report clicks
too, so a click can both activate a tool and pin its flyout).

## Adding / renaming tools

1. Add (or edit) a `DockItem` in the app's `ui/tools.rs` dock function.
2. Panel items: add an arm in the body callback rendering the panel content.
3. Action items: add an arm in the click match.
4. Custom icons: a `fn(&Painter, Rect, Color32)` painter passed via
   `DockIcon::Custom` — Slate bridges `board_icons::paint_tool_icon` this
   way. Shared/general icons go into the shell's `DockIcon` enum.

Subcategories (flyouts) are ordinary popover bodies — Slate's Frame flyout
lists presets plus "Custom…"; rename or extend rows there.

## Interaction rules

- Hover an icon → its popover opens (Panel items). Hovering a sibling
  switches the popover. Leaving unpinned content closes after `close_delay`.
- Click → pins the popover (click again to close), and reports the id so
  tools activate immediately.
- Action items show a tooltip instead of a popover; Panel items never show
  a tooltip (the popover header carries the label).
- Docks float over the canvas and must never reserve layout space or
  intercept input outside their own icon/popover rects.

## Tokens and tuning

All geometry/colors live under `[dock]` in `crates/atlas-shell/ui-tokens.toml`
and in the `ui-tuner` dashboard: icon size/gap, squircle exponent, margins,
popover width/height/padding/radius/shadow, close delay, light/dark colors.

Dev harness: set `ATLAS_DOCK_OPEN=<item id>` to force a panel open (used for
screenshot verification).

## Verification

```powershell
cargo test -p atlas-shell
cargo build --release -p native-file-atlas -p slate
```

Capture both apps (`ATLAS_SHOT` / `SLATE_SHOT`) with and without
`ATLAS_DOCK_OPEN` after dock changes, in light and dark mode.
