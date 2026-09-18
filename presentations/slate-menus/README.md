# Slate menus & workflows board

Editable map of every shipped Slate toolbar, menu, and right-click
surface. `build_board.py` writes native sticky notes and wire
connectors into `../slate-menus-workflows.slate` — not mermaid pictures.

```powershell
python presentations/slate-menus/build_board.py
cargo run -p slate --offline -- presentations/slate-menus-workflows.slate
```

- **Amber stickies** — hubs (the menu or surface)
- **Yellow stickies** — items (native sticky fill `#F4E38C`)
- **Pale stickies** — caveats / conditions
- **Solid wires** — opens / contains / leads to
- **Faint wires** — conditions (`Needs 2+ targets`, unwired panels, …)

Move, reword, and reconnect freely; this is a journaled board, not a
slideshow of images. Labels match current chrome (`menubar.rs`,
`tools.rs`, `board.rs`, `dock_advanced.rs`). Kit-derived flyout names
are user data. View / Lens dock toggles exist in Preferences but have
no wired panel body.

The `.mmd` files next to this README are a leftover inventory from the
first mermaid pass. The board no longer embeds them.
