# 05 — Release Time Report

Purpose: read back actual recorded release work and publication evidence.
[Shared rules](../../AGENTS.md) apply. This postfacto report cannot start a
build, tag, upload or release; those actions need their existing task authority.

## Keep the loop light

- Read existing fixed-candidate, CI, packaging, publication and verification
  receipts. Preserve recorded develop/patch/debug/build/launch work and failed,
  cancelled or superseded attempts rather than displaying only the final success.
- Keep 04 integration/review evidence separate from 05 stable publication.
  A successful build, tag, draft release or process is not a published artifact.
- Attribute every result to its actual source and binary/package identity;
  do not carry a pass across changed inputs without valid scope evidence.
- Store timestamps in UTC; render local time with timezone visible. Deduplicate
  repeated events and do not add overlapping intervals into elapsed time.
- Distinguish measured compiler/runtime/CPU durations from queue time, wall
  intervals and recorded model/chat activity. Never infer unrecorded activity.
- Unknown category durations are **not recorded**, not zero. Keep original
  timestamps, precision, units and links; report missing receipts honestly.
- Do not add automatic CI skips, retries, approval gates or full workspace
  builds through this reporting phase. Keep the static report inexpensive.

## Cheapest valid checks

- Check links, local-time rendering, identity/scope filters, overlap totals and
  deduplication against existing records; no build is needed to view a report.
- Verify that publication claims link recorded tag/source, version/channel,
  assets, package metadata and download/feed hash evidence where required.
- Show failed required gates and missing verification explicitly. A recorded
  isolated pass does not replace a failed full gate.

## Handoff

- Link actual candidate/build receipts, source/binary/package hashes, CI logs,
  public release/feed URLs, recorded download verification and remaining limits.
- Use the existing [release readiness rules](../../../../docs/dev-loop.md#release-readiness-and-fixed-candidates)
  and [distribution guidance](../../../../docs/distribution.md#artifact-reuse-and-promotion)
  for status. A report summarizes evidence; it grants no new execution authority.
