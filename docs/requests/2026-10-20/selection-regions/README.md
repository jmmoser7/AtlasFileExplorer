# Native selection-region acceptance — 20 October 2026

Open [the interactive checklist](index.html) beside the delivered native Slate
build. It works directly from disk and uses no server or external dependency.
It does not launch an executable or read any referenced file.

This checklist translates every golden path, GP1–GP9, in the
[agreed selection-region contract](../../../keymap/contracts/selection-hit-regions.md)
into a reproducible native setup, actions, and observable expected results.
GP1–GP7 evaluate the current selection changes. GP8–GP9 protect the surrounding
native editing, camera, persistence, and export behavior. The immutable
catalog for this revision is [acceptance.cases.js](acceptance.cases.js).

1. Enter the evaluator, evaluation type, running executable path, version,
   SHA-256, workbook and display/input context. Record the executable's
   last-write time and source/build identity when known. Verify the running
   process matches that executable before checking confirmation. A checkout
   revision alone does not identify a running binary.
2. Begin the run, use a disposable workbook and perform each path in native
   Slate. Mark every expected result **Pass, Fail, Blocked or Untested**.
   Record actual behavior, tested zooms/variants and any unmet clause IDs.
   Evidence references are filenames/paths, not uploads or embedded captures.
3. Export results JSON and return that file with the screenshots/video/logs
   it names. The export includes the full case catalog, build context, all
   saved runs and per-result timestamps. If downloads are unavailable, copy
   the portable JSON displayed on the page. Restore results accepts exports
   from this exact checklist revision and adds runs without replacing history.
4. Use **New build / new run** for every new executable or fresh evaluation.
   Previous records stay separate; every new run starts entirely Untested.
   Run context locks when the run begins so later observations cannot quietly
   inherit a different binary. The page does not prefill a delivered candidate.

If the build cannot be launched, record the executable identity you have,
enter `unknown` for an unavailable SHA-256, leave identity confirmation clear,
and mark the affected paths Blocked with the actual error. Do not treat source
inspection, sandbox approval or an unexecuted test as a native observation.

A path shows Fail if any expected result fails, or Blocked if any is blocked
and none fails. It becomes Pass only when **every** expected result passes,
the exact running identity is confirmed with a valid executable SHA-256, and
an actual observation or evidence reference is present. Otherwise the path
remains Untested, with its partial progress and missing requirement shown.
This prevents a successful subset from hiding an unperformed variant.

Select **User acceptance** for the user's explicit evaluation. A **Native
evaluator observation** run records an agent or other evaluator's interactive
result without substituting for user acceptance of appearance, feel and flow.
Apply [the feedback acceptance policy](../../../../feedback/README.md#resolution-and-interactive-acceptance)
before closing any original report. The page never closes or archives reports.

Results are saved locally under a checklist-specific browser-storage key.
If storage fails, the page says so and the current in-memory record can still
be exported. Export before switching browsers, clearing storage or moving the
page. Keep exports with their screenshots and tested build; do not put secrets
or credentials in the free-text fields. No native result is initially Pass.

Implementation: [acceptance.js](acceptance.js) owns run validation and rendering;
[style.css](style.css) provides responsive and print layout. This is an
acceptance record, separate from the adaptive-region prototype and native
runtime implementation. Its browser layout and blocked-run recording/export were reviewed with real IAB
input on 20 October. The retained browser screenshot and blocked-run export are
under evidence/. This verifies the checklist only; no native GP has passed.
