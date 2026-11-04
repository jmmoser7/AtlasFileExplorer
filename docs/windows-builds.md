# Windows builds and execution auditing

## Toolchain

Use stable Rust MSVC, Visual Studio C++ Build Tools (or a full Visual Studio
installation with C++ tools), and a Windows SDK. See `dev-loop.md` for setup.
Use a repository-local `rustup override`, preserving other projects' defaults.
`scripts/_msvc-env.ps1` discovers Visual Studio through `vswhere`; do not restore
hard-coded compiler/SDK paths in Cargo configuration or scripts.

## Preserve the working build

Build configuration changes are a **last resort**. Use the established release
entry point (`scripts/build-release.ps1`) or the existing focused app invocation.
Preserve the toolchain, target triple/directory, profiles, linker, Rust flags,
features and lockfile while repairing application code. Dependency or feature
changes can also create new build helpers and DLL variants without changing a
profile; assess that impact before adding them. Change configuration only for a
demonstrated requirement the existing setup cannot meet, and record the reason,
exact delta and rebuild/approval impact first. Required tests still run.

An execution denial is not a reason to change flags, dependency versions,
profiles or output locations, clean caches, run elevated, or disable protection.
Keep logs and diagnose the enforcing control. For recurring generated binaries,
request one durable IT-approved development policy covering EXEs **and** DLLs,
including build helpers, procedural macros, tests and app outputs. Individual
file hashes change after legitimate rebuilds; they are diagnostic evidence, not
a sustainable development approval scheme. IT owns the policy and its scope.

Before a heavy build, record free space on the local output volume, the target/cache being reused, and a budget based on measured growth. Watch capacity during the owned command and stop only its verified process tree before space becomes critical. An ENOSPC failure is an environment failure; preserve successful artifacts, logs and the last completed cache before retrying the same command. In the 1 November recovery, scoped NTFS compression of inactive generated `.rlib`, `.rmeta` and finalized-session `.o` files reclaimed about 5.5 GB while preserving logical hashes, lengths, mtimes and hardlink identities. That is a separate, reversible capacity operation with explicit owned paths and before/after proof, not permission for blanket cleaning or directory-wide compression policy. Exclude source and protected applications/PDBs; never retry a rejected cleanup through another mechanism.

After the ENOSPC incident, a retry reported compiler-generated LLVM anonymous-symbol linker failures. An exact, reversible quarantine of only the Slate test incremental namespace preserved all dependencies, release artifacts, flags and source; the subsequent canonical build linked successfully in 2024.703 seconds. This is consistent with invalid incremental work products and a fresh rebuild resolved the link failure; it does not prove a sole cause or justify broad cache removal.

## Managed-machine audit workflow

On the September 2026 setup machine, an automated auditor reviews newly generated
executables and DLLs. Cargo has reported `Access is denied (os error 5)` for
`target/release/build/*/build-script-build.exe` and procedural-macro DLLs under
`target/release/deps`. DLL denial can also produce invalid-metadata or missing-crate
errors. Inspect the underlying error before treating these as source defects.
The user reported working with IT to approve the checkout's `target` directory;
do not assume that approval is complete or applies on another machine.
OS error 5 alone does not identify that auditor, App Control, AppLocker, an
endpoint product or a filesystem permission as the cause; correlate logs before
naming one. A local process-start failure does not establish a firewall block.

For an IT-requested one-time artifact inventory, the following broader build
exposes independent targets. It is not the default release command, and must not
be used merely to try another build shape after an execution denial:

```powershell
. .\scripts\_msvc-env.ps1
New-Item -ItemType Directory -Force target | Out-Null
cargo build --locked --release --workspace --all-targets --keep-going *> target\audit-batch-build.log
$batchExit = $LASTEXITCODE
Get-Content target\audit-batch-build.log -Tail 80
Write-Host "Cargo exit code: $batchExit"
```

`--keep-going` continues independent work after errors; it does not bypass
security or guarantee submission to an auditor. Some dependencies cannot build
until an earlier helper is approved, so further passes may be required. A broader
target set can generate additional feature variants with distinct hashes.

- Preserve `target`, the lockfile, profile, and toolchain between audit retries.
  Do not use `cargo clean` or change flags simply to work around a denial;
  rebuilding can generate files requiring fresh review.
- Record exact denied paths and retain the build log under ignored `target/`.
  Generated-file counts are not counts of remaining approvals.
- Retry after approvals or when the user requests it. Do not run a tight retry
  loop, disable protection, change security policy, or relocate/rename artifacts
  to evade the auditor. Policy changes belong to the user/IT.
- Report what Cargo attempted, not an unverified claim about the auditor's queue.

## Reclaiming disk from `target`

Cargo never deletes superseded artifacts. Each compiler, feature, or source
path change leaves another hashed copy in `target\<profile>\deps`, and the
incremental cache grows with every session; worktrees multiply both. Run
`scripts\prune-target.ps1` (add `-DryRun` to preview, `-Target` for a worktree).
It removes the incremental cache and older copies of this workspace's own
artifacts, and never third-party outputs, build scripts, or proc-macro DLLs,
so it creates nothing new for the auditor. On 25 September 2026 it took the
main `target` from 112 GB to 16 GB. Give each worktree its own `target`;
a shared one returns stale test results across checkouts.

## Completion checks

For release requests, use [release readiness and fixed candidates](dev-loop.md#release-readiness-and-fixed-candidates)
to distinguish targeted review evidence from completed release gates and to record
the candidate, build owner, remaining stages and valid evidence reuse.

After audit blocks clear, require successful exit codes from the build and tests.
For the release artifacts above, run `cargo test --locked --release --workspace`
to reuse the same profile, then `scripts/install-shortcuts.ps1 -Configuration Release`.
Launch the actual release executable with the repository as its working directory
so `vendor/pdfium.dll` can be found. Verify the window opens and stays responsive;
do not equate compilation, a process ID, or a shortcut with a successful GUI test.
Report any unperformed tests or blocked launch explicitly.

Review launches must use the normal Windows user execution context, requesting
the required sandbox escalation through the official tool. A responsive process
does not prove a targetable visible window or working child services. On 2 November,
[restricted Codex discovery](../target/studio-native-validation/r5-native-acceptance-20261102/model-catalog-environment-receipt.json)
ended at initialization with EOF; version/help reported a missing-home warning.
Authorized escalated discovery returned eight models in 1.53 seconds. The unchanged R5 executable's
[normal launch](../target/studio-native-validation/r5-native-acceptance-20261102/normal-environment-launch-receipt.json)
then showed [eight Model choices](../target/studio-native-validation/r5-native-acceptance-20261102/37-normal-model-list-eight-agentic.png)
instead of “Choose model.” The earlier native cached error was not observed.
Follow the active Computer Use skill's official target selection and recovery;
preserve running user windows. Do not bypass controls, read private home files,
change global environment/auth configuration, or hardcode models. These local
receipts under ignored `target/` are ephemeral evidence.

Build helpers and compiler-plugin DLLs do not belong in the end-user installer.
See `distribution.md` for signing, managed deployment, and installer smoke tests.
