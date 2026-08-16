# Dimension registry — the permanent matrix

The ever-growing sum of every behavior dimension any contract has ever
answered. Per-contract matrices (the volatile canvases and the
`contracts/<name>.md` files) are projections of this registry: they must
account for **every in-scope** dimension listed here — with an answer, an
inherited pattern reference, or an explicit `n/a`. Silence is not an answer.

Rules:

- **Append-only.** IDs are stable forever; never renumber, reuse, or delete.
  A dimension that stops earning its keep is marked `(deprecated)` in its
  Notes, not removed.
- **Scope.** Every dimension declares which contract families must answer it:
  `tool` (canvas tools), `portal` (portal subtypes), or `any` (both). A
  contract declares its family in its header (`Family: tool | portal`) and
  answers exactly the dimensions in scope for it — a gesture tool is not made
  to write `n/a` fourteen times about export serialization, and a portal is
  not made to invent a numeric-entry story. `cargo xtask contracts` enforces
  this; a missing in-scope row fails the check.
- **Growth.** When a tool request surfaces an axis this registry lacks
  (user adds one in the volatile matrix, or the agent needs one mid-spec),
  append it with the next `D##` **as part of that task's completion
  bookkeeping**, note which tool introduced it, and answer it in that
  tool's contract. Every later contract must then account for it.
- **Companion database.** Agreed answers per tool × dimension live in
  `decisions.json`; approved rows there are precedent that seeds future
  volatile matrices at high confidence.
- **Ordering.** Registry order is the canonical presentation order for
  volatile matrices.

| ID  | Dimension | Question it answers | Scope | Introduced by |
|-----|-----------|---------------------|-------|---------------|
| D01 | Initiation & arming | How is the tool entered — key, palette name + aliases, rail icon, repeat? | any | line |
| D02 | Stickiness & repeat | One-shot or sticky after commit? How does repeat-last interact? | any | line |
| D03 | Gesture grammar | The state machine: states, transitions, what input advances each state? | any | line |
| D04 | Click vs drag rule | If both grammars exist, what disambiguates them (threshold, timing)? | any | line |
| D05 | Modifiers | What do Shift/Ctrl/Alt do, per state? Held vs toggled? | any | line |
| D06 | Constraints & snapping | How do ortho/grid/object snaps apply? What overrides them? | any | line |
| D07 | Direction / value locks | Can a parameter be locked mid-gesture (Tab)? What stays free? | any | line |
| D08 | Numeric / manual entry | What does typing digits do, per state? Edit/apply/clear keys? | any | line |
| D09 | Preview & readouts | What live feedback renders mid-gesture? Where do numbers appear? | any | line |
| D10 | Cursor | Cursor shape per state; glyphs for locked/constrained states? | any | line |
| D11 | Commit | What node/change results? What style state is consumed? Journal command + undo grouping? | any | line |
| D12 | Cancel | What does each Esc press peel, in order (P0.1 layering)? | any | line |
| D13 | Selected presentation | Grips vs bbox: what handles does the selected result expose? | any | line |
| D14 | Post-edit | How is the result re-edited later (grips, Direct Selection, joins)? | any | line |
| D15 | Non-goals | What source-app behavior is deliberately cut (Art. III), so it's a decision, not an omission? | any | line |
| D16 | Create-style inheritance | Does the tool consume the last single-node style edit (stroke, fill, opacity, …)? What are the defaults when none exists? | any | line |
| D17 | Hit-testing & pick | Click and marquee select on stroke geometry (width + `pick.slop`), never the node AABB alone for open curves (P1.curve.pick) | any | line |
| D18 | Portal class & authority | Generated, document, or host (Art. V.3 / decision D7)? Which journal owns mutations made inside it? | portal | portal-lens-repository |
| D19 | Source binding | What does the frame's `source` hold, how is it bound and rebound, and what is refused as a source kind? | portal | portal-lens-repository |
| D20 | Query & parameters | What journaled knobs shape the contents (filters, window, pin, ordering, caps)? | portal | portal-lens-repository |
| D21 | Regeneration & staleness | What triggers recomputation, what is painted while it runs, and what is painted when the source is gone? | portal | portal-lens-repository |
| D22 | Contents interaction | What do hover, click, and double-click do *inside* the frame? Which tools reach the contents versus the frame? | portal | portal-lens-repository |
| D23 | Level of detail | What is drawn at each zoom bucket, and on what measured quantity does the bucket switch? | portal | portal-lens-repository |
| D24 | Export serialization | What does the artifact writer emit for this portal, and what marks keep the export honest (Art. IV.1)? | portal | portal-lens-repository |
| D25 | Bake | What does an explicit bake produce as authored content, and what happens to the portal (Art. VI.3)? | portal | portal-lens-repository |
| D26 | Collaboration & per-peer | What syncs, what each peer regenerates locally, and how a peer who cannot resolve the source is told so | portal | portal-lens-repository |
| D27 | Agent surface | Which registered commands the MCP surface exposes, what the context beacon carries, and what an agent may never do here | portal | portal-lens-repository |
| D28 | Determinism & provenance | What makes two regenerations identical, and what provenance the portal displays about what it is showing | portal | portal-lens-repository |
| D29 | Performance envelope | Async/generation-tagging, caching keys, windowing, and the size budget the portal commits to (Art. II) | portal | portal-lens-repository |
| D30 | Failure & honesty states | Every state the source can be in — missing, wrong kind, partial, unreachable — and what each one paints | portal | portal-lens-repository |
| D31 | View-state ownership | Which knobs are journaled authored intent and which are per-peer derived state (Art. VI.3 / VIII.5) | portal | portal-lens-repository |
| D32 | Trust, sandbox & consent | What foreign code or network access the contents are given, what sandbox holds them, and what the human must permit before the first fetch | portal | portal-web-embed |
