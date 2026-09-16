# Slate document validation — 2026-09-15

Current `crates/slate-doc/src/lib.rs` and all its unit-test modules were compiled afresh using `rustc --test -O -C lto=thin`. Existing matching release `serde` and `serde_json` libraries were reused; no product dependency or security settings were changed. Exact source, compiler, dependencies, fingerprints and arguments are in `slate-doc-test-provenance.txt`. The repeatable runner is `target/stabilization-2026-09-15/test-slate-doc-direct.ps1`.

Results:

- Default suite: **123 passed, 0 failed, 1 ignored** (27.42 seconds). This includes all 17 staging tests, the current derived-lookup regressions, and preexisting document, lease, geometry and spatial tests.
- Explicit ignored lookup benchmark: **1 passed**, 123 filtered out (9.18 seconds).
- One million warm indexed lookups on a 10,000-node scene: **50.8271 ms**.
- One million equivalent baseline linear lookups on that scene: **9.0832706 s**.

This is an optimized unit-test build of the current pure document crate, not a completed Cargo workspace test run. The lookup result measures only the isolated lookup operation, not native frame time, paint, portals, uploads or user-perceived responsiveness. The app build was running concurrently, so these timings are a same-run comparison under that machine load rather than an isolated benchmark certification.

The first direct link omitted thin LTO and failed against the cached release archives; matching the workspace's thin-LTO profile fixed linking. Windows initially denied the sandboxed executable launch. The same executable then ran successfully through the automatically approved unsandboxed execution mechanism. No binary relocation, security exclusion or policy change was used.

Staging coverage includes wrong-workbook and unsaved-target refusals; later acceptance from the correct saved workbook; stale Patch/Remove conflicts preserving human state and undo; sequential proposal validation; durable decision replay suppression; interrupted/unreadable result recovery; initial result-write failure; invalid filename ids; and an injected failure of only the final Accepted confirmation after the Applying marker and journal commit.

The persistence protocol deliberately does not promise an atomic disk-plus-memory transaction. An Applying marker remains an explicit recovery case after an interrupted acceptance, and accepted edits still follow the normal workbook save lifecycle. See `docs/agent-link-contract.md`.
