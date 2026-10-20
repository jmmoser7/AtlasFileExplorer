# Pinned rfd Windows save policy

This directory contains the unchanged contents of the published `rfd 0.15.4`
crate, except for the three source patches recorded in
[`atlas-confirm-overwrite.patch`](atlas-confirm-overwrite.patch) and these two
Atlas provenance files. The upstream MIT license is retained in `LICENSE`.

The recorded diff uses zero context to avoid whitespace-only context markers.
Apply it to a clean upstream snapshot with `git apply --unidiff-zero` (from this
workspace root, add `--directory=vendor/rfd` before the patch filename).

- Package: https://crates.io/crates/rfd/0.15.4
- Repository: https://github.com/PolyMeilex/rfd
- Packaged Git revision: `bac4bd3364f2befaabafa9f58ac77fd8925b2abf`
- Crate archive SHA256:
  `ef2bee61e6cffa4635c72d7d81a84294e28f0930db0ddcb0f66d10244674ebed`

`FileDialog::set_confirm_overwrite(bool)` defaults to `true`, including through
`FileDialog::default()`. On Windows only, an explicit `false` reads the native
save dialog's existing options and clears only `FOS_OVERWRITEPROMPT`. Other
platform backends retain their existing native policy and ignore this setting.
The added tests cover the default and the actual Windows COM option bits without
showing a dialog. They require execution in the corresponding test environment;
vendoring and formatting checks alone do not establish a native interaction pass.

The shared Atlas `PickRequest` forwards this policy. Slate's unique image save
workflow is the only application opt-out: its existing worker publishes complete
files atomically under noncolliding names. Suppressing the native confirmation
does not itself provide overwrite protection. All ordinary save requests retain
their existing confirmation behavior.

The workspace uses `[patch.crates-io]` at the existing version and excludes this
dependency snapshot from workspace membership. No unrelated upstream changes or
dependency upgrades are included. Recheck or remove this local patch when upstream
provides an equivalent policy.
