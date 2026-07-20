# Roadmap — the long-term build

Companion to `CONSTITUTION.md`. Phases are **dependency-ordered, not
calendar-bound** — each unlocks the next, and each must leave the tool
better as a daily driver than the last. No phase is pure infrastructure with
deferred payoff.

Horizon: two-plus years, vibe-coded. Plan for agents to get substantially
more capable across this horizon: invest first in contracts, schemas, and
tests (the durable specs), because implementations get cheaper to regenerate
every year.

---

## Phase 1 — Ratify

Establish the law. **Status: done (2026-07-19).**

- `CONSTITUTION.md`, `.cursor/rules/constitution.mdc`, this roadmap,
  `docs/facet-taxonomy.md`.
- Decisions locked: Rust-native substrate with the renderer-agnostic hedge
  (Art. I); SVG styling ceiling (Art. IV); portals (Art. V); journal
  authorship (Art. VI); data-not-code agent assets with a named script
  amendment path (Art. VII); bandwidth (Art. VIII).

## Phase 2 — Draw (the geometry engine, annotation-first)

**Status: core landed (2026-07-19)** — `vector-ink` crate, scene `Path`
model with caps/joins/width profiles, journal authorship, board tools
(Polyline / Arc / Bezier / Pen) with cached mesh painting, and SVG artifact
serialization. Still open: taper/cap/join inspector UI, the intent-ink
layer, and blend modes (deferred under Art. IV — epaint has no per-shape
blend pipeline yet, so the model may not carry what only one interpreter
can express; revisit with the GPU renderer in Phase 6).

The board learns real vector geometry. The first mandate is the
**architect's red pen** — markup and redlining over drawings, PDFs, and
images — because that is the daily-driver drawing act. Illustration
capability falls out of the same engine.

- New pure leaf crate (working name `vector-ink`; consider `kurbo` for curve
  math) — paths as bezier/arc/line spans, arc-length parameterization,
  zoom-adaptive flattening, hit-testing. No egui dependency (Art. I).
- Stroking to feathered AA meshes: generalize the `atlas-shell/src/taper.rs`
  technique from one straight segment to any path with any width profile —
  variable weights, caps, joins. Tessellate once, cache by
  path + style + zoom bucket (Art. II).
- Scene model: `Path` shape kind in `slate-doc::scene` under the SVG ceiling;
  blend modes (`mix-blend-mode`); stroke caps/joins/dash on paths. Lands in
  the egui painter **and** `slate-artifact` together (Art. IV).
- Board tools: implement the stubbed `Arc`, `Polyline`, `BezierSpan` tools
  plus a freehand pen (fit input to beziers).
- **Intent-ink layer**: ephemeral marks as agent context, not content
  (Art. VIII) — feeds the `atlas-ai` beacon; groundwork for Phase 4.

## Phase 3 — Unify (portals)

Generated views fold into the board (Art. V), retiring view-mode bloat.

- Portal node type: journaled frame `(position, size, source, query)`,
  deterministically regenerated contents.
- Migration order: **Grid → Venn → Lens** (simplest first). Retire each
  tab-level `ViewKind` only at parity.
- Multi-lens dashboards: several portals on one source with different
  filters — glanceability as an architectural property.
- Longer view: a folder portal (Atlas's tree canvas as a portal) begins
  unifying the two apps' canvases.

## Phase 4 — Speak (the agent surface)

Command parity becomes real (Art. VII).

- MCP server exposing the registered command surface; every human action
  available to a linked agent, every agent action journaled and attributed
  (Art. VI).
- The context beacon grows into the two-way channel: canvas-as-prompt
  (selection + viewport + intent ink travel automatically, Art. VIII);
  agent actions stream back onto the canvas, interruptible.
- Agent-authored user-space assets: dashboards (scenes of portals),
  palettes, brushes — data, not code.

## Phase 5 — Reach (sources, facets, formats)

The tool widens to more material without widening the core.

- `Source` abstraction (Art. IX): local paths become one variant; add git
  repositories and one cloud drive as proof of the seam.
- Facet-matrix refactor: `slate-doc::media::MediaKind` evolves into
  `facets(path) -> FacetSet` + decoder providers per
  `docs/facet-taxonomy.md`; tool menus bind to facets, not formats.
- **Print-faithful sheet/PDF export** — competition boards, drawing sets.
  The SVG ceiling makes this a second honest serialization of the same
  scene (Art. IV).
- AEC on-ramp: IFC, then point clouds, ahead of USD (Rust USD tooling is
  immature; USD remains the stated open-format destination). glTF as
  pragmatic 3D interchange alongside `.3dm`.

## Phase 6 — Ascend (substrate and beyond)

The hedges pay off.

- Renderer evolution: GPU vector rendering (Vello or successor) when ripe —
  a port, not a rewrite, if Article I held.
- Platforms: Mac build (near-free with eframe), iPad as a spike, not a
  promise.
- Collaboration: Miro-style multiplayer, built on the authored, attributed
  journal streams that Article VI has been accumulating since Phase 1.
- Revisit the Article VII script amendment (sandboxed workbook automations)
  once the command surface has proven mature.

---

## Standing priorities (all phases)

- Performance regressions block merges (Art. II).
- Every new capability passes the 10% rule (Art. III) — name the real use
  first.
- Specs before code: new contracts get a doc (like `DOCK.md`, `PAINT.md`,
  `docs/lens-agent-contract.md`) and golden tests where meaningful.
- Open-sourcing is assumed: write everything as if strangers will read it.
