# File Atlas — UI architecture

The shell is layered so each concern has one home. When adding features, extend
the matching layer instead of growing `mod.rs`.

## Layer 0 — Top chrome (`ui/tabs.rs`)

- **Scope:** Browser-style tabs, global Undo/Redo only.
- **Rule:** Nothing that acts on the canvas lives here. Tabs sit above all tab
  workspaces.

## Layer 1 — Tab workspace

Everything below the tab bar belongs to the **active tab** (`TabState`):

| Region | Module | Role |
|--------|--------|------|
| Left tools rail | `ui/tools.rs` | Filters, display settings, workflow, tags — actions on the canvas. See `ui/SIDEBAR.md` for panel layout rules. |
| Canvas | `mod.rs` (`canvas`) | Infinite map, selection, thumbnails |
| Bottom readouts | `ui/readouts.rs` | Metrics, scan progress, cache status — read-only |
| Pre-warm dashboard | `ui/readouts.rs` (`prewarm_dashboard`) | Temporary panel above the readouts while a pre-warm runs: discovery, progress, speed control, cancel |
| Staging tray | `mod.rs` (`bottom_tray`) | Assignments / export (appears when needed) |
| Advanced | `ui/advanced.rs` | Floating window (pre-warm, shared cache) — opened from tools gear |

Per-tab state today: `root`, `cam`, `chrome` (which sub-panels are visible).
Filter/search values are still app-global for now; move into `TabState` when
multi-tab filter memory is needed.

## Extension points (`chrome.rs`)

- `ToolPanel` — register a new left-rail panel in the enum, add a `default_on`
  policy, implement a section in `ui/tools.rs`, wire the gear menu (automatic
  via `ToolPanel::ALL`).
- `ReadoutPanel` — same pattern in `ui/readouts.rs`.

## Backend (unchanged boundaries)

| Module | Responsibility |
|--------|----------------|
| `scanner.rs` | Directory walk |
| `index.rs` | SQLite persistence |
| `thumbs.rs` | Thumbnail workers + local + shared cache tiers |
| `tree.rs` | Layout + hit testing |
| `export.rs` / `journal.rs` | Organizing workflow |

## Shared project cache

- Discovered via template anchor `02 DESIGN/05 RESOURCES/03 DATA`.
- Stored at `…/03 DATA/.atlas-cache`.
- Published automatically whenever a thumbnail is read from or written to the
  local cache while a shared tier is active (`thumbs.rs` worker + `sync_to_shared`).
- Pre-warm creates repositories in both directions: walking *up* from the
  picked folder (picked inside a project) and while *descending* (picked a
  folder containing projects) — see `prewarm_walk` in `app/mod.rs`.
