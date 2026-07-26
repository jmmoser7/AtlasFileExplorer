# Standing agent brief

Read this in full before touching the repository. It applies to every task card
in `docs/workplan/tasks/`. Your card overrides this brief only where it says so
explicitly.

You are one agent in a swarm working the same repository. Most of these rules
exist because of that: work that would be harmless from a single contributor is
destructive from twelve.

---

## 0. Execution model — learned the hard way

The first Wave 0 dispatch assumed each agent would get an isolated git worktree.
**It did not.** Eight agents ran in one shared working tree, none committed, and
the result was a single undifferentiated pile of changes that had to be split
apart by hand afterwards. Nothing was lost, but nothing was reviewable either.

Until isolation is demonstrably working, these rules bind:

1. **Assume you are sharing the working tree with the human and possibly another
   agent.** You are not alone in a sandbox.
2. **Check out your branch first and confirm it**, before your first edit:
   `git rev-parse --abbrev-ref HEAD`. If you are on `main`, or on another card's
   branch, stop and escalate rather than guessing.
3. **Commit early and commit often.** An uncommitted change is invisible to
   everyone and is destroyed by the next branch switch. Commit as soon as a
   coherent piece works, not once at the end. If you are interrupted, whatever
   you committed survives and whatever you did not is gone.
4. **Never switch branches** except onto your own, and never `git stash`,
   `git checkout -- .`, `git reset --hard`, or `git clean`. Another agent's
   uncommitted work may be sitting in the tree beside yours, and those commands
   destroy it without warning.
5. **Never run `cargo fmt --all`.** The baseline is not formatted, so it
   reformats files no card owns and buries your real change in noise. Format
   only the files your card owns: `cargo fmt -p <your-crate>` or `rustfmt` on
   specific paths.
6. If the tree contains modifications to files your card does not own when you
   start, **leave them completely alone** and say so in your report.

### Building on this machine

A bare PowerShell has no MSVC environment, so `cargo test` fails while linking
with `LINK : fatal error LNK1104: cannot open file 'msvcrt.lib'`. That error
means the environment is wrong; it never means the code is wrong. The VS 2022
install on this machine is incomplete (no desktop x64 libs); the VS 2019 Build
Tools are intact. **Prefix every cargo command:**

```
cmd /c 'call "C:\Program Files (x86)\Microsoft Visual Studio\2019\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul 2>&1 && cargo test -p <crate> 2>&1'
```

Note the single quotes: PowerShell passes the string to `cmd` literally, and
`cmd` needs `call` before the batch file. Allow a 30-second overhead per command
for environment setup, and set generous timeouts — a cold workspace build takes
minutes.

`cargo` also rewrites `Cargo.lock` on some runs. That is normal and is not
yours to revert unless your card owns the file; report it and leave it.

## 1. Scope discipline — the most important section

**Execute exactly one card. Change nothing your card does not name.**

- If you notice a bug outside your card: do not fix it. Add a row to your PR's
  handoff note. If it is constitutional, propose a row for
  `docs/audit/deviations.md` in the PR body — do not edit that file unless your
  card owns it.
- If your card's approach turns out to be wrong: stop and escalate (§6). Do not
  substitute a better idea. A card that is wrong is a planning defect, and
  silently routing around it destroys the plan's ability to sequence work.
- If you need to edit a file your card does not list: stop and escalate. File
  ownership is in `docs/workplan/README.md` §5 and it is what keeps twelve
  parallel branches mergeable.
- "While I was in there" is the failure mode this brief exists to prevent.

## 2. Law

`CONSTITUTION.md` is the governing document; read it before you start. The
clauses you are most likely to break:

- **Article I** — no document model, geometry, or capability logic may depend
  on `egui`/`eframe`. Pure crates stay pure. If your card creates a crate, it
  gets no renderer dependency unless the card says otherwise.
- **Article II** — no per-frame allocation or tessellation in paint paths;
  heavy work is async and generation-tagged.
- **Article III** — build the named use, not the general capability. If you
  find yourself adding an option nobody asked for, delete it.
- **Article IV** — the egui painter and the artifact writer are two
  interpreters of one model. A style property lands in both or neither.
- **Article VI** — every document mutation is an invertible, journaled,
  authored command. Never mutate `doc.scene` outside
  `patch_nodes` / `add_nodes` / `delete_board_nodes` / `commit_scene`.
- **Article VII** — every user-facing action is a registered command in the
  app's `commands.rs` `ENTRIES`/`SPECS` table.
- **Article X** — chrome painting, colors, and layout primitives live in
  `atlas-shell`. Never in an app crate.
- **Article XI.1** — if your card conflicts with an article, name the article
  and stop. Do not comply silently, and do not amend the constitution: only the
  user ratifies, by editing `CONSTITUTION.md`.

**Unratified proposals are not law.** `docs/audit/amendments/` contains drafts.
Do not implement anything that depends on them unless your card states that the
gate has passed.

## 3. Before you write code

1. Read your card top to bottom, including the acceptance criteria and the
   forbidden list.
2. Read the files the card names. Read them, do not grep them — the card's
   line numbers may have drifted, and the surrounding code is the spec for
   style.
3. Read the relevant contract doc if your card names one
   (`crates/atlas-shell/{TOPBAR,DOCK,PAINT,SIDEBAR}.md`,
   `apps/*/src/app/ARCHITECTURE.md`, `apps/slate/src/app/COMMANDS.md`,
   `docs/keymap/contracts/*.md`, `docs/lens-agent-contract.md`).
4. Confirm the preconditions in your card's "Depends on" line are actually true
   in the tree. If they are not, escalate — your wave gate has not passed.

## 4. While you write

- **Match the surrounding code.** Naming, comment density, error style, test
  style. This repo comments intent and constraints, not mechanics; do not
  narrate what the code does, and never write a comment explaining that your
  change is correct.
- **Tests are part of the change, not a follow-up.** Every card names the tests
  it expects, using the repo's `snake_case` descriptive convention
  (`journal_undo_redo_round_trip`). Add them in the same commit as the code.
- **No new dependencies** beyond those your card names. If you believe one is
  required, escalate with the crate name, version, license, and what it
  replaces.
- **Keep public API stable** unless your card says otherwise. Cards mark
  "frozen" APIs; changing one breaks other agents' branches invisibly.
- **Windows is the reference platform, Linux is the CI floor.** Your code must
  compile and test on Linux. Platform-specific code goes behind
  `#[cfg(windows)]` with a working non-Windows stub, following the existing
  pattern in `atlas-core`.
- **Shell is PowerShell 5.1.** Chain with `;`, not `&&`. Quote paths with
  spaces.

## 5. Definition of done

All of these, every time:

```powershell
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

plus:

- [ ] Every acceptance criterion in the card is demonstrably met, with the
      evidence named (test name or command output) in the PR body.
- [ ] Only files owned by your card are modified.
- [ ] New public items have doc comments in the style of their neighbours.
- [ ] Any contract doc your change affects is updated in the same PR.
- [ ] Branch `feature/<task-id>-<slug>`, one PR, **not merged by you**.
- [ ] PR body follows `docs/workplan/README.md` §6.

If a criterion cannot be met, the PR is opened as a draft titled
`BLOCKED: <card id> — <one line>` with the reason and what you tried. A blocked
PR with an honest diagnosis is a good outcome. A green PR that quietly skipped a
criterion is not.

## 6. Escalation — stop conditions

Stop immediately, write the reason in the PR body (or return it as your final
message if no code exists yet), and do not proceed, when:

1. Your change would require editing a file your card does not own.
2. Your card conflicts with a constitutional article.
3. Your card depends on an unratified amendment.
4. `SlateDoc::CURRENT` is not the value your card expects (the
   `format_version` ledger in `docs/workplan/README.md` §4 is out of date, which
   means tasks landed out of order).
5. A "frozen" API cannot survive your change.
6. You need a new dependency.
7. Tests that passed before your change fail after it and you cannot explain
   why in one sentence.
8. The task would take more than roughly double the card's stated size.

Escalating is cheap. Guessing is not.

## 7. Handoff note

Every PR ends with a handoff note aimed at the next agent in the chain:

```markdown
## Handoff
- What now exists that did not before (one line per public item).
- What the next card in this chain must know (surprises, renames, gotchas).
- What I deliberately did not do, and why.
- Anything I noticed that deserves its own card.
```

This is how a swarm keeps state. It is not optional.
