# Fast development loop

Speeds up edit → see-result without changing app runtime behavior.
This is **auto rebuild + relaunch**, not in-process hot-patching.

## Intent to running evidence

The implementation owner is accountable for the requested user journey, not
just a completed diff. Use the existing task record and, for native agent UI,
the [acceptance gate](../design/agent-interaction-studio/native-implementation/ACCEPTANCE.md),
[operational reference](../design/agent-interaction-studio/native-implementation/JS-UI-REFERENCE.md)
and [state ledger](../design/agent-interaction-studio/native-implementation/JS-UI-STATES.json).
Do not create a second specification or mark unexercised cases Pass.

1. **Identify intent and owner.** Briefly state the goal, primary ordinary
   entry-to-result flow, latest approved reference and capabilities to retain.
   Follow the existing [design authority](../design/agent-interaction-studio/native-implementation/design-intent.md#1-authority-and-the-actual-design-objective)
   for agent surfaces. Chat Train/Images resolve shared-pattern ambiguity only
   where the pattern applies; retain modality-specific native capabilities.
2. **Prove direction early.** Aim to identify the owner/reference and establish
   the existing fast loop within an initial 10–15 minute exploration budget.
   Show a coarse working vertical slice or representative production widget,
   paired with the approved state at matched viewport/DPI/zoom, before fanout
   or fine polish. Compare the whole journey and structure first. For a minor
   change, aim for this first comparison within the same user review window
   (about 20 minutes where the warm loop permits). These are checkpoints, not
   guaranteed build times. Report a missing running loop/evidence and the
   concrete obstacle before spending hours or expanding implementation.
3. **Adapt the same owner.** Once the slice follows the reference, adapt its
   shared production implementation to the remaining surfaces. Do not commission
   independent menu/card reinventions. Batch review findings by failing primary
   journey; defer speculative edge-case polish until that journey works.
   If the slice is wrong, stop expanding/polishing it and revise the approach;
   an independently authorized build may continue to provide review evidence.
4. **Close with the requested artifact.** Exercise the new binary's real entry
   flows, attach matched reference/native evidence and interaction outcomes to
   existing acceptance cases, and name the verifying owner. A production fixture
   proves only its component scope; a screenshot alone cannot prove behavior.
   When asked to launch latest, verify source identity, binary and visible window
   for the requested updated artifact. If fixes are still unbuilt, say so and
   keep the build moving; an older executable is not completion of that request.

Entry-only evidence never promotes the whole tool/journey to Pass. Empty cards,
headers or seeded answers do not prove multi-turn Chat, Text proposal/Replace,
revised Dashboard output or 3D modification. Mark unexercised outcomes Missing
(or unknown in narrative); prioritize the main end-to-end result before further
microgeometry. Validation gaps against a clear approved reference do not justify
spending the mobile approval budget.

Existing authorization persists through these checkpoints. When major new UI
intent is genuinely missing, seek early coarse direction asynchronously if the
user is available and continue independent work. If absent, keep continuation
within the approved reference-backed structure; the coordinator resolves routine
authorized choices without re-asking permission. Do not hide wrong-direction or
missing-evidence findings behind a long polish/build phase.

### Last-resort mobile layout escalation

Only request layout approval when a concrete decision remains genuinely unsure
after consulting authoritative intent. Across **all agents together**, allow at
most **four agent-initiated layout approval screenshot views in any rolling
24 hours**. Each view in a compound image counts individually. User-supplied
screenshots, internal inspection and report/PDF previews are excluded; do not
disguise approval requests as progress screenshots to bypass the limit.

Check and reserve the shared [approval request ledger](../design/agent-interaction-studio/native-implementation/ACCEPTANCE.md#mobile-layout-approval-request-ledger)
before sending. Record UTC timestamp, screenshot/view count, the one decision
and reply status there. Each request explains why existing intent is insufficient,
recommends one choice and offers a phone-friendly approve/reject decision.
Await an explicit reply before implementing that dependent ambiguous choice;
elapsed time is not approval. Continue independent authorized work. If the budget
is exhausted, leave that choice blocked and reuse approved structure elsewhere.
This is a last resort, not a recurring approval workflow or a reason to re-ask
for already authorized reference-backed work.

### Select the cheapest valid loop

Use the existing dev/test profiles, live tokens and production fixtures first.
For conversation controls, the existing fixture imports production widgets:

```powershell
cargo run -p atlas-shell --example agent_presence -- --conversation
cargo check -p slate --lib --tests
cargo test -p atlas-shell --lib <test-filter>
```

Select the affected package and `--lib`, `--bin` or existing integration-test
target as appropriate; dependencies still compile. A test-name filter selects
execution after the test executable is compiled, not a smaller compilation unit.
Finish cheap test-configuration checks and component review before heavy codegen.
Use the [live tuner](ui-tuning-workflow.md) for supported visual tokens; keep its
feature configuration stable instead of toggling it on every edit.

Freeze a coherent candidate at an integration/performance milestone, then run
the applicable consolidated release checks and normal executable acceptance.
Do not pay for release/LTO on every spacing edit. Required impact, contract and
CI checks remain required; reuse a previous result only with exact relevant
source/dependency closure, configuration and binary evidence. During an owned
build keep sources, flags, profile, features and target/cache stable; coordinate
any necessary invalidation with the sole build owner. No cache cleaning or
unmeasured DLL/framework/profile changes as a stall remedy. Independent crates
can narrow compilation but do not remove final relinking; extensions remain
subject to Constitution VII.8. Measure the existing loop before architecture work.

Changing the selected workspace package roots can change feature unification and metadata hashes even with the same profile and target directory. Reuse the measured canonical package set for retries; choose a different scoped loop intentionally, with its cache cost understood. The November assertion-only repair's narrower configuration check took 980.844 seconds, so it was not a faster loop. Test-only assertion changes need the affected unit configuration, not an automatic expansion to integration targets.

The 2 November finalized Slate test caches preserved name/length/mtime for 414/419 pre-LTO bitcode files but only 30/256 objects: observed backend output churn, not proof of wholesale source invalidation. Thin-local LTO fan-out is a hypothesis. Cargo's `lto = false` permits thin-local LTO; `"off"` disables it, and package overrides cannot set LTO ([profiles](https://doc.rust-lang.org/cargo/reference/profiles.html#lto)). Before DLLs or broad decomposition, run one future controlled pilot: verify support for `[profile.test.package.slate] opt-level = 0`, then compare the same bounded test edit against the unchanged baseline. Keep `cargo test --locked --jobs 2 -p slate -p atlas-shell -p slate-doc --lib --no-run`, root set, toolchain, features and target stable; separately charge the pilot's cold bootstrap and compare warm codegen/object reuse. This is a future experiment, not a current profile change or promised speedup; disabling optimization also changes generated code, so causality needs care. Compare runtime performance only on equivalent release builds. Continue using existing production widget fixtures for local UI work.

A prepared script or queued tool call is not running continuation. Before leaving a preauthorized long pipeline unattended, verify an actual OS controller PID/start receipt that continues across the launching tool boundary, persists gate failures, and advances the known success path without waiting for another chat turn. A temporary system-awake lease may prevent idle sleep only while that controller lives; release it at the end and do not override explicit sleep or change the power plan. Record any suspended interval as no active work, not compile or model time.

Treat egui Context `data(_mut)`, `input` and `memory` closures as lock-held: touch only the supplied plain state. Precompute Ui/Context getters outside them; in egui 0.31.1, even `ui.is_visible()` reaches `will_discard()` and re-enters the Context write lock. Diagnostic hooks follow the same rule. Fix the deadlock rather than increasing test timeouts or skipping cases.

### Bounded work and continuous handoffs

Apply [request scale](../.cursor/rules/request-scale.mdc) and the existing
[handoff guidance](../.cursor/agents/health-check.md#handoff). When delegation is
authorized, use a small team with one shared implementation owner, independent
review and a sole build owner; name file ownership and reserve capacity for the
critical next stage. Release idle workers rather than occupying concurrency.
Give each worker a thin packet: goal, named files/owners, approved state,
constraints, checks, next owner and exit criteria. Use the lowest-cost capable
worker available; expand context/capability on evidence. Do not default to full
history forks, whole-repo rereads or duplicated specification documents.

Every stage records active versus blocked status, the next owner, exit criteria
and authority to proceed. Await completion events instead of spending model
turns on minute-by-minute polling. With collaboration tools, `send_message` only
queues a message; use `followup_task` to restart a completed worker and verify
the handoff actually starts. Escalate concrete failures promptly. Do not claim
unattended progress while leaving an unresolved internal handoff. During long
builds report meaningful phase/status/evidence milestones and ETA variance;
application runtime/script progress exposes public outcomes, never private
chain-of-thought or invented activity.

Conditional planning targets for a bounded existing-UI batch with prepared
owners/fixtures and warm caches are 180 minutes total (15 scope, 60 implementation
including pilot, 30 focused validation, 60 final build/smoke, 15 launch), with a
28-minute writing target inside it (4 scope, 7 shared pilot, 10 adapters,
5 review/fix, 2 handoff). These are proposed budgets, not measured savings or
promises. Estimate new runtime integrations or global decomposition separately;
unbuilt foundation work is not completed preparation. Reassess scope/trajectory
at the first failed slice rather than spending the remaining budget polishing it.

Measure nonoverlapping command wall time separately from process CPU; track
source edits, output/context bytes or tokens, retries and rewrites separately.
The weekend audit's 16.8h is build-command wall overlap, 11.03h is event-proximity
activity (actual writing hours unknown), and 39.9 output tokens/gross Rust line
is an associated-response ratio, not necessary authoring cost. Cached context
counts are not billed spend or weekly quota. Preserve these distinctions when
claiming improvements; coordination/polling output is a cost to bound, not proof
of productive activity. Local audit receipts and limits: `target/two-day-audit-20261101/methodology.txt`,
`model-activity/methodology.txt` beneath it, and
`target/performance-recovery/20261031/weekend-ui-build-workflow.md`.

## Release readiness and fixed candidates

Apply this section to release requests and integration milestones. Normal focused
edits and documentation-only changes retain the proportional checks in
[request scale](../.cursor/rules/request-scale.mdc).

Keep these evidence stages explicit:

- **Studio/reference approval:** agreement on intended design and behavior.
- **Native review build:** a named executable with targeted interaction evidence;
  report its exercised scope and validation gaps.
- **Release-ready fixed candidate:** all required checks for that candidate have
  succeeded, with results tied to its source and build identity.
- **Published release:** the exact downloadable version, source and artifacts
  have been verified against public release metadata and package digests.

Partial validation never promotes a candidate to a broader readiness stage.
Before calling a release ready or estimating delivery, inspect the exact snapshot
against `main`, its base and included/excluded dirty scope, existing gate results,
and the actual required stages still to run. Give a remaining elapsed-time
estimate with known cold-build/repair uncertainty. If material repair emerges,
reclassify the work and report the changed scope and ETA promptly; continue
already-authorized repairs without adding a permission checkpoint.

Resolve the current remote `main` and published stable version before choosing
the next version. A branch-limited fetch refspec can leave `origin/main` stale;
use `git fetch origin refs/heads/main:refs/remotes/origin/main` when needed.
Preserve fixes already shipped in that base when reconciling older local work.

Assign one build owner and keep one candidate record in the existing task/evidence
location: SHA or dirty-file manifest/hashes, base, toolchain, profile, features,
root package set, lockfile identity, target directory, commands/results and hashes
of the binaries actually tested. Freeze candidate inputs during its build;
coordinate invalidation with that owner. Other agents must not start duplicate
local build jobs. Independent necessary jobs may proceed with separate ownership
and outputs. Reuse evidence only when the relevant source/dependency closure and
configuration remain valid for the claimed scope; record any invalidation.

Finish cheap checks, including type-checking the affected test configuration
where supported, before expensive executable generation. Collect all available failures from the existing logs and
batch repairs by root cause before the next expensive run. Preserve the actual
interaction and assertion intent; do not disable gates, weaken assertions,
raise timeouts or change global concurrency merely to get green.

Budgets are reassessment checkpoints, not permission to omit mandatory checks or
kill an active job solely because elapsed time exceeded an estimate. For a
suspected environmental/flaky failure, inspect logs first. Allow one justified
unchanged full-job retry while retaining failed evidence and the reason; an
isolated diagnostic pass does not turn the failed full gate green. Repeated same
failures require diagnosis and a justified correction rather than retry-until-green.
This does not bypass [execution-audit retry requirements](windows-builds.md#managed-machine-audit-workflow).

Use the existing [completion-event and bounded-handoff guidance](#bounded-work-and-continuous-handoffs)
with one status source and a designated monitor when polling is necessary. Honor
current communication requirements and report meaningful changes; multiple agents
must not poll the same job. Artifact reuse and any future promotion follow
[distribution's artifact rules](distribution.md#artifact-reuse-and-promotion).

## One-time setup

On Windows, install Visual Studio Build Tools with the **Desktop development
with C++** workload and a Windows SDK. From the repository root, select MSVC
for this checkout (this leaves other projects' Rust defaults unchanged):

```powershell
rustup toolchain install stable-x86_64-pc-windows-msvc --profile minimal --component rustfmt --component clippy
rustup override set stable-x86_64-pc-windows-msvc
cargo test --locked --workspace
.\scripts\build-release.ps1
```

The build/dev wrappers discover the installed Visual Studio C++ environment.
For a manually initialized terminal, run `. .\scripts\_msvc-env.ps1` first.
PDF previews use the checked-in `vendor/pdfium.dll` when launched from the
repository; keep it beside the executables if launching from another directory.

```powershell
cargo install --locked bacon
```

(`bacon` is a Cargo-adjacent watcher; it is not a project dependency.)

If Windows denies execution of generated helpers or compiler DLLs, follow
[Windows builds and execution auditing](windows-builds.md) before retrying.

## Day-to-day

From the repo root (MSVC/SDK env present if your machine needs it):

```powershell
bacon both           # Slate + File Atlas together (recommended for sessions)
bacon slate          # Slate only
bacon atlas          # File Atlas only
bacon check          # workspace check only (default job)
bacon test-slate     # slate crate tests on save
```

Or via wrappers (also set MSVC env on this machine when needed):

```powershell
.\scripts\dev-both.ps1
.\scripts\dev-slate.ps1
.\scripts\dev-atlas.ps1
```

Shortcuts inside bacon: `j`/`k` switch jobs, `r` re-run, `q` quit.
Each save kills the previous run and starts a fresh process — board state
is not preserved across edits (same as a manual relaunch). The `both` job
stops any prior `slate` / `native-file-atlas` processes before relaunching.

### Prefer the dev profile for logic work

Workspace `[profile.dev]` already tunes compile speed (`opt-level = 1` for
workspace crates, `2` for dependencies). Use `bacon slate` or
`.\scripts\run-both-dev.ps1 -App slate` while iterating on board tools,
hit-testing, and contracts. The shared runner builds the required video helper
in the matching profile before each app launch, including watcher relaunches.

Use release only when paint or frame-time matters:

```powershell
bacon slate-release
# or
.\scripts\run-both-dev.ps1 -App slate -Release
```

### Cargo aliases (no watcher)

```powershell
cargo slate          # = cargo run -p slate --
cargo atlas          # = cargo run -p native-file-atlas --
```

Raw Cargo app launches and these aliases do not build a dependency's executable.
First build `cargo build -p atlas-core --bin slate-video-worker`, adding
`--release` when launching a release app. Rebuild it after changes to the video
backend. The runner above handles this prerequisite on every launch and accepts
`-AppArguments` for app arguments; `-UiTuner` preserves the Atlas tuner feature.

Both aliases end in `--`, so everything after them goes to **the app**, not to
cargo. Atlas takes an optional folder to open as its first argument, which makes
one mistake quietly misleading:

```powershell
cargo atlas --release                    # debug build, and "--release" is
                                         # handed to Atlas as the folder to open
cargo run --release -p native-file-atlas # what you meant
cargo atlas "C:\path\to\folder"          # open a folder in the debug build
```

### Script wrappers

```powershell
.\scripts\dev-both.ps1    # both apps (bacon both)
.\scripts\dev-slate.ps1   # Slate only
.\scripts\dev-atlas.ps1   # File Atlas only
```

These prefer `bacon` (with MSVC env fixups when needed). They do not change
how the apps behave.

## Desktop shortcuts

Create or refresh Windows Desktop shortcuts after a successful release build:

```powershell
.\scripts\build-release.ps1
```

The fast two-app dev loop also refreshes the same shortcuts after it builds,
pointing them at `target\debug`. The release wrapper repoints them at
`target\release`. Both targets are stable paths. Desktop shortcuts, Start menu
shortcuts, and an existing taskbar or Start pin whose target is `slate.exe` or
`native-file-atlas.exe` are repointed at that build. The scripts do not add,
remove, or reorder pins.

## Chrome visual tuning (already live)

For spacing/colors/geometry of shared chrome, keep using the existing
feature-gated tuner — that *is* true live reload:

```powershell
cargo run --release -p slate --features ui-tuner
```

The activity timeline is an Atlas readout, so dial its tokens from Atlas —
`bacon atlas-tuner` keeps the watch loop while adding the dashboard.

See `docs/ui-tuning-workflow.md`. Board-tool feel constants still require a
rebuild unless later promoted onto that token path.

## What this deliberately does not do

- No Subsecond / dylib hot-patch of Rust functions.
- No change to release binaries, journals, or document models.
- No agent-facing in-process plugin loading (Constitution Art. VII).
