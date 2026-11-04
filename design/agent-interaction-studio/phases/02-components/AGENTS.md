# 02 — Rust Component

Purpose: review an isolated **real shared production widget**, including its
state and policy. [Shared rules](../../AGENTS.md) apply. The phase page is
currently an empty workspace scaffold until a native artifact is attached;
it does not mean a native harness has already been built.

## Keep the loop light

- Implement or build only specifically authorized element work. Call or
  narrowly extract the production owner; never make a separate Studio widget.
- Include the state/policy being reviewed. Use deterministic stubs only for
  external I/O, not fake picker, journal, permission or lifecycle behavior.
- Preserve production fonts, theme tokens, icons, renderer and scale behavior.
  Make any measured difference or unsupported case explicit.
- Keep the harness and dependencies minimal. Do not import the whole Slate
  app/library or unrelated providers, WebView or Rhino to show a small widget.
- Reuse the established toolchain, profile and warm library target. No profile
  experiments, DLL schemes, cache clearing or duplicate build jobs for previews.
- Measure cold versus warm commands separately. Under 30 seconds for the warm
  loop is a proposed goal, not a promise, timeout or permission to skip checks.

## Cheapest valid checks

- Review the changed owner and dependency closure, format affected Rust, and
  type-check the relevant library/test configuration before executable work.
- Run the smallest meaningful component tests and inspect the actual native
  widget for changed states and gestures. Reuse valid receipts after no
  relevant input/configuration change; no full workspace run for every edit.
- Report actual command exits, build time and limitations. Component tests
  certify this component only, not app integration or ordinary native entry.

## Handoff

- Attach the native artifact/captures with source and binary hashes, command,
  toolchain/profile/features/target, fixtures, theme/DPI/zoom and entry actions.
- Record compiler/runtime measurements separately from elapsed wall time;
  retain failures and unknowns. Name the shared owner and remaining cases.
- Reuse this same production component in 03 and integration. Follow the
  [native acceptance gate](../../native-implementation/ACCEPTANCE.md) when
  claiming native journeys; phase navigation adds no new approval gate.
