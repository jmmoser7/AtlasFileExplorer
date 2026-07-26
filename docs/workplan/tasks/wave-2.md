# Wave 2 — identity, and the collage shipping end to end

**Gate:** Wave 1 merged. Four cards, largely parallel.

This wave is where the plan pays out: by the end of it you can select thirty
images and get a collage in one undo step, and an agent can propose the same
collage through MCP and have you accept or reject it as a unit.

Known collisions:

- `crates/slate-doc/src/lib.rs` module list: **T2.1 merges before T2.3**.
- `apps/slate/src/app/board.rs`: **T2.2 owns it.** T2.1's board-side edits are
  limited to reads of `item.path` and must be listed line-by-line in its PR; if
  they exceed ~30 lines there, T2.1 stops and escalates for a follow-up card.

---

## T2.1 — WI-3 · source identity, relative-first, tri-state health {#t21}

**Owns:** `crates/slate-doc/src/{item.rs, link.rs, doc.rs, source.rs (new), lib.rs}`,
`apps/slate/src/app/{mod.rs, canvas.rs, ui/readouts.rs}`,
`crates/slate-doc/tests/` (fixture + tests), deviation row **DV-03**.
**Depends on:** G0 (Amendment A), T1.1a's v3 landing. **Size:** L.
**Why:** decision D1. Twelve people mount the firm share twelve ways; a workbook
whose links are absolute paths renders differently for each of them. This is
audit F2/F6, and it gets linearly more expensive with every workbook authored.

### Do — the model

New `source.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// The only kind in scope (decision D2 removed platform adapters).
    LocalFs,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceUri {
    pub kind: SourceKind,
    /// Volume or UNC host: `C:`, `\\server\projects`. Empty for relative-only.
    pub authority: String,
    /// The absolute locator as authored on the machine that added the link.
    pub locator: String,
    /// Locator relative to a detected root, with `/` separators. Resolution
    /// prefers this (Amendment A / IX.2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_relative: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ContentId {
    #[serde(default, skip_serializing_if = "Option::is_none")] pub etag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub mtime: Option<i64>,
    pub cache_key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkHealth { Ok, Missing, Unknown }
```

**Root detection reuses what already exists.** `atlas-core` already computes
project-relative thumbnail cache keys against a detected project root — find
that function and call it; do not write a second root detector. The resolution
order is: (1) `root_relative` against the currently detected root, (2)
`locator`, (3) `Missing`.

If no project root is detected, the workbook's own folder is the root when the
item lives under it — which covers the common "board and images in one shared
folder" case that decision D1 makes daily.

### Do — the item

`SlateItem` gains `uri: SourceUri` and `content: ContentId`.

**Blast-radius containment:** roughly forty call sites read `item.path` across
four crates. Keep `path: PathBuf` as a **documented mirror** of `uri.locator`,
written by the constructor, `load_from`, and `relink`, and never written by
anything else. Add a doc comment saying exactly that and naming the follow-up
card (Wave 3, T3.0) that deletes it. `size`, `mtime`, and `cache_key` move into
`content` with mirrors on the same terms.

This is a deliberate, temporary duplication chosen so that one card does not
touch forty call sites in three lanes at once. Say so in the code comment; an
undocumented mirror is a trap, a documented one is a migration step.

### Do — tri-state health

- `link_status` returns `LinkHealth` and never touches the filesystem on the UI
  thread. `LinkStatus` is renamed with a re-export for one version if that keeps
  the diff small.
- The app runs a background sweep on the existing pattern (spawned thread,
  `crossbeam_channel`, generation tag, drained in the frame pump next to
  `drain_thumbs`). Items start `Unknown`; the sweep resolves them.
- `Unknown` is a first-class UI state, not an error: the grid shows a neutral
  marker, not the missing-link marker, and the readout counts the three states
  separately.

### Do — migration to v4

`SlateDoc::CURRENT` 3 → 4 (stop and escalate if it is not 3). On load, every
item without a `uri` gets `SourceUri::local(path)` with `root_relative` computed
against the detected root; `content` is built from the existing `size`, `mtime`,
`cache_key`. No thumbnails are invalidated — the cache key is carried over
verbatim, and a card that reflows the thumbnail cache is out of scope.

### Accept

- [ ] `source_uri_prefers_root_relative_on_resolution`.
- [ ] **`workbook_opens_when_the_root_moves`** — author a document with items
      under a temp root, move the whole tree to a different absolute path,
      reload, and assert every item resolves `Ok`. This is the acceptance test
      decision D1 turns on.
- [ ] `absolute_fallback_resolves_when_no_root_is_detected`.
- [ ] `unknown_health_is_the_initial_state_and_never_shown_as_missing`.
- [ ] `v3_fixture_upgrades_to_v4_with_uris` using the T0.3 harness, plus a new
      committed `v3-*.slate.json` fixture.
- [ ] `cache_keys_are_unchanged_by_migration` — thumbnails do not re-render for
      an existing workbook (verify on Windows and say so in the PR).
- [ ] Health sweep never blocks: no `Path::exists` call on the UI thread
      remains — grep and paste the result.
- [ ] DV-03 set to `closed`.

### Forbidden

Adding a `Source` trait, an `opendal` dependency, or any non-local
`SourceKind` — decision D2 removed them, and Article III forbids the seam
without the use. Version pinning (Amendment A.3 was dropped). Touching
`board.rs` beyond `item.path` reads. Re-keying the thumbnail cache.

---

## T2.2 — WI-7 · collage, align, and distribute as commands {#t22}

**Owns:** `apps/slate/src/app/{board.rs, dispatch.rs, commands.rs, ui/tools.rs}`,
`apps/slate/src/app/COMMANDS.md`, `docs/keymap/specs/arrange.md` (new),
deviation row **DV-05**.
**Depends on:** T0.5 (`crates/collage`), T1.1c (property-scoped commands).
**Size:** M.
**Why:** decision D4 — this is the named weekly use, and the first agent task.
It is also the vertical slice that proves the command surface: one arrangement,
invoked identically by a human keystroke, a dock button, and an agent.

### Do — wire the dead code first

`align_board_selection(BoardAlign)` and `distribute_board_selection(DistributeAxis)`
already exist in `board.rs` (~lines 818–892) with **zero call sites**, and
`commands.rs` documents `board.align` with no dispatch arm (DV-05). Register and
wire:

| Command id | Binding | Notes |
|---|---|---|
| `board.align.left` / `.center_h` / `.right` / `.top` / `.middle_v` / `.bottom` | palette + dock menu, no chords | needs 2+ selected |
| `board.distribute.horizontal` / `.vertical` | palette + dock menu | needs 3+ selected |

Follow the existing `spec(...)` table style verbatim, including the description
prose style, `Repeat` choice, availability flags (`BOARD | NEEDS_SELECTION`),
and aliases. Add the dock menu entries where the board's bottom dock already
expects an align group.

### Do — the collage

| Command id | Binding | Layout |
|---|---|---|
| `board.arrange.collage` | one chord, chosen per `COMMANDS.md`'s free-key rules | `collage::Layout::JustifiedRows` |
| `board.arrange.grid` | palette only | `collage::Layout::Grid` |

Behaviour, specified so it is not a judgement call:

1. **Participants** — selected nodes that are images or shapes, excluding
   frames, connectors, locked, and hidden nodes. Groups participate as their
   members. Fewer than two participants: toast and do nothing.
2. **Area** — if `Scene::frame_of` (from T0.6) reports that every participant is
   owned by the *same* frame, the area is that frame's rect inset by the frame's
   padding token; otherwise it is the bounding box of the participants.
   Arranging inside a frame must not change slide membership.
3. **Aspect ratios** — from the existing `image_natural_size`; a node whose
   natural size is unknown uses its current rect's aspect.
4. **Order** — participants keep z-order (ascending), so the arrangement is
   deterministic and re-running it is idempotent.
5. **Apply** — one `patch_nodes` call over every participant, producing one
   journal group and therefore **one undo step**. Rotation is preserved; the
   command only writes `Rect`.
6. **Idempotence** — running the command twice in a row produces no second
   visible change (assert it).

Then write `docs/keymap/specs/arrange.md` in the style of the existing
`docs/keymap/specs/*.md`: the rules above, the parameter defaults, the
participant/exclusion table, and an explicit non-goals list.

### Accept

- [ ] `collage_arranges_selection_in_one_undo_step` — 12 nodes, one `Ctrl+Z`
      restores every rect.
- [ ] `collage_uses_the_frame_rect_when_all_participants_share_a_frame`.
- [ ] `collage_preserves_aspect_ratios` (within 1e-3).
- [ ] `collage_is_idempotent`.
- [ ] `collage_excludes_locked_hidden_frames_and_connectors`.
- [ ] `collage_below_two_participants_is_a_no_op`.
- [ ] `align_and_distribute_commands_dispatch` — one test per registered id,
      asserting the dispatch arm exists and mutates through the journal.
- [ ] `registry_validate_passes` — the existing registry validation still
      passes with the new entries (no duplicate ids, chords, or aliases).
- [ ] Manual on Windows, in the PR: drop 30 photos of mixed orientation, select
      all, run the command, screenshot before and after.
- [ ] DV-05 set to `closed`; `COMMANDS.md` updated.

### Forbidden

Cropping images to make a tidier grid (the tile aspect is sacred — see the
`collage` crate's non-goals). Adding layout options beyond the two commands.
Mutating the scene outside `patch_nodes`. Writing a second layout
implementation in the app: all arithmetic lives in `crates/collage`, and
`board.rs` only converts selection → `Tile`s and `Placement`s → rects.

---

## T2.3 — The staging layer: agents propose, humans accept {#t23}

**Owns:** `crates/slate-doc/src/stage.rs` (new) + `lib.rs` module line,
`crates/atlas-shell/src/ai_panel.rs`, `docs/agent-staging-contract.md` (new),
`apps/slate/src/app/mod.rs` (watcher wiring only — coordinate with T2.1, which
also touches it: **T2.1 merges first**).
**Depends on:** G0 (Amendment C / VII.6), T1.2. **Size:** M.
**Why:** Amendment C / VII.6 — agent mutations enter a staging layer and require
human acceptance. Audit §5.5: an agent commits at machine speed; a free agent
rearranging a board while you work is unusable however correctly it merges. The
pattern already exists in this repo as the Lens overlay; this card generalises
it from a Lens feature into the ecosystem's staging model.

### Do — the model

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Proposal {
    pub id: String,              // agent-chosen, stable, unique per workspace
    pub author: String,          // agent name; becomes CmdAuthor::Agent(name)
    pub title: String,           // human sentence: "Collage 12 images into frame 3"
    pub created_at: i64,
    pub target: ProposalTarget,  // workbook path + expected format_version
    pub cmds: Vec<SceneCmd>,
    #[serde(default)]
    pub status: ProposalStatus,  // Pending | Accepted | Rejected | Stale
}
```

- `accept(proposal, &mut scene, &mut journal)` commits every command as one
  group with `CmdAuthor::Agent(author)`; a rejection from `Scene::apply`
  (T1.1b) marks the proposal `Stale` and commits nothing — partial application
  is prohibited.
- `reject` drops it. Neither path mutates anything outside the journal.

### Do — the file contract

Write `docs/agent-staging-contract.md` in the style of
`docs/lens-agent-contract.md` (numbered sections, exact paths, schema table,
staleness rules, forward compatibility). The contract:

- Agents write `<ai-workspace>/.atlas-ai/stage/<id>.json`; the app watches the
  directory on the existing throttled-poll pattern (≥1s, mtime-gated) that
  `LensBeacon` uses.
- The app writes back `<id>.result.json` with the outcome so the agent learns
  whether its proposal landed.
- A proposal whose `target.format_version` does not match, or whose commands are
  rejected, is `Stale` and says why.
- Presence and cursors are **not** in this channel (Amendment B / VIII.5).

### Do — the surface

Staged proposals appear in the shared AI panel (`atlas-shell/src/ai_panel.rs`):
one row per pending proposal with title, author, command count, and **Accept** /
**Reject**. Painting staged geometry on the canvas as a tinted overlay is
deliberately deferred to Wave 3 — a list satisfies "visible, attributed, and
rejectable as a unit" and does not collide with T2.2's ownership of `board.rs`.

### Accept

- [ ] `accept_commits_as_agent_author` — the journal group's author is
      `CmdAuthor::Agent("...")`.
- [ ] `accept_is_all_or_nothing` — a proposal with one invalid command mutates
      nothing and is marked `Stale`.
- [ ] `reject_leaves_the_scene_untouched`.
- [ ] `proposal_round_trips_through_json`, including unknown-field tolerance.
- [ ] `stale_format_version_is_refused_with_a_reason`.
- [ ] Watcher test: dropping a file into the stage directory surfaces a row
      within two poll intervals (headless test on the same pattern as the Lens
      overlay tests).
- [ ] `docs/agent-staging-contract.md` exists and an example proposal JSON is
      committed under `docs/agent-staging/example-collage-proposal.json`.

### Forbidden

Letting any agent write directly to the journal (autonomy grants are a later
card and require an explicit user setting). Auto-accepting anything. Putting
presence, cursors, or chat in this channel. Painting on the canvas in this card.

---

## T2.4 — WI-6b · `atlas-mcp`, the command registry over MCP {#t24}

**Owns:** `crates/atlas-mcp/**` (new), root `Cargo.toml` members entry,
`crates/atlas-ai/src/config.rs` (the `.cursor/mcp.json` scaffold only).
**Depends on:** T1.2 (provider seam), T2.3 (staging contract). **Size:** M.
**Why:** Article VII.1 — the MCP surface exposes the same commands as the human
UI. The specification's 2026-07-28 revision is final, so the adapter is built
against a released spec rather than a release candidate. Amendment C / I.2
requires that protocol churn stay in a leaf crate.

### Do

New binary+lib crate `atlas-mcp`, stdio transport, using `rmcp` (the official
Rust SDK — **this is the one card permitted to add a third-party dependency**;
pin it in `[workspace.dependencies]`). It depends on `atlas-commands`,
`slate-doc`, `atlas-ai`, `serde`, `serde_json` — and on **no** app crate and no
renderer.

Three tools, and no more:

| Tool | Reads / writes | Behaviour |
|---|---|---|
| `commands_list` | reads `atlas_commands::Registry` | returns every `CommandSpec` as id, name, category, description, availability flags, chord text. This is generated from the same table that drives the palette and the Advanced window — the fourth consumer of "specs are data" |
| `context_read` | reads `<ai-workspace>/.atlas-ai/<app>-context.json` | the existing beacon, unchanged. Do not modify the beacon format |
| `board_propose` | writes `<ai-workspace>/.atlas-ai/stage/<id>.json` | takes a title and a list of `{ command_id, params }` steps, writes a `Proposal` per the T2.3 contract, returns the proposal id |

**The app is never called directly.** The MCP server and the running app
communicate only through the AI-workspace files, which is the pattern the Lens
contract already established and the reason this crate cannot destabilise the
app.

`board_propose` is the collage path: an agent reads the context, sees the
selection, and proposes `board.arrange.collage` with parameters. Verifying that
round trip is this card's whole purpose.

### Accept

- [ ] `commands_list_matches_the_registry` — count and ids equal
      `Registry::new(SPECS).iter()`.
- [ ] `board_propose_writes_a_valid_proposal` — the file validates against the
      T2.3 schema and appears in Slate's AI panel.
- [ ] **End-to-end, recorded in the PR:** an external MCP client (Cursor, or
      `rmcp`'s example client) lists the commands, proposes a collage over the
      current selection, the proposal appears in Slate, accepting it rearranges
      the board, and the journal group is attributed to
      `CmdAuthor::Agent("...")`. Screenshot the before and after.
- [ ] `unknown_command_id_is_refused_before_the_proposal_is_written`.
- [ ] The `.cursor/mcp.json` scaffold written by `AiConfig::set_workspace`
      registers this server.
- [ ] `cargo tree -p atlas-mcp` shows no `egui`, no `eframe`, no app crate.

### Forbidden

Exposing a tool that mutates a document directly (everything goes through the
staging layer — Amendment C / VII.6). Adding an HTTP transport. Implementing
elicitation, sampling, or server-initiated requests. Reaching into the running
app's memory. Adding any dependency other than `rmcp`.
