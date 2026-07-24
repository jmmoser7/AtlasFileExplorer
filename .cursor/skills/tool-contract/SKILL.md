---
name: tool-contract
description: >-
  Codify the interaction contract and feel of a canvas tool before building it.
  Use whenever the user asks to add or refine a canvas tool ("let's add a line
  tool like the one in Rhino", "the circle tool feels wrong", "add an arc
  tool"), or mentions tool feel, interaction contracts, behavior matrices,
  dimensions, or gesture semantics for Slate / File Atlas. Produces an
  interactive per-tool behavior matrix canvas (with confidence scores) for the
  user to accept / alter / reject, then a contract document in
  docs/keymap/contracts/, with approved decisions stored in decisions.json as
  precedent for future tools, recurring patterns promoted up a shared
  hierarchy, and new dimensions appended to the permanent dimension registry.
---

# Tool interaction contracts

Purpose: collapse the possibility space of a tool's feel **before** code is
written, using a fixed vocabulary instead of prose ping-pong. The user
initiates with one sentence ("add a line tool like Rhino's"); the agent
answers with an **interactive behavior-matrix canvas** of best-guess
defaults scored by confidence; the user accepts / alters / rejects per row;
the confirmed matrix becomes a contract document, and every approval becomes
**precedent** that raises confidence on the next tool.

## Persistent artifacts (the memory between tool requests)

- **Permanent matrix** — `docs/keymap/contracts/DIMENSIONS.md`. Append-only
  registry of every behavior dimension ever used (`D01`, `D02`, …). New axes
  discovered during any tool request are appended with the next ID **on task
  completion** and persist for all future requests. Registry order is the
  canonical row order everywhere.
- **Decisions database** — `docs/keymap/contracts/decisions.json`. Every
  tool × dimension decision: behavior text, source, confidence, verdict
  (`proposed` → `approved`/`rejected`), decided date. Approved rows are
  precedent: when a new tool overlaps (bezier after line: same archetype,
  same object class), seed its matrix from the approved answers at high
  confidence instead of re-guessing.
- **Pattern hierarchy** — `docs/keymap/contracts/PATTERNS.md` (L0 universal
  → L1 object-class → L2 tool-family archetypes → L3 tool-specific).
  Contracts never restate an inherited pattern; they reference it and list
  only deviations.
- **Contracts** — `docs/keymap/contracts/<tool>.md` (from `TEMPLATE.md`);
  `line.md` is the worked reference example.
- **Volatile matrix** — a per-tool interactive canvas
  (`<tool>-tool-contract.canvas.tsx` in the workspace's managed `canvases/`
  directory). Disposable once the contract is agreed — the contract `.md`
  and `decisions.json` are the durable record.

Source-app UX research lives in `docs/keymap/research/*.md` (Rhino, Miro,
Photoshop, Illustrator, Grasshopper). Search these before the web.

## Confidence rubric (every matrix row carries a score)

| Score | Meaning |
|-------|---------|
| 100 | `stated` — the user explicitly asked for it in this request. |
| 85–95 | `precedent` — approved in decisions.json for another tool and the same archetype / object class applies cleanly. |
| 75–90 | `pattern` — a cataloged PATTERNS.md rule directly covers the dimension. |
| 60–80 | `research` — documented source-app behavior that translates cleanly to the board. |
| < 60 | `guess` — agent inference; flag the lowest few as open questions. |

Score at the low end of a band when the mapping needed adaptation; high end
when it is verbatim. A rejected precedent drops back to guess next time.

## Workflow

1. **Intake.** Parse the request: tool name, reference app ("like Rhino's"),
   any stated behaviors. Stated behaviors are ground truth — mark them
   `stated`, confidence 100, pre-accepted; never re-ask.
2. **Research.** Read DIMENSIONS.md, PATTERNS.md, and **decisions.json**.
   Pick the closest archetype (L2) and pull every approved decision from
   overlapping tools as `precedent` rows. Read the relevant
   `docs/keymap/research/` file(s); web-search only for remaining gaps.
   Read the current implementation (`apps/slate/src/app/board*.rs`,
   `commands.rs` SPECS) enough to know what exists vs. what's new.
3. **Build the volatile matrix canvas** (read the Cursor `canvas` skill
   first — mandatory before writing any `.canvas.tsx`). One row per
   registry dimension in registry order — answered, pattern-referenced, or
   `n/a`; never skip a dimension. Per row: proposal text, source pill
   (`stated` / `precedent` / `pattern` / `research` / `guess`),
   **confidence score**, and Accept / Alter / Reject controls (Alter
   reveals a free-text field). Include: an inherits preamble, the open
   questions (max 4, with options — draw them from the lowest-confidence
   rows), a "propose new dimension" input, and a "send decisions to agent"
   button. **The send button must dispatch `openAgent` targeting the
   conversation that is building the canvas — never `newComposerChat`** (a
   new chat loses the working agent's context). Find your own conversation
   UUID as the most recently modified entry in the workspace's
   `agent-transcripts` folder and bake it into the dispatch as `agentId`.
   The canvas SDK has no action that injects a prompt into an existing
   chat, so the button focuses the owning agent on the taskbar and the
   canvas copy tells the user to say "done" there; the decisions
   themselves always travel through the data sidecar, not the prompt.
   Write the same proposals to decisions.json with `verdict: "proposed"`.
   Post a chat link to the canvas plus a 2–3 sentence summary of the
   weakest guesses.
4. **Read back decisions.** The canvas persists state to a sidecar
   `<name>.canvas.data.json` next to the `.canvas.tsx`. When the user says
   they're done (or the send button fires), read the sidecar and apply:
   rejected rows get re-proposed (alternatives), altered rows adopt the
   user's text verbatim at confidence 100, new dimensions are answered for
   this tool. Iterate on the canvas until every row is accepted.
5. **Write the contract** to `docs/keymap/contracts/<tool>.md` from
   TEMPLATE.md: inherits-list + full dimension matrix (with confidence) +
   feel constants + golden paths. Update `KEYMAP.md` if a new binding
   appeared.
6. **Completion bookkeeping (never skip).**
   - Append user-added dimensions to **DIMENSIONS.md** with the next `D##`
     and the introducing tool noted — they appear in every future matrix.
   - Flip this tool's rows in **decisions.json** to `verdict: "approved"`
     (or `"rejected"`) with the decided date and final behavior text.
   - **Promote patterns**: any rule now appearing in ≥2 contracts moves UP
     to the right level in PATTERNS.md; both contracts replace their copy
     with a reference. Never duplicate a pattern downward.
7. **Implement + pin — not optional.** The contract flipping to
   Status: agreed is the trigger, not the finish line: proceed directly
   to implementation in the same task, without waiting for a further
   user prompt, unless the user has explicitly deferred it. Build to the
   contract (constitution rules apply: journal-only mutation, registry
   SPECS row, Art. II caching). Every golden path becomes a headless
   input-script test. Feel constants (tolerances, radii, preview alphas)
   go in `ui-tokens.toml` or a named constants block — never magic
   numbers — so refinement rounds are tuning, not rewrites. A contract
   may be marked Status: shipped only when the build compiles, the
   golden-path tests pass, and the SPECS row exists.

## Volatile matrix conventions

- Decision states per row: `pending` (default for guesses), `accepted`
  (default for `stated` rows), `altered` (+ note), `rejected` (+ optional
  reason). Proposal cells stay concrete: key names, state names, numbers,
  token names — "intuitive" and "smooth" are banned words.
- Canvas state keys: `decisions` (record by dimension ID), `questions`
  (record by question ID), `newDims` (array of `{name, desc}`). Keep these
  key names stable so sidecar readback is uniform across tools.
- A contract may flip to Status: agreed only when no row is `pending` or
  `rejected` and open questions are empty.

## Refinement after implementation

When the user says a shipped tool "feels wrong": identify the **dimension +
state** in contract terms, check whether the offending value is a tunable
token first (tune, don't rewrite), fix, and update the contract + golden
test + decisions.json in the same change. The contract is the source of
truth — code that disagrees with it is wrong (mirror of the constitution's
rule).
