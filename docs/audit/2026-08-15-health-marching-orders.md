# Audit №3 — Marching orders

**For:** agents executing the next week of work.
**Human companion:** [`2026-08-15-health-executive.md`](2026-08-15-health-executive.md).
**Cards:** [`docs/workplan/tasks/wave-perf.md`](../workplan/tasks/wave-perf.md).
**Standing rules:** [`docs/workplan/agent-brief.md`](../workplan/agent-brief.md)
— one card, one worktree, one PR. Do not "notice" adjacent work.
**Date:** 2026-08-15. **Base:** `main` @ `1dd849c`.

This is Part 1. If a card and this file disagree, the card wins on files
and tests; this file wins on diagnosis and forbidden moves.

---

## 1. How to run this week

Dispatch **one agent per card**. Prompt shape:

```
Read, in order:
  1. CONSTITUTION.md
  2. AGENTS.md
  3. docs/workplan/agent-brief.md
  4. docs/audit/2026-08-15-health-executive.md   §2–3 and the hotspot table
  5. docs/audit/2026-08-15-health-marching-orders.md
  6. docs/workplan/tasks/wave-perf.md, section <CARD ID>

Execute <CARD ID> and nothing else. Open a PR. Do not merge.
```

**Review lane.** Implementer and reviewer are never the same agent.

**Windows verification** (human or a Windows agent) runs the commands in
§6 against the same folders that feel slow. Cloud agents on Linux cannot
see COM, WebView2, or share metadata. A card that is green on Linux and
unverified on Windows is not done.

**Do not give one agent two cards.** P0.1 and P1.4 both touch
`thumbs.rs` — sequential, P0.1 first. P0.2 and anything in `board.rs`
are parallel. P1.3 owns only the board paint loop.

---

## 2. Diagnosis agents must not re-litigate

These are findings, not hypotheses. A card that "fixes" them by slowing
what the user is watching (holding scan population, deferring the thumb
pool, raising `LIVE_POOL` without a contract amendment) is rejected.

### File Atlas is cold because the epoch moved

`CACHE_KEY_VERSION` is `"5"` (`crates/atlas-core/src/thumbs.rs`). Keys
embed the version. `5a9ea0b` (2026-08-03) did `3 → 4` for the icon-vs-
preview fix — **necessary**. `dc3178c` (2026-08-08) did `4 → 5` because
an SVG extractor was added — **not necessary for any other format**.
Every previously warmed non-SVG thumbnail is an orphan. There is no
migration. `has_local` only looks for `{key}.jpg` under the *current*
key.

Do not bump the version in any Wave P card. Do not try to read v4 keys
(opaque hashes; would also revive cached icons). Accept the re-warm.
Make the next visit fast once disk is hot again (P0.1, P1.4).

### Slate leaked into Atlas by commit shape, not by a shared setting

`dc3178c` is titled "Web portals" and rewrites 2,733 lines of
`apps/file-atlas/src/app/mod.rs` plus `scanner.rs`, `tree.rs`, `cloud.rs`,
and new `fsops` / `skiplist` / `dirmeta` / `shell_drag`. Atlas does **not**
use `PreviewPool`. The shared surfaces are the disk cache directory and,
in a linked session, one process's threads and one SMB link.

A Slate card may not edit `apps/file-atlas/**` or `CACHE_KEY_VERSION`.
An Atlas card may not edit `apps/slate/**`.

### Web-portal zoom gutter is sync capture + pool churn

`apps/slate/src/app/board_web.rs` `web_pump`: on demotion,
`host.capture_poster` then `host.evict`. On Windows that is D3D11
readback on the UI thread (`board_web_win.rs`). Contract D21 / D29
already require async, generation-tagged capture. `LIVE_POOL = 6` and
`LIVE_MIN_PX = 160` stay. Do not raise the pool to "fix" jank.

### Article II still binds

- 60 fps. A change that breaks interactive frame rate is a regression.
- No per-frame tessellation in paint paths. Cache by path + style + zoom
  bucket. LRU, not `map.clear()`.
- Heavy work is async and generation-tagged.
- Never fix a stall by holding population back
  (`apps/file-atlas/src/app/ARCHITECTURE.md` invariant 7).

---

## 3. Wave P — task index

Gate: **none**. Article II work does not wait on Wave 1/2. File ownership
below is the conflict-prevention table; if your card does not list a
file, you may read it and must not edit it.

| ID | Title | Sev | Owns (primary) | Size | Parallel with |
|---|---|---|---|---|---|
| **P0.1** | Cache epoch hygiene + revisit fast-path | P0 | `thumbs.rs` (comments/API only), `performance.md`, `folder_probe`, Atlas `advanced.rs` / `ingest_loaded` / `maybe_request_full` | M | nothing else on `thumbs.rs` |
| **P0.2** | Web portal capture off-thread + LOD hysteresis | P0 | `board_web.rs`, `board_web_win.rs` | M | all Atlas cards |
| **P0.3** | Watcher metadata off the frame loop | P0 | Atlas `mod.rs` (`apply_fs_change`, `drain_channels` watcher arm), `watcher.rs` only if a new message variant is required | M | P0.2, P1.2, P1.3, P1.5 |
| **P1.1** | Slate thumb drain budget + LRU | P1 | `apps/slate/src/app/mod.rs` (`drain_thumbs`, `textures`, `thumb_pixels`) | S | not P1.5 (same crate, different files — OK) |
| **P1.2** | Path mesh LRU + fill cache + hash | P1 | `board_path.rs` | S | P0.2, P0.3, P1.1 |
| **P1.3** | Viewport-culled board paint | P1 | `board.rs` paint loop only (~2146–2181) | M | not T1.1c, not T2.2 |
| **P1.4** | Bound the warm queue | P1 | `thumbs.rs` queues only | S | **after P0.1 merges** |
| **P1.5** | Cache Grid/Venn layout | P1 | `canvas.rs` | S | P0.2, P1.2 |
| **P1.6** | Preview decode cancellation | P1 | `crates/atlas-core/src/preview.rs`, Slate `preview.rs` | S | not P0.1 |
| **P2.1** | `atlas-core::display` camera/zoom/budgets | P2 | new `crates/atlas-core/src/display.rs` + call-site wiring listed on the card | L | after P0 |
| **P2.2** | Wire Slate `ViewState` camera | P2 | `apps/slate/src/app/mod.rs` `open_doc_at` / `save_doc_to` | S | not P1.1 (same file — sequential) |
| **P2.3** | Linked-session worker cap | P2 | `atlas-session`, both apps' pool startup | S | after P0.1 |
| **P2.4** | Rename Atlas tree "portal" in UI copy | P2 | Atlas `tools.rs` labels, `COMMANDS.md`, `ARCHITECTURE.md` wording | XS | anything not those files |
| **P2.5** | Refresh `cargo xtask metrics` baseline | P2 | `docs/metrics/**` only | XS | anything |

**Already owned elsewhere — do not touch**

| Finding | Owner | Why Wave P leaves it |
|---|---|---|
| DV-08 `SceneJournal` unbounded | WI-2 / T1.1 | `scene.rs` is Wave 1's file |
| DV-01 index-addressed `SceneCmd` | T1.1b/c | same |
| DV-02 `atlas-ai` renderer-bound | T1.2 | same |
| DV-03 absolute paths | T2.1 | same |
| DV-05 / DV-12 dead commands / panels | T2.2 / Wave 2 chrome | same |
| Atlas `mod.rs` 9k-line split | future dedicated wave | a decomposition during Wave P will collide with P0.3 |

---

## 4. File ownership (conflict prevention)

| File / dir | Owner |
|---|---|
| `crates/atlas-core/src/thumbs.rs` | **P0.1 → P1.4**, strictly in that order |
| `crates/atlas-core/src/preview.rs` | P1.6 |
| `crates/atlas-core/src/display.rs` (new) | P2.1 |
| `crates/atlas-core/src/watcher.rs` | P0.3 (only if a new variant is required; prefer app-side) |
| `docs/performance.md` | P0.1 |
| `crates/atlas-core/tests/folder_probe.rs` | P0.1 |
| `apps/file-atlas/src/app/mod.rs` | **P0.3** (watcher/scan drain) and **P0.1** (ingest/maybe_request only). Split by function: P0.1 may edit `ingest_loaded`, `maybe_request_full`, `entry_key`. P0.3 may edit `apply_fs_change`, the `scan_rx` / watcher loops in `drain_channels`. Nothing else. |
| `apps/file-atlas/src/app/ui/advanced.rs` | P0.1 |
| `apps/file-atlas/src/app/ui/tools.rs`, `COMMANDS.md`, Atlas `ARCHITECTURE.md` (wording) | P2.4 |
| `apps/slate/src/app/board_web.rs`, `board_web_win.rs` | P0.2 |
| `apps/slate/src/app/board_path.rs` | P1.2 |
| `apps/slate/src/app/board.rs` | P1.3 (paint loop only) |
| `apps/slate/src/app/canvas.rs` | P1.5 |
| `apps/slate/src/app/preview.rs` | P1.6 |
| `apps/slate/src/app/mod.rs` | **P1.1 → P2.2**, strictly in that order |
| `crates/atlas-session/**` | P2.3 |
| `docs/metrics/**` | P2.5 |
| `docs/audit/deviations.md` | row-by-row: the card in *Closes with* |
| `CONSTITUTION.md`, `ROADMAP.md`, `docs/keymap/contracts/**` | **nobody in Wave P** except P0.2 may add one sentence to `portal-web-embed.md` Implementation note if D21 is now met |

`apps/file-atlas/src/app/mod.rs` is the collision. Two cards, named
functions only. If you need a third function, escalate; do not take it.

---

## 5. Standing forbidden moves

Copied here so a card cannot "local-override" them.

1. Do not bump `CACHE_KEY_VERSION`.
2. Do not persist type icons to `{key}.jpg`.
3. Do not defer the thumb pool until `ScanMsg::Done`.
4. Do not put `metadata`, owner lookup, or WebView readback on the frame loop.
5. Do not raise `LIVE_POOL` or `LIVE_MIN_PX` without a contract amendment
   the user has ratified.
6. Do not `map.clear()` a tessellation cache as an eviction policy.
7. Do not add a `ViewKind`.
8. Do not mutate `doc.scene` outside a journaled path.
9. Do not define chrome colors outside `atlas-shell`.
10. Do not run `cargo fmt --all` (agent-brief §0.5).
11. Do not edit `CONSTITUTION.md`.
12. Do not merge Atlas and Slate canvases, cameras, or journals "while
    you are here."

---

## 6. Windows verification (human)

Run these on the machine that feels slow, **before** merging P0, and
again after. Paste the numbers into the P0.1 / P0.3 PRs.

```powershell
# 1. What each tier returns for a folder that used to be instant
$env:ATLAS_PROBE_DIR = "C:\path\to\previously-warm-folder"
cargo test -p atlas-core --release --test folder_probe -- --ignored --nocapture

# 2. Load jitter — compare to docs/performance.md (120k, p95 was 13.2 ms)
$env:ATLAS_BENCH_FILES = "120000"
cargo test -p native-file-atlas --release load_jitter -- --ignored --nocapture

# 3. Cloud guard, if the folder is OneDrive / SharePoint
cargo test -p atlas-core --release --test cloud_guard batch -- --ignored --nocapture

# 4. Confirm the epoch. Advanced readout (after P0.1) must show version 5.
#    Count files: %LOCALAPPDATA%\NativeFileAtlas\thumbs\*.jpg
#    After a revisit, new keys appear; old hashes remain until prune.
```

Web-portal zoom: open a board with ≥8 web portals, wheel from min zoom
to max and back in one gesture. The frame must not hitch on the 160 px
crossing. That is P0.2's acceptance; Linux cannot run it.

---

## 7. New deviations this audit opened

See `docs/audit/deviations.md`. Cards close the row named in *Closes with*.

| ID | Article | Closes with |
|---|---|---|
| DV-13 | II.3 | P0.2 — web eviction capture is sync on the frame loop |
| DV-14 | I.1 | later (not Wave P) — `atlas-core/tree.rs` depends on egui |
| DV-15 | II.2 | P1.3 + P1.2 — board paint ignores the spatial index; fills flatten every frame |
| DV-16 | II | P1.1 — Slate `textures` / `thumb_pixels` unbounded |
| DV-17 | II | P0.3 — watcher `stat_file` on the frame loop |

---

## 8. Suggested dispatch order

**Day-one, four-wide:** P0.1, P0.2, P0.3, P2.5 (metrics is tiny and
unblocks the next audit).

**After P0.1 merges:** P1.4, P2.3.

**Anytime after day one, no file overlap:** P1.2, P1.3, P1.5, P1.6, P2.4.

**After P1.1 merges:** P2.2.

**After P0s merge:** P2.1 (display params) — it will touch call sites;
rebase on whatever landed.

Wave 1/2 agents may keep working **only** on files this table does not
name. `slate-doc/src/scene.rs` is still theirs.
