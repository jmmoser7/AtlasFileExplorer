# File Atlas — UI architecture

The shell is layered so each concern has one home. When adding features,
extend the matching module instead of growing `mod.rs`.

## Module map (`src/app/`)

| Module | Role | Notes |
|--------|------|-------|
| `mod.rs` | `AtlasApp` state, workspace/tab lifecycle, channel draining, filtering, organizing actions, frame pump | Integration point — everything else is `impl AtlasApp` blocks or free functions in child modules |
| `canvas.rs` | Camera math, canvas input/hit-testing, all world-space painting (branches, dir nodes, portals, file cards) | LOD thresholds and zoom limits stay as consts in `mod.rs` |
| `overlays.rs` | Staging tray, welcome screen, context menu, tag/assign editor, detail window, hover tip, drag ghost, toasts, texture eviction | Per-frame egui Areas/Windows; no state of their own |
| `theme.rs` | `Palette` + dark/light egui visuals | Take colors from `AtlasApp::palette()`; never hardcode |
| `platform.rs` | OS file-manager integration (open / reveal) | The one place for `cfg(windows)` shell-command forks — always provide a non-Windows fallback |
| `prewarm.rs` | Overnight shared-cache pre-warm (job, walk, controls) | Root-independent by design (`PINNED_GENERATION`) |
| `commands.rs` | Canonical input-binding registry | See `COMMANDS.md` before touching bindings |
| `chrome.rs` | Gear-menu panel registry (`ToolPanel`, `ReadoutPanel`) | Extension point for new rail/readout panels |
| `tests.rs` | Headless frame-loop harness + multi-tab stress tests | `cargo test app::tests` |
| `ui/` | Chrome: tab strip, tools rail, readouts bar, advanced window, shared widgets | See layer rules below |

**Conventions that keep this layout healthy:**

- New code goes in the module whose role matches; `mod.rs` only gains state
  fields, lifecycle logic, and thin wiring.
- Methods called across `app/` child modules are `pub(in crate::app)`;
  everything else stays private to its module.
- Shared UI helpers (`trunc`, `chip`, `group_digits`, sliders, timeline) live
  in `ui/widgets.rs` — never re-implement them locally.
- A warning-free `cargo clippy --all-targets` is the baseline; don't add
  `#[allow(...)]` without a comment explaining why.

## Layer 0 — Top chrome (`ui/tabs.rs`)

- **Scope:** Browser-style tabs, global Undo/Redo only.
- **Rule:** Nothing that acts on the canvas lives here. Tabs sit above all tab
  workspaces.

## Layer 1 — Tab workspace

Everything below the tab bar belongs to the **active tab** (`TabState`):

| Region | Module | Role |
|--------|--------|------|
| Left tools rail | `ui/tools.rs` | Filters, display settings, workflow, tags — actions on the canvas. Built from `ui/sidebar.rs` primitives; see `ui/SIDEBAR.md` for layout rules. |
| Canvas | `canvas.rs` | Infinite map, selection, thumbnails |
| Bottom readouts | `ui/readouts.rs` | Gear-togglable panels (metrics, activity heatmap) — read-only |
| Pre-warm dashboard | `ui/readouts.rs` (`prewarm_dashboard`) | Temporary panel above the readouts while a pre-warm runs: discovery, progress, speed control, cancel |
| Staging tray | `overlays.rs` (`bottom_tray`) | Assignments / export (appears when needed) |
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
   tab), never ingested into the active workspace. Pre-warm results are the
   deliberate exception: they are `PINNED_GENERATION` and survive root
   changes (`flush_thumb_results` keeps their accounting).
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
- `ReadoutPanel` — same pattern in `ui/readouts.rs` (metrics and the activity
  heatmap are the existing examples).

## Backend (flat modules under `src/`)

| Module | Responsibility |
|--------|----------------|
| `scanner.rs` | Parallel streaming directory walk |
| `index.rs` | SQLite persistence on a dedicated thread |
| `thumbs.rs` | Thumbnail workers + local + shared cache tiers |
| `tree.rs` | Layout + hit testing |
| `export.rs` / `journal.rs` | Organizing workflow (copy-only export, undo ledger) |
| `watcher.rs` | Filesystem watcher keeping the index live |
| `metadata.rs` | Owner / ctime lookups (`cfg(windows)` with fallbacks) |
| `pdf.rs` / `office.rs` / `threedm.rs` | Format-specific preview extractors |
| `types.rs` | `FileEntry`, `Family`, date/size helpers |

### Cross-platform rule

The crate targets Windows but **must build and pass tests on Linux** (cloud
agents and CI run there). Windows API usage is `#[cfg(windows)]`-gated with a
functional fallback, and the `windows` crate is a target-specific dependency.
`FileEntry::rel` and cache keys are backslash-normalized on every platform so
tree building, the index, and shared cache keys behave identically.

## Shared project cache

- Discovered via template anchor `02 DESIGN/05 RESOURCES/03 DATA`.
- Stored at `…/03 DATA/.atlas-cache`.
- Published automatically whenever a thumbnail is read from or written to the
  local cache while a shared tier is active (`thumbs.rs` worker + `sync_to_shared`).
- Pre-warm creates repositories in both directions: walking *up* from the
  picked folder (picked inside a project) and while *descending* (picked a
  folder containing projects) — see `prewarm_walk` in `app/prewarm.rs`.
