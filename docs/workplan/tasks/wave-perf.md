# Wave P — performance and seam hygiene

Cards from audit №3 (`docs/audit/2026-08-15-health-*.md`). Article II work.
**No ratification gate.** One agent per card. Read
`docs/workplan/agent-brief.md` first.

Deviation rows in `docs/audit/deviations.md` are owned row-by-row: the
card named in *Closes with* may set that row's Status and Closed columns.

Two ordering constraints, and no others:

- P0.1 merges before P1.4 (`thumbs.rs`).
- P1.1 merges before P2.2 (`apps/slate/src/app/mod.rs`).

---

## P0.1 — Cache epoch hygiene and revisit fast-path {#p01}

**Owns:** `crates/atlas-core/src/thumbs.rs` (comments, `has_local` docs,
optional `cache_epoch()` accessor — no version bump, no extractor
change), `docs/performance.md` (version-5 section),
`crates/atlas-core/tests/folder_probe.rs`,
`apps/file-atlas/src/app/ui/advanced.rs` (readout),
`apps/file-atlas/src/app/mod.rs` functions **`ingest_loaded`** and
**`maybe_request_full`** only.
**Depends on:** nothing. **Size:** M.
**Closes:** nothing in the ledger (process fix). Updates `performance.md`.

### Why

`CACHE_KEY_VERSION` is `"5"`. The 8 August bump was for SVG. Every other
format's on-disk JPEG is now an orphan, which is why previously warmed
folders feel cold. Revisit (`ingest_loaded`) also resets every card to
`NotAsked`, so even a disk-hot folder re-uploads at 24 textures/frame.

### Do

1. Add `pub fn cache_epoch() -> &'static str` returning
   `CACHE_KEY_VERSION`. Show it on Advanced → cache status
   (`advanced.rs`) as `thumb cache v5`.
2. Rewrite the version comment in `thumbs.rs` so the next agent cannot
   treat a new extractor as a reason to bump. Rule: bump only when
   *existing* `{key}.jpg` bytes would be wrong (the icon episode). Adding
   a format is not that.
3. Update `docs/performance.md` "Why the cache version had to move to 4"
   with a following section: version 5 was SVG-only and invalidated
   everything; that was a mistake; machines must re-warm; do not dual-read
   v4 keys.
4. `folder_probe`: print epoch, key, `has_local`, icon-tier presence.
5. Revisit fast-path: after `ingest_loaded` installs a snapshot, do **not**
   `stat` the corpus on the UI thread. Queue on-demand (`maybe_request_full`)
   for the current visible LOD as today. Optionally, spawn a background
   thread that checks `has_local` for the first screenful and pushes those
   keys onto the **hot** queue (not warm). Warm still writes no pixels.
6. Optional orphan note in Advanced: "v4 files in this folder are ignored;
   they will be replaced as cards are viewed." Do not delete the user's
   cache from this card (a prune is a later card if the human asks).

### Accept

- [ ] `cache_epoch()` equals `"5"`; a unit test locks it so a silent bump
      fails.
- [ ] Advanced readout shows the epoch.
- [ ] `docs/performance.md` describes version 5 accurately.
- [ ] `folder_probe` prints epoch + `has_local`.
- [ ] `ingest_loaded` still resets GPU state (indices changed) but a
      disk-hot visible card is requested on the hot path, not the warm
      path.
- [ ] `previews_stream_while_the_folder_is_still_arriving` still passes.
- [ ] `CACHE_KEY_VERSION` is still `"5"`.

### Forbidden

Bumping the version. Dual-reading v4 keys. Deleting the thumbs directory.
Blocking the frame loop on `has_local` for the whole corpus. Deferring
the thumb pool until scan Done. Editing `extract_thumbnail` or COM code.

---

## P0.2 — Web portal capture off-thread + LOD hysteresis {#p02}

**Owns:** `apps/slate/src/app/board_web.rs`,
`apps/slate/src/app/board_web_win.rs`.
**Depends on:** nothing. **Size:** M.
**Closes:** DV-13.

### Why

`web_pump` calls `host.capture_poster` then `host.evict` on the frame
loop when a live portal is demoted. On Windows that is a D3D11 map +
CPU copy (`read_frame`). A fast zoom across 160 px also creates and
destroys WebView2 with no hysteresis. Contract D21 / D29 already require
async, generation-tagged capture.

### Do

1. Queue eviction capture to a worker (or reuse the existing
   generation-tagged poll pattern in `board_web.rs`). Keep the last
   poster visible at `stale_alpha` while in flight. Discard stale
   generations. Then `evict`.
2. Hysteresis: eligible at `LIVE_MIN_PX` (160), demote only after
   `height_px < 128` (or two consecutive frames below 160). Focused
   portal still ignores the gate. Do not change `LIVE_POOL`.
3. Unit-test admission with an oscillating `height_px` sequence
   (159, 161, 159, 161) — must not evict/admit every frame.
4. Keep `UPLOADS_PER_FRAME = 2`. Cap `backlog` length (drop oldest).
5. Linux / fake host: tests stay on the fake host. Windows
   `eviction_releases_the_view` remains `#[ignore]` unless you can run it.

### Accept

- [ ] No `capture_poster` / `read_frame` on the `web_pump` call stack
      (search the function; a worker or channel is required).
- [ ] Oscillating-height unit test passes.
- [ ] Existing GP8 / `a_hundred_tiled_pages_admit_only_the_pool` still
      pass; admitted set still ≤ 6.
- [ ] DV-13 Status `closed`, Closed = this commit.
- [ ] `LIVE_POOL` still 6, `LIVE_MIN_PX` still 160.

### Forbidden

Raising `LIVE_POOL`. Journaling poster pixels. Putting WebView2 types in
`slate-doc` or `atlas-core`. Editing `board.rs` paint. Changing the
contract matrix (an Implementation note that D21 is now met is allowed).

---

## P0.3 — Watcher metadata off the frame loop {#p03}

**Owns:** `apps/file-atlas/src/app/mod.rs` functions **`apply_fs_change`**
and the watcher / `scan_rx` loops inside **`drain_channels`** only.
`crates/atlas-core/src/watcher.rs` only if you must add a variant — prefer
not to.
**Depends on:** nothing. **Size:** M.
**Closes:** DV-17.

### Why

`apply_fs_change(Upsert)` calls `scanner::stat_file` on the UI thread, up
to `FS_EVENTS_PER_FRAME` (32) times. On a share that is the freeze
`docs/performance.md` already documented for `Tree::build`. `scan_rx` is
drained to exhaustion. `FsChange::Rescan` is `{}`.

### Do

1. Stop calling `stat_file` inside `apply_fs_change`. Enqueue paths to an
   existing or new worker; apply the resulting `FileEntry` on a later
   frame, generation-tagged. The per-frame event budget stays 32
   *admissions*, not 32 stats.
2. Give `scan_rx` a per-frame batch budget (start at 2 `ScanMsg::Batch`
   or ~512 new entries, whichever you can test). Remainder stays in the
   channel; `request_repaint`.
3. Implement `FsChange::Rescan`: one `start_quiet_refresh`, not a storm
   of upserts. On `fs_backlog` overflow, same.
4. Keep `a_watcher_storm_is_spread_across_frames`. Add: overflow triggers
   a quiet refresh; `Rescan` is not a no-op.

### Accept

- [ ] `apply_fs_change` contains no `stat_file` / `metadata` call.
- [ ] `a_watcher_storm_is_spread_across_frames` still passes.
- [ ] New test: `FsChange::Rescan` requests a quiet refresh.
- [ ] `scan_rx` loop has a documented per-frame cap.
- [ ] `previews_stream_while_the_folder_is_still_arriving` still passes.
- [ ] DV-17 closed.

### Forbidden

Deferring scan population. Doing owner lookup in the watcher path
(`watcher_stat_does_not_pay_for_owner` must stay true). Editing
`ingest_loaded` / `maybe_request_full` (P0.1). Touching `thumbs.rs`.

---

## P1.1 — Slate thumb drain budget and LRU {#p11}

**Owns:** `apps/slate/src/app/mod.rs` — `drain_thumbs`, `textures`,
`thumb_pixels`, and any small helpers you add next to them.
**Depends on:** nothing. **Size:** S.
**Closes:** DV-16.

### Why

Atlas uploads ≤ 24 textures/frame and evicts at `TEXTURE_CAP = 1100`.
Slate's `drain_thumbs` is `while let Ok` with no cap, and
`thumb_pixels` / `textures` grow for every key across every tab.

### Do

1. Cap drain at 24 uploads/frame (same number as Atlas, or a named
   constant `THUMB_UPLOADS_PER_FRAME`). `request_repaint` if more remain.
2. LRU-evict `textures` and `thumb_pixels` together, byte- or
   count-capped (1100 entries or the preview budget, pick one and
   document it). Image-FX must survive eviction by re-reading the thumb
   cache, not by assuming `thumb_pixels` is immortal.
3. Tests: burst of N results uploads ≤ 24 in one `drain_thumbs`; a 1200th
   distinct key evicts an old one.

### Accept

- [ ] `drain_thumbs` cannot upload more than the cap in one call.
- [ ] `thumb_pixels.len()` and `textures.len()` have a documented ceiling.
- [ ] Existing preview / FX tests still pass.
- [ ] DV-16 closed.

### Forbidden

Clearing caches on every tab switch (that is a different card). Editing
`preview.rs` (P1.6). Editing `open_doc_at` / `save_doc_to` (P2.2).

---

## P1.2 — Path mesh LRU, fill cache, allocation-free hash {#p12}

**Owns:** `apps/slate/src/app/board_path.rs`.
**Depends on:** nothing. **Size:** S.
**Closes:** the fill-flatten half of DV-15 (P1.3 closes the paint-cull half;
leave DV-15 open unless both have merged — if you land second and P1.3 is
already closed, you may close DV-15).

### Why

`PathMeshCache` does `map.clear()` at 256. `path_content_hash` allocates a
`format!(Debug)` string every lookup. Closed fills `flatten()` every
frame (`paint_path_shape`). Fast zoom crosses ~29 buckets and remeshes.

### Do

1. Replace nuclear clear with LRU (evict oldest `(NodeId, key)`).
2. Hash `PathData` / stroke / rect / bucket with `Hash` impls or
   `DefaultHasher` on fields — no `format!`.
3. Cache closed fills with the same key policy (include zoom bucket).
4. Keep `zoom_bucket(z) = (z * 8).round()`. Do not coarsen it on this
   card (that is a feel change).

### Accept

- [ ] No `map.clear()` in `get_or_tessellate`.
- [ ] Unit test: hash is stable for identical inputs; differs on bucket.
- [ ] Unit test: 300 distinct keys leave the cache at cap, not empty.
- [ ] Existing path tests pass.

### Forbidden

Per-frame `flatten()` for authored fills. Changing stroke math in
`vector-ink`. Editing `board.rs` except a one-line call if the fill API
moves (prefer not to).

---

## P1.3 — Viewport-culled board paint {#p13}

**Owns:** `apps/slate/src/app/board.rs` **paint loop only**
(the block that clones `scene.nodes` and paints frames then non-frames,
today ~2146–2181). You may add a private helper in the same file.
**Depends on:** T0.8 already merged (`Scene::query_rect`). **Size:** M.
**Closes:** the paint-cull half of DV-15 (see P1.2).

### Why

`Scene` has a spatial index. Board paint still
`nodes.iter().filter(...).cloned().collect()` for the whole scene every
frame. Search-dim and eraser-dim can apply to the culled set.

### Do

1. `query_rect` the visible world rect plus a small margin (off-screen
   stroke width / selection halo). Paint only those nodes, frames first.
2. Keep hidden-node skip, search dim, eraser dim, portal-focus dim.
3. Hit-test / marquee are **not** this card unless they share the helper
   and you can do it in the same ~80 lines. Prefer paint only.
4. Test: a scene of ≥ 200 nodes with the camera on one of them paints
   (or considers) a proper subset. Headless harness in `apps/slate/src/app/tests.rs`
   is allowed; do not take ownership of the whole tests file — add one
   test module function.

### Accept

- [ ] The paint path no longer clones the full `nodes` vec unconditionally.
- [ ] A test asserts cull: off-screen nodes are not in the paint set.
- [ ] Presentation mode and export are unchanged (they are not this loop).

### Forbidden

Editing `slate-doc`. Changing tool gesture code. Touching `board_web.rs`.
A drive-by refactor of `board.rs`.

---

## P1.4 — Bound the warm queue {#p14}

**Owns:** `crates/atlas-core/src/thumbs.rs` queue methods only
(`request_warm`, `shed_excess` or a warm equivalent, constants).
**Depends on:** P0.1 merged. **Size:** S.

### Why

Hot queue is capped at 512. Warm and slow `VecDeque`s are not. After
scan, `queue_cache_warming` can push tens of thousands of jobs.

### Do

1. Cap `warm` (propose 4096) and `slow` (propose 8192). Drop oldest or
   skip enqueue if the key is already queued. Document the policy next
   to `HOT_QUEUE_CAP`.
2. Do not lower `WARM_CONCURRENCY`.
3. Tests: enqueue 5000 warm jobs → len ≤ cap; on-demand hot pop still
   precedes warm.

### Accept

- [ ] Warm / slow lengths cannot exceed the caps.
- [ ] Existing shed / generation tests pass.
- [ ] `CACHE_KEY_VERSION` still `"5"`.

### Forbidden

Changing extractors. Changing `CACHE_KEY_VERSION`. Deferring warm until
some other signal than scan Done (that policy stays).

---

## P1.5 — Cache Grid and Venn layout {#p15}

**Owns:** `apps/slate/src/app/canvas.rs`.
**Depends on:** nothing. **Size:** S.

### Why

`grid_layout()` / `venn_layout_now()` run every frame over every item.
Atlas already forbids whole-corpus work on the batch path. Same bug,
other app.

### Do

1. Store the last `Layout` on the tab or app, keyed by a cheap
   fingerprint (item count + assignments generation + focused tags +
   view kind + canvas size bucket). Recompute on mismatch only.
2. Camera pan/zoom must **not** invalidate layout (world space).
3. Test: two paints with no doc change call the builder once.

### Accept

- [ ] Layout builder is not invoked on an unchanged doc + size.
- [ ] Tag toggle / add-item / view-kind switch still relayout.
- [ ] Existing grid/venn tests pass.

### Forbidden

Per-frame `combination_buckets` "just in case." Editing `board.rs`.
Changing circle-pack.

---

## P1.6 — Preview decode cancellation {#p16}

**Owns:** `crates/atlas-core/src/preview.rs`,
`apps/slate/src/app/preview.rs`.
**Depends on:** nothing. **Size:** S.

### Why

Fast zoom queues 256 → 512 → 1024 → 2048. Workers are not cancelled.
Stale lower tiers still decode.

### Do

1. Generation or target-tier token on each request. Worker drops the
   job if a newer token for that key exists *before* the expensive
   decode (check once at pop).
2. Keep LIFO. Keep `REQUESTS_PER_FRAME = 3` unless you have a test that
   says otherwise.
3. Extend existing preview tests: a superseded tier does not land in
   `preview_cache` as the best entry.

### Accept

- [ ] `full_res_preview_upgrades_and_evicts` still passes.
- [ ] New test: enqueue 256 then 1024 for one key; cache best is 1024;
      a late 256 result is discarded.
- [ ] Atlas still does not use `PreviewPool` (do not wire it).

### Forbidden

Putting preview work on the Atlas frame loop. Reading dehydrated cloud
files (`is_dehydrated` stays in front of every byte read).

---

## P2.1 — Shared display params {#p21}

**Owns:** new `crates/atlas-core/src/display.rs` (or `camera.rs` if you
prefer that name), `crates/atlas-core/src/lib.rs` export, and the
**call sites listed here only**:

- `apps/file-atlas/src/app/mod.rs` — `ZOOM_MIN` / `ZOOM_MAX` / default `z`
  and the wheel expression in the canvas zoom arm
- `apps/slate/src/app/canvas.rs` — `ZOOM_MIN` / `ZOOM_MAX` / wheel
- `apps/slate/src/app/board.rs` — the duplicate `ZOOM_MIN` / `ZOOM_MAX` /
  wheel (constants and the one wheel line)

**Depends on:** P0 cards merged (rebase). **Size:** L.

### Why

Zoom limits, wheel formulas, and upload budgets are triplicated with
different values. Unification of backend display parameters is a named
goal of this audit. Two *profiles*, one module, no egui (Art. I).

### Do

1. Pure types: `CameraState { offset: [f32; 2], z: f32 }`,
   `ZoomProfile { min, max, default_z, wheel }`,
   `fn clamp_zoom`, `fn apply_wheel(profile, z, scroll) -> f32`.
2. Profiles: `ATLAS_TREE` (0.02, 32.0, 0.6, Atlas's `exp` wheel) and
   `SLATE_CANVAS` (0.05, 3.5, 0.8, Slate's linear wheel). **Do not
   silently make the apps feel the same.** Document why the numbers
   differ (directory-tag fill vs board detail).
3. Optional: `TextureBudget { uploads_per_frame, resident_cap }` as
   constants the apps *read*. Do not rewrite LRU in this card.
4. Delete the duplicate Slate constants so `canvas.rs` and `board.rs`
   share `SLATE_CANVAS`.

### Accept

- [ ] `atlas-core` still has no `egui` / `eframe` dependency in
      `display.rs`.
- [ ] Atlas zoom range unchanged. Slate zoom range unchanged.
- [ ] Wheel feel unchanged (same formula, now in one place).
- [ ] Unit tests for clamp and both wheel policies.

### Forbidden

Unifying the two zoom ranges. Adding egui `Vec2` to `atlas-core`.
Editing chrome tokens. A drive-by `mod.rs` split.

---

## P2.2 — Wire Slate `ViewState` camera {#p22}

**Owns:** `apps/slate/src/app/mod.rs` functions **`open_doc_at`** and
**`save_doc_to`** only.
**Depends on:** P1.1 merged. **Size:** S.

### Why

`SlateDoc.view` persists `cam_x`, `cam_y`, `zoom`. Runtime uses
`SlateTab.cam`. Load and save ignore `doc.view` camera fields. The
saved view is fiction.

### Do

1. On successful load: set `tab.cam` from `doc.view` (clamp through
   whatever zoom limits are in force).
2. On save: write `tab.cam` back into `doc.view` before `save_to`.
3. Tab switch already has a per-tab `cam` — do not break that.
4. Test: save a workbook with a non-default camera, load it, assert
   match (temp file, existing harness style in `tests.rs`).

### Accept

- [ ] Round-trip test passes.
- [ ] Blank new workbook still opens at the default camera.
- [ ] `active_view` behavior unchanged.

### Forbidden

Editing `slate-doc` format. Adding a format_version bump. Touching
`drain_thumbs`.

---

## P2.3 — Linked-session worker cap {#p23}

**Owns:** `crates/atlas-session/**`, Atlas thumb-pool startup in
`apps/file-atlas/src/app/mod.rs` (the `ensure_workers` / network-24
site only), Slate `ensure_workers(4)` site in
`apps/slate/src/app/mod.rs`.
**Depends on:** P0.1 merged. **Size:** S.

### Why

Linked session = one process, two `ThumbPool`s, up to 24 + 4 + 2
preview workers on one SMB link. That is how a Slate window makes
Atlas thumbnails "mysteriously" slow.

### Do

Default (audit D19): **cap, do not merge pools.** When
`SharedSession` is live, Atlas workers ≤ 16, Slate thumbs stay at 4,
preview stays at 2. When standalone, Atlas may still use 24 on
network. Document the numbers next to the calls.

### Accept

- [ ] A unit or app test asserts the linked cap (session present ⇒
      Atlas workers ≤ 16).
- [ ] Standalone network path still allowed to request 24.
- [ ] Disk cache directory unchanged.

### Forbidden

Two cache directories. Merging the pools (identity of in-flight jobs).
Changing `WARM_CONCURRENCY`.

---

## P2.4 — Rename Atlas tree "portal" in UI copy {#p24}

**Owns:** `apps/file-atlas/src/app/ui/tools.rs` user-visible strings,
`apps/file-atlas/src/app/COMMANDS.md`,
`apps/file-atlas/src/app/ARCHITECTURE.md` (wording only).
**Depends on:** nothing. **Size:** XS.

### Why

Atlas "portal" = collapsed folder preview threshold. Slate "portal" =
Art. V node. Agents and the Display panel keep crossing the streams.

### Do

User-visible copy becomes "group preview" / "stack threshold" (pick
one pair and use it everywhere in those three files). Internal
identifiers (`PORTAL_W`, `portal_threshold`) may stay — this is not a
rename-the-universe card.

### Accept

- [ ] Display panel and COMMANDS.md no longer call the tree feature
      "portal" in user-facing sentences.
- [ ] No layout or default-threshold change.

### Forbidden

Renaming Slate portals. Changing `tree.rs` constants. Editing
`atlas-shell`.

---

## P2.5 — Refresh the metrics baseline {#p25}

**Owns:** `docs/metrics/**` only (the tool will also rewrite the counts
block in `docs/audit/deviations.md` — that rewrite is allowed).
**Depends on:** nothing. **Size:** XS.

### Why

Decision D6: audits diff numbers. The snapshot is `4d40a33` /
2026-07-25. The tree has grown by tens of thousands of lines.

### Do

Run `cargo xtask metrics` from the repo root. Commit the new JSON and
the rewritten `docs/metrics/README.md`. Do not "fix" any number. If
`pure_ratio` fell, say so in the PR body — that is the finding.

### Accept

- [ ] Newest snapshot commit hash is this branch's HEAD (or the
      commit that added the snapshot).
- [ ] PR body quotes `lines_total`, `pure_ratio`, crate lines for
      `native-file-atlas`, `slate`, `atlas-core`, `atlas-shell`.

### Forbidden

Editing Rust to game the counts. Deleting old JSON snapshots.
