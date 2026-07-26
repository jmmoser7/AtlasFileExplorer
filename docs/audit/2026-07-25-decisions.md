# Decisions on Audit №1 — 2026-07-25

**Answers to** `audit-2026-07-25-protocols-collaboration-agents-api.md` §11 (open
questions), recorded as binding project decisions.
**Audit commit:** `c808966` (`origin/main`).
**Status:** decisions D1–D6 are settled. The constitutional amendments they
imply are drafted in `docs/audit/amendments/2026-07-25-amendments.md` and are
**not yet ratified** — Article XI.2 reserves that to the user.

This file is the input to `docs/workplan/README.md`. Where the audit and this
file disagree, this file wins; where this file and `CONSTITUTION.md` disagree,
the constitution wins.

---

## D1 — Sharing is daily, and meetings are large

> **Partly superseded by D11** (`2026-07-25-decisions-flexibility.md`). D1's
> reading — that a large design review is a presentation with markup, so live
> editing could be tiered into C0/C1/C2 — was corrected by the user: **live
> multi-user editing is a core feature**, participation is hybrid, and the
> transport is a self-hosted relay. Everything below about the *urgency* of the
> lease guard, relative locators, and the convergent journal stands and is in
> fact strengthened. The C0/C1/C2 tiering and the LAN assumption do not.

> *"Shared daily, likely prime use case in future. Will commonly have multiple
> contributors on board at same time. Think design meeting with 12 to 20
> participants."*

**Consequences.**

1. **Product B (shared file) is promoted** from "weeks, later" to the primary
   near-term collaboration target (Wave 3).
2. **Product C (live co-presence) is no longer deferred indefinitely** — but it
   is split, because 12–20 *participants* is not 12–20 *editors*:

   | Tier | Who mutates | What it costs | When |
   |---|---|---|---|
   | **C0 — presence & follow** | nobody but the presenter; everyone else is a live read-only viewer with cursors and follow-the-presenter | small: presence is ephemeral (Art. VIII.5), viewers never commit, no conflict model needed | Wave 4 |
   | **C1 — few editors, many viewers** | 2–6 editors + N viewers | medium: host-authoritative LWW per property, delta wire format | Wave 4–5, after C0 is used |
   | **C2 — all participants editable** | up to 20 | large, and mostly unnecessary | not planned; needs a named use |

   **This is the single highest-leverage reframing of the audit.** A 12–20
   person design review is a *presentation with markup*, not twenty
   simultaneous authors. C0 delivers ~80% of the meeting value at ~10% of C1's
   cost, and it is the honest reading of Article III.
3. **WI-1 (open-file guard) moves to today.** Daily sharing without a lease is
   guaranteed data loss, and the audit's E2 becomes a weekly event rather than a
   theoretical one.
4. **WI-3 (`root_relative` locators) becomes near-term critical.** Twelve
   participants mount the firm share twelve different ways; without it every
   shared workbook renders differently per machine (audit E1/F6).
5. **The journal fix (WI-2) is on the critical path**, not a hedge. C0 needs
   stable node identity for presence targeting and membership diffs; C1 needs
   property-scoped commands or it cannot converge at all.

**Assumptions to confirm** (defaults chosen; say so if wrong):

- *Same LAN.* C0 discovery defaults to LAN (mDNS-style broadcast) with a manual
  "connect to host address" fallback. Remote/hybrid participants over the
  internet would need a relay — out of scope until asked.
- *One presenter at a time.* Follow-the-presenter is a single-token role that
  can be handed over, not a democracy.
- *Participants have the material.* Viewers resolve images from the shared
  source, not from the host's bytes. Where they cannot, the board shows an
  honest `Unknown` card (Amendment A.4), and the fix is a `.slatepack` (WI-5).

---

## D2 — Autodesk / Tier-2 platform adapters are out

> *"This is not high priority. T2 can be removed from consideration."*

**Consequences.**

- **Cut permanently from planning:** ACC/APS adapter (audit decision 15),
  Google Docs renditions (18), iCloud native (19). The sync-client shortcut
  (audit §3.4) is the answer, and it is already working.
- **Deferred with no scheduled wave:** the `Source` *trait* plus a Tier-1
  `opendal` backend (audit decision 11). No named weekly use survives D2, so
  Article III forbids building it now.
- **Kept:** the *identity model* the trait would have needed —
  `SourceUri { kind, authority, locator, root_relative }` and `ContentId`
  (WI-3). It is required by D1 regardless of cloud sources, and it is the
  cheapest it will ever be (audit F2).
- **Amendment A.3** (declared version discipline) legislates a subject that no
  longer exists in scope. Recommendation: **do not ratify A.3 now.** See the
  amendments file.

**Net effect:** roughly six weeks of first-build work plus a permanent quarterly
maintenance obligation removed from the plan.

---

## D3 — Frame membership stays geometric

> *"Yes, keep them geometric."*

**Consequences.**

1. `EdgeRole::Membership` is **not implemented**. When WI-4 lands the `Edge`
   record, it ships with `Connector` only, and `Context` / `Provenance` follow
   when agents need them. Unused roles stay documented, not coded (Art. III).
2. Geometric membership is a **pure function of converged geometry**, so it
   converges for free under C0/C1 — provided it is deterministic. It is not
   quite deterministic today: `Scene::members_of` returns every non-frame node
   whose centre lies in the frame rect, so a node inside two overlapping frames
   belongs to **both** and appears on two slides. That is a latent bug in
   single-player export and an outright divergence risk in multiplayer.
   **Fix scheduled as Wave 0 task T0.6:** exactly one frame owns a node — the
   topmost containing frame, using the same rule as `Scene::frame_at`.
3. Audit edge case **E6 is accepted, not engineered away.** Moving a node can
   change a deck. Mitigation is disclosure, not prevention: in a shared session,
   a membership change caused by another participant raises a visible,
   attributed notice ("Jane's move added *plan-03* to slide 4"). Scheduled with
   Product B (Wave 3) and enforced in C1.
4. This is written into Amendment E (Article V.2) so the rule survives the next
   person who thinks explicit parent pointers would be tidier.

---

## D4 — The first agent task is the collage

> *"First task will be to automatically scale and align a collection of images
> on the page into a collage."*

This is a real Article III use, and it settles the Mode 1 / Mode 2 question:
**Mode 1 (sidebar, selection as context) is first**, because the task's context
is *the current selection*, which the existing beacon already carries. No agent
node, no typed edges, no graph work required.

**Consequences.**

1. New pure crate **`crates/collage`** — a deterministic layout solver, no
   renderer, no egui, no document types beyond plain rects and aspect ratios:
   justified rows (the Flickr/Google-Photos linear-partition algorithm),
   uniform aspect-fit grid, and masonry columns. Fully unit-testable on Linux
   CI. (Wave 0 — it depends on nothing and can start immediately.)
2. New registered commands `board.arrange.collage` (+ variants) that convert the
   solver's output into journaled `patch_nodes` calls (Wave 2).
3. **Free win discovered during the survey:** `align_board_selection` and
   `distribute_board_selection` already exist in `apps/slate/src/app/board.rs`
   (lines ~818–892) and have **zero call sites** — implemented, tested by hand,
   never wired to a command or a dock button. `commands.rs` even documents
   `board.align` with no dispatch arm. Wiring them is an afternoon and it ships
   alongside the collage.
4. **This task is the proof harness for the whole agent surface.** It exercises,
   in one vertical slice: the command registry (Art. VII.1), journaled
   attributed mutation (Art. VI), property-scoped commands (WI-2), the staging
   layer (Amendment C / VII.7 — the agent proposes a collage, the human accepts
   or rejects as one unit), and MCP invocation (WI-6). Everything else in the
   agent plan is validated by this one command working end to end.
5. **The honest note:** the layout itself is arithmetic, not intelligence. The
   agent's contribution is choosing the arrangement, the parameters, and the
   grouping from a plain-language request. Build the deterministic command
   first; it is useful on its own from the toolbar, and it is the only version
   that can be tested.

---

## D5 — Windows stays the reference platform

> *"Yes, it will be Windows first."*

**Consequences.**

- No Mac/Linux port work is planned. The Win32 shell thumbnail pipeline
  (`atlas-core/src/thumbs.rs`) stays as-is and stays off-limits to refactors
  (per `AGENTS.md`).
- **But** the non-Windows stubs must keep compiling, because the swarm's cloud
  agents run on Linux VMs and CI will run there. `cargo check/test --workspace`
  green on Linux is a hard gate — see Amendment E (Article I.3) and the CI task
  T0.1.
- New pure crates (`collage`, and later `atlas-collab`) must be
  platform-independent. That is not portability work; it is the Article I
  substrate hedge already in force.

---

## D6 — The audit tracks a metrics baseline

> *"Yes please."*

**Consequences.** New `xtask` crate providing `cargo xtask metrics`, writing
`docs/metrics/<date>.json` plus a human-readable table, recording:

| Metric | Source |
|---|---|
| LOC per crate / app | file walk, `.rs` only |
| Pure-vs-renderer ratio | crates whose dependency closure excludes `egui`/`eframe` |
| Command count | `Registry::new(SPECS).iter().count()` per app |
| Node kind count / edge role count | `slate-doc` enum variants |
| `format_version` | `SlateDoc::CURRENT` |
| Open constitutional deviations | count of open rows in `docs/audit/deviations.md` |
| Test count | `#[test]` occurrences per crate |
| Longest file, `unsafe` block count, direct dependency count | drift canaries |

Baseline is recorded in Wave 0 (task T0.2) against the state the audit measured,
so audit №2 can diff numbers rather than impressions.

---

## Revised decision matrix

Re-scored from audit §9 under D1–D6. Same scale (payoff / cost-cheapness /
reversibility / debt / constitutional fit, 1–5).

| Rank | Decision | Was | Now | Wave | Why it moved |
|---|---|:--:|:--:|:--:|---|
| 1 | Open-file lease + read-only guard | 21 | **23** | 0 | D1 makes this a weekly loss risk, not a hypothetical |
| 2 | Convergent journal (`ZOrder`, id-addressed, property-scoped) | 19 | **21** | 1 | D1 puts C0/C1 on the map; this gates them |
| 3 | Source identity (`SourceUri` + `root_relative`, tri-state health) | 19 | **21** | 2 | D1: twelve machines, twelve mount points |
| 4 | **Collage + align/distribute commands** | — | **20** | 0/2 | D4: the named weekly use; also the agent-surface proof harness |
| 5 | `.slatepack` sealed bundle | 20 | **20** | 3 | unchanged — highest visible payoff |
| 6 | Product B: shared-file safety, authorship, reload | — | **19** | 3 | D1 promotion |
| 7 | `atlas-ai` split + `Provider` trait | 19 | **19** | 1 | unchanged; precedes all agent work |
| 8 | Staging layer generalised from the Lens overlay | 18 | **18** | 2 | required before the collage agent can write |
| 9 | `atlas-mcp` adapter crate | 18 | **18** | 2 | MCP spec finalises 2026-07-28 |
| 10 | Extract `Edge` from `NodeKind::Connector` | 18 | **17** | 3 | still right, slightly less urgent: D3 removes `Membership` from scope |
| 11 | Metrics + CI | — | **17** | 0 | D6; and a swarm without CI breaks `main` |
| 12 | **Live collaboration** (relay, presence, assets) | 9 (defer) | **20** | 4 | **D11**: a core feature, not a tier. Supersedes the C0/C1 rows this table originally carried |
| 13 | Agent Mode 1 (sidebar) on the beacon | 18 | **16** | 2 | folded into the collage slice |
| 15 | `ControlSurface` + `PanelSpec` registry | 15 | **14** | 5 | unchanged, still after the graph work |
| 16 | Agent Mode 2 (agent nodes) | 15 | **13** | 5 | D4 says Mode 1 first; Mode 2 needs typed edges |
| 17 | Edge roles beyond `Connector` + legality schema | 17 | **13** | 5 | D3 removes `Membership`; remaining roles await Mode 2 |
| — | `Source` trait + Tier-1 backend | 17 | **deferred** | — | D2: no named use survives |
| — | ACC adapter · Google Docs · iCloud · hyperedges · C2 | 8–12 | **cut** | — | D2, D1 (C2), audit (hyperedges) |

---

## What is now out of scope (say so out loud)

Removing work is the most valuable output of an audit. These are not "later" —
they are **not planned**, and re-adding any of them requires a named weekly use:

- Autodesk ACC / APS adapter, and every other Tier-2 platform adapter.
- Google Docs as a source (needs a rendition facet that will not be written).
- iCloud native support.
- A plugin system for sources.
- Hyperedges.
- C2: twenty simultaneous editors.
- Mac / Linux / iPad builds (D5), other than keeping CI green on Linux.
- Ratifying Amendment D (control surfaces) before anything implements it.

*(The "C2 — twenty simultaneous editors" exclusion listed here originally is
withdrawn by D11. Twenty simultaneous editors is the design target.)*

---

## Ratification gate

Wave 0 is deliberately constitution-neutral and may start immediately. Waves 1+
are **blocked** until the user edits `CONSTITUTION.md` per Article XI.2. The
exact paste-ready text is in
`docs/audit/amendments/2026-07-25-amendments.md`.
