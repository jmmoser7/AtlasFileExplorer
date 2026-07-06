# File Atlas — UI architecture

The shell is layered so each concern has one home. When adding features, extend
the matching layer instead of growing `mod.rs`.

## Layer 0 — Top chrome (`ui/menubar.rs` + `ui/tabs.rs`)

- **Scope:** the Windows-style File/View menu bar (topmost, full width), then
  browser-style tabs. Global Undo/Redo stays keyboard-only.
- **Rule:** Nothing that acts on the canvas lives here. The menu bar spans the
  whole window; the tools rail is registered *before* the tab strip so the
  rail runs from the readout bar up to the menu bar, with tabs nested in the
  remaining width (see the panel-order comment in `mod.rs::update_app`).
- **Painting lives in `atlas-shell`** (`atlas_shell::menubar`,
  `atlas_shell::tabs`) so Slate renders identical chrome; these modules only
  adapt `AtlasApp` state to `MenuSpec`s / `TabSpec`s.
- **Full-screen canvas** (`ChromeConfig::canvas_fullscreen`, toggled by F11,
  View → Full-screen canvas, or ⛶ in the canvas mini menu) suppresses the
  tools rail and readout bar; menu bar and tabs stay.

## Layer 1 — Tab workspace

Everything below the tab bar belongs to the **active tab** (`TabState`):

| Region | Module | Role |
|--------|--------|------|
| Left tools rail | `ui/tools.rs` | Filters, display settings, workflow, AI (Cursor launcher + AI workspace; body shared with Slate via `crates/atlas-ai`) — actions on the canvas. Free-text tagging lives in Slate; Atlas keeps destination assignment only. See `ui/SIDEBAR.md` for panel layout rules. |
| Canvas | `mod.rs` (`canvas`) | Infinite map, selection, thumbnails |
| Bottom readouts | `ui/readouts.rs` | Metrics, scan progress, cache status — read-only |
| Pre-warm dashboard | `ui/readouts.rs` (`prewarm_dashboard`) | Temporary panel above the readouts while a pre-warm runs: discovery, progress, speed control, cancel |
| Staging tray | `mod.rs` (`bottom_tray`) | Assignments / export (appears when needed) |
| Advanced | `ui/advanced.rs` | Floating window (pre-warm, shared cache, commands reference) — opened from tools gear |
| Commands | `commands.rs` | Canonical keyboard/mouse bindings; see `COMMANDS.md` |

Per-tab state today: `id` (stable identity), `root`, `cam`, `chrome` (which
sub-panels are visible). Filter/search values are still app-global for now;
move into `TabState` when multi-tab filter memory is needed.

### Tab lifecycle invariants (multi-tab safety)

The heavyweight workspace (entries, tree, textures, selection…) is a single
set of fields on `AtlasApp` that is **swapped** on tab switch. That makes
these rules load-bearing — breaking any of them is an
index-out-of-bounds crash the moment another tab's entries load:

1. **Every root change goes through `reset_workspace()`** (called by
   `set_root` / `clear_root`). It clears the entries vec, every parallel
   vector (`thumb_state`, `avg_color`, `file_match`), and *all* interaction
   state that carries entry ids: `selection`, `hovered_file`/`hovered_dir`,
   `last_selected_file`, `detail`, `menu_at`, `drag_chip`, `rubber_origin`,
   `pending_cam`, `pending_view`. New per-root state must be reset there,
   not in the callers.
2. **Async results are tagged and checked on arrival.** Scan batches and
   thumbnails carry a `generation`; the index load carries its `root`; the
   folder picker carries the requesting tab's `id`. A late result for a
   root/tab that is no longer current is dropped (or parked on its owning
   tab), never ingested into the active workspace.
3. **Tabs are referenced by stable `TabState::id` across async boundaries**
   — indices shift when tabs close.
4. **`active_tab` is always `< tabs.len()` and `tabs` is never empty.**
   `close_tab`/`switch_tab` maintain this; `active_chrome` clamps
   defensively.

`src/app/tests.rs` drives the real frame loop headlessly (12-tab stress,
mid-scan switches, picker routing, pointer torture) and asserts these
invariants after every frame. Run with `cargo test app::tests`.

## Extension points (`chrome.rs`)

- `ToolPanel` — register a new left-rail panel in the enum, add a `default_on`
  policy, implement a section in `ui/tools.rs`, wire the gear menu (automatic
  via `ToolPanel::ALL`).
- `ReadoutPanel` — same pattern in `ui/readouts.rs`.

## Backend (unchanged boundaries — now in `crates/atlas-core`)

| Module | Responsibility |
|--------|----------------|
| `scanner.rs` | Directory walk |
| `index.rs` | SQLite persistence |
| `thumbs.rs` | Thumbnail workers + local + shared cache tiers (also read by Slate) |
| `tree.rs` | Layout + hit testing |
| `export.rs` / `journal.rs` | Organizing workflow |

## Linked Slate sessions

When Slate hosts Atlas as a second viewport, `AtlasApp.session` holds the
`atlas_session::SharedSession` bridge: the right-click menu grows a
"Slate tags" section, and click-hold-drag on thumbnails carries files toward
the Slate window. Standalone runs have `session: None` and none of this UI.

## Shared project cache

- Discovered via template anchor `02 DESIGN/05 RESOURCES/03 DATA`.
- Stored at `…/03 DATA/.atlas-cache`.
- Published automatically whenever a thumbnail is read from or written to the
  local cache while a shared tier is active (`thumbs.rs` worker + `sync_to_shared`).
- Pre-warm creates repositories in both directions: walking *up* from the
  picked folder (picked inside a project) and while *descending* (picked a
  folder containing projects) — see `prewarm_walk` in `app/mod.rs`.
