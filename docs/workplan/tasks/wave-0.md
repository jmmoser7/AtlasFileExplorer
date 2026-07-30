# Wave 0 — guardrails and the first win

Nine cards, near-fully parallel, **no ratification gate**. Nothing here depends
on an unratified amendment.

Read `docs/workplan/agent-brief.md` before any card. One agent per card.

Two ordering constraints, and no others:

- T0.2 and T0.5 both add a `[workspace] members` entry to the root
  `Cargo.toml`. **T0.2 merges first; T0.5 rebases.**
- T0.6 and T0.8 both touch `crates/slate-doc/src/scene.rs`. **T0.6 merges
  first; T0.8 rebases.** T0.8 may develop against T0.6's branch.

Deviation rows in `docs/audit/deviations.md` are owned row-by-row: the card
named in a row's *Closes with* column may edit that row (set `Status` to
`closed`, fill the `Closed` column with its commit). No card may edit another
card's row or delete any row.

---

## T0.1 — Continuous integration {#t01}

**Owns:** `.github/workflows/ci.yml` (new).
**Depends on:** nothing. **Size:** XS.
**Why:** twelve agents are about to push branches into a repository with no
automated check of any kind. Everything else in this plan assumes CI exists.

### Do

Create one workflow, `CI`, triggered on `push` and `pull_request`:

- Job **`check`** on `ubuntu-latest`:
  - checkout; install stable Rust with `rustfmt` and `clippy`;
    cache `~/.cargo` and `target` keyed on `Cargo.lock`;
  - `cargo fmt --all -- --check`
  - `cargo test --workspace`
- Job **`lint`** on `ubuntu-latest`: `cargo clippy --workspace --all-targets -- -D warnings`
- Job **`windows`** on `windows-latest`: `cargo check --workspace`

Mirror the toolchain setup that `.cursor/environment.json` already uses for
cloud agents so local, cloud, and CI environments agree.

### Baseline-dirty rule (read this before you start)

Run `cargo clippy --workspace --all-targets -- -D warnings` **first**.

- If it passes: `lint` is a blocking job. Done.
- If it fails: **do not fix the warnings** — that is not your card. Set
  `continue-on-error: true` on the `lint` job only, record the exact warning
  count and the top three warning kinds in your PR body, and propose (in the PR
  body, not in the file) a new deviation row so a future card can clean it up.

Same rule for `cargo fmt --all -- --check` and for the Linux test run: if the
baseline is already red, report it precisely rather than repairing it.

**Backlog cleared 2026-07-30.** `cargo clippy --workspace --all-targets --
-D warnings` is green and `continue-on-error` is gone, so `lint` blocks like
the other two jobs. The sweep also opened DV-12. `push` now only triggers on
`main`; pull requests run the suite once instead of twice.

### Accept

- [ ] A pull request against `main` runs all three jobs.
- [ ] A deliberately mis-formatted file fails `check` (demonstrate in the PR by
      describing the trial run; do not leave the bad file committed).
- [ ] The PR body states, as numbers, the baseline results of fmt, clippy, and
      test on Linux.

### Forbidden

Changing any Rust source. Adding a release-build or artifact-publishing job.
Adding third-party actions beyond `actions/checkout`, `actions/cache`, and a
Rust toolchain action.

---

## T0.2 — `cargo xtask metrics` and the first baseline {#t02}

**Owns:** `xtask/**` (new crate), `docs/metrics/**` (new), root `Cargo.toml`
(`[workspace] members` + `[workspace.dependencies]` only), the metrics block in
`docs/audit/deviations.md` between the `<!-- metrics:deviations:* -->` markers.
**Depends on:** nothing (merge before T0.5). **Size:** M.
**Why:** decision D6 — audit №2 should diff numbers, not impressions.

### Do

Add a `xtask` binary crate (the standard cargo-xtask pattern: a workspace member
invoked as `cargo run -p xtask -- metrics`; add
`xtask = "run -p xtask --"` to `.cargo/config.toml` `[alias]` so
`cargo xtask metrics` works).

Dependencies: `serde`, `serde_json`, `toml`, `syn` (all already in the workspace
tree — reference them with `{ workspace = true }` where the root declares them,
per `AGENTS.md`). No others.

`cargo xtask metrics` walks the workspace and writes two artifacts:

1. `docs/metrics/<YYYY-MM-DD>.json` — the snapshot.
2. `docs/metrics/README.md` — regenerated: one column per snapshot date, one row
   per metric, newest column first, capped at the eight most recent snapshots
   with older ones remaining as JSON on disk.

It also rewrites the counts between the markers in `docs/audit/deviations.md`.

### Snapshot schema (exact)

```json
{
  "date": "2026-07-25",
  "commit": "bb1acfc",
  "totals": {
    "lines_total": 0, "lines_code": 0,
    "pure_lines_code": 0, "renderer_lines_code": 0, "pure_ratio": 0.0,
    "crates": 0, "tests": 0, "unsafe_blocks": 0, "direct_dependencies": 0
  },
  "crates": [
    { "name": "slate-doc", "kind": "lib", "renderer": false,
      "lines_total": 0, "lines_code": 0, "tests": 0,
      "unsafe_blocks": 0, "longest_file": { "path": "src/scene.rs", "lines": 2050 } }
  ],
  "model": {
    "format_version": 2,
    "node_kinds": 5, "node_kind_names": ["frame", "image", "shape", "text", "connector"],
    "edge_roles": 0, "edge_role_names": [],
    "scene_cmd_variants": 3
  },
  "commands": { "slate": 0, "file-atlas": 0 },
  "deviations": { "open": 0, "accepted": 0, "closed": 0 }
}
```

### How each metric is computed (be literal — determinism is the point)

| Metric | Rule |
|---|---|
| `lines_total` | every line of every `.rs` file under the crate's `src/` and `tests/` |
| `lines_code` | as above, excluding blank lines and lines whose trimmed form starts with `//` |
| `renderer` | `true` when the crate's own `Cargo.toml` names `egui`, `eframe`, or a crate that does (one level of workspace-internal resolution is enough) |
| `pure_ratio` | `pure_lines_code / lines_code`, rounded to 3 decimals |
| `tests` | count of `#[test]` attributes, parsed with `syn`, not regex |
| `unsafe_blocks` | count of the token `unsafe` at statement or block position, parsed with `syn` |
| `format_version` | the literal in `pub const CURRENT: u32 = N;` in `crates/slate-doc/src/doc.rs`, parsed with `syn` |
| `node_kinds` / `edge_roles` / `scene_cmd_variants` | variant counts of `enum NodeKind`, `enum EdgeRole` (0 if absent), `enum SceneCmd` in `crates/slate-doc/src/`, parsed with `syn` |
| `commands` | number of elements in the `SPECS` static array in each app's `src/app/commands.rs`, parsed with `syn` |
| `deviations` | rows of the table in `docs/audit/deviations.md` grouped by the `Status` column |
| `direct_dependencies` | distinct non-workspace dependency names across all member `Cargo.toml` files |
| `commit` | `git rev-parse --short HEAD`; if git is unavailable, `"unknown"` |

Everything sorts deterministically (crates by name, kinds by declaration order).
Running the command twice on an unchanged tree must produce byte-identical
output — assert this in a test.

### Accept

- [ ] `cargo xtask metrics` writes `docs/metrics/2026-07-25.json` and
      regenerates `docs/metrics/README.md`.
- [ ] Test `metrics_snapshot_is_deterministic` runs the collector twice on the
      repo and asserts identical JSON.
- [ ] Test `deviation_counts_match_ledger` parses `docs/audit/deviations.md` and
      asserts that every `DV-` row is classified as open, accepted, or closed.
      **Do not assert a fixed open count** — every card that closes a row would
      break it, which is a test that fails on success.
- [ ] The committed baseline's per-crate LOC is within ±5% of audit §2.1's
      table; explain in the PR body any crate that is not (a different counting
      rule is a fine explanation; a missing crate is not).
- [ ] `cargo xtask metrics` exits non-zero with a clear message if run outside
      the workspace root.

### Forbidden

Editing any file outside the owned set. Making the tool network-aware. Adding a
`--fix` mode. Counting anything not in the schema above (a metric nobody asked
for is Article III debt in a tool whose whole job is measuring debt).

---

## T0.3 — Migration fixture harness {#t03}

**Owns:** `crates/slate-doc/tests/**` (new), `crates/slate-doc/tests/fixtures/**` (new).
**Depends on:** nothing. **Size:** S.
**Why:** three separate cards (T1.1a → v3, T2.1 → v4, WI-4 → v5) will bump
`SlateDoc::CURRENT`. Each one's hardest acceptance criterion is "an old workbook
still opens". Build the harness once, now, so those cards only add a fixture.

### Do

Create `crates/slate-doc/tests/migration.rs` plus a fixtures directory holding
**verbatim, hand-checked JSON** for each historical format version:

- `fixtures/v1-tags-items.slate.json` — pre-board: no `scene`, no `lens_root`,
  `format_version: 1`, at least two tag groups, three items with assignments,
  a `view` block.
- `fixtures/v2-board.slate.json` — `format_version: 2`, containing at least one
  frame with a non-zero `order`, one image, one shape with a `path`, one text
  node, one connector anchored at both ends, one group, and one locked and one
  hidden node.

Write these by authoring the documents through the public API in a generator
test that is `#[ignore]`d (`generate_fixtures`), then committing the output.
The committed fixtures are the truth; the generator exists so the next version's
fixture is cheap to produce.

Then the harness itself:

```rust
/// Loads every fixture, asserts it upgrades to `SlateDoc::CURRENT`, saves it,
/// reloads it, and asserts the two in-memory documents are equal.
fn round_trip(fixture: &str) -> SlateDoc
```

with tests:

- `v1_fixture_upgrades_to_current`
- `v2_fixture_upgrades_to_current`
- `every_fixture_round_trips` — load → save → load → `assert_eq!`
- `v2_fixture_preserves_scene_shape` — node count, kinds, frame order, group
  keys, connector endpoints survive the round trip
- `future_version_is_rejected` — a fixture with `format_version: 99` returns
  `SlateLoadError::UnsupportedVersion`

Document the convention at the top of `migration.rs` in one short comment: **a
card that bumps `CURRENT` adds a fixture for the version it supersedes and a
`vN_fixture_upgrades_to_current` test; it never edits an existing fixture.**

### Accept

- [ ] All five tests pass; `cargo test -p slate-doc` is green.
- [ ] Fixtures are committed JSON files, not string literals in the test.
- [ ] Editing any committed fixture makes at least one test fail (verify by
      trial, describe in the PR).
- [ ] The PR handoff note tells T1.1a exactly which file to add and which test
      name to use.

### Forbidden

Changing any `src/` file. Changing `SlateDoc::CURRENT`. Using `serde_json::Value`
comparisons in place of `SlateDoc` equality (the point is that the *model*
round-trips, not that the text matches).

---

## T0.4 — WI-1 · workbook lease and read-only mode {#t04}

**Owns:** `crates/slate-doc/src/lease.rs` (new), `crates/slate-doc/src/lib.rs`
(one `pub mod` line and re-exports), `apps/slate/src/app/mod.rs`,
`apps/slate/src/app/ui/readouts.rs`, deviation row **DV-06**.
**Depends on:** nothing. **Size:** S (half a day).
**Why:** decision D1 — the workbook is shared daily. Today two people opening
the same `.slate` on the firm share silently destroy each other's work, and
there is no lock, no dirty re-check, and no external-change detection anywhere
in the codebase. This is the one card that prevents real loss this week.

### Do — part 1, the pure lease (`slate-doc`)

```rust
pub const LEASE_STALE_SECS: i64 = 30;
pub const LEASE_HEARTBEAT_SECS: i64 = 10;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaseInfo {
    pub holder: String,     // OS user name, best effort; "unknown" if unavailable
    pub host: String,       // machine name, best effort
    pub pid: u32,
    pub acquired_at: i64,   // unix seconds
    pub heartbeat_at: i64,
}

#[derive(Debug)]
pub enum LeaseState {
    /// This process owns the lease.
    Acquired(Lease),
    /// Someone else holds a live lease; open read-only.
    Held(LeaseInfo),
}

#[derive(Debug)]
pub struct Lease { /* lock path + own info */ }

impl Lease {
    /// Attempts to take the lease for `doc_path`. A lease whose heartbeat is
    /// older than [`LEASE_STALE_SECS`] is stolen (the holder crashed).
    pub fn acquire(doc_path: &Path) -> io::Result<LeaseState>;
    /// Refreshes `heartbeat_at`; cheap enough to call every frame, writes at
    /// most once per [`LEASE_HEARTBEAT_SECS`].
    pub fn heartbeat(&mut self) -> io::Result<()>;
    /// Deletes the lock file. Also runs on `Drop`, best effort.
    pub fn release(self);
}
```

- Lock file is `<workbook file name>.lock` beside the workbook (so
  `board.slate` → `board.slate.lock`); contents are `LeaseInfo` as JSON.
- Acquisition uses `OpenOptions::new().create_new(true)` — the atomic primitive.
  On `AlreadyExists`, read and parse; unreadable or unparsable lock files older
  than `LEASE_STALE_SECS` are treated as stale; unparsable and *fresh* is
  `Held(LeaseInfo::unknown())`.
- Stealing a stale lease overwrites the file and returns `Acquired`.
- `Drop` releases. Never panic in `Drop`.

### Do — part 2, the app (`apps/slate`)

- `SlateTab` gains `lease: Option<Lease>` and `read_only: bool`.
- `open_doc_at` acquires. On `Held(info)`, the tab opens with `read_only = true`
  and a toast naming the holder and host.
- `save_doc` on a read-only tab refuses with a toast that points at **Save a
  copy…**; `save_doc_as_dialog` stays available and, on success, acquires the
  lease for the new path and clears `read_only`.
- `close_tab` releases; so does app exit for every tab.
- The frame pump calls `heartbeat()` for the active tab (add it to the existing
  pump sequence in `update_app`, next to `drain_pickers`).
- Read-only state is visible in two places that already exist: the tab title
  gains a ` (read-only)` suffix next to the existing ` •` dirty marker, and the
  bottom readout row shows the holder. **Do not invent a banner widget** —
  Article X: no new chrome in an app crate.
- A read-only tab must still allow *viewing, presenting, and exporting*. Board
  edits are refused at the mutation helpers with a single toast (rate-limited to
  one per second), not by disabling every tool.

### Accept

- [ ] `lease_acquire_then_second_acquire_is_held` (pure, temp dir).
- [ ] `stale_lease_is_stolen` — write a lock with `heartbeat_at` 60s in the
      past; `acquire` returns `Acquired`.
- [ ] `heartbeat_keeps_lease_fresh` — heartbeat advances the file's
      `heartbeat_at` at most once per interval.
- [ ] `release_removes_lock_file`, and `drop_releases_lease`.
- [ ] `unparsable_fresh_lock_is_held_not_stolen`.
- [ ] Manual, recorded in the PR: two `slate.exe` instances open the same
      workbook; the second is read-only and names the first; closing the first
      lets the second acquire on reopen within 30s.
- [ ] `docs/audit/deviations.md` DV-06 set to `closed` with this PR's commit.
- [ ] `apps/slate/src/app/ARCHITECTURE.md` lifecycle section documents the
      lease in two or three sentences.

### Forbidden

Any locking scheme that blocks the UI thread on the network. Auto-save. Merging
or diffing two versions of a workbook (that is Product B, Wave 3). Retrying
acquisition in a loop. Making the lock file hidden or putting it anywhere other
than beside the workbook — it must be obvious and manually deletable.

---

## T0.5 — `crates/collage`, the pure layout solver {#t05}

**Owns:** `crates/collage/**` (new), root `Cargo.toml` members entry.
**Depends on:** merge after T0.2. **Size:** M.
**Why:** decision D4 — "automatically scale and align a collection of images
into a collage" is the named first agent task. This card builds the arithmetic,
with no board, no egui, and no document types, so it is fully testable on Linux
CI and cannot conflict with any other lane.

### Do

New pure crate, **no dependencies at all** (`std` only), Article I clean.

```rust
/// One input tile. `aspect` is width / height, finite and > 0.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tile { pub key: u64, pub aspect: f32 }

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect { pub x: f32, pub y: f32, pub w: f32, pub h: f32 }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// Rows of uniform height, each row scaled to fill the width exactly.
    JustifiedRows,
    /// Uniform cells; every tile is aspect-fit inside its cell, centred.
    Grid,
    /// Fixed column count; tiles stack in the currently shortest column.
    Masonry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LastRow { /// Scale it like every other row.
                   Justify,
                   /// Keep the target height and left-align (the honest default).
                   Natural }

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Options {
    pub area: Rect,
    pub gutter: f32,
    pub padding: f32,
    pub target_row_height: f32,   // JustifiedRows
    pub columns: u16,             // Grid / Masonry; 0 = solver picks ceil(sqrt(n))
    pub last_row: LastRow,
    pub max_scale: f32,           // never enlarge a tile beyond this multiple of its
                                  // natural size when the caller supplies one; 0 = unbounded
}

impl Default for Options { /* gutter 16, padding 0, target_row_height 240,
                              columns 0, last_row Natural, max_scale 0 */ }

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement { pub key: u64, pub rect: Rect }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolveError { EmptyInput, DegenerateArea, InvalidAspect }

/// Deterministic: identical inputs always produce identical output, in input
/// order. Never allocates inside a loop over tiles more than once per row.
pub fn solve(layout: Layout, tiles: &[Tile], opts: &Options)
    -> Result<Vec<Placement>, SolveError>;

/// The bounding box the solution actually occupies (may be shorter than
/// `opts.area` — the solver fills width, not height).
pub fn extent(placements: &[Placement]) -> Rect;
```

**Justified rows** (the main algorithm, and the one the collage command uses):
greedily append tiles to the current row while the row's height at full width
stays above `target_row_height`; close the row, distribute `gutter` between
tiles, and scale so the row spans the content width exactly. The final row
follows `LastRow`. Do not implement a dynamic-programming linear partition —
greedy is what Flickr and Google Photos ship, it is O(n), and Article III says
build the fraction that is used. Note the DP option in a doc comment and stop.

**Grid**: `columns` (or `ceil(sqrt(n))`) equal cells filling the content width;
each tile aspect-fits and centres inside its cell, so cells are uniform and
images are not distorted.

**Masonry**: fixed columns, each tile scaled to column width, appended to the
shortest column; ties broken by lowest column index (never by hash order).

Invariants that hold for every layout and are asserted in tests:

1. Aspect preserved within `1e-3` relative error for every placement.
2. No two placements overlap (allowing for floating-point slop of `1e-3`).
3. Every placement lies within `area` horizontally, inset by `padding`.
4. Gaps between neighbours equal `gutter` within `1e-3`.
5. Output order matches input order.

### Accept

- [ ] Tests: `justified_rows_preserves_aspect`, `justified_rows_fills_width`,
      `justified_rows_natural_last_row_is_not_stretched`,
      `grid_cells_are_uniform_and_tiles_are_fitted`,
      `masonry_balances_column_heights`, `no_overlaps_in_any_layout`
      (table-driven across all three), `solve_is_deterministic` (same input
      twice, byte-identical), `empty_input_is_an_error`,
      `degenerate_aspect_is_an_error`, `single_tile_centres_naturally`.
- [ ] Bench-style test `three_hundred_tiles_under_five_ms` using
      `std::time::Instant` (assert generously — this is a canary for accidental
      O(n²), not a benchmark).
- [ ] `crates/collage/README.md`: what it does, the three layouts with one
      ASCII sketch each, the invariants above, and an explicit
      "what this deliberately does not do" list (no text flow, no captions, no
      cropping, no face detection, no content-aware anything).
- [ ] Zero dependencies in `Cargo.toml`; crate compiles with
      `cargo check -p collage --no-default-features`.

### Forbidden

Any dependency. Any reference to `slate-doc`, `egui`, or images. Cropping tiles
to fit (the tile's aspect is sacred — the whole point is that the images are not
distorted). Randomness of any kind, including `HashMap` iteration.

---

## T0.6 — One node, one slide: membership determinism {#t06}

**Owns:** `crates/slate-doc/src/scene.rs`, `crates/slate-artifact/tests/**`,
deviation row **DV-04**.
**Depends on:** nothing. Must merge **before** T1.1a starts. **Size:** S.
**Why:** decision D3 keeps frame membership geometric, which is only safe if it
is deterministic. Today `Scene::members_of` returns every non-frame node whose
centre lies inside the frame rect, so a node inside two overlapping frames
belongs to both and `slate-artifact::collect_slides` puts it on two slides. That
is an Article IV honesty bug now, and a divergence source the moment two people
share a board.

### Do

Add the ownership rule and make membership use it:

```rust
/// The frame that owns `node` — the topmost frame whose rect contains the
/// node's centre, matching [`Scene::frame_at`]'s pick order. A node belongs to
/// exactly one frame, or to none. Membership is derived, never stored.
pub fn frame_of(&self, node: NodeId) -> Option<NodeId>;
```

`members_of` is then "every non-frame node whose `frame_of` is this frame",
preserving its current return type and ordering (z-order ascending).

Both functions must agree with `frame_at` on the tie-break, so that dropping an
image where two frames overlap tags it with the same frame that will present it.
If `frame_at`'s reverse-z scan and the new rule can disagree in any case, make
`frame_at` the single implementation and have both call it.

### Accept

- [ ] `members_of_excludes_nodes_owned_by_an_overlapping_frame` — two frames
      overlap, one node's centre falls in both, only the topmost lists it.
- [ ] `frame_of_matches_frame_at_for_node_centres` — property-style test over a
      handful of constructed overlaps.
- [ ] `frame_of_is_none_for_nodes_outside_every_frame`.
- [ ] `members_of_is_unchanged_for_non_overlapping_frames` — the existing
      behaviour is preserved where frames do not overlap (this is the
      regression guard for every current test).
- [ ] New `crates/slate-artifact` integration test
      `overlapping_frames_do_not_duplicate_a_slide_member` — export a scene with
      two overlapping frames and assert the node appears on exactly one slide.
- [ ] Every pre-existing test in `slate-doc` and `slate-artifact` passes
      unchanged.
- [ ] DV-04 set to `closed`.

### Forbidden

Adding a stored parent pointer, a `Membership` edge, or a `frame: Option<NodeId>`
field to `Node` — decision D3 and Amendment E (V.2) forbid it; membership stays
derived. Changing `FrameNode.order` semantics. Changing how `frame_at` picks
(only where it lives, if you unify the implementations).

---

## T0.7 — Palette becomes data {#t07}

**Owns:** `crates/atlas-shell/src/{theme.rs, tokens.rs}`,
`crates/atlas-shell/ui-tokens.toml`, `crates/atlas-shell/TOPBAR.md` (tokens
section only), deviation row **DV-09**.
**Depends on:** nothing. **Size:** S (half a day).
**Why:** decision D6 in the flexibility record. `Palette` already defines
fourteen *semantic* slots — `bg`, `grid_dot`, `card`, `card_hover`, `border`,
`border_strong`, `ink`, `sub`, `line`, `accent`, `portal`, `thumb_bg`, `select`,
`staged`. That is the hard part and it is already right. But the slots are
hardcoded Rust constructors while the metrics beside them are live-tunable data,
and egui's own `Visuals` are set *beside* the palette rather than derived from
it — two colour systems, hand-synchronised, which is exactly the divergence
Article X exists to prevent.

### Do

- Add `[palette.light]` and `[palette.dark]` tables to `ui-tokens.toml`, one key
  per semantic slot, hex strings. Bump the file's `schema_version`.
- `Palette::light()` / `Palette::dark()` read from tokens instead of literals.
  The rendered result must be **identical** — same hex values, no "improved
  while I was there".
- **Derive egui `Visuals` from `Palette`** rather than constructing it
  separately. This is the structural half of the change: after it, a theme
  cannot be half-applied.
- Load additional themes from a user-space directory beside the other prefs
  (`{app_key}-chrome.json`'s folder); a theme is a `[palette.<name>]` table in a
  `.toml` file. Unknown or missing slots fall back to the built-in dark values
  with a one-line warning, never a panic.
- The live tuner (`ui-tuner` feature) picks up colours the same way it already
  picks up metrics.

### Accept

- [ ] `palette_from_tokens_matches_previous_constants` — a test asserting every
      slot equals the hex it had before this change (paste the old constants
      into the test as the expected values).
- [ ] `visuals_are_derived_from_palette` — changing one slot changes the
      corresponding egui visual.
- [ ] `unknown_theme_slot_falls_back_without_panicking`.
- [ ] `user_theme_file_overrides_builtin`.
- [ ] Both apps look pixel-identical before and after (screenshots in the PR).
- [ ] A 14-line example theme is committed at
      `crates/atlas-shell/themes/example-high-contrast.toml` and documented in
      `TOPBAR.md`'s tokens section.
- [ ] DV-09 set to `closed`.

### Forbidden

Changing any colour value. Adding a theme picker UI (that is a later card — this
card makes themes *possible*, it does not surface them). Moving non-colour
chrome constants. Touching app crates.

---

## T0.8 — Spatial index over node bounds {#t08}

**Owns:** `crates/slate-doc/src/spatial.rs` (new) and the query paths in
`scene.rs` it replaces, deviation row **DV-10**.
**Depends on:** T0.6 merged (both touch `scene.rs`; T0.6 first). **Size:** M.
**Why:** the only item in either audit with a deadline set by someone else's
enthusiasm. Every hit-test, marquee, and paint cull is a linear scan over
`Scene.nodes`. Fine at a thousand nodes, unusable at twenty thousand — and
adding the index *after* the painter and hit-tester have grown twenty call sites
that assume linear iteration costs a week and a regression hunt instead of a
day.

### Do

- A grid or R-tree over node AABBs in `slate-doc`, pure, no dependencies.
  **Prefer a uniform spatial hash grid** unless you can argue otherwise in the
  PR: it is a hundred lines, it rebuilds incrementally, and it does not degrade
  on the clustered layouts real boards have.
- The index is **derived state** (Amendment F / VI.3): rebuilt from the scene,
  never serialized, invalidated by the existing `scene_gen` counter.
- Add `Scene::query_rect(&self, rect: WorldRect) -> Vec<NodeId>` and
  `Scene::query_point(&self, x: f32, y: f32) -> Vec<NodeId>`, both returning
  candidates in z-order; exact geometry tests stay where they are.
- Convert `node_at`, `frame_at`, and the marquee path to query the index. Do not
  convert anything else in this card.
- Rotated nodes and connectors use their AABB as the broad phase; the narrow
  phase is unchanged.

### Accept

- [ ] `query_rect_matches_linear_scan` — property-style test over a few hundred
      constructed scenes asserting the index returns exactly what a brute-force
      scan returns.
- [ ] `query_survives_add_remove_move` — mutations keep the index correct.
- [ ] `hit_test_is_unchanged_for_existing_scenes` — every existing hit-test test
      passes untouched.
- [ ] `ten_thousand_nodes_marquee_under_two_ms` — canary, generous bound.
- [ ] The PR reports the before/after timing of a 10k-node marquee.
- [ ] DV-10 set to `closed`.

### Forbidden

Serializing the index. Adding a dependency. Changing hit-test *semantics*
(topmost wins, frames behind content, locked/hidden rules) — this card is a
broad phase, nothing more. Converting paint culling in this card.

---

## T0.9 — `EXTENDING.md` and the false-affordance register {#t09}

**Owns:** `EXTENDING.md` (repo root, new), `docs/false-affordances.md` (new).
**Depends on:** nothing. Documentation only. **Size:** S.
**Why:** the repository is going public. A stranger with an idea should be able
to self-triage in ten minutes — is this a weekend or a rewrite? — without
opening an issue somebody has to answer. This is the cheapest recruitment
artifact available and it is also how "no" becomes impersonal.

### Do

**`EXTENDING.md`**, written for someone who has never read the constitution:

1. The six flexibility classes from audit №2 §2 (Token, Leaf, Variant, Organ,
   Transplant, Different Animal), with **one worked example each drawn from real
   requests** and an honest statement of cost.
2. **The open/closed enum rule**, with the table: *close an enum where an agent
   must enumerate it; open an enum where a human must express themselves through
   it.* Closed — `NodeKind`, `EdgeRole`, `Facet`, `CommandId`, `SourceKind`.
   Open — `FontChoice`, `Dash`, `EdgeRouting`, theme names, corner and cap
   profiles.
3. **The extension ladder** (Amendment F / VII.8): declarative assets, then
   out-of-process MCP servers, then — subject to a future amendment — sandboxed
   in-process code. Native binaries never. State plainly that **MCP is the
   plugin API**.
4. **The capability-crate contract**: what a new capability may touch (its own
   crate, the command registry, facets, portals) and may not (the core scene
   model, chrome painting, another capability).
5. **Published fork seams** — one paragraph each for the requests whose right
   answer is a fork: a VR canvas forks at `WorldRect` and the camera; an
   OS-window-embedding build forks at `NodeKind` and is Windows-only forever; a
   two-hundred-person hosted whiteboard forks at the session layer. Name the cut
   and name what is reusable. MIT means this is generosity, not rejection.

**`docs/false-affordances.md`** — features refused *because the architecture
would make them lies*, each with its reason: permission-hidden or encrypted
board regions (content sits in the file and in every export; hiding at paint
time is theatre), legal redaction (a black rectangle in SVG leaves recoverable
text), DRM or view-limited artifacts, hallucinated analysis graphs (Article
IV.2), and **silent or agent-initiated write-back** (decision D9 permits
explicit human write-back and forbids every other form).

Note in the header that this register exists so that a good-faith request with a
convincing-looking implementation gets a real answer instead of a debate.

### Accept

- [ ] Both files exist, are readable by someone who has not read
      `CONSTITUTION.md`, and link to it rather than restating it.
- [ ] Every class has a worked example and a cost.
- [ ] Every fork seam names both the cut and what is reusable.
- [ ] `README.md` links to `EXTENDING.md`.
- [ ] No claim in either file contradicts `CONSTITUTION.md` or the two decision
      records; where a decision overturned an audit verdict, the decision wins
      and the file says nothing about the audit.

### Forbidden

Proposing amendments. Softening a refusal to sound welcoming — the register's
value is that it is blunt and gives a reason. Documenting features that do not
exist as though they do.
