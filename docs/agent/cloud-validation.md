# Cloud handoff and verification

This file owns cloud dispatch and validation procedure. `AGENTS.md` owns the
platform and task split; [health guidance](../health/README.md#4-multi-agent-operating-rules)
owns machine concurrency; the [health-check brief](../../.cursor/agents/health-check.md#handoff)
owns model allocation. Card-specific scope and review authority still apply.
Windows setup and execution denials stay in [windows-builds.md](../windows-builds.md).

## Before paying for a cold build

1. Inventory the current reports and existing fixes once. Keep one request
   ledger, with each report's owner, implementation, evidence, and open work.
   Give workers narrow file ownership and known locations; follow
   [reading.mdc](../../.cursor/rules/reading.mdc), rather than repeating discovery.
2. Confirm the execution surface. Workspace subagents share this machine;
   a cloud job requires an actual cloud environment. Check the installed CLI's
   help, `codex login status`, and `codex cloud list --limit 5` before dispatch.
   Environment IDs are separate from ChatGPT project IDs. The owner supplied
   AtlasFileExplorer environment `6acceb0f4b3c8191bafb47522fcbbd23` on 2026-10-13;
   verify access rather than assuming that identifier remains usable.
   Inspect existing CI before paying for another validator: a draft PR can run
   the repository's existing hosted tests with retrievable logs, including the
   Windows runtime suite when configured. Confirm workflow triggers, checkout
   SHA, prerequisites and publishing conditions rather than assuming a branch
   push runs tests. Inspect concurrency and cancellation too: release workflows
   may serialize PR checks, and pushing a newer commit need not stop the older
   run. Distinguish that queue from execution before dispatching more work.
   CLI, Git transport and connector authentication are separate;
   use an already authorized connector when the CLI is signed out, without
   extracting credentials. Record the actual merge SHA tested by PR workflows.
3. Finish cheap preparation before the expensive integration run: compile
   where available, check formatting and existing audits, validate edited JSON
   and UTF-8, and include required untracked fixtures referenced by `include_str!`.
   Record `rustc -V` and `cargo -V`, and use the CI lint flags where applicable.
   An older local Clippy can miss new stable-toolchain warnings promoted by CI's
   `-D warnings`; a Windows check also cannot establish Linux cfg hygiene.
   A clean compile does not establish runtime behavior.
4. Freeze and push the intended snapshot on a dedicated branch. Record its
   full SHA and verify the remote branch points there. Uncommitted Windows
   changes are not uploaded by cloud dispatch. Preserve the owner's branch,
   work, and real staging index; use an isolated checkout or a separate temporary
   index when transferring an authorized dirty snapshot. Stage selected paths,
   excluding secrets and generated outputs. Compare through that isolated index
   too: ordinary working-tree diffs can report snapshot files as deleted when
   those files are untracked in the owner's real index. Keep the submitted ref fixed until
   the job confirms its HEAD; give later snapshots a distinct ref if necessary.
   Preserve baseline source line endings in the review snapshot. If a temporary
   index normalizes them, verify that only the intended content differs and
   distinguish EOL-equivalent source from byte-identical Git objects. Do not
   rewrite the owner's working files or advance an active dispatch ref merely
   to remove review noise.

## Dispatch an identified snapshot

Use the authorized environment and an explicit pushed branch, for example:

```text
codex cloud exec --env <environment-id> --branch <pushed-branch> <task-prompt>
```

Verify model/effort flags against current CLI help; do not assume a model alias,
price, resume command, or environment capability from an earlier session.
Choose according to the existing model-allocation owner linked above.

Every task prompt includes this handoff, with real values substituted:

```text
Purpose: implementation or validation; named reports and owned files.
Expected HEAD: full pushed commit SHA; branch and environment.
The pushed branch selects the source. A local cloud branch named work is
normal and authorized; its symbolic name is not a provenance failure.
Read AGENTS.md and the named request ledger. Read CONSTITUTION.md when
required by AGENTS.md, the user, or the assigned task; use relevant sections.
Before building, report git rev-parse HEAD and git remote -v.
If HEAD differs, report the mismatch and stop validation of that snapshot.
Only the commit SHA mismatch stops this check, not a local branch-name difference.
Do not silently test the checkout you happened to receive.
Commands and budget: affected packages, checks, Cargo jobs, runtime prerequisites.
Evidence: exact HEAD, command, exit, counts, logs, elapsed time if captured.
Preserve every failing assertion's diagnostic output, not just test names.
Repair assigned failures; keep assertions and existing audit ceilings intact.
Return reviewable changes and a report; follow the existing branch/PR rules.
Answer provenance/status questions, then continue unless the user cancels.
```

A cloud checkout may have **no `origin` remote**. Do not submit a job that
depends on fetching later edits without establishing that capability. Prefer
starting directly from the finished pushed snapshot. If a required fetch fails,
report its exact command/error and source mismatch before spending time building.
Do not describe a stale run as verification of newer local fixes.

## Make a run useful even when it fails

- Batch tests for the affected packages in one prepared environment. Use
  `cargo test --locked ... --no-fail-fast` so one failing binary does not hide
  independent results. On Linux, retain the Cargo exit status through `tee`
  with `pipefail`; a successful log-writing command is not a successful test.
- Save full stdout/stderr in the job and return assertion diagnostics in the
  report or another retrievable artifact. A VM-only log path is insufficient
  if the coordinator cannot retrieve its content. Include compile errors,
  panics, expected/actual values, and counts per test binary where relevant.
- Wait on a running Cargo command through the execution tool's process-session
  protocol when available, preserving its real completion and exit. Shell
  `pgrep` loops can match their own supervisory command, and `kill -0` can stay
  true for a zombie; PID existence alone does not prove useful work continues.
  Keep waits bounded and inspect new output rather than layering long sleeps
  around an uncertain process identity.
- Run required commands even when optional telemetry tools are missing. Cloud
  images may lack `/usr/bin/time`; use a shell built-in or leave elapsed time
  unmeasured. Recover routine wrapper failures without changing the required
  Cargo command. A stop-on-error instruction must not turn a missing logging
  or timing convenience into abandonment of the requested validation.
- Check required runtimes before a long suite. Missing Pdfium, denied downloads,
  and Windows execution blocks need exact evidence and a blocked-check label;
  they do not justify ignoring tests or changing expected behavior.
- Update a fixture only when the current approved interaction explains the
  mismatch. Keep its behavioral assertion; reproduce unexplained failures
  before changing production code or widening waits.
  For asynchronous preparation, await the current captured revision's result,
  not merely a previously published filename; a later operation in that same
  frame can invalidate the earlier result. Assert that the revision stays
  unchanged during a no-change cache test. For paint-based probes, identify the
  owning UI's clip and semantic label, since overlapping parent text can share
  screen coordinates without belonging to the control being tested.
- Reuse an environment and its build cache when the available tooling supports
  it. Give runtime repair to a worker that can execute the failing tests in
  that checkout, especially when local execution is blocked. Supply the known
  diagnostics and trace locations; source-only review can support the repair
  but cannot identify every event-routing failure. Let that worker iterate
  focused checks and then run the integrated suite in the same warm environment.
  Do not launch duplicate cold workspace builds merely because status is quiet.
  Repeat green checks only for a new change or concern.
- Review returned diffs before applying them to the owner's dirty checkout.
  Passing the reported positive cases does not prove that new input routing
  preserves overlay ownership, cancellation, or foreground occlusion. Review
  those paths against the existing interaction owners and retain meaningful
  negative regressions when changing capture or layer behavior.
  Run the relevant native compile check for code changes; final Windows GUI
  verification remains a separate result.

## Monitoring and reporting

`pending` does not distinguish queued, building, testing, or awaiting attention.
Cross-check status with `codex cloud list --json`, matching the exact task ID.
In the 2026-10-13 session, `codex cloud status` printed `[PENDING]` for a task
whose JSON status and interactive task list were `error`. Do not use that
status command or its exit code as the sole polling condition. If they disagree,
stop the pending loop and inspect the task page/list; record both observations.
Two such tasks later returned `ready` with reports and source diffs; one
recovered more than thirty minutes after `error` was observed. The CLI exposed
no failure reason; a subsequent older recovery returned more than an hour after
an observed `error`. A timeout alone does not establish that execution stopped.
Use bounded rechecks and the live task page when available before replacing a
disputed job. If recovery work is necessary without that evidence, record the
possible overlap; keep late results tied to their SHAs and review competing diffs.
An `error` list entry or a red information icon does not by itself prove that
execution has stopped: the owner later supplied ongoing command/edit activity
from a task with that state. Likewise, `Could not open file` beside a cloud
source preview is not a Cargo failure diagnostic. Reconcile copied activity by
its task prompt and source SHA, especially when it arrives as a reply to an old
question, and require the relevant command result before classifying a failure.
Use bounded waits and back off unchanged polls; avoid sending the same status
update each minute. Ask the user to inspect a task only when their action can
resolve a specific blocker unavailable through the tools. CLI task completion
does not prove the requested commands ran: inspect the report and exit codes.

Keep the task ID, submitted SHA, submission time, observed completion time,
model/effort selection, commands, results, and accessible evidence location in
the request ledger/report. Label compile-only, executed, blocked, and GUI checks
separately. Use the [spend measurement rules](../health/README.md#5-log-spend-without-logging-secrets)
for time and usage; overlapping job hours are not billable model hours.

These additions follow the 2026-10-13 feedback batch. The
[stale-snapshot report](../requests/2026-10-13/cloud-slate-followup.md) records
the missing remote and retained failures; the
[shared validation report](../requests/2026-10-13/cloud-shared-final.md) shows why
runtime results and review of the returned fix were necessary.
