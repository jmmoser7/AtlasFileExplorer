# Cloud runtime recovery — 2026-10-13

## Provenance and scope

- Initial `git rev-parse HEAD`: `ef30fc9ef139bcf7287b023506a3266028e31b93` (exact requested HEAD).
- Initial `git remote -v`: no output; this cloud checkout has no configured remote.
- Checked-out local branch: `work`; no fetch or branch switch was performed.
- `git diff --quiet ac812a2524348cb076d16dd474b7ad3b2edc848b ef30fc9ef139bcf7287b023506a3266028e31b93 -- apps crates Cargo.toml Cargo.lock` exited 0, confirming the requested source objects match exactly.
- Scope stayed on the six named non-environment regressions and the production interaction owners they exercised. No dependency, provider, secret, cloud-placeholder, behavior-spec, ignore, timeout, or Pdfium behavior was changed.

## Repaired regressions and retained real-input evidence

1. **Selected-picture agent editor.** The fixed editor reveals through the shared strip fade, and the narrow image puts three datum rows above the prompt. The fixture now lets the existing animation settle before checking the same painted prompt hint. It still selects the real image, paints the actual Agent editor, and checks model, source, mode, settings, and prompt-face output.
2. **Text selection outline while navigating the editor.** The canvas-hosted property buttons paint on foreground chrome, so egui could visually hover the button one frame after the press without assigning the press to its background response. The palette now records the exact real primary-press row and accepts only a release on that same row. The test still performs pointer move/down/up on the painted Text squircle, navigates between strip and editor, and checks the real painted silhouette.
3. **Wheel over typeface list.** The same ordered property-button ownership fix opens Text from the painted squircle. The existing test then opens the real family list, sends wheel events over its actual painted rectangle, and verifies the camera does not zoom.
4. **Delete rectangle edge.** Exact points on a closed form's boundary can be outside the filled-body picker. Curve sub-selection now prefers the routed body hit but falls back to the topmost actual curve segment hit, retaining lock/hidden and segment-distance rules. Primary Ctrl+Shift pointer input also contributes its event-local modifiers, rather than relying on the secondary-button snapshot. The test retains real move/down/up, edge selection, Delete, path opening, journal undo, and exact restoration.
5. **Tiny shape body move.** Screen-constant transform chrome can cover the entire body at low zoom, preventing the canvas response from winning egui drag ownership. A tiny-body ordered press now starts the normal shared `Move` gesture; active Move update/release is independent of response ownership. Pointer loss cancels and restores rather than committing, and the existing test retains real drag frames, unchanged geometry, movement at 0.5/1/8 zoom, and undo.
6. **Wire cursor on text.** The 160×40 host is mathematically below the approved short-host suppression floor at the fixture's default 0.8 zoom (`32 < 4 × 8 × 0.8^-0.35`, approximately 34.6 px). The fixture pins zoom to 1.0, where the authored host is eligible, while retaining the explicit eligibility assertion, painted port hover, and real-pointer Grab cursor assertion for every node kind.

## Commands, exits, counts, and timing

The first focused reproduction used the unchanged requested source and reproduced the prompt failure. Its cold build stopped after **27m 01s** with exit 101 because an in-progress fixture edit referenced the test pause helper through the wrong module path; this routine iteration error was corrected before runtime validation.

Focused runs retained in `/tmp/atlas-six.log`, `/tmp/atlas-three2.log`, `/tmp/atlas-edge-final.log`, and `/tmp/atlas-move-final.log` were copied into the results summarized here; those VM paths are not the sole evidence.

- The six named focused tests each passed at runtime with their original downstream assertions. Each test binary reported **1 passed, 0 failed**; the Slate main binary reported zero tests. The initial warm six-test loop took **391s** for its one rebuild and about **1s** per subsequent filter. Later source-guided iterations rebuilt Slate in **5m 33s–6m 43s** and then ran each focused case in **0.09s–0.24s**.
- The final integrated command and audit results are recorded below after completion.

## Environmental PDF/Office failures

Linux still reports `[atlas] pdfium.dll not found — PDF previews limited to shell handlers`. The known six failures remain environment-blocked and were neither downloaded around, disabled, ignored, nor changed:

- `app::tests::media::assets::unbundle_word_of_an_unsaved_workbook_writes_into_the_data_dir` — `Word did not produce a readable PDF. Check the PDF preview runtime.`
- `app::tests::media::assets::unbundle_word_writes_page_images_beside_a_saved_workbook` — same Word diagnostic.
- `app::tests::media::unbundle::unbundle_of_a_single_page_does_not_place_a_grid` — `Could not read PDF pages. Check the document and the PDF preview runtime.`
- `app::tests::media::unbundle::unbundle_pdf_places_pages_through_dispatch_and_undoes_in_one_step` — same PDF diagnostic after `Added 1 file(s)`.
- `app::tests::media::unbundle::unbundle_pptx_and_legacy_ppt_when_office_can_render` — `PowerPoint did not produce a readable PDF. Check the PDF preview runtime.`
- `app::tests::media::unbundle::unbundle_while_the_preview_is_pending_finishes_instead_of_failing` — same PowerPoint diagnostic.

## Final integrated validation and audits

### Integrated Slate/shared run

```text
cargo test --locked -p slate -p atlas-shell -p atlas-ai --no-fail-fast -j2
```

The final run completed in **362s** with Cargo exit **101**, retained through `PIPESTATUS[0]`. Per-binary results were:

| Test binary | Passed | Failed | Ignored | Result |
|---|---:|---:|---:|---|
| `atlas-ai` unit tests | 76 | 0 | 2 | PASS |
| `atlas-shell` unit tests | 272 | 0 | 2 | PASS |
| `slate` library unit tests | 1379 | 7 | 53 | FAIL |
| `slate` binary unit tests | 0 | 0 | 0 | PASS |
| Doc tests (`atlas-ai`, `atlas-shell`, `slate`) | 0 | 0 | 0 | PASS |
| **Total** | **1727** | **7** | **57** | **FAIL** |

All six assigned regressions passed in that integrated execution. Six failures were the known Pdfium/Office blockers listed above. One independent test,
`app::board_agent::life::tests::agent_life_publishing_context_never_decodes_pictures_on_the_frame_loop`, observed that its fixture timestamp changed from `1700000000` to `1791940191` during the parallel integrated run. An immediate focused rerun exited **0** (**1 passed, 0 failed**, 1.17s), retaining its no-frame-loop-decode assertion; this is recorded as parallel test-environment interference rather than hidden or changed.

### Targeted runtime results

The final focused interaction checks all exited **0**:

- Agent editor prompt: 1 passed, 0 failed (0.13s after the initial rebuild).
- Text editor outline: 1 passed, 0 failed (0.20s final run).
- Typeface-list wheel ownership: 1 passed, 0 failed (0.15s final run).
- Rectangle edge deletion/opening: 1 passed, 0 failed (0.10s final run).
- Tiny-body move at all three zooms: 1 passed, 0 failed (0.16s final run).
- Wire-port Grab cursor across node kinds: 1 passed, 0 failed (0.24s).
- Pointer-gone move restoration guard: 1 passed, 0 failed (0.09s).
- Agent-life timestamp check focused rerun: 1 passed, 0 failed (1.17s).

### Audits

- `cargo test --locked -p xtask -j2`: final exit **0**. Totals: **40 passed, 0 failed, 0 ignored** across the xtask library, contracts, kits, metrics, size, and theme binaries; zero-test main/doc binaries also passed. An earlier audit correctly rejected temporary growth above the existing `board_flow.rs` ceiling and the 500-line `palette_ui.rs` cap; the implementation was extracted/shrunk and the unchanged ceilings then passed.
- `cargo fmt --all -- --check`: exit **0**, no output. Existing CRLF line endings in `board.rs` and `board_flow.rs` were preserved.
- `cargo clippy --locked -p slate -p atlas-shell -p atlas-ai --all-targets -j2`: final exit **0** (0.92s warm confirmation after a 7m 27s cold run). Existing warnings remain: atlas-shell redundant closures; Slate dead-code, duplicate test-only `cfg`, and existing boolean simplification suggestions. No new warning was suppressed.

## Final source identity

The final production/test source patch (`git diff --binary -- apps crates`) has SHA-256 `d746a1f0ab866543d8538cae52556ec1d7e4dcb005047cc225ef59b467fcad59`. Changed source is limited to the existing Slate board input, gesture, curve-pick, property-palette, flow-test, and wire-cursor-test owners listed by the final commit. The committed tree and commit SHA are reported in the handoff because a commit cannot embed its own identity.
