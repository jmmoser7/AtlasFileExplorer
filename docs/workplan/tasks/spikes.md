# Design spikes — run alongside Wave 1

Three spikes, added after audit №2. A spike produces **a contract document and,
where noted, a prototype in a new pure crate** — never an app change and never a
migration. Their purpose is to make the Wave 3–5 cards writable and to let the
user review a design before anyone implements against it.

Spikes are the one place in this plan where an agent is asked to exercise
judgement. Give them to your strongest models, not the cheapest ones, and expect
to iterate on the output with the user before it is adopted.

Each spike is done when its document is adopted, not when it is written.

---

## S1 — The portal taxonomy contract {#s1}

**Produces:** `docs/portal-contract.md`. No code.
**Depends on:** G0 (Amendment F / V.3). **Size:** M.
**Why:** decision D7. Three different things are called "portal", they have
different rules, and Roadmap Phase 3 folds Grid, Venn, and Lens into them. If
the taxonomy is not written first, Phase 3 will bake in the Lens-shaped
assumption that every portal is deterministic — which is exactly the mistake
audit №2 recommended and decision D7 rejected.

### Do

Write the contract in the style of `docs/lens-agent-contract.md` (numbered
sections, exact schemas, staleness rules, forward compatibility). It must
answer, for each of the three classes — **Generated**, **Document**, **Host** —
these questions and no others:

1. **Authority.** Which journal owns mutations made inside it? (Nobody / the
   child document's / the foreign application's, i.e. none of ours.)
2. **Serialization.** What does the artifact writer emit? Generated portals emit
   their regenerated contents; document portals emit the child rendered; host
   portals emit a poster plus a pointer **and a visible mark saying what it is**.
   Article IV.1 is satisfied by honesty, not by fidelity.
3. **Regeneration and staleness.** When are contents recomputed? What does the
   node show while it is loading, and what does it show when its source is gone?
   `Unknown` is a first-class state (Amendment A / IX.3).
4. **Interaction.** What does click, double-click, and hover do? Which tools
   apply to the portal frame versus its contents? **Decision D17 binds the
   document class here:** double-click opens the child in its own tab in version
   one, and in-place editing inside the parent frame comes later, committing to
   the child's journal when it does. The contract must specify the coordinate
   mapping from parent-frame space into child-scene space even though nothing
   consumes it yet — that is what keeps version two from being a rewrite.
5. **Hazards.** Cycle detection for document portals (A contains B contains A),
   a paint-depth cap, lazy resolution above a zoom threshold, and what happens
   when a child moves or is renamed (it is a `SourceUri` problem in a scene
   costume — reuse WI-3).
6. **Collaboration.** Which parts sync and which are per-peer. Contents are
   per-peer for generated portals; the frame always syncs; a host portal's
   surface is visible only to the peer running it, and the others see the poster
   — say so in the interface, or it reads as a bug.

Then map today's code onto the taxonomy: Grid, Venn, and Lens as generated
portals; the nested workbook as a document portal; the planned web view as the
first host portal. Include a table of which `NodeKind`/`ViewKind` becomes what
in Phase 3.

### Accept

- [ ] Every class answers all six questions with no "TBD".
- [ ] A worked example per class, with the JSON the node serialises to.
- [ ] An explicit statement that determinism binds generated portals only, with
      the Article IV.2 reasoning.
- [ ] Phase 3's migration order (Grid → Venn → Lens) is re-checked against the
      taxonomy and either confirmed or challenged with reasons.
- [ ] No implementation, no migration, no `NodeKind` change in this spike.

---

## S2 — The canvas clock and the dynamics layer {#s2}

**Produces:** `docs/clock-and-dynamics.md`, plus a prototype pure crate
`crates/dynamics` with the integrator and force solver — **not wired to the app**.
**Depends on:** G0 (Amendment F / VI.3). **Size:** L.
**Why:** decision D12. Interactive nodes that affect each other at a distance
are wanted, and so are video playheads (D13) and parameter animation. All three
are the same subsystem: state derived from journaled intent plus time.

### Do — the document

Specify, against Articles II, IV, and VI:

- **What is journaled:** dynamics components (mass, velocity, damping, pinned),
  force edges and their strengths, impulses with their epoch, integration
  parameters, the clock's own state (running, paused, time origin).
- **What is derived:** every transform between impulses, trails, playheads.
- **The bake command:** how derived state becomes authored content, and what
  exactly it commits (one `SetProp` group for positions; a new `Path` node for a
  trail).
- **Determinism:** fixed timestep (propose a value and defend it), integration
  scheme (semi-implicit Euler is the default answer; argue if you disagree),
  and the rule that identical journals produce identical motion on every peer.
  This is what makes simulation collaborative for free — only impulses cross the
  wire.
- **The Article II budget:** body-count soft cap with a visible readout, pause
  when off-screen, a fixed per-frame time budget with graceful degradation, and
  what happens when the simulation cannot keep up (slow the clock, never drop
  the frame rate).
- **Export:** an exported artifact shows the state at the exported instant, and
  says which instant. Trails export as paths.
- **Interaction:** how the user flings a body (drag with velocity), how sliders
  drive parameters, what selection and undo mean while the clock runs. **Undo of
  an impulse is an open question** — rewinding to the impulse's epoch is
  coherent but means one undo can move every body on screen; removing the
  impulse from that point forward without disturbing current positions is
  cheaper and less honest. Present both, recommend one, and defend it. The user
  decides; do not assume.

### Do — the prototype

`crates/dynamics`, pure, zero dependencies, Linux-testable:

```rust
pub struct Body { pub key: u64, pub pos: [f32; 2], pub vel: [f32; 2],
                  pub mass: f32, pub damping: f32, pub pinned: bool }
pub enum Force { Gravity { strength: f32 }, Spring { rest: f32, k: f32 },
                 Repulsion { strength: f32 } }
pub struct Link { pub a: u64, pub b: u64, pub force: Force }
pub struct World { /* bodies, links, fixed timestep, accumulated time */ }
impl World {
    pub fn step(&mut self, dt: f32);
    pub fn impulse(&mut self, key: u64, dv: [f32; 2]);
    pub fn trail(&self, key: u64) -> &[[f32; 2]];   // ring buffer, bounded
}
```

Tests must include `two_body_orbit_is_stable_over_ten_thousand_steps` and
`identical_inputs_produce_identical_trajectories` — the second is the
collaboration guarantee, and it is the reason the timestep is fixed.

### Accept

- [ ] Document answers every heading above with no "TBD".
- [ ] Prototype crate compiles, has zero dependencies, and passes the two named
      tests plus a body-count performance canary.
- [ ] The document states plainly which parts of the design would break if the
      timestep were variable.
- [ ] Nothing in `apps/`, `slate-doc`, or any migration is touched.

---

## S3 — The collaboration session protocol {#s3}

**Produces:** `docs/collab-protocol.md`, plus a prototype pure crate
`crates/atlas-collab` holding the wire types and the merge rules — **no
transport, no app wiring**.
**Depends on:** G0, and T1.1c's property-scoped commands landing (the wire
format *is* `SetProp`). **Size:** L.
**Why:** decision D11 — live multi-user editing is a core feature, participation
is hybrid, and the transport is a self-hosted relay that can later be deployed
as a service. This is the largest single design commitment in the plan and it
should be reviewed on paper before a line of it is built.

### Do — the document

**Session model.** Who starts a session, how peers find it (relay address plus a
session code), how a peer joins mid-session (snapshot plus delta log), what
happens when the relay restarts, and who owns the file on disk. The workbook
lease from T0.4 is the natural anchor for "who saves" — reuse it rather than
inventing a second authority.

**Ordering and conflict.** Server-authoritative: the relay assigns a monotonic
sequence number, and last-writer-wins applies per property. Spell out exactly
why this removes vector clocks and wall-clock timestamps (audit E10), and what
happens to a command the relay rejects (surfaced, never dropped — Amendment B /
VI.2).

**The wire.** Deltas only, never whole scenes. Every message type, with its
schema:

| Message | Direction | Payload |
|---|---|---|
| `join` / `welcome` | both | participant identity, colour, snapshot generation |
| `snapshot` | relay → peer | the scene at a sequence number |
| `commit` | peer → relay → peers | a command group: `Vec<SceneCmd>`, author, sequence |
| `reject` | relay → peer | the command and why |
| `presence` | peer → relay → peers | cursor, viewport, selection — **never journaled** (Amendment B / VIII.5) |
| `asset` | peer ↔ host | tier request and bytes (see below) |

**Asset delivery — the part neither audit caught.** Remote peers cannot resolve
the host's file paths, so a remote participant sees an empty board unless the
session streams thumbnails and previews. Specify: request by `cache_key` (the
keys already exist and are already project-relative), serve from the host's
existing cache tiers, cache locally on receipt, and fall back to an honest
`Unknown` card while in flight. Include a bandwidth estimate for a
forty-image board joining cold.

**Identity and trust.** No accounts. A participant is a name and a colour; a
session is protected by a code; transport is TLS. State explicitly what this
does *not* protect against, so nobody assumes more than is there.

**Undo.** Author-filtered: undo means *my* last action (audit E3). Specify what
happens when my last action targeted a node someone else has since deleted.

**Membership changes.** Frame membership is geometric (decision D3), so another
participant's move can change your deck. Announce it with the author's name
(Amendment E / V.2). Specify the notice, and when it is suppressed to avoid
noise.

**Agent velocity.** Agents commit through the staging layer, so their proposals
are per-peer until accepted (Amendment C / VII.6). Confirm this holds in a live
session and say what an accepted agent proposal looks like to the other twenty
participants.

**Scale.** Design for twenty simultaneous editors. Give the honest bottleneck
analysis: property-level deltas at N=20 are trivial bandwidth; the real
constraints are asset delivery and the legibility of twenty cursors. Propose
what happens above twenty rather than pretending it will not occur.

**The relay.** A separate small binary (`slate-relay`) with no document
knowledge beyond ordering and fan-out — it must be deployable by a firm on a VPS
today and by this project as a hosted service later, from the same source
(Amendment F / I.4). Specify its lifecycle and its failure modes.

**Relay storage — decision D18: the relay persists the delta log.** A late
joiner or a reconnecting peer catches up from the relay without the host being
present. Specify retention (the log is deleted when the session ends, plus a
configurable reconnection grace window), what is stored where, and an operator
statement for the relay's README. Two invariants: the relay never becomes the
store of record — the host's `.slate` remains the document (Article IX) — and
persisted assets follow the same retention as the log, since a cached
forty-image board is the bulk of what sits on that disk.

### Do — the prototype

`crates/atlas-collab`, pure, depending only on `slate-doc` and `serde`: the
message enum, sequence assignment, the LWW merge rule, and an in-memory
simulated relay used for tests. No sockets in this spike.

Tests must include `twenty_peers_converge_under_random_interleaving` (a
deterministic pseudo-random schedule, not an RNG dependency) and
`presence_never_enters_the_journal`.

### Accept

- [ ] Document answers every heading with no "TBD", including the asset
      bandwidth estimate and the above-twenty behaviour.
- [ ] Prototype converges under the interleaving test.
- [ ] The document names, explicitly, the three things that must land first
      (T1.1a/b/c) and what would have to change if any of them were skipped.
- [ ] A one-page "what this is not" section: not accounts, not a cloud, not
      offline-divergent editing, not two hundred participants.
- [ ] No transport, no app wiring, no `slate-relay` implementation in this
      spike.
