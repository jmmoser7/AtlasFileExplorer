# Windows builds and execution auditing

## Toolchain

Use stable Rust MSVC, Visual Studio C++ Build Tools (or a full Visual Studio
installation with C++ tools), and a Windows SDK. See `dev-loop.md` for setup.
Use a repository-local `rustup override`, preserving other projects' defaults.
`scripts/_msvc-env.ps1` discovers Visual Studio through `vswhere`; do not restore
hard-coded compiler/SDK paths in Cargo configuration or scripts.

## Managed-machine audit workflow

On the September 2026 setup machine, an automated auditor reviews newly generated
executables and DLLs. Cargo has reported `Access is denied (os error 5)` for
`target/release/build/*/build-script-build.exe` and procedural-macro DLLs under
`target/release/deps`. DLL denial can also produce invalid-metadata or missing-crate
errors. Inspect the underlying error before treating these as source defects.
The user reported working with IT to approve the checkout's `target` directory;
do not assume that approval is complete or applies on another machine.

To expose as many independent build artifacts as possible in one pass:

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

## Completion checks

After audit blocks clear, require successful exit codes from the build and tests.
For the release artifacts above, run `cargo test --locked --release --workspace`
to reuse the same profile, then `scripts/install-shortcuts.ps1 -Configuration Release`.
Launch the actual release executable with the repository as its working directory
so `vendor/pdfium.dll` can be found. Verify the window opens and stays responsive;
do not equate compilation, a process ID, or a shortcut with a successful GUI test.
Report any unperformed tests or blocked launch explicitly.

Build helpers and compiler-plugin DLLs do not belong in the end-user installer.
See `distribution.md` for signing, managed deployment, and installer smoke tests.
