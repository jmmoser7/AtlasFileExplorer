# Decisions on Audit №2 — flexibility stress test

**Answers to** the flexibility audit (`Architecture Audit №2 — Flexibility
Stress Test`), recorded as binding decisions D7–D16. Continues the numbering in
`docs/audit/2026-07-25-decisions.md`.

Several of these **overturn** audit №2's verdicts. Where they do, the reason is
recorded, because a future reader will otherwise find the audit and the code
disagreeing and assume the code drifted.

---

## The organising principle (supersedes audit №2 §8.1)

Audit №2 proposed generalising a portal to "a deterministic function of declared
inputs". **Rejected** — that over-constrains. Determinism is a property that
Lens-class views need because Article IV.2 forbids hallucinated analysis; it is
not a property a live web page or a linked spreadsheet can or should have.

The generalisation that actually covers the cases:

> **Journaled state is authored intent. Derived state is anything reproducible
> from authored intent plus time. Every portal declares two things: which
> journal owns mutations inside it, and what it serialises to on export.**

One sentence covers Lens contents, nested boards, live web surfaces, video
playheads, and orbital positions — and it keeps Article VI honest without
demanding 60 journal commits a second from a simulation.

---

## D7 — Portals have classes

Three, each answering the two obligations:

| Class | Mutation authority | Exports as | Examples |
|---|---|---|---|
| **Generated** | none — contents regenerate deterministically | the regenerated contents | Lens, Grid, Venn |
| **Document** | the child document's own journal | the child, rendered | nested `.slate`, linked Rhino, linked workbook |
| **Host** | the foreign application, entirely outside Slate | a poster plus a pointer, **and it says so** | web page, external app surface |

Consequences:

- A nested workbook is a **Document portal**: the parent journals the frame, the
  child journals its own contents. In-place editing inside the parent is
  permitted provided the mutation is committed to the *child's* journal. This
  resolves audit №1's R4 undo question without banning the interaction.
- **Edits inside a Host portal are not Slate mutations** and never enter the
  journal. This is the correction to audit №2, which treated that as
  disqualifying. It is not: a host portal is a surface, and the foreign
  application owns what happens on it.
- Cycle detection and lazy resolution remain mandatory for Document portals
  (audit №2's R4 hazards stand).

## D8 — Derived state and the canvas clock

The board gains a clock. Node state may be **derived** from journaled intent
plus elapsed time, and derived state is not a mutation:

- **Journaled:** initial conditions, parameters, forces, impulses, trim points,
  the frame itself.
- **Derived, never journaled:** simulated positions, playhead position, portal
  contents, trails before baking.
- **Baking** is the bridge: an explicit command converts derived state into
  journaled content (freeze these positions; turn this trail into a `Path`).

Determinism is required here even though it is not required of Host portals —
fixed timestep, reproducible integration — because it is what makes a simulation
exportable honestly and collaborative for free (peers replay from the same
journal; only impulses cross the wire).

## D9 — Write-back to sources is explicit, and human-only

Overturns audit №2's false-affordance row "destructive source editing — never".

- **Default is unchanged:** filters, crops, and adjustments live in the workbook
  and never touch the original. This stays the default forever.
- **Write-back is a named per-action command** that names the file it will
  overwrite in its confirmation and records provenance in the journal.
- **Agents may not invoke write-back**, at all, until a named use exists. A
  model misreading an instruction and overwriting a client original is the one
  failure in this whole plan that is not undoable.
- The safe middle path — write a derived copy beside the original — is the
  default offered by the command; true overwrite is the opt-in inside it.

## D10 — The package is a fork, not a sealed snapshot

Corrects audit №1 §4.4 and my WI-5 spec, which built a "sealed" state machine
with re-link machinery. What is wanted is InDesign's **Package**:

- Copy every linked file into the package, rewrite the workbook's locators to
  package-relative, done.
- **A permanent fork.** No re-merge, no diffing against origins, no divergence
  tracking, no sealed mode in the UI. The packaged workbook is an ordinary
  workbook that happens to own its assets.
- A `manifest.json` records where each asset came from — pure provenance, so a
  human can find the original by hand. That satisfies the honesty requirement
  without building a sync system.

This roughly halves WI-5.

## D11 — Live collaboration is a core feature

Overturns audit №1's F5 sequencing and my own C0/C1/C2 tiering. The target is
the Miro/Figma experience: **multiple people editing one board simultaneously
and seeing each other's contributions live.** Not a review mode, not an edge
case.

Settled by the follow-up questions:

- **Participation is hybrid** — some in the room, some remote. LAN-only host
  election is therefore dead.
- **Transport is a self-hosted `slate-relay` binary now, with the same binary
  deployable as a hosted service later.** No vendor cloud, no accounts, no
  subscription; a firm runs one small process on any box or VPS.
- **Server-authoritative ordering** (Figma's model): the relay defines operation
  order, which removes vector clocks, tombstone collection, and most of what
  makes CRDTs expensive. Last-writer-wins per property.

Three consequences neither audit caught:

1. **Remote peers cannot resolve your file paths.** The session must stream
   thumbnail and preview tiers from the host. This reuses the existing cache
   keys and thumbnail pool, but it is a real subsystem and it is on the critical
   path for hybrid.
2. The convergent journal (WI-2) is not merely a precondition — **the
   property-scoped command *is* the wire format.** Nothing about the wire needs
   inventing once T1.1c lands.
3. **Running a relay is a change to "Slate is a local desktop application"** and
   is ratified explicitly (Amendment F / I.4) rather than arriving as an
   implementation detail.

At twenty simultaneous editors the merge mathematics are fine. The binding
constraints are asset delivery to remote peers and the legibility of twenty
cursors — both interface problems, not distributed-systems problems.

## D12 — Nodes can be interactive, with forces between them

Audit №2 read the orbital-gravity request as an embedded simulation portal. That
was wrong. The request is that **ordinary scene nodes become interactive
entities that affect each other at a distance**, respond to being flung, and
emit artifacts onto the canvas.

Shape:

- Nodes gain an optional **dynamics component**: mass, velocity, damping,
  pinned.
- **Forces are typed edges** — `Gravity`, `Spring`, `Repulsion`, each with a
  strength. This is the first use for typed edges beyond connectors, and it
  materially strengthens the case for the edge extraction (WI-4).
- **Impulses are journaled commands** carrying an epoch ("body 3 gets this
  velocity at t"); positions between impulses are derived (D8).
- Fixed timestep, deterministic integration, bounded body count with a visible
  readout, paused when off-screen (Article II).
- **Trails are ephemeral generated paths** until an explicit bake turns one into
  a journaled `Path` node.

The same clock serves video playheads and parameter animation, so this is one
subsystem rather than three.

## D13 — Video viewports, Frame.io style

Wanted, and scoped as a **node interaction**, not an NLE (audit №2's D3 verdict
of "be an arranger, not an editor" stands for the editing question; the
scrubbing interaction does not need an NLE).

- **Hover-scrub** across the node sweeps the timeline. Implemented with a
  **filmstrip**: extract ~100 keyframes at import into the existing thumbnail
  cache tier, so sweeping is a lookup rather than a seek.
- **Click plays** with real decode.
- Trim in/out already exists in `VideoOpts`; the artifact writer already emits
  `#t=` media fragments.

The filmstrip cache is the only genuinely new piece, and it can land early
because it reuses the thumbnail pool.

## D14 — The extension model: declarative assets plus MCP servers

Audit №2 called the plugin marketplace Class 5 terminal. Half right — **native
in-process binary plugins stay prohibited** (process corruption, ABI churn,
platform lock). But the community-sharing goal is legitimate and mostly already
paid for:

| Tier | What ships | Verdict |
|---|---|---|
| **Declarative assets** — themes, keymaps, skills, brushes, palettes, board templates, portal definitions | a package format and an install path | **v1.** Covers most of what people trade |
| **MCP servers as plugins** | an out-of-process, language-agnostic, OS-sandboxed extension exposing the same command surface, with a permission model | **v1.** This is already being built for agents (T2.4); a third-party "tool" is an MCP server plus assets |
| **In-process WASM tools** | a real third-party canvas tool with its own interaction | **Deferred.** This is Article VII.4's named amendment, and its host API is the command registry plus staging plus MCP — so it cannot be designed before those exist |
| **Native binaries** | — | **Never** |

The reframe worth writing down publicly: **MCP is already a plugin API.**

## D15 — Host portals: web first

Web views are the first and only host-portal tier to be designed. Rendered
offscreen to a texture, they are canvas-native — they rotate, zoom, hit-test,
and export as a poster plus a link.

**Foreign OS application windows (a live Excel or Rhino instance) are not
planned.** The technique — reparenting another process's window over the canvas
— costs rotation, smooth zoom, correct occlusion, export, and visibility to
remote collaborators. Four losses, all of which break promises made elsewhere in
this plan. The agent-driven tool-to-tool goal that motivated the request is
served better by an MCP server for that application plus a Document portal on
its file.

## D16 — Generative image nodes are authored content

Permitted. Article IV.2 ("graphs are extracted, never hallucinated") governs
**analysis views** — a Lens portal may never invent a node. A generated image a
human placed is authored content, no different from a photograph. The
distinction is written down now so that nobody cites IV.2 to block it, and more
importantly so nobody cites this decision to justify a hallucinating Lens.

Generated images carry their provenance (prompt, model, seed) as item metadata,
because provenance is what keeps them honest.

## D17 — Nested workbooks: open in a tab first, edit in place later

A Document portal is double-clickable. **Version one opens the child in its own
tab**, where journal ownership is obvious and nothing new has to be invented.
**In-place editing inside the parent frame comes second**, once the portal
contract has proven itself in use, and when it arrives it still commits to the
child's journal — the parent never gains authority over the child's content.

This sequencing matters for S1: the portal contract must describe both, and must
not make a choice in version one that forecloses the second. Concretely, a
Document portal's input coordinate mapping has to be defined even while nothing
consumes it.

## D18 — The relay persists the session log

The relay stores the ordered delta log for a session so that a late joiner or a
reconnecting peer can catch up without the host being present. This is the
convenient answer and it is the right one for a twenty-person meeting where
people arrive late and laptops sleep.

It has a consequence that must be stated rather than discovered: **session
content lives on the relay host for the life of the session.** For a firm
running its own relay on its own hardware that is the same trust boundary as the
file share. S3 must therefore specify:

- retention — a session's log is deleted when the session ends, plus a
  configurable grace window for reconnection;
- an explicit operator statement in the relay's README about what is stored,
  where, and for how long;
- that the relay stores deltas and assets, and never becomes the store of record
  (Article IX) — the host's `.slate` file remains the document.

---

## Governance artifacts adopted from audit №2

Three of audit №2's recommendations are adopted verbatim as work:

1. **The open/closed enum rule.** *Close an enum where an agent must enumerate
   it; open an enum where a human must express themselves through it.* Closed:
   `NodeKind`, `EdgeRole`, `Facet`, `CommandId`, `SourceKind`. Open (or to be
   opened): `FontChoice`, `Dash`, `EdgeRouting`, theme names, corner and cap
   profiles.
2. **`EXTENDING.md`** — the six flexibility classes with a worked example each,
   the enum rule, the capability-crate contract, and published fork seams for
   the requests that should be forks. This is the recruitment artifact.
3. **The false-affordance register** — features refused because the
   architecture would make them lies (permission-hidden regions, legal
   redaction, DRM, hallucinated analysis graphs). **With D9's correction:** the
   "destructive source editing" row is removed and replaced by "silent or
   agent-initiated write-back".

---

## Newly dated work

Audit №2 is right that exactly one item has a deadline set by someone else's
enthusiasm rather than by us:

- **A spatial index over node AABBs.** Every hit-test, marquee, and paint cull
  is a linear scan today. It costs a day now and a week plus a regression hunt
  after the painter and hit-tester grow twenty call sites that assume linear
  iteration. Scheduled into Wave 0 (T0.8).

And one item that is nearly free and disproportionately valuable the moment the
repository is public:

- **Palette into `ui-tokens.toml`.** Fourteen semantic slots already exist and
  are already correct; they are simply hardcoded in Rust while the metrics
  beside them are data. Moving them makes every future theme a text file and
  turns cross-app visual agreement from a discipline into a structural property.
  Scheduled into Wave 0 (T0.7).

---

## Revised scope summary

**Added to the plan:** portal taxonomy, canvas clock and dynamics, live
collaboration as a core capability with a self-hosted relay, session asset
streaming, web-view host portals, filmstrip video scrubbing, the extension
package format, `EXTENDING.md`, the false-affordance register, the spatial
index, palette tokens, wire-routing consolidation.

**Removed or refused:** foreign OS window embedding (D15), native binary plugins
(D14), permission-hidden regions and legal redaction (false-affordance
register), sealed-bundle re-link machinery (D10), the blanket
portals-are-deterministic generalisation (D7).

---

## Still open

Two questions the spikes must answer and the user has not yet decided. Neither
blocks Wave 0.

1. **Does undoing an impulse rewind the simulation?** Rewinding to the impulse's
   epoch is coherent and means undo can move every body on screen; removing the
   impulse from that point forward without disturbing current positions is
   cheaper and less honest. S2 presents both with a recommendation.
2. **Write-back granularity.** Currently specified as per-action, human-only,
   with a confirmation naming the file, and no setting anywhere. The alternative
   is a per-workbook "this workbook may write back" permission. Decide before
   anything implements write-back; nothing in Waves 0–4 does.
