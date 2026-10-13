# Cloud runtime repairs — 2026-10-13

## Source identity and scope

- Required initial HEAD: `ef30fc9ef139bcf7287b023506a3266028e31b93`.
- Observed initial `git rev-parse HEAD`: `ef30fc9ef139bcf7287b023506a3266028e31b93` (exact match).
- `git remote -v` produced no output; no fetch or snapshot switch was attempted.
- Checked-out local branch at preflight: `work`. The requested immutable source was preserved and implementation continued in that checkout.
- Final source identity is the commit containing this report; its SHA is reported in the handoff alongside `git rev-parse HEAD`. Cargo manifests and lockfiles were not changed.
- Work stayed inside the existing board input, direct-selection, property-palette, and regression-test owners. No portal host, `board_*.rs` module, command, contract, allowlist, dependency, provider, secret, cloud-placeholder, or ignored benchmark was added or changed.

## Six repaired behaviors and retained evidence

1. **Selected-picture Agent prompt** — The prompt body has fixed geometry and the image palette reveals it by clipping that geometry. The assertion sampled only two frames after opening, while the reveal still exposed the datum rows above the prompt. The regression now waits through the existing animation helper, then performs the same real paint and requires the same `Describe the picture…` hint plus model/source/mode/settings labels. Focused result: **1 passed, 0 failed**.
2. **Text editor suppresses the selection outline** — Runtime tracing showed the real primary press was inside the Text squircle, but egui reported `hovered=false`, `contains_pointer=false`, and `clicked=false` on that press because the control is painted in foreground canvas chrome while its interaction response was evaluated on the caller's canvas layer. The palette now records a primary press inside each live squircle and accepts the release only when it returns inside the same squircle; normal egui clicks remain accepted. The original real pointer sequence, palette state assertion, light/dark outline paint assertions, repeated navigation, selection retention, and Fill follow-up all remain. Focused result: **1 passed, 0 failed**.
3. **Typeface list owns the wheel** — This shared the lost Text-squircle press above. With ordered palette click ownership repaired, the test still opens Text and the family list through real pointer events, wheels over the painted list, and asserts that the board zoom is unchanged. Focused result: **1 passed, 0 failed**.
4. **Deleting a rectangle edge opens its outline** — A pointer exactly on a fill boundary was intentionally outside the body-fill picker, so sub-object routing never reached `curve_edge_at` even though segment geometry recognized the edge. The direct-selection owner now prefers the routed body candidate and then checks topmost scene edge geometry, allowing a real boundary edge without making empty space or arbitrary chrome eligible. The raw route still requires Ctrl+Shift, Select, canvas/clip eligibility, no popup, and no open property/number/corner editor; only a recognized real edge can override stale same-layer chrome. The test retains real Ctrl+Shift press/release, exact edge `[1]`, Delete, open-path, style, and undo assertions. Focused result: **1 passed, 0 failed**.
5. **Tiny shape body moves before overlapping grips** — The body won at the press origin, but gesture admission recomputed toolbar/dimension capture after the pointer crossed the drag threshold; by then it had left the four-by-three-unit body, so the move never began. Tiny-body ownership is now latched from the primary press origin for gesture admission, and dimension stringers treat that recognized body as grip-owned. Popup, text-edit, Space-pan, web/model toolbar, and ordinary transform guards remain unchanged. The regression still performs real drags at zoom `0.5`, `1.0`, and `8.0`, checks unchanged shape kind and size, checks movement, and verifies undo. Focused result: **1 passed, 0 failed**.
6. **Wire grab cursor on every eligible node kind** — The text fixture authored a `160 × 40` box but ran at Slate's `0.8` default zoom; font auto-fit did not enlarge the authored box, so it correctly failed the explicit `wire_ports_fit` eligibility guard before hover. The fixture now establishes a deliberate 1:1 camera, retaining the explicit eligibility assertion and the real hover/`Grab` cursor assertion for text and every other node kind. No wire-port threshold or text-node eligibility rule was loosened. Focused result: **1 passed, 0 failed**.

The final focused stream used each test name as a Cargo substring (not `--exact`, which would require the fully qualified module path). All six executions reported `running 1 test`, **1 passed, 0 failed**, and true Cargo exit **0** through `PIPESTATUS[0]`. Existing dead-code warnings for clipboard/external-drop test surfaces remained unchanged.

## Final integrated validation

### Required packages

```text
cargo test --locked -p slate -p atlas-shell -p atlas-ai --no-fail-fast -j2
```

True Cargo exit: **101**, retained through `tee` with `PIPESTATUS[0]`. Every independent binary and doc-test target ran.

| Test binary | Passed | Failed | Ignored | Result |
|---|---:|---:|---:|---|
| `atlas-ai` unit tests | 76 | 0 | 2 | PASS |
| `atlas-shell` unit tests | 272 | 0 | 2 | PASS |
| `slate` library unit tests | 1380 | 6 | 53 | ENVIRONMENT-ONLY FAIL |
| `slate` binary unit tests | 0 | 0 | 0 | PASS |
| Doc tests (`atlas-ai`, `atlas-shell`, `slate`) | 0 | 0 | 0 | PASS |
| **Total** | **1728** | **6** | **57** | **Six known runtime blockers only** |

The Slate library finished in `350.51s`. Optional end-to-end timing was not measured because no timing wrapper was used.

### Remaining environmental failures (unchanged)

Linux printed `[atlas] pdfium.dll not found — PDF previews limited to shell handlers`. The denied runtime download was not retried, and no assertion was disabled or changed.

1. `app::tests::media::assets::unbundle_word_of_an_unsaved_workbook_writes_into_the_data_dir`
   - `Added 1 file(s)`
   - `Word did not produce a readable PDF. Check the PDF preview runtime.`
2. `app::tests::media::assets::unbundle_word_writes_page_images_beside_a_saved_workbook`
   - `Added 1 file(s)`
   - `Word did not produce a readable PDF. Check the PDF preview runtime.`
3. `app::tests::media::unbundle::unbundle_of_a_single_page_does_not_place_a_grid`
   - `Added 1 file(s)`
   - `Could not read PDF pages. Check the document and the PDF preview runtime.`
4. `app::tests::media::unbundle::unbundle_pdf_places_pages_through_dispatch_and_undoes_in_one_step`
   - `pdf unbundle did not finish: Added 1 file(s)`
   - `Could not read PDF pages. Check the document and the PDF preview runtime.`
5. `app::tests::media::unbundle::unbundle_pptx_and_legacy_ppt_when_office_can_render`
   - `deck.pptx: Added 1 file(s)`
   - `PowerPoint did not produce a readable PDF. Check the PDF preview runtime.`
6. `app::tests::media::unbundle::unbundle_while_the_preview_is_pending_finishes_instead_of_failing`
   - `Added 1 file(s)`
   - `PowerPoint did not produce a readable PDF. Check the PDF preview runtime.`

No non-environment assertion failed in the final integrated run.

## Audits

- `cargo test --locked -p xtask -j2` — true exit **0**: xtask library 27, contracts 4, kits 3, metrics 4, size 1, theme 1; **40 passed, 0 failed, 0 ignored**. This includes contract, kit, source-size, and theme audits. The initial audit exposed one-line allowlist overruns in existing CRLF files; relevant comments/spacing were compacted without changing the ceilings, and the complete audit was rerun green.
- `cargo fmt --all -- --check` — exit **0**, no output.
- `cargo clippy --locked -p slate -p atlas-shell -p atlas-ai --all-targets -j2` — exit **0**. Existing warnings (including redundant closures, nonminimal booleans, and test-only dead code) remain; no warning policy or unrelated code was changed.
- `git -c core.whitespace=cr-at-eol diff --check` — exit **0**. The override makes Git treat these files' preserved CRLF endings as end-of-line rather than trailing whitespace.

## Runtime-iteration note

The first focused wrapper mistakenly combined unqualified names with `--exact`, producing zero-test successes; those outputs were rejected as evidence and all six were rerun without `--exact`, each with `running 1 test`. An earlier compile wrapper also survived its parent shell after a compile error and briefly held Cargo's build lock; it was terminated before the single final sequential stream. Neither event changed source behavior, and neither is counted as validation evidence above.
