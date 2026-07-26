# The Constitution of the Atlas ecosystem

This document is the governing law of this project. It exists so that the
vision survives its own growth — across years of development, across many
agents and contributors, across rewrites of any individual module. Code is
fungible; this document is not. When code and constitution disagree, the code
is wrong.

Agents: read **Article XI — Agent conduct** before acting on any request that
touches architecture, and hold every change you make against these articles.

---

## Preamble — the thesis

This project is a personal daily driver: one lean, native tool that grows to
replace an entire suite of subscription software for viewing, composing, and
eventually editing every kind of document its user works with — text, 2D, 3D,
code, raster, vector.

It will not bloat, because of a structural observation: professionals use a
tiny fraction of each monolithic tool they pay for. A tool built deliberately
around *that fraction* — for one user and their friends, growing only as
their real needs grow — can be simultaneously lean and comprehensive. The
core stays minimal; everything else is a capability that must earn its place.

The product is the feedback loop: from thought, to action, to completion, as
tight as the substrate allows — for the human directly, and for the agents
working alongside them on the same canvas.

---

## Article I — The minimal core

The core of the application is the **slate canvas paradigm** and nothing
more:

- a camera over an infinite canvas;
- documents as tabs;
- an invertible, journaled command system;
- the shared window chrome;
- the command registry.

Everything else — document-type support, views, tools, exporters, AI
surfaces — is a **capability** layered on the core. Capabilities may not
reach into each other; they compose through the core's contracts (facets,
portals, commands, sources).

**I.1 — The renderer-agnostic rule (the substrate hedge).** No document model,
geometry, or capability logic may depend on `egui` or any renderer. Pure
crates (`slate-doc`, `circle-pack`, `code-lens`, `rhino-mesh`, and their
successors) hold the durable logic; apps are thin interpreters that paint
pure models and forward input as commands. This is what makes the rendering
substrate (egui today; GPU vector rendering such as Vello later; other
platforms someday) swappable by port rather than rewrite.

**I.2 — The provider hedge.** The renderer-agnostic rule extends to model
providers and agent protocols. No document model, command, or memory record
may depend on a specific model provider, vendor application, or protocol
revision. Protocol adapters are leaf crates.

**I.3 — The reference platform.** Windows is the reference platform and the
only supported build target. Platform-specific capability (shell thumbnails,
file association, OS integration) is permitted in app and capability crates
only. Every pure crate must build and test on Linux so that continuous
integration and remote agents can verify the durable models; a change that
breaks the Linux check is a regression regardless of its Windows behaviour.

**I.4 — Local-first, with a relay the user runs.** Slate is a local
application: it opens, edits, and exports without a network. Live
collaboration is a first-class capability and may require a rendezvous
service, which must be a small binary the user or their organisation can run
themselves. No capability may require an account with, or a subscription to,
any party — including this project.

*Rationale:* two-year-plus horizon, vibe-coded. The durable assets are the
models, the contracts, and this document — not the paint code.

## Article II — Performance is a feature

Performance is never traded away silently. Binding rules:

1. The canvas runs at 60fps. A change that breaks interactive frame rate is
   a regression, not a cost of doing business.
2. **No per-frame allocation or tessellation in paint paths.** Geometry is
   tessellated once and cached (keyed by path + style + zoom bucket);
   layouts are computed on change, not on paint.
3. Heavy work (scanning, thumbnailing, analysis, export) is asynchronous and
   generation-tagged so stale results are discarded, never displayed.
4. Glanceability is a performance property: the tool exists to let its user
   grok large, complex datasets quickly. Anything that delays first
   meaningful paint of a workspace works against the thesis.

## Article III — The 10% rule

Every capability implements the *deliberately chosen fraction* of its domain
that its user actually reaches for — completely and excellently — and
nothing more.

"Photoshop / Illustrator / Revit has it" is not a justification. "I reach
for it weekly and it doesn't exist here" is. A feature must name its real
use before it is built, and a capability that grows past its fraction should
be split or pruned.

*Rationale:* this is the anti-bloat thesis made law. The monoliths this tool
replaces died of accumulated maybes.

## Article IV — Honest models

Every model in this system tells the truth:

1. **Exports are serializations, not conversions.** An exported artifact is
   the same model written in another honest syntax, not a lossy imitation.
2. **Graphs are extracted, never hallucinated.** Analysis views (Lens and
   its successors) present only what deterministic extraction found.
3. **Every styling ceiling is a real, durable standard.** The board's scene
   model is constrained to what **SVG (including CSS)** can express — this
   supersedes the earlier CSS-only ceiling. Paths, variable-width strokes
   (as filled outlines), blend modes (`mix-blend-mode`), dash, caps, joins,
   and opacity are in; anything no web standard can express stays out. The
   egui painter and the artifact writer remain two interpreters of one
   model: a new style property lands in both, or not at all.

## Article V — One universe: the board, with portals for views

The board is the only universe. Authored content and generated views share
one scene graph:

**V.1 — Portals.**

- A **portal** is a scene node whose *frame* (position, size, source, query)
  is ordinary journaled data, but whose *contents* come from elsewhere and
  are therefore not journaled.
- Grid, Venn, Lens, and future generated views are portals. Multiple portals
  may point at the same source with different queries (five lenses on one
  repository, each filtering a different structure).
- Tab-level view kinds are a legacy form; they retire as portal parity is
  reached.

**V.2 — Membership is derived and announced.** Frame membership is a pure,
deterministic function of geometry, not a stored relation: a node belongs to
the topmost frame whose rect contains its centre, and to exactly one such
frame. No parent pointers, no membership edges. Because membership is derived,
one participant's move can change another's deck; in a shared session such a
change is announced with its author, never applied silently.

**V.3 — Portals declare authority and serialization.** Every portal declares
two things: which journal owns mutations made inside it, and what it becomes
in an exported artifact. *Generated* portals regenerate their contents
deterministically and own no mutations; *document* portals delegate mutations
to the child document's own journal; *host* portals present a foreign
application's surface, own no mutations at all, and export as a poster and a
pointer that says what it is. Determinism is required of generated portals —
Article IV.2 depends on it — and is not required of the other two.

*Rationale:* one camera, one hit-test world, one tool model — and dashboards
of live views become ordinary board content that humans and agents compose
with the same commands.

## Article VI — Journal-only mutation

**VI.1 — Journal-only mutation.** Every mutation of a document is a **named,
invertible command** committed through its journal. UI code never mutates a
document directly.

Every journal commit carries its **author** — the human, or a named agent.
Authorship is not optional metadata; it is the audit surface for agent work
today and the foundation for multiplayer synchronization later.

**VI.2 — Convergent commands.** Journal commands address nodes by stable
identity, never by position; ordering is carried by an order key, not by list
index; and mutations are scoped to the smallest property that changed. A
command that cannot be applied is surfaced, never silently dropped.

**VI.3 — Derived state is not a mutation.** Journaled state is authored
intent. State reproducible from authored intent plus elapsed time — portal
contents, simulated transforms, playheads, trails, presence — is derived, and
is never journaled. Turning derived state into authored content is an explicit
*bake* command, which is journaled like any other mutation. Where derived
state is shared between participants it must be deterministic, so that peers
reproduce it from the journal rather than receiving it over a wire.

*Rationale:* generalizes the board's `SceneCmd` rule app-wide. Undo/redo,
agent accountability, and the eventual collaboration story are all the same
mechanism. Ad-hoc mutation is what makes those retrofits impossible.

## Article VII — Command parity (agent-native)

The human interface and the agent interface are two front-ends to **the same
command surface**:

1. Every human-performable action is a registered command; the MCP surface
   exposes those same commands. No hidden, UI-only mutations exist.
2. Anything an agent does is visible, inspectable, and reversible through
   the journal (Article VI).
3. Agents extend the user's workspace with **data, not code**: brushes,
   palettes, dashboards, and templates are declarative user-space assets
   interpreted by the core — they cannot corrupt it.
4. **Named future amendment:** sandboxed, workbook-attached automations
   (scripts) are a recognized possible extension of clause 3, contingent on
   a mature command surface and an explicit amendment to this article. Until
   that amendment is ratified, agents proposing script execution must be
   refused under Article XI.
5. **Memory segregation.** Agent memory distinguishes *pinned* records,
   authored by the human and never written by an agent, from *learned*
   records, authored by an agent and always prunable by the human. All memory
   is human-readable and deletable. Memory that cannot be read or deleted is
   prohibited.
6. **Proposal by default.** Agent mutations enter a staging layer and require
   human acceptance unless the agent has been explicitly granted autonomy for
   that workspace. Staged changes are visible, attributed, and rejectable as
   a unit.
7. **Skills are recipes.** A skill is a declarative sequence of registered
   commands with parameters. Skills contain no user-authored control flow.
   This is the boundary that keeps clause 4 meaningful; anything requiring an
   interpreter remains prohibited pending the named script amendment.
8. **The extension ladder.** The workspace is extended, in order of
   preference: by declarative assets (clause 3); by out-of-process servers
   speaking the command surface, which the operating system sandboxes and the
   user permits; and, subject to the amendment named in clause 4, by sandboxed
   in-process code. Native in-process binary extensions are prohibited at
   every stage: an extension may not be able to corrupt the core.

## Article VIII — Bandwidth

The interface exists to maximize throughput between the user's mind and the
canvas — and between the user and their agents. Binding rules:

1. **The canvas is the prompt.** Selection, viewport, spatial arrangement,
   and intent-ink marks *are* agent context, carried automatically (the
   `atlas-ai` context beacon is the seed of this channel). Typing is a
   fallback channel, not the primary one.
2. **Every input modality compiles to the command surface.** Mouse, pen,
   keyboard, ink, voice, agent — all express intent as the same registered
   commands. New modalities are adapters, never parallel mutation paths.
3. **The human is never blocked on an agent.** Direct manipulation stays at
   millisecond latency; agent work runs asynchronously and streams onto the
   canvas as journaled, attributed, interruptible actions.
4. Ephemeral **intent ink** (marks made to communicate, not to author) is a
   distinct layer: it feeds context, it is not content, and it never
   pollutes the document.
5. **Presence is not content.** Cursors, viewports, selections, and session
   membership are ephemeral: they are broadcast, never journaled, never
   exported, and never restored. This extends the intent-ink principle of
   clause 4 to multi-participant sessions, human or agent.

## Article IX — Slate is a linker, never a database

**IX.1 — Link, do not absorb.** Workbooks link to material; they do not become
a store of record. All links resolve through a **`Source` abstraction** —
local paths today; git repositories, cloud drives, and URLs later — each
resolving to content, facets, and thumbnails. Slate may point at other
databases; it must not quietly become one.

**IX.2 — Locators are relative first.** Every link stores a locator relative
to a detected source root wherever one exists, alongside its absolute form.
Resolution prefers the relative form. A workbook must be openable on a machine
that mounts the same material at a different absolute location.

**IX.3 — Latency is a source property, not an error.** Link health is
tri-state (`Ok` / `Missing` / `Unknown`). No user-facing operation blocks on a
source round trip.

**IX.4 — Packages are forks.** A workbook may be *packaged*: its linked
material is copied beside it and its locators rewritten to point at the
copies. A package is a permanent fork, not a synchronised mirror — nothing
re-merges, nothing diffs against the origin, and the packaged workbook is an
ordinary workbook that owns its assets. A package records where each asset
came from, so that a human can always find the original. A package that
cannot name its origins is prohibited.

**IX.5 — Write-back is explicit and never default.** Edits made on the canvas
do not touch source material. A user may explicitly direct a specific edit to
be written back to a specific source; that direction is per action, names the
file it will change, and is journaled. Write-back is never implicit, never a
setting that applies to future actions, and never available to an agent.

## Article X — No chrome divergence

The shared-chrome law is ratified by reference: all chrome painting, colors,
and layout primitives live in `atlas-shell`; apps supply data and handle
actions. See `.cursor/rules/shared-chrome.mdc`, `AGENTS.md`, and the
contract documents (`TOPBAR.md`, `DOCK.md`, `PAINT.md`, `SIDEBAR.md`). Both
apps look and feel identical, always.

## Article XI — Agent conduct and amendment

1. **The pushback mandate.** An agent asked to do something that conflicts
   with an article must not silently comply. It must name the conflicting
   article, explain the damage, and propose either a conforming alternative
   or an explicit amendment. Pushback is a feature of this project, not
   insubordination.
2. **Amendment process.** This constitution changes only by explicit,
   user-approved edit to this document. An amendment states what changes,
   why, and what it supersedes (see Article IV's SVG ceiling as the model).
   Agents may draft amendments; only the user ratifies them.
3. **Precedence.** Where this document conflicts with any other rule, doc,
   or code comment in the repository, this document wins, and the other
   artifact should be updated to conform.

---

## Companion documents

- `ROADMAP.md` — the dependency-ordered phases of the long-term build.
- `docs/facet-taxonomy.md` — the file-type classification scheme (tools bind
  to facets, not formats).
- `AGENTS.md` — day-to-day working instructions for agents in this repo.
- `.cursor/rules/constitution.mdc` — the always-applied distillation of this
  document.

## Amendment log

- **2026-07-25 — Audit №1 and №2 amendments (A, B, C, E, F).** Ratifies
  relative-first locators and tri-state link health (IX.2–IX.3); convergent
  journal commands (VI.2), ephemeral presence (VIII.5), and packages-are-forks
  (IX.4); the provider hedge (I.2) and the agent surface's memory, proposal,
  and skill boundaries (VII.5–VII.7); the Windows reference platform with a
  Linux verification floor (I.3); and derived, announced frame membership
  (V.2). From audit №2: local-first with a self-hosted relay (I.4), portal
  authority and serialization classes (V.3), derived state and the bake rule
  (VI.3), the extension ladder (VII.8), and explicit human-only write-back
  (IX.5). Supersedes the audit's draft A.3 (version discipline, dropped with
  the Tier-2 platform work) and its sealed-snapshot draft, which becomes IX.4's
  package model. Rejects the proposed "portals are deterministic functions"
  generalization: determinism binds generated portals only. Holds back the
  agent-node clause (reserved as VII.9) and the control-surface clauses
  pending implementation. Numbers the previously unnumbered clauses of
  Articles I, V, VI, and IX so the new clauses can be cited; no existing rule
  changed meaning.
- **2026-07-19 — Founding.** Articles I–XI ratified. Supersedes the board's
  CSS-only styling ceiling with the SVG ceiling (Article IV). Ratifies
  portals (Article V), journal authorship (Article VI), command parity with
  the data-not-code boundary and the named script amendment path
  (Article VII), and the bandwidth article (Article VIII).
