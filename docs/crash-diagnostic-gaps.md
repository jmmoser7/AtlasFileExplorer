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
- Capture a coarse graphics backend and Windows build number when available.
  Use a fixed allowlist and avoid hardware serials, display identifiers, driver
  paths, environment inventories, and loaded-module inventories.
- Add a structured indication of stale or truncated evidence. A present
  session checkpoint can be older than the failing action. Tests should cover
  termination between launch and first frame, a native fail-fast before the
  exception handler runs, and a recovered session with partial files.
- Add optional local repro export only for a user-selected synthetic workbook
  or redacted action sequence. Preview the exact content before any transfer.
  Keep ordinary crash reporting independent of workbook export.
