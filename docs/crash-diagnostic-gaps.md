# Slate crash diagnostic gaps

Slate's launcher supervises the window process and saves completed reports under
Advanced settings → Crash reports. It records the exit code, a bounded stderr
tail, and session checkpoints it can attribute to the same launch. The Rust
panic hook records caught as well as unhandled panics. On Windows, a best-effort
exception handler records the exception code, module-relative location, and
access operation when available. A subsequent launch can recover an interrupted
report after confirming its processes have ended.

The local report also records the release channel, supervision outcome, and
whether native exception metadata, a session checkpoint, or stderr was
captured. Runtime is available for an observed process exit. Windows exit
statuses and a checkpoint can help narrow down the cause. An unobserved exit
stays unknown, even when both processes have ended.

The optional summary in Suggestion Box contains the release version, platform,
exit information, evidence flags, last known Slate view, bounded frame timing,
and recognized native exception fields. The user reviews it before sending.
The full report and its stderr, panic payloads, stack traces, absolute addresses,
paths, and workbook information stay local.

## Observed evidence

A bounded read of the twelve most recent completed reports from one Windows
development installation found native exception details in three and an
attributable session checkpoint in five. The stderr tails were empty in all
twelve. The sample measures evidence availability across crashes and launcher
interruptions on that installation.

In a Windows desktop check of the installed Slate 1.0.12 build, the scratch
window showed corrupted geometry and stopped responding. We captured local
triage while the process was still running, then stopped the scratch process.
The opt-in summary correctly recorded the observed exit code (`-1`, from the
forced stop). It had no native exception, launch-attributed session checkpoint,
or stderr. That evidence describes the exit. The preceding loss of UI
responsiveness was only visible in the live run.

The local stack showed Intel's Windows OpenGL driver spinning in WGL
`DrvDeleteContext` while waiting for a worker. This locates the wait at the
time of the sample. The trigger for the earlier graphics corruption remains
unknown.

## Follow-up work

- Separate build identity from the semantic version. Embed the source revision
  and package channel in release artifacts, retain matching symbol files for
  every public installer, and test symbol lookup against a generated native
  crash using its module-relative location.
- Add an inexpensive, local startup milestone for graphics, window creation,
  workbook recovery, and update handoff. Keep the last completed milestone in
  the report so a failure before the first session checkpoint is diagnosable.
- Keep a small local record of native worker exits and their ownership. Use
  fixed subsystem names and status codes so a decoder, WebView, or 3D helper
  that fails independently can be distinguished from the Slate window process.
- Record graphics information for the window process and launch that produced
  the report. Include the Windows build, active graphics backend, coarse
  renderer family and driver version when available, and whether software
  rendering was active. Refresh the record if the actual renderer changes.
  Mark unavailable fields as unknown. Use a fixed allowlist and avoid hardware
  serials, display identifiers, driver paths, environment inventories, and
  loaded-module inventories.
- Let the supervisor preserve a bounded local record of the last UI progress
  signal and graphics phase for that launch, including how old the signal was
  at exit. Store evidence of stalled UI progress separately from the exit
  outcome, which can remain unknown. Use coarse phases such as frame rendering,
  graphics context creation, and teardown. A detected stall records evidence
  while leaving the process under the user's control.
- Add a structured indication of stale or truncated evidence. A present
  session checkpoint can be older than the failing action. Tests should cover
  termination between launch and first frame, a native fail-fast before the
  exception handler runs, a recovered session with partial files, and a
  synthetic hang followed by forced termination. Check that the forced exit
  remains distinct from the stalled UI and that another launch's evidence
  cannot be attached to the report.
- Keep any additional hang capture user-initiated and local. For the optional
  Suggestion Box summary, show the exact allowlisted fields, including their
  age and unavailable values, before an explicit send. Raw dumps, memory
  contents, and detailed stacks stay local and are excluded from all
  submission payloads.
- Add optional local repro export only for a user-selected synthetic workbook
  or redacted action sequence. Preview the exact content before any transfer.
  Keep ordinary crash reporting independent of workbook export.
