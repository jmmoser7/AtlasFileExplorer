# Loading big folders fast

The target case is a folder of 20,000+ images, often on an SMB share with a cold
cache, and the goal is that opening it feels immediate: cards within a frame or
two, the visible screenful of thumbnails filled in well under a second.

This document records what was actually measured, what the numbers led to, and
what is deliberately not done yet. Numbers come from the two benchmarks in
`crates/atlas-core/tests/`, on a local NVMe disk. Both are `#[ignore]`d because
they write large corpora:

```powershell
cargo test -p atlas-core --release --test thumb_bench -- --ignored --nocapture
cargo test -p atlas-core --release --test scan_bench  -- --ignored --nocapture
```

## The files that are not on the disk

Everything below assumes the bytes are reachable. On a work machine that is often
false, and it dominates every other number here.

A OneDrive / SharePoint library with Files On-Demand leaves a full-looking
directory entry whose content lives on a server. Reading one byte of it makes the
sync client download the **whole file**. A measured folder on the reporting
machine: 3,516 files, **every one of them dehydrated**, average size 1.7 MB.

That is the real explanation for "about 15 images per second". Nothing was slow;
Atlas was downloading a document library one file at a time to draw thumbnails,
at roughly 15 × 1.7 MB/s — a saturated office link. Two consequences, and the
second is worse than the first: the user waits, and someone else's file server
gets a sustained unattended read of everything the user happened to point a
window at. The overnight pre-warm made that unbounded.

So: **Atlas never downloads a file in order to draw a thumbnail.**
`cloud::is_dehydrated` gates every extractor in `thumbs::extract_thumbnail`, which
is the one place pixels are produced on a cache miss, so on-demand requests, warm
passes, and the pre-warm crawl are all covered by the single check. A placeholder
gets `SIIGBF_MEMORYONLY` — documented as "do not access the disk even if the
cached version is not present" — and otherwise its type icon, which the icon tier
keeps out of the persistent cache and re-checks later, so the real preview appears
once the file is local.

Reading a file the user explicitly opened is a different question, and the preview
pane still does it. Doing it to 30,000 files nobody asked about is not the same
act.

Measured on 300 real long-path placeholders: 88 files/sec resolved to icons, zero
bytes downloaded, verified per file by re-reading the attributes afterwards
(`tests/cloud_guard.rs`).

### What a placeholder will give up, and what it will not

Worth knowing before trying to be clever here. Asked for a thumbnail of a 1.7 MB
dehydrated JPEG, the shell returns **nothing at all** — with or without
`SIIGBF_MEMORYONLY`, and without hydrating. There is no cloud-served thumbnail to
borrow. The only ways to preview a cloud-only file are to download it or to ask
the provider's own API (SharePoint/Graph, which needs auth and is out of scope).

Which means the honest behavior is what is implemented: cloud-only files show
type icons, and previews appear when the user makes the files local. That is
their decision to make, not the file browser's — so **File → Download cloud
files…** exists to let them make it. It counts the cloud-only files in the
selection (else the current filter, else the folder), states the total transfer in
the confirmation window, and only then fetches them one at a time. Sequentially,
because the constraint is an office link rather than our CPU, and because a file
browser opening dozens of parallel streams against a corporate server is how a
user ends up explaining themselves to an administrator.

As each file lands, its cached type icon is dropped (`ThumbPool::forget_icon`) and
its card is reset, so the real preview replaces the icon without a restart. The
icon tier is otherwise deliberately sticky — a folder of preview-less CAD files
should not re-run shell extraction on every scroll — so it has to be invalidated
at the exact moment the reason for the icon goes away.

### Fail closed, and mind `MAX_PATH`

The first version of this check was wrong in the expensive direction, and the way
it was wrong is worth keeping on record.

`GetFileAttributesW` fails outright on a path of 260 characters or more unless it
is given the `\\?\` extended-length prefix. The check treated an unreadable
attribute as "this file is local" — reasoning that a failure should never disable
previews for ordinary files — and so every placeholder nested deeply enough was
handed straight to the byte-reading extractors.

In one measured folder that was 502 of 1,662 files: exactly the files whose paths
crossed 259 characters, and only those. Perfect correlation, and the same subtree
held 5,997 more. OneDrive trees reach that depth without trying — the folder in
question was 229 characters before the filename.

Both halves are now fixed, and both are load-bearing:

- paths are queried through `\\?\` (`cloud::extended_wide`, including the
  `\\?\UNC\` form for shares), so the answer is available at any depth;
- an unreadable entry counts as **cloud-only**. The costs are not symmetric.
  Guessing "local" wrongly downloads a file server; guessing "cloud" wrongly
  costs one thumbnail on a file that could not be stat'd and was unlikely to
  yield one.

`cloud::attributes_are_readable_past_max_path` builds a >260-character fixture and
fails if the query stops working. Anything that reads file bytes in bulk belongs
behind this gate.

## Thumbnails

Twelve 6000x4000 JPEGs (37.9 MB), thumbnailed to 192 px:

| Source | ms/file | thumbs/sec |
|---|---|---|
| `image::open` + resize | 281.6 | 3.6 |
| Windows shell (`IShellItemImageFactory`) | 75.2 | 13.3 |
| Our 1/8-scale DCT decode | 80.8 | 12.4 |
| **Our embedded EXIF preview** | **0.64** | **1555** |

Two things fall out of this.

**The embedded preview is the entire win — 438x.** A camera has already written a
~160x120 JPEG into the EXIF block near the front of the file, so the whole job
becomes a 128 KB read plus a tiny decode. On a share this matters twice over,
because it is kilobytes crossing the wire instead of megabytes.
`rasterthumb::exif_preview` walks the real IFD chain (IFD0 → IFD1, tags
0x0201/0x0202) rather than scanning for a nested `FFD8`, because a scan also
matches compressed pixel data and would return garbage.

**Scaled decode is not a win on its own** — 80.8 ms against the shell's 75.2 ms.
Decoding at 1/8 skips the inverse DCT but still entropy-decodes every coefficient
in the file, so it is the same order of work. It stays as the fallback for files
with no usable preview (many exported or re-saved JPEGs), and because it is at
least not *worse* than the shell while avoiding COM entirely.

A preview is only accepted at 60% of the requested size or above
(`MIN_PREVIEW_FRACTION`); below that the upscale looks soft enough to be worth
paying for a real decode.

### Orientation

Cameras record rotation as an EXIF tag and leave the pixels alone. The shell
applied that for us; our decoders do not, so `rasterthumb` reads tag 0x0112 and
applies the transform itself. Without this every portrait photo lands on its
side — the kind of regression a photographer notices in the first second.

### Why the cache version had to move to 4

The first version of this work left `CACHE_KEY_VERSION` at `3`, reasoning that
old shell-era JPEGs were 192 px and therefore as good as anything we would
produce. That was wrong, and the folder that disproved it was a OneDrive
desktop folder whose PNG cards had shown the generic blue Photos icon for
months:

- Three different PNGs shared one **byte-identical 5238-byte 192x192** cache
  entry, written months earlier. It was not a thumbnail at all — it was the
  file-type icon the shell substitutes when it cannot reach the pixels, which on
  OneDrive means a file that was a dehydrated placeholder at the time.
- The key is `path + size + mtime`. None of those change when a placeholder is
  hydrated, so the icon was a permanent cache hit. Waiting, revisiting, and
  re-warming all re-served it. From the outside the app looked like it was
  loading forever.

So the epoch is a correctness boundary, not a quality tweak: everything the
shell-first era wrote is suspect, because we could not tell an icon from a
preview and cached both the same way. Re-warming is cheap now — that is the
whole point of `rasterthumb`.

### Every derived-artifact cache needs a recipe version

The icon episode above is a specific case of a general trap, and the home shelf's
baked cover PNGs were sitting in it too: `cover_cache_path` was `hash(path).png`,
so a cover was keyed by its *input* and nothing about how it was made. Improving
the bake could not reach any machine that already had covers — the fix would ship
and nothing would change, which is a genuinely confusing way to lose an afternoon.

`recent::COVER_RECIPE_VERSION` now prefixes the filename (`v2-<hash>.png`), and
`prune_stale_covers` deletes earlier generations once per run so a bump reclaims
its predecessor's disk instead of leaving a copy behind forever. **Bump it in the
same commit that changes what a bake produces**, the same discipline
`CACHE_KEY_VERSION` asks for.

Version 2 exists because mosaic tiles were being squashed. `image`'s two
`thumbnail` functions differ in exactly the way that matters and are easy to
confuse: `DynamicImage::thumbnail` preserves aspect ratio, while
`imageops::thumbnail` resizes to precisely the dimensions asked for. The mosaic
called the latter with square cells, so every landscape photo was flattened and
every portrait one stretched — nine per cover. Tiles now center-crop to the cell's
aspect first (`cover_crop`), filling rather than letterboxing, which is the right
trade for a mosaic where gaps would look worse than a crop.

### Never persist a file-type icon

An icon is worth *showing* (better than a card that spins forever) and never
worth *keeping*, because it encodes a temporary condition — a cloud placeholder,
a missing codec — under a key that never expires.

`SIIGBF_RESIZETOFIT` cannot fail: with no thumbnail to be had it silently draws
the type icon, indistinguishable downstream from real pixels. We now ask the
shell a second question, `SIIGBF_ICONONLY`, and compare: identical pixels mean
we were handed an icon. `Extracted::cacheable` carries that verdict to the
worker.

Icons then go to their own tier, `{key}.icon.jpg`, which is never read in place
of a real preview and never published to the shared project cache. Icons are
still stored, because the alternative is worse: re-extraction costs three shell
calls (≈55 ms for a `.3dm`, since Rhino's provider runs and fails), and a folder
of preview-less CAD files would pay that on every pan. Each key instead earns one
fresh attempt per process (`Shared::should_retry_icon`) — enough to notice a
hydrated placeholder or a newly installed codec, bounded enough to stay cheap.
`has_local` only counts `{key}.jpg`, so that retry is normally spent by a
background warm job rather than on a card the user is looking at.

### The blank card the version bump uncovered

Retiring the old entries meant those files were suddenly "not warmed", which
enqueued warm jobs for them — and warm jobs carry no pixels by design. When one
answered for a card whose on-demand request was already in flight, the card sat
in `AskedFull` with nothing to draw, because the paint pass only ever re-requests
`NotAsked`/`HasColor`. The comment there always claimed "the UI re-requests
pixels on demand"; nothing made that true. It does now, and
`a_warm_result_releases_a_card_that_was_waiting_on_pixels` fails if it stops
being true. This was never specific to Rhino files — any format could strand.

That verdict also fixed an ordering bug it exposed. A *successful* icon used to
shadow our own extractors entirely — `.3dm` files showed the Rhino type icon
while `threedm::embedded_preview` never ran. `shell_then_builtin` now treats an
icon as "no answer yet" and tries our extractors before settling for it.

(For the Rhino files in that folder the icon is still the honest answer: they
contain no PNG or JPEG signature anywhere, having been saved without a preview
image. Nothing is there to extract.)

### PNG

PNG has neither an embedded preview nor a scaled decode, so the pixels must be
decoded: 15 ms at 1080p, 95 ms at 12 MP, 347 ms at 48 MP. We still do it
ourselves. The shell only beats that when Explorer already has the file cached;
on a miss it decodes the same pixels *after* a COM round trip, and on a folder
nobody has browsed in Explorer a miss is the normal case.

## Discovery

20,000 files across 20 subdirectories, OS cache warm:

| Stage | Time |
|---|---|
| Discovery (first batch on screen) | 3.2 ms |
| Discovery (complete) | 11 ms |
| Owner resolution, now deferred | 397 ms |

The walk itself was never the problem. Two things were.

**Owner lookup was inside the walk.** Owner is the one field a directory read
cannot give you — Windows keeps it in the file's security descriptor, so learning
it costs `GetNamedSecurityInfoW` plus a SID translation *per file*. Measured at
0.26 ms locally before caching; on a share it is a full request/response per
file, and 20,000 of those serialized behind eight workers is the reported
fifteen-second wait. Nothing about layout, filtering, or thumbnails needs it, so
`scanner.rs` now leaves `owner` empty and `owners.rs` backfills it once the canvas
is already up. The owner filter facet simply grows as results arrive, and a
revisit pays nothing because the index stored what the pass learned last time.

For the same reason the overnight pre-warm walk no longer resolves owners at all;
it only ever used the path and the cache key.

**Two bugs turned up in that code while measuring it.**

- The account name was truncated by one character. On success
  `LookupAccountSidW` reports a length that *excludes* the terminator, and the
  code sliced `name_len - 1`, so `jmoser` was stored, filtered, and displayed as
  `jmose`. Regression test:
  `metadata::tests::a_new_files_owner_is_the_current_account_in_full`.
- `GetNamedSecurityInfoW` was called with a null security-descriptor out-param.
  The returned SID points *into* that buffer and the caller owns it, so every
  scanned file leaked one descriptor — 20,000 per folder. It is now requested
  properly and `LocalFree`d.

SID → name results are also memoized now, which took the local cost from 0.26 ms
to 0.08 ms per file. That translation can leave the machine to ask a domain
controller, and every file in a folder almost always has the same owner.

### A syscall storm in the thumbnail queue

`pop_preferred_hot` prefers local files over network ones so a fast local disk
never waits behind a slow share. It found them with
`hot.iter().rposition(|r| !is_network_path(&r.path))` — and `is_network_path` did
a `GetDriveTypeW` plus a string allocation per call. That is up to
`HOT_QUEUE_CAP` (512) syscalls **on every pop, while holding the queue lock**,
which put hundreds of serialized syscalls in front of every thumbnail during
exactly the pan-and-zoom sessions the queue exists to serve. It is now a
two-bitmap memo over drive letters A–Z and allocation-free. A drive does not stop
being remote while we are looking at it.

## The frame loop while a folder is still arriving

Three symptoms, one cause: panning and zooming juddered during a load, the file
count in the readout appeared to freeze, and the display tree collapsed folders
nobody had touched.

The first two were the same bug. Every scan batch — one per 512 files or per
30 ms, whichever comes first — set `filter_dirty`, and `recompute_matches` is
several passes over *every* entry plus a whole-tree relayout, a rebuilt timeline
index, and a rebuilt folder-heat map. So each batch bought a full-corpus sweep,
and since batches arrive faster than frames, that sweep ran on essentially every
frame of a load, growing as the folder grew. The count looked frozen for the same
reason it juddered: the readout was only being redrawn a few times a second.

`load_jitter_benchmark` streams synthetic batches through the app's own scan
channel, one batch per frame, with the pointer moving — a hand on the canvas
while a folder loads. `ATLAS_BENCH_LEGACY=1` restores the old per-batch work, so
the two are measurable side by side on the same machine:

```powershell
$env:ATLAS_BENCH_FILES = "120000"
cargo test -p native-file-atlas --release load_jitter -- --ignored --nocapture
```

120,000 files, release build. 16.7 ms is the 60 fps budget (Art. II):

| Frame time | Per-batch sweep | Now |
|---|---|---|
| p50 | 12.8 ms | 5.0 ms |
| p95 | 27.1 ms | 13.2 ms |
| p99 | 53.9 ms | 30.0 ms |
| worst | 84.4 ms | 56.9 ms |
| over budget | 34% of frames | 4% of frames |

The averages understate it. What the hand feels is the shape of the cost against
folder size, and that is where the old path fails: 6.0 ms at 30k entries, 13.1 at
60k, 20.8 at 90k, 33.9 at 120k — it degrades as the folder loads, which is
exactly when the user is trying to look around. The current path is flat
(2.5 / 4.8 / 6.7 / 7.1 ms).

What changed:

- **Batches fold in instead of triggering a re-examination.**
  `absorb_new_entries` matches only the files that just arrived and adds them to
  the aggregates, because appended files can only *add* to a count, a byte total,
  or a date span. `folding_in_a_batch_matches_a_full_recompute` asserts the fast
  path lands on exactly the values the full recompute would produce — counts that
  silently drift are worse than counts that are slow. A re-reported file (changed
  size or date) is not additive, so that still takes the full pass, as does
  duplicate-hiding, which is a global decision no batch can make alone.
- **The count is live again**, and has a test that says so
  (`the_file_count_keeps_up_with_every_batch`).
- **The timeline index and folder-heat map ride the tree's cadence**, not the
  batch rate. Both are whole-corpus passes keyed on `heatmap_data_rev`, which was
  bumped per batch; it is now bumped in `adopt_tree`, which is as often as the
  canvas they annotate actually changes shape.
- **Rebuilds are gated on growth, not a stopwatch.** A rebuild copies every entry
  for the background build thread and re-lays out the canvas, so its cost scales
  with the folder — on a fixed 700 ms timer a big root pays more per second the
  bigger it gets. It now waits until the canvas is a quarter out of date (with a
  4 s ceiling so a slow trickle still lands, and prompt rebuilds outside a scan so
  watcher events are not delayed). Total rebuild work over a load goes from
  O(n × frames) to O(n log n).
- **The owner pass stopped re-tallying every file per batch.** It adjusts the
  facet count for the one label that changed.

### The remaining spike

`rebuild_tree` still clones the entry vector to hand it to the background build
thread: 42 ms at 120k entries, and that is the p99 above. It is bounded now —
roughly twenty rebuilds across a 120k load rather than one every 700 ms — so it
reads as an occasional dropped frame rather than sustained mud. Removing it means
either an `Arc` snapshot of entries or a narrower projection of the five fields
`Tree::build` actually reads (`dead`, `rel`, `name_lc`, `size`, `family`), both of
which change an `atlas-core` API. Not worth it until it is the worst thing left.

### Collapse is a decision, recorded once

Collapse state lived only on the tree, and the tree is thrown away and rebuilt
while a scan streams. Two consequences, both reported as "the tree collapses on
its own":

- `Tree::build` re-runs `default_collapse`, whose rules read counts that are
  still growing (`desc_files > 300`, direct children over the portal threshold).
  A folder expanded at 20 files slammed shut on the rebuild that took it past 300.
- A background build carries its own snapshot of collapse state. Large roots build
  off-thread for seconds, and a folder the user opened during that window was
  overwritten the moment the build landed.

`AtlasApp::dir_collapsed` now records the decision for every directory the root
has seen, keyed by `rel` — set by the default rule the first time the folder
appears, or by the user, and never silently revisited. `adopt_tree` reconciles
every incoming tree against that record (relaying out if it had to override
something, since positions were computed from the collapse it was built with), so
a build that started before a click cannot undo it. Tests:
`a_folder_stays_expanded_while_the_scan_keeps_arriving` and
`expanding_during_a_background_build_is_not_undone`.

Anything that deliberately re-decides collapse — a grip click, moving the portal
threshold in Display settings — must call `record_collapse_state()`, or the next
rebuild will put it back.

### A folder card's date and owner are not worth a stat

`Tree::build` filled each `DirNode`'s `ctime` and `owner` by calling
`std::fs::metadata` and `owner_short` on the directory. On a local disk that is
invisible. On the reference machine's SharePoint roots it measured **five seconds
per folder**, and it ran *on the UI thread*, inside the build — so opening a
first-visit share froze the window for minutes with a progress bar and no canvas.
Two labels on a card, paid for with the whole app.

Both halves are now sourced honestly:

- **The date is free.** A Windows `DirEntry`'s metadata comes from the
  `FindFirstFile` data the walk already read, so the scanner harvests each
  folder's `ctime` as it discovers it and ships it with the batch
  (`ScanMsg::Dirs`). Instrumented across a 53k-file share this cost **0 µs** at
  every checkpoint — it is the same bytes, already in hand.
- **The owner is demand-driven.** It lives in the security descriptor, so it is a
  round trip whatever else happens (measured 4.5–5.8 s per folder on that share).
  `dirmeta.rs` resolves it off-thread for folders that are *in view and zoomed in
  far enough to show it* (`lod >= 2`), and results land on the live tree in place
  — a label refresh, no rebuild, no relayout.

`Tree::build` now performs **no I/O at all**: it takes a `DirMetaMap` and reads
what has already been learned. Keep it that way — it runs on the frame loop.

### Opening a slow share must not freeze the window

`R:\Cad\Rhino` (`\\ngrimshaw.int\Resources`) taught a second lesson: even the
*shallow* open looked frozen for minutes, with Windows marking the process Not
Responding, before the user had drilled into anything. Listing a single directory
on that share costs **3–7 seconds** (18 ms for a local folder), and the UI thread
was waiting on it rather than computing:

1. **Watcher events were applied on the frame loop**, each one through
   `stat_file` → `owner_short` — a security-descriptor round trip measured at
   4.5–5.8 s on this share, so a handful of events was a minute of frozen window.
2. **Cover-bake DFS of up to 4 000 entries** kicked off the moment the folder was
   recorded as recent. At ~4 s per directory listing that alone can pin the link
   for the length of the freeze — off-thread, so it never blocked the frame
   directly, but it starved discovery. Network bakes now read the root only.
3. **`create_dir_all` for the shared project cache** ran inline on open. Only
   ~54 ms here, but it is a network write on the frame loop; it is spawned now.

`stat_file` also stopped asking for the owner; the deferred owner pass fills it,
and the frame loop applies at most `FS_EVENTS_PER_FRAME` events per frame,
leaving the rest in `fs_backlog`.

#### The fix that was not a fix

The first pass at this also deferred growing the thumbnail pool to 24 workers
until discovery finished, on the theory that warmers were competing with the
walk. That was wrong twice over. It was not a cause: worker threads never touch
the frame loop, and bulk warming does not start until `ScanMsg::Done`, so the
extra workers only ever serve cards that are already on screen. And it broke the
thing the app is for — previews stopped appearing until the scan ended, which on
this share is minutes of empty cards.

**Watching a folder populate is the feature, not the cost.** Fix a stall where it
lives, on the frame loop; never by slowing down what the user is watching. See
invariant 7 in `apps/file-atlas/src/app/ARCHITECTURE.md`.

### When the folder itself is the problem

After the above, a 53k-file share delivered **47,144 files in 6 seconds** with the
canvas smooth throughout — and then crawled for five more minutes. Instrumentation
ruled out every subsystem: `active_readers` stayed at 7–8 of 8 workers (no
self-starvation), and `thumbs_pending` and `dir_meta_active` were both zero for the
whole crawl (no contention). The walk was simply reading directories that took
1.5–2.7 seconds each, and it named them:

```
21_Megascans Library\Downloaded\surface\brick_rough_uehgba0g\Thumbs\1k          2022ms
21_Megascans Library\Downloaded\support\plugins\unreal\...\Private\Wrappers     2341ms
```

A Quixel Megascans library: a `Thumbs` folder per surface plus a vendored Unreal
plugin source tree. Thousands of directories holding a handful of files each. At
two seconds a read, eight workers clear about four directories a second, so **900
of them cost 288 seconds and yielded 5,000 files** — after the real content had
already arrived.

Nothing in the code was wrong, so nothing in the code was the fix. Two changes,
both about what the user sees rather than about throughput:

- **The queue is breadth-first.** Taking directories from the back of the queue
  made the walk dive down whichever branch it happened to open first. Level by
  level, the shape of the folder arrives first and pathological depth is charged
  last.
- **A user-editable skip list** (`skiplist.rs`, `scan-skip.json`, edited in
  **Advanced → Folders never scanned**), seeded with caches and build scaffolding
  in the same spirit as the `node_modules` entry that was already there. Scanning,
  pre-warming, and cover art all consult it, so a name listed once is invisible
  everywhere. Our own `.atlas-cache` is *not* in that list: it is a hard floor in
  the module, because indexing the app's own output is a correctness bug, not a
  preference. Test: `scan_never_enters_our_own_cache`.

The list is a judgement about what counts as content, which is why it is the
user's and not ours. Resist the urge to grow the defaults by guessing.

### Returning to a tab must not re-frame it

Camera-follow (*Zoom to matches*, on by default) is edge-triggered on the bounds
it last framed, held in `auto_zoom_last`. That value is per-root, was reset in
`reset_workspace`, but was never carried in `ParkedWorkspace` — so switching tabs
left the *other* tab's bounds in place, the first filter recompute after a restore
read that as a new edge, and the camera flew away from the position the restore had
just put back. Runtime evidence, from the tab whose tree is 958 units tall
returning to find its neighbour's 694 still recorded:

```
auto_zoom_after_filter fires: last=Some([-12,0]-[346,694]) bounds=[-12,0]-[346,958])
fly_to from=[77,-13] to=[623.4 91.2]
```

The bug predates the scan work; making builds fast is what made it reachable
within a frame or two of the switch. `auto_zoom_last` is now parked with the tab.
The lesson is the one `ARCHITECTURE.md` already states — per-root state belongs in
*both* `reset_workspace` and `ParkedWorkspace` — and half of it is easy to forget
because the symptom only appears when timing cooperates.

The harness earned a matching correction: `pump_until_idle` now also waits for
`anim`, because a camera in flight is not an idle canvas, and a test that plants a
camera mid-fly is planting it into an animation that overwrites it next frame.
Every real navigation cancels the fly first; only a test can reach past that.

## Diagnosing "this folder never loads a preview"

`tests/folder_probe.rs` points the real pipeline at a real folder and reports,
per file, what each tier returned and whether the result would be cached:

```powershell
$env:ATLAS_PROBE_DIR = "C:\path\to\folder"
cargo test -p atlas-core --release --test folder_probe -- --ignored --nocapture
```

It prints `ICON(not cached)` for files the shell can only answer with a type
icon, which is what distinguishes "nothing to extract" from "extraction broke",
and `CLOUD-ONLY (not downloaded)` for placeholders — for which it deliberately
skips the per-tier timings, since probing them tier by tier would download the
folder it was asked to diagnose.

`tests/cloud_guard.rs` answers the other question — whether a folder is slow
because Atlas is downloading it. It re-reads each file's attributes after
thumbnailing and names any file that got hydrated:

```powershell
$env:ATLAS_PROBE_DIR = "C:\path\to\folder"; $env:ATLAS_PROBE_LIMIT = "300"
cargo test -p atlas-core --release --test cloud_guard batch -- --ignored --nocapture
```

Two things to check before believing a card is slow rather than wrong:

- Compare the cached JPEG against a fresh extraction. A **square** entry for a
  non-square source, or one file's bytes shared by several sources, means an icon
  is being served as a preview. Cache lives in
  `%LOCALAPPDATA%\NativeFileAtlas\thumbs\`; `{key}.jpg` is a real preview and
  `{key}.icon.jpg` is a known type icon.
- Cache keys are **root-relative**, so the same file opened as `Desktop\GG\x.png`
  and as `GG` → `x.png` are two different keys. Warmth does not transfer between
  roots, and a probe run against one root proves nothing about the other.

## Deliberately not done

- **Bulk Win32 enumeration** (`FindFirstFileEx` + `FindExInfoBasic` +
  `FIND_FIRST_EX_LARGE_FETCH`, or `GetFileInformationByHandleEx` with a 64 KB
  buffer). Rust's `read_dir` on Windows is already `FindFirstFileW`-based and
  `DirEntry::metadata()` is free there — it reuses the `WIN32_FIND_DATAW` the
  walk already fetched — so there is no per-file stat to remove. The remaining
  gain is buffer size: a 4 KB buffer holds roughly 30–40 entries, so 20k files is
  ~600 SMB round trips against ~40 with 64 KB. Real, but roughly 30x smaller than
  the owner problem that was just removed, and published benchmarks disagree
  about whether `LARGE_FETCH` helps or hurts on local NTFS. It needs measurement
  on an actual share before it is worth unsafe enumeration code.
- **libjpeg-turbo (`turbojpeg`)** for faster entropy decoding on the no-preview
  fallback. It needs a C toolchain (cmake + nasm), which would cost the pure-Rust
  Linux build the constitution requires in Article I. Worth revisiting only if
  measurement shows the corpus is mostly preview-less JPEGs.
- **`fast_image_resize`.** The resize input is already at most a few hundred
  pixels on a side after a preview read or a 1/8 decode, so SIMD resampling has
  almost nothing left to speed up.
- **MFT / USN journal enumeration** (the Everything approach). Local NTFS only —
  it cannot work over SMB, which is the case that hurts.
