# Wave 1 — convergent foundations

**Gate:** G0 (amendments A, B, C, E ratified in `CONSTITUTION.md`) **and** Wave 0
merged. An agent that starts here without the gate must stop and escalate.

Two lanes:

- **Lane A — the journal.** T1.1a → T1.1b → T1.1c, strictly sequential, one PR
  each. Together they own `crates/slate-doc/src/{scene.rs, order.rs, doc.rs}`
  and the z-order and mutation helpers in `apps/slate/src/app/board.rs`.
- **Lane D — the agent crate.** T1.2, independent, no shared files.

Lane A implements Amendment B / Article VI.2 and closes deviations DV-01 and
DV-08. It is split into three cards because it is the single highest-risk
refactor in the plan: a mistake in whole-scene addressing is invisible until
someone loses an undo step. Each card leaves the tool fully working.

---

## T1.1a — `ZOrder` fractional index, format v3 {#t11a}

**Owns:** `crates/slate-doc/src/order.rs` (new),
`crates/slate-doc/src/{scene.rs, doc.rs, lib.rs}`,
`apps/slate/src/app/board.rs` (z-order commands only),
`crates/slate-doc/tests/` (one added fixture + test).
**Depends on:** T0.3 (fixture harness), T0.6 (membership). **Size:** M.
**Why:** z-order is currently the position of a node in `Scene.nodes`. Two
participants inserting concurrently cannot merge positional inserts (audit F4.1).
An order key on the node fixes that, and it is a strict improvement in
single-player: reordering becomes a property change instead of a
remove-and-reinsert pair.

### Do — the key type

New module `order.rs`, no dependencies:

```rust
/// A lexicographically-ordered fractional index. Two nodes never need the same
/// key, and a key can always be generated between any two others without
/// renumbering anything else.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ZOrder(String);

impl ZOrder {
    /// A key strictly between `a` and `b`. `None` means "unbounded on that
    /// side". Panics in debug if `a >= b`.
    pub fn between(a: Option<&ZOrder>, b: Option<&ZOrder>) -> ZOrder;
    /// `n` ascending keys, evenly spread — used when assigning keys to an
    /// existing scene during migration.
    pub fn sequence(n: usize) -> Vec<ZOrder>;
    pub fn as_str(&self) -> &str;
}
```

Digits are the 62 ASCII characters `0`–`9`, `A`–`Z`, `a`–`z`, in that order, so
byte-wise string comparison *is* the ordering. Implement `between` exactly as
follows — this algorithm is specified rather than left to judgement because
subtle versions of it produce keys that compare wrong:

```
i = 0, out = ""
loop:
    da = digit of a at i, or 0 if a is None or exhausted
    db = digit of b at i, or 62 if b is None or exhausted
    if db - da > 1:
        out.push(digit(da + (db - da) / 2)); return out
    out.push(digit(da)); i += 1
```

The midpoint digit is always `> da`, so a key never ends in digit `0` and keys
stay canonical. `sequence(n)` places digit `round((i + 1) * 62 / (n + 1))` for
`i` in `0..n` when `n <= 61`, and otherwise builds keys by repeated `between`.

### Do — the scene

- `Node` gains `pub z: ZOrder` with `#[serde(default)]`.
- `Scene.nodes` **stays a `Vec<Node>` and stays sorted by `(z, id)`.** Paint
  order therefore remains "vec order", so no painter, exporter, hit-test, or
  clipboard code changes in this card. `Scene::apply` inserts at the position
  that keeps the vec sorted rather than at a caller-supplied index.
- `Scene::build_node` assigns `z = between(top, None)` so new nodes land on top,
  matching today's behaviour.
- Add `Scene::z_between(&self, below: Option<NodeId>, above: Option<NodeId>) -> ZOrder`
  and use it for the board's bring-forward / send-backward / bring-to-front /
  send-to-back commands: they now emit a `Patch` that changes `z`, not a
  `Remove` + `Add` pair. Behaviour from the user's seat is identical.
- `SceneJournal` gains a capacity limit: `const JOURNAL_CAP: usize = 200`
  command groups on `done`, oldest dropped; `undone` cleared on new commit as
  today. Closes DV-08.

### Do — migration to v3

- `SlateDoc::CURRENT` 2 → 3 (this card's reserved number; if it is already 3 or
  more, **stop and escalate**).
- On load, any node missing a `z` (every v2 document) is assigned
  `ZOrder::sequence(n)` in current vec order, so an existing board's stacking is
  preserved exactly. Follow the `migrate_legacy_lines()` pattern already in
  `load_from`.
- Add fixture `v2-board.slate.json` coverage per T0.3's handoff: new test
  `v2_fixture_upgrades_to_current_with_z_keys` asserts the upgraded document's
  node order is unchanged and every node has a distinct key.

### Accept

- [ ] `zorder_between_is_strictly_ordered` — 1,000 random-ish insertions
      (deterministic seed or fixed table, no RNG dependency) always satisfy
      `a < mid < b`.
- [ ] `zorder_between_none_none_is_stable`, `zorder_sequence_is_ascending`,
      `zorder_never_ends_in_min_digit`.
- [ ] `zorder_serializes_as_a_bare_string`.
- [ ] `scene_stays_sorted_by_z_after_apply`.
- [ ] `bring_to_front_changes_only_z` — the emitted commands contain no
      `Add`/`Remove`.
- [ ] `journal_cap_drops_oldest_group`.
- [ ] `v2_fixture_upgrades_to_current_with_z_keys`.
- [ ] Every existing `slate-doc`, `slate-artifact`, and `apps/slate` test passes
      **unchanged**. If a test must change, that is a signal your card broke
      behaviour — escalate instead of editing the test.

### Forbidden

Changing `SceneCmd` (that is T1.1b). Changing the painter, exporter, or
hit-testing. Removing `Scene.nodes` as a `Vec`. Adding a dependency (a
fractional-index crate exists; this is 60 lines and a hard dependency boundary
in a pure crate is worth more than the 60 lines).

---

## T1.1b — Commands address nodes by identity {#t11b}

**Owns:** same set as T1.1a. **Depends on:** T1.1a merged. **Size:** M.
**Why:** audit F4.1 and F4.3 — positional `Add`/`Remove` do not commute, and
`Scene::apply` returning `false` drops a command silently. Amendment B / VI.2
requires identity addressing and surfaced rejection.

### Do

```rust
pub enum SceneCmd {
    Add    { node: Node },                    // z lives on the node
    Remove { id: NodeId, node: Node },        // node retained for undo
    Patch  { before: Box<Node>, after: Box<Node> },   // unchanged; T1.1c splits it
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SceneReject {
    DuplicateId(NodeId),
    UnknownNode(NodeId),
    IdMismatch { before: NodeId, after: NodeId },
    StaleNode(NodeId),      // the node changed under a command that expected otherwise
}

impl Scene {
    pub fn apply(&mut self, cmd: &SceneCmd) -> Result<(), SceneReject>;
    pub fn apply_all(&mut self, cmds: &[SceneCmd]) -> Result<(), SceneReject>;
    pub fn revert_all(&mut self, cmds: &[SceneCmd]) -> Result<(), SceneReject>;
}
```

`SceneJournal::commit` / `commit_as` / `record` / `record_as` return
`Result<(), SceneReject>`; `undo` / `redo` likewise.

**The app-facing API stays frozen**: `commit_scene(&mut self, cmds) -> bool`,
`add_nodes`, `delete_board_nodes`, `patch_nodes` keep their current signatures,
so no call site outside `board.rs` changes. On rejection, `commit_scene`:

1. logs the `SceneReject` with the command that failed,
2. raises a toast — "This edit no longer applies (the board changed)." —
   rate-limited to one per second,
3. returns `false`.

Silence is the one prohibited outcome (Amendment B / VI.2).

### Accept

- [ ] `apply_add_rejects_duplicate_id`, `apply_remove_rejects_unknown_id`,
      `apply_patch_rejects_id_mismatch` — each asserting the specific
      `SceneReject` variant, not just failure.
- [ ] `remove_then_undo_restores_z_position` — a node removed from the middle
      and undone returns to the same place in the z-order (this is the property
      positional commands got for free and identity addressing must preserve).
- [ ] `concurrent_inserts_commute` — build two command streams authored
      independently (one adds two nodes, the other adds three, all with distinct
      ids and keys), apply them to a common base in both orders, assert the two
      resulting scenes are equal. **This is the acceptance test the whole wave
      exists for.**
- [ ] `rejected_command_is_returned_not_swallowed`.
- [ ] Existing undo/redo tests pass unchanged.
- [ ] DV-01 remains open (it closes with T1.1c) and the PR says so.

### Forbidden

Changing the app-facing mutation helpers' signatures. `unwrap()` on a
`Result<_, SceneReject>` anywhere outside tests. Introducing an
`apply_or_ignore`-style helper: a caller that wants to ignore a rejection must
say so at the call site.

---

## T1.1c — Property-scoped mutation {#t11c}

**Owns:** same set as T1.1a, plus the mutation helpers in
`apps/slate/src/app/board.rs`. **Depends on:** T1.1b merged. **Size:** L.
**Why:** audit F4.2 — a whole-node `Patch` carries every property, so two people
editing *different* properties of one node still clobber each other. It also
makes the journal unreadable ("replace node") and agent attribution coarse.

### Do

```rust
/// Properties that mutate often enough to deserve their own command. Anything
/// not listed here changes through `ReplaceNode`. This list is CAPPED AT 16 —
/// adding a seventeenth requires a new card, not a judgement call.
pub enum PropKey {
    Rect, Rotation, Opacity, Locked, Hidden, Group, Z,
    Fill, Stroke, Corner,
    Crop, Adjust,
    Text, TextStyle,
    FrameTitle, FrameOrder,
}

pub enum PropValue { /* one variant per PropKey, carrying the property's type */ }

pub enum SceneCmd {
    Add         { node: Node },
    Remove      { id: NodeId, node: Node },
    SetProp     { id: NodeId, key: PropKey, before: PropValue, after: PropValue },
    ReplaceNode { before: Box<Node>, after: Box<Node> },
}
```

**The critical design instruction — read twice.** The app-facing helper
`patch_nodes(&mut self, ids: &[NodeId], f: impl Fn(&mut Node))` **keeps its exact
signature**, and there are 44 call sites across ten files that must not change.
Inside it:

1. clone the node, run `f`, and diff before-vs-after with a new
   `Node::diff(before, after) -> NodeDiff`;
2. if every changed field maps to a `PropKey`, emit one `SetProp` per changed
   property;
3. otherwise emit a single `ReplaceNode`.

That containment is what keeps this card from touching the whole app. If you
find yourself editing inspector, wire, path, or clipboard code, stop — the diff
is in the wrong place.

Also:

- `SceneCmd::inverted()` swaps `before`/`after` for `SetProp` as it does for
  `ReplaceNode`.
- `amend_last_patch(after: &Node)` (slider-scrub coalescing, 1.5s window) must
  keep working: coalesce by `(id, key)` when the last group is entirely
  `SetProp`s for one node, and fall back to today's behaviour for
  `ReplaceNode`. The user-visible rule is unchanged — dragging a slider is one
  undo step.
- Every `SetProp` carries the author already threaded through `commit_as` /
  `record_as`; do not add a second authorship channel.

### Accept

- [ ] `patch_nodes_emits_setprop_for_a_single_property_change` — moving a node
      produces exactly one `SetProp { key: Rect, .. }`.
- [ ] `patch_nodes_emits_replacenode_for_unmapped_changes`.
- [ ] `setprop_round_trips_through_undo_redo` for every `PropKey` variant
      (table-driven; a missing variant fails the test).
- [ ] **`concurrent_property_edits_do_not_clobber`** — participant A sets
      `Opacity` on node X while B sets `Rect` on node X; applying both in either
      order yields a scene with both changes. This is the LWW-per-property
      property that Product C depends on, proven in single-player.
- [ ] `slider_scrub_is_one_undo_step` — 20 successive amended patches undo in
      one step.
- [ ] All 44 `patch_nodes` call sites are **untouched** — prove it with the
      diff's file list in the PR body.
- [ ] Existing undo/redo, clipboard, wire, and inspector tests pass unchanged.
- [ ] DV-01 set to `closed`; `docs/audit/deviations.md` DV-08 already closed by
      T1.1a.
- [ ] `apps/slate/src/app/ARCHITECTURE.md`'s board-invariants section documents
      the new command set in three or four sentences.

### Forbidden

Growing `PropKey` past 16 variants. Changing any `patch_nodes` call site.
Persisting the journal (it stays session-local). Introducing a generic
reflection or property-bag mechanism — an explicit enum is what makes the set
enumerable for agents (audit §6.4).

---

## T1.2 — `atlas-ai` becomes renderer-free; the `Provider` trait {#t12}

**Owns:** `crates/atlas-ai/**`, `crates/atlas-shell/src/ai_panel.rs` (new) and
`atlas-shell/src/lib.rs`, `apps/slate/src/app/ui/tools.rs`,
`apps/file-atlas/src/app/ui/tools.rs`, `.cursor/rules/shared-chrome.mdc`,
`AGENTS.md` (the AI-integration section only), deviation row **DV-02**.
**Depends on:** G0 (Amendment C / I.2). Independent of Lane A. **Size:** M.
**Why:** the crate that grows into the agent surface currently depends on
`eframe` and `atlas-shell`, so the durable agent model is renderer-bound —
Article I's substrate hedge inverted, and Amendment C extends that hedge to
model providers. Audit §5.6, decision matrix rank 7.

### Do — move the UI out

- Move `crates/atlas-ai/src/ui.rs` to `crates/atlas-shell/src/ai_panel.rs`,
  keeping `ai_body` and `AiPanel` behaviour byte-identical. The panel is chrome;
  Article X says chrome lives in `atlas-shell`, so this is where it always
  belonged.
- Both apps call `atlas_shell::ai_panel::ai_body(...)` instead of
  `atlas_ai::ui::ai_body(...)`. **The rendered panel must not change by one
  pixel** — no restyling, no relayout, no "while I'm here".
- `crates/atlas-ai/Cargo.toml` loses `eframe`, `atlas-shell`, `rfd`, and
  `crossbeam-channel`. The folder-picker plumbing moves with the panel; the
  *config* (`AiConfig`, workspace validation, scaffolding) stays in `atlas-ai`
  and is called by the panel.
- Update rule 6 of `.cursor/rules/shared-chrome.mdc` and the AI-integration
  section of `AGENTS.md` to name the new path. Both currently say
  `atlas_ai::ui::ai_body`; leaving them stale would make the always-applied rule
  lie.

### Do — the provider seam

Add `crates/atlas-ai/src/provider.rs`, renderer-free and vendor-free:

```rust
pub struct ProviderId(pub String);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Capabilities { pub completes: bool, pub streams: bool, pub cancels: bool }

pub trait Provider: Send + Sync {
    fn id(&self) -> &ProviderId;
    fn display_name(&self) -> &str;
    fn capabilities(&self) -> Capabilities;
    /// Providers that cannot complete in-process (Cursor today) return
    /// `Err(ProviderError::Unsupported)` — the seam exists so that a workbook
    /// never names a vendor, not so that every provider can do everything.
    fn complete(&self, req: &Request) -> Result<Response, ProviderError>;
    fn cancel(&self, id: RequestId);
}

/// Today's reality: Cursor is launched as a partner application and reads the
/// beacon; it completes nothing in-process.
pub struct CursorPartner { /* ... */ }
```

Ship **exactly one** implementation (`CursorPartner`) plus the registry that
resolves a provider by name from user settings. **Do not** write an HTTP client,
an Ollama implementation, or a streaming runtime — Article III, and there is no
named use yet. The point of this card is the seam, not the traffic.

### Accept

- [ ] `crates/atlas-ai` builds with no `eframe`, `egui`, `atlas-shell`, or `rfd`
      dependency: `cargo tree -p atlas-ai` shows none of them (paste it in the
      PR).
- [ ] Both apps compile; the AI panel looks and behaves identically (state so
      explicitly, with a screenshot of each app's panel in the PR).
- [ ] `provider_registry_resolves_by_name`, `unknown_provider_is_an_error`,
      `cursor_partner_reports_no_completion_capability`.
- [ ] `.cursor/rules/shared-chrome.mdc` and `AGENTS.md` name the new path; grep
      for `atlas_ai::ui` returns nothing.
- [ ] DV-02 set to `closed`.

### Forbidden

Changing what the panel looks like. Adding a model provider. Adding an async
runtime. Touching `context.rs`'s beacon format — `atlas-mcp` (T2.4) and the Lens
contract both read it, and changing it here would break them silently.

---

## T1.3 — Put the app on the spatial index {#t13}

**Owns:** the marquee and pick paths in `apps/slate/src/app/board.rs`.
**Depends on:** T0.8 merged. **Size:** S.
**Why:** T0.8 built the index in `slate-doc` and converted `node_at` and
`frame_at`, but its card deliberately forbade touching the app crate. So the
`slate` marquee still scans `scene.nodes` linearly, which is the exact call site
whose growth the index was rushed in to prevent. The measured win in the pure
crate was 254µs → 30µs on a ten-thousand-node marquee; the app currently gets
none of it.

### Do

- Use `Scene::query_rect` as the broad phase for the board marquee, then run the
  existing narrow-phase predicate (`board_path::marquee_hits_node`) on the
  candidates only. The narrow phase is stroke-precise and must not change.
- Do the same for `board_pick_node` via `query_point`.
- Leave paint culling alone until someone measures it; that is a separate card
  with a separate risk profile.

### Accept

- [ ] Selection behaviour is byte-identical: every existing board selection test
      passes unmodified, including the stroke-precise line picking from
      `bb1acfc`.
- [ ] A ten-thousand-node marquee in the app is timed before and after, in the
      PR body.
- [ ] No change to `slate-doc`.

### Forbidden

Changing hit-test semantics. Converting paint culling. Editing an existing test
to make it pass — if one fails, behaviour changed and the card has failed.
