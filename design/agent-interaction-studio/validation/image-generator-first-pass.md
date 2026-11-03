# Image Generator first-pass verification

Historical record only. The user rejected this schematic presentation; native parity work supersedes it. Do not use this pass as a design baseline.

30 October 2026. Local Studio, http://127.0.0.1:8765/.

## Executed

- Loaded the existing browser preview, with Chat Train first and Image Generator second/selected. Only the active canvas and its footer are shown.
- Opened the shared picker: ComfyUI / Auto; effort and speed disabled. Switched to OpenAI API and selected GPT Image 1 mini. The image catalogue contains the native image fixtures, independently of the text-agent catalogue.
- Opened Provider → API chevron → Add API key. Confirmed the dedicated masked dialog and preview-only notice. Cancelled without entering a credential.
- Changed Source to 3:2, entered a sample prompt, increased count to two and generated. Observed running status and two completed history thumbnails.
- Started another sample and immediately stopped it. Two completed results remained; no cancelled output was added.
- Resized with the keyboard from 360 to 400 world units. At 400 the five control groups shared the same Y coordinate and retained positive widths; native two-row layout was observed at 360.
- Opened Debrief and confirmed the image draft label, download link and rendered document.
- Retested option-menu anchor clicks after the final click-away fix: a second click closes the menu. Selected Seed locked and Live; prompt editing produced sample results, and Keep retained the preview.
- Increased count to eight and verified the increment control becomes disabled. Resized to 200 world units and verified all five control groups reflow into three distinct rows.
- Switched back to Chat Train: original three long conversations retained. Its picker still showed Cursor / Auto / Medium / Standard, independently of image selection.
- Ran Chat Train model checks (22 behavior groups) and capability checks: passed. JavaScript syntax checks: passed.
- Compared all 11 locked reference hashes: only `chat-train/agent-picker.js` differs, for the explicitly recorded optional-catalogue compatibility extension. All ten other baseline files remain byte-identical.

## Shared-picker amendment

Original SHA-256: `da2be0df10f25ab501c80aa5bfdcc22341f7a2dae30d1572a27eba82ef1eeaed`.

Current SHA-256: `c80623f9f60741a6eaccf508a00225752f0e6b0c3d04dbaba6310d428a0793a1`.

The original lock manifest is not rewritten. Existing calls still default to ChatAgentCatalog. New image calls provide ImageAgentCatalog. Provider normalization, field rendering, option lists and speed metadata use the supplied catalogue within that open popup. Styling and default Chat Train capabilities are unchanged.

## Limits

This is browser verification of a Studio prototype, not native renderer equivalence or a real provider integration. Output is a local vector sample; no credential was entered or transmitted. Native source-reference image assets are absent, so no fresh native comparison screenshot was claimed. See the draft contract for deferred image-source, export, runtime and conversation features.
