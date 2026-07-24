# Dimension registry — the permanent matrix

The ever-growing sum of every behavior dimension any tool contract has ever
answered. Per-tool matrices (the volatile canvases and the
`contracts/<tool>.md` files) are projections of this registry: they must
account for **every** dimension listed here — with an answer, an inherited
pattern reference, or an explicit `n/a`. Silence is not an answer.

Rules:

- **Append-only.** IDs are stable forever; never renumber, reuse, or delete.
  A dimension that stops earning its keep is marked `(deprecated)` in its
  Notes, not removed.
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

| ID  | Dimension | Question it answers | Introduced by |
|-----|-----------|---------------------|---------------|
| D01 | Initiation & arming | How is the tool entered — key, palette name + aliases, rail icon, repeat? | line |
| D02 | Stickiness & repeat | One-shot or sticky after commit? How does repeat-last interact? | line |
| D03 | Gesture grammar | The state machine: states, transitions, what input advances each state? | line |
| D04 | Click vs drag rule | If both grammars exist, what disambiguates them (threshold, timing)? | line |
| D05 | Modifiers | What do Shift/Ctrl/Alt do, per state? Held vs toggled? | line |
| D06 | Constraints & snapping | How do ortho/grid/object snaps apply? What overrides them? | line |
| D07 | Direction / value locks | Can a parameter be locked mid-gesture (Tab)? What stays free? | line |
| D08 | Numeric / manual entry | What does typing digits do, per state? Edit/apply/clear keys? | line |
| D09 | Preview & readouts | What live feedback renders mid-gesture? Where do numbers appear? | line |
| D10 | Cursor | Cursor shape per state; glyphs for locked/constrained states? | line |
| D11 | Commit | What node/change results? What style state is consumed? Journal command + undo grouping? | line |
| D12 | Cancel | What does each Esc press peel, in order (P0.1 layering)? | line |
| D13 | Selected presentation | Grips vs bbox: what handles does the selected result expose? | line |
| D14 | Post-edit | How is the result re-edited later (grips, Direct Selection, joins)? | line |
| D15 | Non-goals | What source-app behavior is deliberately cut (Art. III), so it's a decision, not an omission? | line |
| D16 | Create-style inheritance | Does the tool consume the last single-node style edit (stroke, fill, opacity, …)? What are the defaults when none exists? | line |
| D17 | Hit-testing & pick | Click and marquee select on stroke geometry (width + `pick.slop`), never the node AABB alone for open curves (P1.curve.pick) | line |
