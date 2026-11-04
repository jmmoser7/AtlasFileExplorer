# Picker parity handoff

## TOP — current A03/A04 state (30 October 2026)

A03 source integration is implemented, pending validator compile/focused tests and native interaction acceptance. No native captures or pixel certification were produced by this worker. Root baseline remains `target/studio-native-baseline/20261030-204228`; broad preexisting dirty work was preserved, with no staging, commit, reset, Cargo invocation or provider submission.

- Shared shell `generator::model_picker` is the provider/model/effort/speed popup for generators and Chat Train. `compact_model_parameters` reads selected values only when advertised by the selected model; unavailable effort/speed remain disabled in the existing rows. Standard has zero bolts, Fast one, Ultra/Ultra fast two. Closed chrome reserves muted effort and intact vector bolt outlines before its chevron; model text clips inside its remaining space and hover exposes the complete selected display name. Rich popups remain 232 screen units wide with 30-unit rows while the canvas zooms.
- `SlateApp::paint_chat_model_picker` routes normal and artifact-spawned Chat Train through `GeneratorChromeData`, existing provider/model dispatch and journaled parameter command. Provider selection does not require sending a prompt. Chat title renaming and tail-only composer ownership remain in their original owners. The historical separate model popup interpreter was removed.
- Provider key chevrons retain explicit Add/Change API key rows. Key actions route through existing secret-store paths. Artifact Cursor now paints the existing password field/editor under its generator; OpenAI routes to the existing pack key editor. No credential bytes enter picker metadata or the document model.
- A04 assessment: the existing `dashboard_set_full_access` writes the machine preference through `atlas_ai::access::set_artifact_default_full_access_in` and reloads its access snapshot; effective grants still use the existing policy. No demonstrated persistence gap was established in this narrow source pass. Restart/effective-authority native exercise remains outstanding; no permission subsystem or new authority was added.

### Exact changed owners / APIs

`crates/atlas-shell/src/selection_tools/generator.rs`: exports `model_picker`, `PickerResponse`, `compact_model_parameters`.
`generator_picker.rs` and `generator_picker/tests.rs`: public shared entry/owned response intents; capability-derived closed summary and regression.
`menu_selector.rs`: reserved suffix/chevron geometry and intact bolts; screen-sized rich popup.
`generator_options.rs`: opts out of model-specific closed suffix.
`apps/slate/src/app/board_agent/portal_picker.rs`: `paint_chat_model_picker` app adapter.
`portal_header.rs`: invokes shared picker for a tail with a configured provider.
`results.rs`: removes obsolete independent `model_menu_items`.
`runtime.rs`: removes obsolete `model_menu_armed` state.
`generator_chrome.rs`: allows installed provider choices for Chat Train through existing dispatch.
`artifact/editor.rs`: existing masked Cursor key editor reachability.
`paint_probe.rs`, `picker_tests.rs`: nested shared Model submenu and screen-sized popup expectations.

### Checks and acceptance limits

Targeted `rustfmt --edition 2021 --config skip_children=true` passed on all changed Rust files via the stable MSVC toolchain. Validator owns all Cargo/check/test runs. Queued focused tests: `compact_chrome_uses_only_advertised_effort_and_exact_speed_bolts`, shell generator-picker tests including `root_keeps_provider_model_effort_speed_separate_and_disables_unsupported_choices`, `provider_key_chevron_emits_an_explicit_key_intent_without_selecting_that_provider`, and app `board_agent::picker_tests` / `board_agent::stop_tests`.

Still visually unverified: ordinary and artifact-spawned Chat Train picker opening/provider switching; long names at narrow widths and zoom; muted effort/whole bolts; supported and unsupported effort/speed; real expired-key replacement on all hosts; popup edge placement, wheel ownership and return grace; key row clipping/hit testing. Validate against HTML Studio `a7599065`, matched theme/DPI/viewport/zoom and the current built source. Native screenshot acceptance is distinct from source and unit tests.

The integration handoff already flags production unbound starter-to-new-agent text selection propagation as a residual acceptance gap. It was not expanded into this picker task.
