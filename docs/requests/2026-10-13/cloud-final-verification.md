# Cloud final verification — 13 October 2026

## Provenance

Authorized Linux verification and repair follow-up for parent desktop chat
`01a135d1-428e-7131-aae4-96de004b8a38`. Before running any tests, this checkout
reported the requested immutable snapshot exactly:

```text
$ git rev-parse HEAD
f74744b969108a12242fe2c13e83b8145540f86c
```

No remote, fetch, reset, fast-forward, ignored corpus, GUI, live provider, secret
value, or placeholder-byte read was used.

## Initial requested run

Command (Bash `pipefail`; Cargo's actual exit was retained separately):

```text
cargo test --locked -p slate -p atlas-shell -p slate-doc -p slate-artifact --no-fail-fast -j4
```

Cargo exit: **101**. The run completed every test binary: **2,014 passed, 20
failed, 58 ignored**. The Slate library accounted for **1,363 passed, 20 failed,
53 ignored**. Complete stdout/stderr is retained in ignored build output at
`target/cloud-final-verification/scoped-full.log`; Cargo's exit is in
`scoped-full.exit`.

Six failures printed the documented missing-Pdfium errors. Fourteen other
failures were reproduced individually and repaired or corrected at their
fixture's stale coordinate/lifecycle setup without weakening the behavioral
checks.

## Repairs and evidence

- Ctrl+Shift segment picking now asks the already-selected curve first, so a
  filled primitive body cannot mask its stroked edge. A stationary chord is
  routed as a click rather than a zero-area sweep; rectangle-edge deletion now
  opens the outline and targeted tests pass.
- OpenAI pack health changes invalidate the same derived engine/catalog
  lifecycle used after saving a key; the chooser immediately loses the
  `add API key` suffix in its regression.
- The PDF page pallet is 48 units high at zoom 1 (30-unit shared button plus
  9-unit vertical insets), satisfying its low-profile contract.
- Generated text override release is checked through immediate journal
  undo/redo before derived output arrives, proving the authored words restore
  and the released generated state redoes as one step.
- Image palette recovery is allowed its next frame, matching application frame
  ordering. Generic color-eyedropper coordinates now come from the generic
  editor rather than the image-only layout. Popup placement checks follow the
  shared `place_popup` contract by requiring an in-canvas non-overlap rather
  than an obsolete preferred side.
- Text-palette tests open through the object-panel owner before exercising real
  hover, fade, family-list and wheel input. The corners fixture checks the
  current compact shared panel geometry and enters crop through the production
  crop owner; separate interaction regressions retain pointer-input coverage.
- Primitive vertex pick, fillet/resize priority, tiny-body movement, and wire
  cursor fixtures now target their owning routes and current non-overlapping
  geometry. They retain geometry, selection, journal, undo, and cursor
  assertions.

Targeted rerun logs are retained at
`target/cloud-final-verification/targeted-after-fixes.log`,
`target/cloud-final-verification/targeted-final.log`, and
`target/cloud-final-verification/targeted-last.log`. After the final lifecycle and fixture cleanup, the OpenAI-key lifecycle and compact Corners/crop regressions were rerun together and both passed; complete output is retained in `target/cloud-final-verification/post-static-targeted.log`.

## Final requested run

The exact requested command was rerun under `pipefail` after repairs. Cargo exit:
**101**, solely because the same six runtime-blocked PDF/Office tests failed.
Every other requested test passed: **2,028 passed, 6 failed, 58 ignored** across
all completed binaries. The Slate library result was **1,377 passed, 6 failed,
53 ignored**. Complete stdout/stderr is retained at
`target/cloud-final-verification/scoped-final.log`; Cargo's actual exit is in
`scoped-final.exit`.

Per-binary counts reported by Cargo:

```text
atlas-shell lib:                  270 passed; 0 failed; 2 ignored
slate lib:                      1,377 passed; 6 failed; 53 ignored
slate main:                         0 passed; 0 failed; 0 ignored
slate-artifact lib:                75 passed; 0 failed; 1 ignored
hosted_shape_text integration:      1 passed; 0 failed; 0 ignored
artifact parity integration:        4 passed; 0 failed; 0 ignored
artifact runtime integration:       1 passed; 0 failed; 0 ignored
artifact video integration:         1 passed; 0 failed; 0 ignored
slate-doc lib:                     290 passed; 0 failed; 1 ignored
image_paint_layers integration:      3 passed; 0 failed; 0 ignored
migration integration:               6 passed; 0 failed; 1 ignored
doc tests (four crates):             0 passed; 0 failed; 0 ignored
```

### Complete remaining failure stdout

```text
---- app::tests::media::assets::unbundle_word_of_an_unsaved_workbook_writes_into_the_data_dir stdout ----
[atlas] pdfium.dll not found — PDF previews limited to shell handlers

thread 'app::tests::media::assets::unbundle_word_of_an_unsaved_workbook_writes_into_the_data_dir' panicked at apps/slate/src/app/tests/media/assets.rs:69:5:
Added 1 file(s)
Word did not produce a readable PDF. Check the PDF preview runtime.

---- app::tests::media::assets::unbundle_word_writes_page_images_beside_a_saved_workbook stdout ----

thread 'app::tests::media::assets::unbundle_word_writes_page_images_beside_a_saved_workbook' panicked at apps/slate/src/app/tests/media/assets.rs:26:5:
Added 1 file(s)
Word did not produce a readable PDF. Check the PDF preview runtime.

---- app::tests::media::unbundle::unbundle_of_a_single_page_does_not_place_a_grid stdout ----

thread 'app::tests::media::unbundle::unbundle_of_a_single_page_does_not_place_a_grid' panicked at apps/slate/src/app/tests/media/unbundle.rs:470:5:
Added 1 file(s)
Could not read PDF pages. Check the document and the PDF preview runtime.

---- app::tests::media::unbundle::unbundle_pdf_places_pages_through_dispatch_and_undoes_in_one_step stdout ----

thread 'app::tests::media::unbundle::unbundle_pdf_places_pages_through_dispatch_and_undoes_in_one_step' panicked at apps/slate/src/app/tests/media/unbundle.rs:316:5:
pdf unbundle did not finish: Added 1 file(s)
Could not read PDF pages. Check the document and the PDF preview runtime.

---- app::tests::media::unbundle::unbundle_pptx_and_legacy_ppt_when_office_can_render stdout ----

thread 'app::tests::media::unbundle::unbundle_pptx_and_legacy_ppt_when_office_can_render' panicked at apps/slate/src/app/tests/media/unbundle.rs:385:9:
deck.pptx: Added 1 file(s)
PowerPoint did not produce a readable PDF. Check the PDF preview runtime.

---- app::tests::media::unbundle::unbundle_while_the_preview_is_pending_finishes_instead_of_failing stdout ----

thread 'app::tests::media::unbundle::unbundle_while_the_preview_is_pending_finishes_instead_of_failing' panicked at apps/slate/src/app/tests/media/unbundle.rs:362:5:
Added 1 file(s)
PowerPoint did not produce a readable PDF. Check the PDF preview runtime.
```

These are classified only as Linux runtime blockers: the output proves the
prepared documents reached the Pdfium-dependent read/render boundary, and no
Linux Pdfium library is installed. The prior primary-source Chromium 7543
runtime download attempt returned HTTP 403; this run found no newly available
allowed runtime setup. Tests were not disabled and application behavior was not
changed to conceal the dependency.

## Static verification

- `cargo fmt --all -- --check`: exit 0.
- `cargo xtask size`: exit 0; 788 Rust files scanned, 111 existing oversized
  files allowlisted, no new finding and no allowlist increase.
- `cargo xtask contracts`: exit 0; 40 dimensions, registry/contracts/decisions
  agree.
- `cargo xtask kits`: exit 0; one kit, eight tools.
- `cargo clippy --workspace --all-targets -j4`: exit 0; existing warnings remain in
  `target/cloud-final-verification/clippy.log`.
