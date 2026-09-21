# Windows distribution and updates

Download the **Setup.exe** asset from
[the latest stable release](https://github.com/jmmoser7/AtlasFileExplorer/releases/latest).
The one-time, per-user installer includes Slate and File Atlas. Both appear in
Start; Slate also gets a desktop shortcut and registers `.slate` on first use.
Windows x64 is the supported distribution target.

Installed copies check their release feed in the background on launch. A newer
version opens the shared update window; **Later** defers it for this session.
**Advanced → Software updates** checks again manually. Offline checks never
prevent opening documents. Source-tree builds do not check the release feed.

Download while working, save every open workbook, then choose **Update and
restart**. The initiating app closes through its normal lifecycle. Other Slate
and File Atlas windows must be closed by their users before installation starts.
The coordinator waits up to 30 minutes; it never terminates them. Windows file
sharing guards keep new instances out during replacement. Slate refuses to close
with dirty workbooks, and the update action waits for active file operations and
exports. The app reopens after installation; saved workbooks remain available
through Home/recents. Open tab layout is not restored by the updater.

## Release channels

- **Stable:** explicitly publish a `vX.Y.Z` tag matching `workspace.package.version`
  in `Cargo.toml`. The Windows workflow tests, builds, packages, verifies and then
  publishes an immutable versioned release. Friends on stable receive that version
  on their next launch. Set the next workspace version before tagging again.
- **Preview:** every successful `main` build publishes `X.Y.Z-preview.RUN_NUMBER`
  to the reserved rolling `preview` release. Install its Setup.exe to opt in.
  Preview has a separate installation ID and shortcuts. It does not replace the
  stable installation, though both use the user's existing app settings/cache.
- Pull requests build/test/package artifacts without publishing. Stable clients
  use GitHub's latest stable download endpoint; previews use the dedicated tag,
  so preview volume cannot hide a stable release.

The public release feed needs no login, embedded tokens or separate server.
Do not make the distribution repository private without providing an accessible
replacement feed. GitHub Actions uses its scoped `GITHUB_TOKEN` to publish.

## Package and dependencies

`scripts/package-windows.ps1 -Version 0.1.0 -Channel stable` is the same packaging
entry point used in CI. It requires Rust/MSVC, .NET 9 and the pinned
`dotnet tool install --global vpk --version 1.2.0`. Users need none of these.
Outputs live under a fresh `target/distribution/` directory. Only explicitly
selected assets ship; no workbooks, local settings, API keys or developer logs.

The suite includes both release executables, PDFium 151.0.7920.0 and its license
bundle, Node 24.21.0/npm, the Cursor sidecar's source/manifests, and Rust dependency
license texts. Archives are pinned by SHA-256. Velopack bootstraps WebView2 and
the Visual C++ 2015–2022 runtime if needed (internet may be required at setup).
Cursor's optional SDK installs on first configured use; Cursor/Codex accounts,
external applications and Microsoft PowerPoint are not included. Core editing,
viewing and export remain local and account-free.

`atlas-update` owns update state, workers and installation coordination;
`atlas-shell::updates` paints both apps' UI. The installed `current` folder is
replaceable. Workbooks remain in their chosen locations and settings/cache remain
under `%LOCALAPPDATA%/NativeFileAtlas`, outside the installation. A helper copied
beside `Update.exe` waits on process-lifetime Windows file handles, verifies the
downloaded package's size and SHA-256 (including cached downloads), then delegates
installation and rollback to Velopack. Update hooks maintain the File Atlas
shortcut. Failed installs write `atlas-update-error.txt` beside `Update.exe`;
the previous installation can be reopened. Reinstall the current Setup.exe if
the installation itself is damaged. Avoid running Setup.exe over an active
session; the in-app updater is the coordinated path.

## Publisher signing

The workflow works unsigned until a publisher certificate is configured.
Add GitHub Actions secrets `WINDOWS_CERTIFICATE_BASE64` (base64 PFX) and
`WINDOWS_CERTIFICATE_PASSWORD`. Packaging passes these to Velopack's signing step
for shipped executables and the installer, with timestamping; the certificate is
temporary and is never uploaded in artifacts. Local packaging uses
`WINDOWS_SIGN_CERTIFICATE` (PFX path) and `WINDOWS_SIGN_PASSWORD` instead.
Signing requires a certificate/account owned by the publisher. It cannot be
created from repository credentials, and signing alone does not guarantee that
SmartScreen has established reputation for a new release.

## Verification

### Managed machines and trust

Development-time Rust build helpers and procedural-macro DLLs are not shipped;
never package `target/release` wholesale. End users do not need to approve those
build artifacts or install the Rust/C++ development toolchain.

An installer approval is not a blanket approval of its contents. Endpoint
controls can separately inspect installed executables, DLLs, runtime components,
and updater files; new versions may require fresh review. Sign the installer and
application binaries with a consistent publisher identity and verify the actual
release signatures. Optional signing support in this repository does not prove
that a published release was signed. Signing identifies the publisher but does
not guarantee immediate SmartScreen reputation or approval by corporate policy.

Coordinate publisher/application approval with the recipient's IT team under its
supported policy; do not ask users to disable protection or copy development-folder
exceptions to installed applications. Test install, both app launches, and an
update on a representative managed machine. Include optional AI/runtime paths
when those features are part of the deployment.

References: [Microsoft application control guidance](https://learn.microsoft.com/en-us/windows/security/book/application-security/application-and-driver-control)
and [SmartScreen reputation guidance](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation).

### Release checks

The release job runs `cargo test --locked --workspace` on Windows before building.
`test-windows-update.ps1` installs a headless fixture with real Velopack Setup,
holds two fixture processes open, verifies the helper waits for both, then checks
a real upgrade, restart and preservation of data outside `current`.
`verify-windows-package.ps1` checks required assets, feed version/channel, full
package size/hash and Setup.exe. Updater tests exercise offline/manual errors,
invalid channels, path traversal, checksum requirements and installation locks.
Before broad distribution, test a clean Windows user: install, open both apps,
save a workbook, update across two versions with both windows open, and verify
the saved workbook and settings survive. This machine's Windows application
control may block newly built executables; compilation alone is not an installer
or GUI smoke test.
