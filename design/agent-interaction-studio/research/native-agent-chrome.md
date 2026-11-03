# Native agent chrome audit — 30 October 2026

The earlier three Studios invented a product shell. That presentation is rejected and superseded. This audit uses current native source and the already-reviewed Image Studio to establish the replacement. No Slate window was running during the audit; native source and stored image captures were the references. No native executable was built or modified.

## Shared native owner

Dashboard `apps/slate/src/app/board_agent/artifact/editor.rs` calls `atlas_shell::selection_tools::generator::generator_editor`. Images use the same generator framework. `crates/atlas-shell/src/selection_tools/generator.rs`, `generator_layout.rs`, `generator_body.rs` and `generator_history.rs` own the header, prompt, lifecycle and revision row. `apps/slate/src/app/board_properties/palette_ui/apron.rs:32–66` places the editor through the image presentation/blister owner.

| Native baseline | Source | Studio mapping |
|---|---|---|
| Header 24 units, label 11px, Play 20 units | generator.rs:344–370 | Same top datum as reviewed Images: compact model, three dots, revisions, Play |
| Inset 12, gap 6, summary row 24 | generator_layout.rs:4–6 | Use reviewed Image packing (8 horizontal inset, 28-unit top region); a deliberate existing Studio refinement |
| Prompt minimum 40, text wraps at authored world width | generator.rs:79; generator_layout.rs | Retain compact Image prompt with native prompt language |
| Circular revision selector, 26 native diameter, gap 3, navigation 16 | generator_history.rs:67 onward | 18-unit circles per accepted Image refinement, all in header lane |
| Collapsed blister band 40, relief 20, shoulders 84 | selection_tools/blister.rs | Same ImageStudioLayout geometry; no new toolbar design |
| Natural strip width 48 × count + 168, at most 20% host area | blister.rs | Optional tool-count input extends the one reviewed Image layout owner |
| Detached strip buttons 30, gap 7; image gap 14; editor width 420 | shared image presentation | Reused directly for narrow hosts |
| Activity initially collapsed; partial 96, maximum 480, text 14 | generator_activity.rs | Public summary with fold control; no permanent Progress/Script tab bar |

## Dashboard

A dashboard is a generic Web artifact. Its HTML contents are not fixed Slate controls. The neutral chart in this Studio is sample output only; it no longer includes invented page navigation, reporting-period controls, widget selection or a revision footer.

Native property order is **Fill, Stroke, Agent, Capture**. `board_properties/property_strip.rs:23–54` supplies the agent/media early branch and `:191–195` adds capture for a Portal. See also `board_image_layers.rs:243–250` and `board_flow.rs:282–295`.

The exact native hint is **Describe the dashboard or the next change**. Settings contains **Fork, Fresh eyes, Inspect memory, Open source**; **Apply preview** and **Discard preview** appear only while a preview is pending. See `board_agent/artifact/editor.rs:209` onward. Those actions are in the three-dot Settings submenu, not in the dashboard content.

## Text

`board_flow.rs:571–579` delegates `paint_agent_text_window` to `paint_agent_note`. `board_flow/text_note.rs:5–7` describes the note as latest reply plus a bottom prompt, not a document-editor application. The simple fallback Run at bottom −40 is suppressed when shared property controls own the editor (`text_note.rs:45–47,88–92`). Text body font is 14 (`board_flow.rs:37`). Generic text blocks can use the same rectangular blister (`property_strip.rs:102–113`).

The Studio therefore shows bare editable text, its property controls, and the compact agent prompt. The prior Courtyard brief / Editable text heading is removed. Highlight-to-query and scoped replacement remain explicit owner-requested additions, not claimed native baseline. Dialogue is enabled via Preserve model history; the shared output-drag gesture creates a downstream train. There is no separate Create chat train menu command.

## Rhino / 3D

The Slate 3D viewport is the content surface. Model display, measurement, screenshot, maximize, filter and connector operations belong to the property strip (`property_strip.rs:65–92`), not a fabricated file-title/navigation bar. Display modes are **Shaded, Arctic, Hidden line, Material mask, Z-buffer** (`crates/slate-doc/src/scene/model_camera.rs:25,45`).

Native-connected Rhino has a compact lower **Slate 3D** control and, where available, **Grasshopper** (`application_connector/presentation.rs:12,62,84`). This does not justify adding a generic view/title/status header. The Studio retains the model and axes, removes the invented header/footer and exposes the native display choices via the property icons. Actual host switching remains simulated.

## Deliberate refinements inherited from Images

- Compact adjacent model picker and three-dot control; provider/model/effort/speed stay explicit inside the shared picker.
- Single Preserve model history check remains open after toggling, enables in-place dialogue and inherited downstream continuation.
- Transcript typography and overlay scrolling follow Chat Train. Unfocused ordinary prompt hover still allows canvas zoom; explicitly opened dialogue owns normal wheel scrolling.
- History handle location, hiding, terminal cut policy and blank-canvas dismissal remain shared design requirements.
- Native palette colors, fill/stroke/crop implementation, permissions and tool execution are not redefined by this prototype.

## Verification boundary

The replacement is source-informed and uses the exact reviewed Image layout owner for frame/toolbar geometry. It is not advertised as a native screenshot pixel match. Remaining native capture, connector execution and production input protocols must be reconciled in the future Rust phase. The prior invented layout is retained only in historical screenshots and the earlier commit, never as the current contract.

## Owner correction after native audit — authoritative Studio v0.3

The native-source facts above are evidence, not a mandate to restore controls the owner rejected. The v0.3 review explicitly removes Fork, Fresh eyes, Inspect memory, Open source, separate Settings and the separate activity-summary row. Use one Preserve model history checkbox and only relevant pending Apply/Discard actions. Spinner and Chat Train hover chevrons occupy the same header datum; public progress/script output is inside the actual shared Images dialogue. The four generators reuse generator-ui.js, and all applicable source channels reuse ImageGraft and the input-target menu recipes. The v0.3 contracts supersede conflicting v0.2 descriptions.
