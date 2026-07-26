# Proposed amendments A, B, C, E — 2026-07-25

**Source:** Audit №1 (`c808966`) proposed amendments A–D. This file revises them
against the decisions in `docs/audit/2026-07-25-decisions.md` and adds
Amendment E.

**Article XI.2 status: UNRATIFIED.** Nothing here is law until the user edits
`CONSTITUTION.md`. Agents must treat every clause below as a proposal and must
not implement work that depends on it. Wave 0 of the workplan is deliberately
free of such dependencies; Waves 1+ are blocked on this gate.

**Summary of changes from the audit's draft:**

| Amendment | Audit's version | Recommendation now | Why |
|---|---|---|---|
| **A** — sources | IX.2 relative locators, IX.3 version discipline, IX.4 tri-state health | ratify **IX.2 and IX.4**; **drop IX.3** | D2 removed versioned sources from scope. Law with no subject is decoration (Art. III applied to the constitution itself). |
| **B** — collaboration | IX.5 bundles, VI.2 convergent commands, VIII.5 presence | ratify **as written** | D1 makes all three load-bearing sooner, not later. |
| **C** — agents | VII.5–VII.8, I.2 provider hedge | ratify **VII.6, VII.7, VII.8, I.2**; **hold VII.5** | D4 puts Mode 1 first; agent *nodes* (VII.5) have no implementation on the horizon. Ratify it when Mode 2 is scheduled. |
| **D** — control surfaces | V.2–V.4 | **do not ratify yet** | Wave 5 at the earliest, and the design will change once the graph work lands. Ratifying unexercised law invites drift. |
| **E** — new | — | ratify | Records D3 (geometric membership) and D5 (Windows-first with a Linux CI floor) as law, since both are decisions the next contributor would otherwise "tidy away". |
| **F** — new (audit №2) | — | ratify | Portal classes, derived state, the extension ladder, explicit write-back, and local-first-with-a-relay. These are the load-bearing answers to the flexibility stress test; without them the next contributor guesses. |

---

## Paste-ready text

Insert each block into `CONSTITUTION.md` under its article, then add the log
entry at the end of this file to the **Amendment log**.

### Amendment A — Article IX (sources)

> **IX.2 — Locators are relative first.** Every link stores a locator relative
> to a detected source root wherever one exists, alongside its absolute form.
> Resolution prefers the relative form. A workbook must be openable on a machine
> that mounts the same material at a different absolute location.
>
> **IX.3 — Latency is a source property, not an error.** Link health is
> tri-state (`Ok` / `Missing` / `Unknown`). No user-facing operation blocks on a
> source round trip.

*(Renumbered: the audit's IX.4 becomes IX.3 because version discipline is
dropped. If a versioned source is ever adopted, the dropped clause is recorded
below for reinstatement as IX.4.)*

<details>
<summary>Dropped clause, retained for the record</summary>

> **IX.x — Links declare their version discipline.** A link to a versioned
> source either pins a version or tracks the tip, and says which. Silent
> tip-tracking is prohibited: a workbook that shows different content on two
> days must be able to say so.

Reinstate only if a versioned source (ACC, Drive revisions) enters scope.
</details>

### Amendment B — Articles VI, VIII, IX (collaboration)

> **VI.2 — Convergent commands.** Journal commands address nodes by stable
> identity, never by position; ordering is carried by an order key, not by list
> index; and mutations are scoped to the smallest property that changed. A
> command that cannot be applied is surfaced, never silently dropped.
>
> **VIII.5 — Presence is not content.** Cursors, viewports, selections, and
> session membership are ephemeral: they are broadcast, never journaled, never
> exported, and never restored. This extends the intent-ink principle of VIII.4
> to multi-participant sessions, human or agent.
>
> **IX.4 — Packages are forks.** A workbook may be *packaged*: its linked
> material is copied beside it and its locators rewritten to point at the
> copies. A package is a permanent fork, not a synchronised mirror — nothing
> re-merges, nothing diffs against the origin, and the packaged workbook is an
> ordinary workbook that owns its assets. A package records where each asset
> came from, so that a human can always find the original. A package that
> cannot name its origins is prohibited.

*(Revised after audit №2 / decision D10. The original draft built a "sealed
snapshot" with re-link machinery; what is wanted is InDesign's Package. The
honesty requirement survives as a provenance manifest, which is a text file
rather than a synchronisation system.)*

### Amendment C — Articles I, VII (agents)

> **I.2 — The provider hedge.** The renderer-agnostic rule extends to model
> providers and agent protocols. No document model, command, or memory record
> may depend on a specific model provider, vendor application, or protocol
> revision. Protocol adapters are leaf crates.
>
> **VII.5 — Memory segregation.** Agent memory distinguishes *pinned* records,
> authored by the human and never written by an agent, from *learned* records,
> authored by an agent and always prunable by the human. All memory is
> human-readable and deletable. Memory that cannot be read or deleted is
> prohibited.
>
> **VII.6 — Proposal by default.** Agent mutations enter a staging layer and
> require human acceptance unless the agent has been explicitly granted autonomy
> for that workspace. Staged changes are visible, attributed, and rejectable as
> a unit.
>
> **VII.7 — Skills are recipes.** A skill is a declarative sequence of
> registered commands with parameters. Skills contain no user-authored control
> flow. This is the boundary that keeps VII.4 meaningful; anything requiring an
> interpreter remains prohibited pending the named script amendment.

*(Renumbered: the audit's VII.5 "agent nodes" clause is held back, so memory
segregation takes VII.5. Reserve VII.8 for agent nodes when Mode 2 is
scheduled.)*

### Amendment E — Articles I, V (membership and platform)

> **I.3 — The reference platform.** Windows is the reference platform and the
> only supported build target. Platform-specific capability (shell thumbnails,
> file association, OS integration) is permitted in app and capability crates
> only. Every pure crate must build and test on Linux so that continuous
> integration and remote agents can verify the durable models; a change that
> breaks the Linux check is a regression regardless of its Windows behaviour.
>
> **V.2 — Membership is derived and announced.** Frame membership is a pure,
> deterministic function of geometry, not a stored relation: a node belongs to
> the topmost frame whose rect contains its centre, and to exactly one such
> frame. No parent pointers, no membership edges. Because membership is derived,
> one participant's move can change another's deck; in a shared session such a
> change is announced with its author, never applied silently.

### Amendment F — Articles I, V, VI, VII, IX (audit №2)

Added after the flexibility stress test; the reasoning is in
`docs/audit/2026-07-25-decisions-flexibility.md` (D7–D16).

> **I.4 — Local-first, with a relay the user runs.** Slate is a local
> application: it opens, edits, and exports without a network. Live
> collaboration is a first-class capability and may require a rendezvous
> service, which must be a small binary the user or their organisation can run
> themselves. No capability may require an account with, or a subscription to,
> any party — including this project.
>
> **V.3 — Portals declare authority and serialization.** A portal is a scene
> node whose frame is ordinary journaled data and whose contents come from
> elsewhere. Every portal declares two things: which journal owns mutations made
> inside it, and what it becomes in an exported artifact. *Generated* portals
> regenerate their contents deterministically and own no mutations; *document*
> portals delegate mutations to the child document's own journal; *host* portals
> present a foreign application's surface, own no mutations at all, and export
> as a poster and a pointer that says what it is. Determinism is required of
> generated portals — Article IV.2 depends on it — and is not required of the
> other two.
>
> **VI.3 — Derived state is not a mutation.** Journaled state is authored
> intent. State reproducible from authored intent plus elapsed time — portal
> contents, simulated transforms, playheads, trails, presence — is derived, and
> is never journaled. Turning derived state into authored content is an explicit
> *bake* command, which is journaled like any other mutation. Where derived
> state is shared between participants it must be deterministic, so that peers
> reproduce it from the journal rather than receiving it over a wire.
>
> **VII.8 — The extension ladder.** The workspace is extended, in order of
> preference: by declarative assets (VII.3); by out-of-process servers speaking
> the command surface, which the operating system sandboxes and the user
> permits; and, subject to the amendment named in VII.4, by sandboxed in-process
> code. Native in-process binary extensions are prohibited at every stage: an
> extension may not be able to corrupt the core.
>
> **IX.5 — Write-back is explicit and never default.** Edits made on the canvas
> do not touch source material. A user may explicitly direct a specific edit to
> be written back to a specific source; that direction is per action, names the
> file it will change, and is journaled. Write-back is never implicit, never a
> setting that applies to future actions, and never available to an agent.

---

## Amendment log entry

Append to the **Amendment log** in `CONSTITUTION.md`:

```markdown
- **2026-07-25 — Audit №1 and №2 amendments (A, B, C, E, F).** Ratifies relative-first
  locators and tri-state link health (IX.2–IX.3); convergent journal commands
  (VI.2), ephemeral presence (VIII.5), and sealed snapshots (IX.4); the provider
  hedge (I.2) and the agent surface's memory, proposal, and skill boundaries
  (VII.5–VII.7); the Windows reference platform with a Linux verification floor
  (I.3); and derived, announced frame membership (V.2). Supersedes the audit's
  draft A.3 (version discipline, dropped with the Tier-2 platform work) and
  holds back its agent-node and control-surface clauses pending implementation.
  From audit №2: local-first with a self-hosted relay (I.4), portal authority
  and serialization classes (V.3), derived state and the bake rule (VI.3), the
  extension ladder (VII.8), and explicit human-only write-back (IX.5). Revises
  the draft sealed-snapshot clause into packages-are-forks (IX.4) and rejects
  the proposed "portals are deterministic functions" generalization.
```

---

## Consequence: Article VI's live violation closes here

The audit's one real violation (§8) is that Article VI promises the journal is
"the foundation for multiplayer synchronization later" while `SceneCmd` is
positional and whole-node. Ratifying **B / VI.2** does not fix it — work item
WI-2 does. Until WI-2 lands, the deviation stays open in
`docs/audit/deviations.md` and is counted by `cargo xtask metrics`, so the gap
between law and code is a number in every future audit rather than a paragraph.
