# 04 — Integration Time Report

Purpose: read back actual recorded integration work for an element or the
shared Studio audit. [Shared rules](../../AGENTS.md) apply. This is a postfacto
report, not authority to develop, build, launch, approve or publish anything.

## Keep the loop light

- Read existing receipts/logs for the reported source and work scope. Show
  recorded develop, patch, debug, check, build, launch and interaction work;
  retain failed and cancelled attempts with their actual outcomes.
- Report only observed activity. Do not invent records to fill a chart, infer
  model reasoning time from silence, or use a sample as a real measurement.
- Store timestamps in UTC and render them in the viewer's local time with
  timezone visible. Preserve source timestamps and their precision.
- Deduplicate the same event by stable identity. Do not sum overlapping
  intervals into elapsed time; distinguish union wall time from recorded
  per-process CPU/compiler/runtime durations and model/chat activity intervals.
- Mark unknown categories or durations **not recorded**, never fake zero.
  Distinguish queue/wait from execution only when the evidence records it.
- Keep source/log/receipt edits with their existing owner. This page reads
  those artifacts; it does not trigger builds or silently rewrite history.

## Cheapest valid checks

- Verify report links, timestamp parsing, local rendering, scope filters,
  deduplication and overlap totals against a small set of actual records.
- Check source/binary attribution and units; absence of evidence stays visible.
  Documentation/report rendering needs no Rust or full workspace build.
- Keep failed results alongside later successes. A green summary must not
  conceal an unsuccessful attempt or promote a component result to app evidence.

## Handoff

- Link the actual source manifest/SHA, binary hash where available, commands,
  configurations, logs, outcomes and limitations used by the report.
- Distinguish integrated/reviewed candidate evidence from stable publication.
  The existing [release readiness rules](../../../../docs/dev-loop.md#release-readiness-and-fixed-candidates)
  determine that status; elapsed time alone proves neither readiness nor parity.
