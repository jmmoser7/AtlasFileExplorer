# Health checks

The health-check agent (`.cursor/agents/health-check.md`) runs on a cadence
and asks one question: is the codebase, and the way agents work on it,
getting cheaper to change without getting worse? Law stays in
[`CONSTITUTION.md`](../../CONSTITUTION.md) — Article II (performance),
Article III (the 10% rule), Article XI (pushback; amendments only by the
owner), Article XII (one owner of knowledge). This file owns the procedure,
the budgets, and the backlog. The agent brief owns authority and handoff.
Do not copy either into the other.

`cargo xtask metrics` already records per-crate size, purity, and deviation
counts (`docs/metrics/`). This check adds the cross-crate top files, agent
spend, and the lint allowlists. It does not invent a second metrics system.

## Cadence

Weekly, and after any multi-agent batch. The weekly run is Deep when the
machine is free, Quick otherwise. The post-batch run is Quick.

Each run branches `health/YYYY-MM-DD` from current `main`, writes the
report, and stops. It does not merge or push.

### Quick (~15 minutes)

No cargo. One pass:

1. Top 15 `.rs` files by line count, plus character count (script below).
2. Always-on context: `AGENTS.md` and every `.cursor/rules/*.mdc` with
   `alwaysApply: true`. Tokens ≈ characters / 4.
3. Rows in `docs/audit/deviations.md` by status. Count the table. The
   generated block under `metrics:deviations` is rewritten only by
   `cargo xtask metrics` and can be stale.
4. `[[entry]]` rows in each `xtask/*allowlist*.toml`. Contracts and kits
   have no allowlist; they fail closed.
5. Diff against the previous report. Write the new one. Hand off at most
   the top 3.

### Deep

Quick, then one cargo at a time on this branch (see operating rules):

1. `Measure-Command { cargo test --workspace }` — wall time, fail count,
   and any test that fails once and passes on a single retry (a flake).
2. `cargo clippy --workspace --all-targets` — warning count.
3. `cargo xtask metrics` — commit the snapshot it writes.
4. `Measure-Command { cargo build -p slate }` — wall time, cold or warm,
   labeled which.
5. If a spend log exists (below), sum tool calls and tokens per agent.

A flake is recorded and handed off. The suite is not switched to
`--test-threads=1` to paint it green.

### How to measure file size

From the workspace root, excluding `target`:

```powershell
$root = (Get-Location).Path
Get-ChildItem -Recurse -Filter *.rs -File |
  Where-Object { $_.FullName -notmatch '\\target\\' } |
  ForEach-Object {
    $t = [System.IO.File]::ReadAllText($_.FullName)
    $n = ($t -split "`n").Count
    if ($t.EndsWith("`n")) { $n-- }
    [PSCustomObject]@{ Lines = $n; Chars = $t.Length; Rel = $_.FullName.Substring($root.Length + 1) }
  } | Sort-Object Lines -Descending | Select-Object -First 15
```

Lines are newline-separated rows, not non-blank lines. Characters are the
.NET string length (UTF-16 units; this tree is effectively ASCII, so they
match bytes). Tokens for prose are characters / 4, rounded. That estimate
matched the owner's overnight figure for `AGENTS.md` and the always-on
rules on 2026-10-09.

Read frequency comes from transcripts when they record opens. Otherwise the
cell stays blank.

## Metrics

| Metric | How | Budget |
|---|---|---|
| Largest `.rs` files | Script above. Read frequency from transcripts when present | No file over **2,000** lines without a row in the latest report: owner, why it is still one file, split milestone. Near-term priority: anything over **4,000** |
| Always-on context | `AGENTS.md` + `alwaysApply: true` rules, chars/4 | **≤ ~3,000** tokens |
| Test wall time | `Measure-Command { cargo test --workspace }` | **Under 15 minutes** on the reference Windows machine, default threads. Revise only with the owner's approval if an honest run is slower |
| Flaky tests | Failed once, passed on one retry; plus known wall-clock waits | **Zero** tests whose pass/fail depends on the wall clock |
| Allowlist entries | `[[entry]]` in each `xtask/*allowlist*.toml` | Must not rise between reports without a named reason. A new lint starts at the count of its first green run |
| Clippy warnings | `cargo clippy --workspace --all-targets` | **0** |
| Duplicate helpers | Until a lint exists: open Article XII rows in `docs/audit/deviations.md`. Then: the lint's finding count | Findings trend to **0**. Allowlist rows are counted above |
| Open deviations | Status column of `docs/audit/deviations.md` | Trend down. A new row names its closing milestone. Rows are closed, never deleted |
| Build time | `Measure-Command { cargo build -p slate }`, labeled cold or warm | No minute budget until the first deep run records one. An OOM or a disconnected agent during build is already a breach of the operating cap |
| Agent spend | Spend log, else transcript estimates labeled as such | Exploration (reads and greps) trends down as a share of tool calls. Record the share; do not set a fake token cap |

`xtask` is a workspace member, so `cargo test --workspace` runs its audits.
`.cargo/config.toml` maps `cargo xtask` to `cargo run -p xtask --`. On
`main` at the 2026-10-09 baseline the commands are `metrics`, `contracts`,
and `kits`. `theme` (hardcoded `Color32` constructors) is the same shape —
library audit, command, `xtask/tests/` assertion — on `feature/theme-sweep`.

## Backlog

Ordered. Each item names the saving, the risk, and the guard that keeps
quality. Article III: do this fraction, not a second architecture review.

### 1. Split `board.rs` and `board_agent.rs` along seams that already exist

Overnight, exploration was about 70–75% of ~3,300 tool calls.
`apps/slate/src/app/board.rs` is 434,600 characters and was opened 39 times
(estimated). `board_agent.rs` is larger (564,885 characters). Agents
re-read the whole file because the unit of navigation is the file, and two
agents cannot own one file.

`board.rs` already marks four regions (line numbers on `592f702`, banners
are the seam): tools and gestures (279), board state helpers (1153),
outline geometry (2498), painting (3082–10731, about 7,650 lines). Move
each banner into a module that `board` re-exports. Painting stays over
2,000 lines after that move; split it further only where a sibling module
already owns the behavior (`board_path`, `board_color`, `board_handles`,
`board_portal`). `board_agent.rs` already has `crosstalk` (3,481),
`life` (1,482), `outputs` (1,733), `schedule` (412), and `train_ux` (276).
Continue that split: session pump, card paint, generator.

`tests.rs` is the largest file in the tree (28,166 lines). Fold its split
into item 2, grouped by the module under test.

**Saving:** fewer full-file reads; parallel agents stop serializing on one
path. **Risk:** a move that quietly changes behavior, or a new helper that
copies an owner (Article XII). **Guard:** move-only commits, existing tests
green, dry-review before any new function. `scene.rs` and `atlas-shell`
chrome stay out of this wave.

### 2. Make the timing-dependent tests deterministic

Under load, four families fail and force a full-suite rerun, often with
`--test-threads=1`. Validation was about 20% of tool calls. Known sites on
`592f702`:

- Eraser settle — `thread::sleep` loops in `board_path.rs` tests, and the
  `settle_*` helpers in `tests.rs` (including
  `a_tab_switch_during_an_eraser_settle_never_shows_the_uncut_stroke`).
- Video pan scrub — `video_pan_scrubs_the_full_trim_and_does_not_journal`
  (playhead tolerance after click-to-play).
- Crosstalk — `frames_until` in `board_agent/crosstalk.rs` (5s deadline,
  10ms sleep). `tests.rs` has 68 `thread::sleep` hits.
- atlas-ai roots snapshot —
  `sources_consume_a_return_only_once_the_roots_arrive` (200ms sleep, 10s
  deadline).

**Saving:** one suite run stands. The 15-minute budget becomes reachable.
**Risk:** a fake clock that no longer observes the race. **Guard:** inject
a clock or call the existing settle hook; keep the same assertion. Do not
raise a timeout or add `#[ignore]`.

### 3. Turn written rules into xtask lints, then slim always-on context

Last night every serious defect the integration reviewer caught was a
written rule the building agent already had: Article XII duplicates,
Article II per-frame allocation and frame-loop file I/O, the cloud-file
rule, the secrets rule, an Article IV magic-value sentinel. The rules
work. The cost is ~8,900 always-on tokens on every turn, and detection
only at review.

Existing lints, same hook as contracts and kits: theme (Color literals) on
`feature/theme-sweep`, 24 allowlist entries at `aa4f27b`. Candidates, in
order:

- Frame-loop I/O and blocking calls (Article II; the stall rules in
  `AGENTS.md`).
- Allocation and tessellation in paint paths (Article II.2).
- Canvas text and size clamps (P0.9, `.cursor/rules/canvas-scale.mdc`).
- Duplicate functions (Article XII.3): a normalized body that appears
  three times fails the build. An allowlist row names XII.2 (a second
  interpreter) or XII.3 (a different reason to change) and is counted.

**Saving:** violations fail in the builder's test run, and the always-on
prose can shrink. **Risk:** a noisy lint that someone then disables, or an
allowlist that hides the debt. **Guard:** a new lint fails closed, lands
as an `xtask` test so `cargo test --workspace` runs it, and never ships by
relaxing an older lint. Allowlist growth without a report note is a
regression.

**Context slim, owner-ratified, after those lints exist.** `AGENTS.md`
(~5,660 tokens) and `.cursor/rules/constitution.mdc` (~1,060 tokens) are
the bulk of the ~8,900. Proposal: a short law card (the twelve one-line
articles plus the pushback sentence) and an `AGENTS.md` that indexes docs
instead of restating them. Performance and cloud-file essays already live
in `docs/performance.md`. A sentence leaves the always-on set only when a
lint enforces it or a one-line pointer remains. The owner approves the
draft before any deletion. This check does not edit `CONSTITUTION.md`.

### 4. Multi-agent operating rules

Five worktrees compiling at once produced four "Workspace Disconnected"
crashes and one out-of-memory build. Git and coordination were only ~4% of
tool calls; the damage was the parallel cargo.

- At most **three** concurrent cargo processes on one machine.
- When more than one cargo runs, set `CARGO_BUILD_JOBS=2`. A lone build may
  use the machine.
- Do not point several worktrees at one `CARGO_TARGET_DIR`. Evaluate
  `sccache` in a deep run before sharing any cache. A shared target
  directory with concurrent writers corrupts builds.
- Commit a green slice early on the feature branch so a disconnect keeps
  it. Do not commit a red tree to survive. Do not push unless the owner
  asks.
- Builder and reviewer are different agents. Model tiers are in the agent
  brief: Composer for easy, Grok 4.7 for hard, the strongest model for
  integration review.

**Saving:** the next batch finishes. **Risk:** a cap that is tighter than
the machine needs. **Guard:** the cap is on cargo, not on agents that are
only reading. The reviewer still reads the diff.

### 5. Log spend without logging secrets

Tool outputs were not in the transcripts, so the shares above are
estimates. Propose a JSONL line per agent turn:
`ts`, `agent`, `model`, `branch`, `tool`, `count`. Counts and tool names
only. No arguments, no file bodies, no command output. Secrets stay out
(the secrets rule). Until that log exists, the spend row says "not logged"
or "estimated", and this check does not invent a token total.

## Report template

`docs/health/reports/YYYY-MM-DD.md`:

```markdown
# Health report YYYY-MM-DD

**Tree:** `<commit>` on `<branch measured>`. **Mode:** Quick | Deep.
**Previous:** `reports/<prior>.md` or none.

## Metrics

| Metric | Budget | Previous | This run | Delta |
|---|---|---|---|---|

One line under the table for anything estimated, and for anything not measured.

## Regressions

## Top 3 recommendations

## For the owner

Amendments (draft only) and rule changes. "None" is a complete answer.

## Auto-fixed

Commit hashes, or "none".
```

Tables are for metrics only, including the top-files list. Recommendations are prose.
