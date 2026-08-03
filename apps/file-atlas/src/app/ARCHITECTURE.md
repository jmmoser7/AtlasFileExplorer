# File Atlas — UI architecture

The shell is layered so each concern has one home. When adding features, extend
the matching layer instead of growing `mod.rs`.

## Layer 0 — Unified top bar (`ui/menubar.rs`)

One Chrome-style strip replaces the old two-row header (title bar + tab strip).
The app icon is a **menu portal**: hover or click reveals **File** and **View**;
tabs sit inline to the right of the portal; window controls stay on the far
right. All painting lives in `atlas-shell` (`menubar::unified_top_bar`,
`tabs::tab_strip`) — see `crates/atlas-shell/TOPBAR.md`. This module only
adapts `AtlasApp` state to `UnifiedTopBarModel` / `MenuSpec` / `TabSpec` and
applies the returned actions.

**Home:** Orthogonal to work tabs (no tab selected while home is up). With no
folder mapped, the central panel is the shared Cover Flow (`atlas_shell::home`)
— recent folders, or template placeholders until templates ship. Re-open via
**File → Home**. Opening is the shelf; **New** starts a folder pick.

- The top bar is registered first so it remains outermost and spans the full
  viewport width; side and bottom panels begin below it.
- **Full-screen canvas** (`ChromeConfig::canvas_fullscreen`, toggled by F11,
  View → Full-screen canvas, or ⛶ in the canvas mini menu) suppresses the
  tools rail and readout bar; the unified top bar stays.

## Layer 1 — Tab workspace

Everything below the tab bar belongs to the **active tab** (`TabState`):

| Region | Module | Role |
|--------|--------|------|
| Floating tools dock | `ui/tools.rs` + `atlas-shell::dock` | Left-centered squircle icons for Filters, Display, Workflow, AI. Popovers reuse the former sidebar bodies without reserving canvas space. Free-text tagging lives in Slate; Atlas keeps destination assignment only. See `crates/atlas-shell/DOCK.md`. |
| Canvas | `mod.rs` (`canvas`) | Infinite map, selection, thumbnails. Draws the scanned tree only — the parent-folder chain above the mapped root was removed as clutter, so `map_bounds` is the tree's own bounds. Filters' *Zoom to matches* (`auto_zoom_after_filter`, on by default) flies the camera to the surviving files, edge-triggered on the framed bounds so scan batches do not yank it. |
| Bottom readouts | `ui/readouts.rs` | Metrics, scan progress, cache status — read-only apart from the timeline below. Padding, item spacing, row height, text size, and separator rules come from `[readouts]` in `ui-tokens.toml`; `text_size` is an `override_font_id` for the whole row so the counts and the root path scale together |
| Activity timeline | `ui/readouts.rs` + `atlas-shell::timeline` | Contribution graph **and** the date window on one axis: semantic zoom (weekday grid → staggered days → bucket strip → per-file dashes), range handles, discrete picks. Data is a cached `atlas_core::timeline::ActivityIndex`; the filter is a `TimePicks` set. Spec: `docs/keymap/specs/activity-timeline.md` |
| Pre-warm dashboard | `ui/readouts.rs` (`prewarm_dashboard`) | Temporary panel above the readouts while a pre-warm runs: discovery, progress, speed control, cancel |
| Staging tray | `mod.rs` (`bottom_tray`) | Assignments / export (appears when needed) |
| Advanced | `ui/advanced.rs` | Floating window (pre-warm, shared cache, commands reference) — opened from tools gear |
| Commands | `commands.rs` | Canonical keyboard/mouse bindings; see `COMMANDS.md` |

Per-tab state today: `id` (stable identity), `root`, `cam`, `chrome` (which
sub-panels are visible), and `parked` (heavyweight canvas snapshot while
inactive). Filter/search values are still app-global for now; move into
`TabState` when multi-tab filter memory is needed.

### Tab lifecycle invariants (multi-tab safety)

The heavyweight workspace (entries, tree, textures, selection…) is a single
set of fields on `AtlasApp` while a tab is active. On switch, that state is
**parked** onto `TabState::parked` and the target tab's parked snapshot is
restored in place — not torn down and reloaded. A quiet background refresh
then checks for disk changes. First visit to a tab still uses the
index-first `set_roots` path. These rules are load-bearing — breaking any of
them is an index-out-of-bounds crash the moment another tab's entries load:

1. **Every destructive root change goes through `reset_workspace()`**
   (called by `set_roots` / `set_root` / `clear_root`). It clears the entries
   vec, every parallel vector (`thumb_state`, `avg_color`, `file_match`), and
   *all* interaction state that carries entry ids: `selection`,
   `hovered_file`/`hovered_dir`, `last_selected_file`, `detail`, `menu_at`,
   `drag_chip`, `rubber_origin`, `pending_cam`, `pending_view`. Multi-folder
   opens share one canvas under a common ancestor (`root`) with
   `scan_seeds` listing the picked folders. New per-root state must be reset
   there (and included in `ParkedWorkspace`), not in the callers.
2. **Tab switches park/restore** via `park_active_workspace` /
   `activate_tab_workspace`. Returning to a tab must not blank the canvas.
3. **Async results are tagged and checked on arrival.** Scan batches and
   thumbnails carry a `generation`; the index load carries its `root`; the
   folder picker carries the requesting tab's `id`. A late result for a
   root/tab that is no longer current is dropped, never ingested into the
   active workspace.
4. **Owner is enrichment, never identity.** Discovery leaves it empty because the
   lookup is a security-descriptor query per file (`docs/performance.md`), and
   `queue_owner_pass` backfills it after the canvas is up. So nothing may treat a
   missing owner as a change: the refresh-mode diff ignores the field and carries
   resolved owners forward, or every rescan would declare the workspace stale and
   throw it away.
5. **Collapse is recorded, not derived.** The tree is rebuilt repeatedly while a
   scan streams, and `default_collapse` reads counts that are still growing, so
   collapse state cannot live only on the tree — a folder would re-decide itself
   mid-load, and a background build would undo a click made while it ran.
   `dir_collapsed` holds the decision per directory `rel` (parked with the
   workspace); `adopt_tree` reconciles every new tree against it, and anything
   that deliberately changes collapse calls `record_collapse_state()`.
6. **A streaming batch may not do whole-corpus work.** Batches arrive faster than
   frames, so an O(entries) pass on that path runs every frame of a load and gets
   worse as the folder grows. `absorb_new_entries` folds appended files into the
   filter aggregates; everything heavier waits for the tree's rebuild cadence.
   See `docs/performance.md` and the `load_jitter` benchmark.
7. **Tabs are referenced by stable `TabState::id` across async boundaries**
   — indices shift when tabs close.
8. **`active_tab` is always `< tabs.len()` and `tabs` is never empty** while
   not on the home shelf. `close_tab`/`switch_tab` maintain this;
   `active_chrome` clamps defensively.

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
| `scanner.rs` | Directory walk. Emits an empty `owner` on purpose — see below |
| `owners.rs` | Deferred owner resolution, consumed by `queue_owner_pass` |
| `index.rs` | SQLite persistence |
| `thumbs.rs` | Thumbnail workers + local + shared cache tiers (also read by Slate) |
| `rasterthumb.rs` | Photo fast path: embedded EXIF preview, else scaled decode |
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
