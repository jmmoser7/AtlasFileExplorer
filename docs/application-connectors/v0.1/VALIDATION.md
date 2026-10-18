# Candidate validation record

16 October 2026. **Not release-ready: compilation and executable tests remain
blocked.** Source review and static checks do not establish runtime correctness.

## Build evidence

The initial release check of `atlas-connector-runtime` and `atlas-packs` exited
101 before checking the connector code. Windows refused to execute these newly
generated dependency helpers with `Access is denied (os error 5)`:

- `target/release/build/serde-8752bc1ff8ce84e8/build-script-build.exe`
- `target/release/build/ref-cast-fd6914a3ef0ac0be/build-script-build.exe`

Following the repository's documented audit-batch workflow, one broader build
was attempted with the same target directory and release profile:

```powershell
cargo build --locked --release -p atlas-connector -p atlas-connector-runtime -p atlas-packs -p slate --all-targets --keep-going
```

It exited 101, again before checking the connector code, with execution denied
for the serde helper above and:

- `target/release/build/serde_json-9d89698b4a73f49b/build-script-build.exe`

The exact logs remain in ignored local build evidence:
`target/connector-evidence/initial-check.log` and
`target/connector-evidence/audit-batch-build.log`. These are observed execution
failures, not evidence of the contents or status of an auditor's queue.

No protection settings were changed, and no artifacts were cleaned, renamed or
relocated. Per [Windows builds](../../windows-builds.md), retry after approval or
an explicit user request, preserving the profile, toolchain, lockfile and target.

## Completed checks

- Locked Cargo metadata resolves the workspace and both new crates successfully.
- Rust source formatting/parsing checks pass for the connector crates and pack
  integration files.
- Cargo manifests, lockfile and the mock pack JSON parse successfully.
- New connector Rust files stay below the 500-line cap. The edited Enscape host
  remains within its existing legacy ceiling.
- Documentation links resolve locally; the artifact address derivation matches
  its published fixture; the real PNG fixture's structure and CRCs validate.
- Independent reviews covered protocol admission, lifecycle races, transport
  identity, native thread ownership, asynchronous hiding and duplicate owners.

Retained static evidence lives beside the build logs in `metadata.json`,
`rustfmt-check.log` and `static-checks.json`. The working tree already contained
extensive unrelated changes; those are not claimed as connector work.

## Checks still required

Run the exact release commands in the [candidate guide](README.md) after the
execution block clears. They must establish successful compilation, passing
protocol/pack/runtime tests, generated-schema consistency, the separate-process
mock exercise and Slate integration compilation. The new tests are written but
have **not run**. Public schemas and JSON message fixtures are deliberately
generated from the Rust owner; they have not yet been emitted by the blocked
exporter.

Then verify native restoration on Windows, including a real foreign UI thread,
and the responsive companion-window fallback. The current surface tests exercise
real Win32 windows but do not establish that an arbitrary commercial application's
full interface can be safely reparented.

Subsequent UI acceptance must cover explicit connector activation, capture
journaling, poster/pointer export and missing-connector workbooks. Rhino plugin
acceptance belongs to the independent project after this version is frozen.
