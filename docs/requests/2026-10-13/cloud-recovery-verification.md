# Cloud recovery verification — 13 October 2026

## Snapshot and environment

- Expected and checked HEAD: `b9d43087f7aebb5d84922f80adb195308ec2cf3d`.
- `git rev-parse HEAD` exited 0 and returned the expected SHA before any build.
- `git remote -v` exited 0 with no output: this immutable cloud checkout has no
  configured remote, and no fetch was attempted.
- The checkout started clean on branch `work` (`git status --short --branch`
  printed only `## work`).
- Host capacity: 17 GiB RAM (16 GiB available), no swap, and 3 online logical
  CPUs (`Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz`).
- Toolchain: `rustc 1.89.0 (29483883e 2025-08-04)` and
  `cargo 1.89.0 (c24e10642 2025-06-23)`.
- Linux Pdfium is absent. A bounded search under `/usr`, `/opt`, and `/workspace`
  found only the Windows binary `vendor/pdfium.dll`; it found no
  `libpdfium.so`. Provisioning history in `cloud-slate-followup.md` records the
  single matching Linux download attempt and its HTTP 403 response, so this
  recovery run did not repeat that failed download.

## Required integrated runtime command

The first required integrated-run invocation was:

```text
set -o pipefail
mkdir -p target/cloud-verification
/usr/bin/time -f '\nCARGO_ELAPSED=%e\nCARGO_MAX_RSS_KB=%M' cargo test --locked -p slate -p atlas-shell -p slate-doc -p slate-artifact --no-fail-fast -j2 2>&1 | tee target/cloud-verification/recovery-integrated.log
status=${PIPESTATUS[0]}
printf '\nCARGO_EXIT=%s\n' "$status" | tee -a target/cloud-verification/recovery-integrated.log
exit "$status"
```

At exact HEAD `b9d43087f7aebb5d84922f80adb195308ec2cf3d`, it exited
127 immediately, before Cargo started, because this image does not contain the
requested timing executable:

```text
/bin/bash: line 3: /usr/bin/time: No such file or directory

CARGO_EXIT=127
```

Elapsed wall time reported by the command runner was 0.2 seconds. There are no
test-binary counts or assertion diagnostics because no Cargo process ran. In
accordance with the recovery request's explicit setup/tool-failure rule, the
failing operation was stopped and was not hidden behind retries or a changed
command. Consequently this run does **not** validate the 25 fixes, and it makes
no source-regression or Pdfium-blocked-test claim.

## Repairs and targeted reruns

Not reached after the setup/tool failure. No production code or fixture was
changed.

## Final audits

Not reached after the setup/tool failure.
