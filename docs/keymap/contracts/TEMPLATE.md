# Contract template

Copy for a new tool. The matrix rows come from `DIMENSIONS.md` — the
permanent, append-only dimension registry — in registry order. Every
dimension must be either **covered by an inherited pattern** (listed in
Inherits, not restated), **answered by a matrix row**, or explicitly marked
`n/a`. Silence is not an answer. New axes discovered during a tool request
are appended to `DIMENSIONS.md` first (next `D##`), then answered here.

Keep cells concrete: key names, state names, numbers, token names.
"Intuitive", "natural", and "smooth" are banned words.

---

```markdown
# <Tool> — interaction contract

Status: draft | agreed | shipped
Reference: <source app + tool, e.g. "Rhino Line">
Command: <CommandId> · Key: <chord> · Palette: <name + aliases>
Inherits: P0.* (all), <P1.class>, <P2.archetype> — deviations flagged below.

## Behavior matrix

One row per registry dimension, in registry order. Cite pattern IDs where
the answer is inherited; write `n/a` where the dimension doesn't apply.

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-----------------|--------|------|
| D01 | Initiation & arming | | | |
| …   | (every ID in DIMENSIONS.md) | | | |

Source values: stated (user), precedent (approved in decisions.json for an
overlapping tool), pattern (catalog), research (source app), guess (agent
proposal — must be confirmed before Status: agreed).

Conf is the confidence score per the rubric in the tool-contract skill:
100 stated · 85–95 precedent · 75–90 pattern · 60–80 research · <60 guess.
User-altered rows become 100 once approved. Mirror every row into
`decisions.json` (verdict: proposed → approved on completion).

## Feel constants

| Token | Meaning | Initial value |
|-------|---------|---------------|

## Golden paths

Numbered input scripts (`GP1`, `GP2`, …): exact input sequence → exact
expected outcome. Each becomes a headless test when the tool ships.

## Open questions

Unresolved dimensions (empty once Status: agreed).
```
