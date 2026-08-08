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
| Floating tools dock | `ui/tools.rs` + `atlas-shell::dock` | Left-centered squircle icons for Filters, Display, Mode, Workflow, AI. Popovers reuse the former sidebar bodies without reserving canvas space. Free-text tagging lives in Slate; Atlas keeps destination assignment only. See `crates/atlas-shell/DOCK.md`. |
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
7. **Watching a folder fill in is the feature; the lag is the bug.** Cards
   appear from the first batch and previews land while discovery is still
   running — on a slow root that *is* the experience, so never fix a stall by
   holding population back (deferring the network worker pool until the scan
   finished bought nothing and cost minutes of empty cards). Fix it where it
   actually is: get blocking work off the frame loop, and put a per-frame budget
   on anything whose volume the app does not control — texture uploads (24),
   watcher events (`FS_EVENTS_PER_FRAME`), tree rebuilds (`tree_rebuild_due`).
   Throttle *bulk* background work only: on-demand requests serve what is on
   screen and are bounded by the screen, while `queue_cache_warming` sweeps the
   whole corpus and is capped (`WARM_CONCURRENCY`) and deferred to scan end.
   Guarded by `previews_stream_while_the_folder_is_still_arriving` and
   `a_watcher_storm_is_spread_across_frames`.
   *One exception, and only one:* `run_shell_drag` blocks inside Win32's
   `DoDragDrop` while the user drags files out to another application.
   Synchronous and thread-bound is what that API is; the block lasts exactly as
   long as a gesture the user is physically performing, and it is not background
   work. Anything else that wants to block must justify itself the same way.
8. **Tabs are referenced by stable `TabState::id` across async boundaries**
   — indices shift when tabs close.
9. **`active_tab` is always `< tabs.len()` and `tabs` is never empty** while
   not on the home shelf. `close_tab`/`switch_tab` maintain this;
   `active_chrome` clamps defensively.
10. **Real filesystem edits are human-only, Edit-mode-only, and journaled.**
    View is the default on launch and root changes. Edit-mode rename/move/copy,
    new-folder, and delete dispatch through `atlas_core::fsops` off the frame
    loop; completed changes produce `journal::Action::Fs*` entries. Drag release
    on blank canvas or invalid folder targets is a null action with no journal
    entry. Agents must not invoke these write paths.

    Two ordering rules make the result visible and correct. **Claim the paths
    before writing them** (`start_fs_op_unchecked` → `remember_own_write`): the
    watcher can see a move land before the worker reports it, and an unclaimed
    event kills the entry the result is about to relocate. And **a completed
    edit forces the next rebuild** (`force_tree_rebuild`) rather than waiting on
    the streaming-scan cadence — a delete whose card lingers reads as a delete
    that failed.

    A **copy is accounted before it starts**: `cloud::copy_cloud_cost` walks the
    sources — every file, and every subtree of a dragged folder — reading
    directory entries only, and a non-zero result raises the download
    confirmation. The walk is I/O on a share, so it runs on its own thread
    (`cloud_audit` → `poll_cloud_audit`) and the readout says "checking" while it
    does; nothing about "would this download" may be asked on the frame loop.
11. **The left button acts on what is under the cursor; the right button moves
    the view.** Right-drag (and middle-drag) pans unconditionally — no mode, no
    hover target, and no other gesture may take it away, because on a full
    folder there is almost no empty canvas left to aim a pan at. Left-drag is a
    rubber band on empty canvas, and on a card it belongs to whichever of the
    three "pick it up" gestures is in force: Edit-mode filesystem drag, the
    linked-session carry to Slate, or the shell drag-out. Guarded by
    `right_drag_pans_even_when_it_starts_on_a_card` and
    `left_drag_on_empty_canvas_sweeps_a_selection`.

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
| `export.rs` / `fsops.rs` / `journal.rs` | Organizing workflow and human-directed filesystem edit journal |

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
