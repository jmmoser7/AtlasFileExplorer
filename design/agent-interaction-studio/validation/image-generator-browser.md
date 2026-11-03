# Image Generator browser validation â€” 30 October 2026

Implementation: static HTML/JavaScript/CSS/SVG. `native-large-initial.png` and the production source constants were read-only references. No Rust build or Rust edit was made for this rebuild.

## Executed

- JavaScript syntax checks for the browser entry point and renderer.
- Node tests: native-reference image geometry, existing Chat Train model (22 behavior groups), and capability checks â€” all passed.
- In-app browser: inspected small 220Ã—260, medium 360Ã—360, large 680Ã—300, connected image/text inputs, and three-image history sequence.
- Stopped the obsolete `image-studio-first.exe` renderer; reloaded and inspected the browser-only page successfully.
- Shared picker: opened Provider, Model, disabled Effort/Speed, OpenAI API key chevron, Add API key, separate masked dialog; cancelled without entering a key.
- Long prompt: 1,152 characters grow to 144 units with internal scrolling; canvas scrollTop stays zero. Restored the empty prompt.
- Width: large 680â†’500 changes to detached 420-unit editor; collapse/reopen preserves 500 width; 500â†’680 rejoins the image. Restored baseline width.
- History: second image displays first image's prompt; disconnect reduces history links from two to one; restore returns two links.
- Added vertical separation between scenario groups so focused examples do not expose neighboring groups at the canvas edges.

## Native measurements retained

504/552-unit strip eligibility plus the 20% area predicate; detached 420-unit editor; 30-unit buttons, 7-unit gaps, 14-unit image clearance; 40-unit attached band; 12-unit editor padding; 24-unit control rows; 6-unit gaps; 11-unit Ubuntu prompt; 26-unit history chips; native SVG tool paths; port y=30/72/114 and output y=30; native Bezier control distances. The added history port is y=6.

## Limits

This is a design-review fixture, not a connected image generator. Result thumbnails reuse the sample photograph. Browser text rasterization, shadows and SVG edges are not certified bit-for-bit native matches. Detached viewport-edge relocation, native low-zoom detail suppression, real image editing and provider execution are outside this pass. No new visual design has been marked approved.

## v0.4 — Images interaction refinement

- Browser: Small and Medium show rounded standalone cards; Large retains the joined edge. All three show model, Aspect Ratio, overflow and circular results on one datum. No square results are shown while a card is open.
- Browser: Model menu dismisses on canvas click while leaving the card open. Aspect Ratio exposes Custom last; entered 4:3, verified selected state, then restored Source. Mode uses Single Image Mode and Live Mode under a stationary expanding header. Settings uses the same submenu pattern.
- Browser: Unfocused prompt wheel changed 100% to 72%; focused prompt wheel remained at 100%. Restored the scene view afterward.
- Browser: Blank canvas deselects; hovering the image revealed square results without the tool strip. Moved to and selected Result 2, right-clicked it, verified native image recipe and persistent strip. Restored the user's selected Result 7.
- Browser: Activated Fill sub-icon and saw swatches. Verified shared controls on connected inputs and all three series images; two history wires remain.
- Node: Slate navigation event tests passed for right-drag/menu suppression, stationary right-click, middle-drag, Space-drag, focused wheel ownership, explicit zoom and native bounds. Updated image layout checks and unchanged Chat Train model/capability suites passed.
- Fixed a hover-bridge stacking issue found during review so the bridge cannot cover tool buttons.
- Screenshots: `images-hover-context.png` and `images-small-refined.png`. Exact visual approval remains open; basic image tool panels and external context-menu actions remain specimens/simulations.

## v0.5 — Wire handles, borderless results and configurable context menus

- Geometry tests: 65% of circular area outside the image within 0.01%; 1.4-unit clearance; curved notch path; input/output symmetry; 12 square results in two rows of an eight-column grid.
- Small image: all 12 circular results visible; model name fits; Aspect Ratio no longer occupies the top bar; count opacity is 1 outside hover; circles have zero borders/padding. Prompt min height is 32 with reduced padding.
- Hover: 12 borderless square results render as eight plus four. Right-click on Result 10 exposes Use this result, Save image as, Duplicate, Remove result, and the separated Studio editor entry.
- Menu editor: recovered saved recipe, inspected both targets and preview; disabled Bring to front for body and applied; body action disappeared while thumbnail Use this result stayed enabled; restored the body action. Configuration survives reload. Final review defaults are restored.
- Browser parity: Medium, Large, Connected image and all three Series instances show the compact header, no standalone Aspect Ratio, permanent count and borderless circles. Their full fixture histories are visible (8/8/8/3/8/8).
- Non-agent panels removed. Their glyphs remain reference context and their native menus are explicitly excluded from the implementation specification.
- Captures: images-small-v5.png and images-menu-studio-v5.png. No Rust files changed or built.

## v0.6 — Direct menu editing and remembered image panel

Browser checks: model box has about 2 units after its chevron; ellipsis follows the model box by 1 unit; circles follow the ellipsis target by 12 units. Long prompt and count were retained. Deselect small image: panel hides; reselect: it reopens. Right-clicking the model control selects Model picker in the editor; right-clicking the prompt selects Image prompt. Right-clicking a Provider row inside the open model popup produced one editor and removed the popup, with no browser menu shown in the capture. Insert divider increased preview rules 3→4; removing it restored 3; test edits were cancelled. Navigation/geometry and existing Chat Train tests passed. Scope remains browser-only.

## v0.7 — Running bond and draggable menu preview

Browser: small preview has 4-unit corner radii; row 2 starts at x=12/y=28 while row 1 starts x=0/y=0. Selected previews remain visible after moving to the footer. Selecting Medium hides Small's retained previews. Returning and selecting Small shows them; blank-canvas click hides them immediately. Circle/square exclusivity is retained.

Browser: dragged Bring to front from the first row to after Crop image; both preview and editable list updated. Dragged a divider to after Layers; verified its new location. Cancel discarded these test rearrangements. Arrow buttons are absent. Geometry/navigation/Chat Train tests passed. Captures: images-running-bond-v7.png and menu-preview-drag-v7.png. No Rust changes or builds.


## Image action refinement — 30 October 2026

The native return grace is 1500 ms (board_properties/selection.rs). Reopening after longer absence no longer applies. Cutout shoulder fillet is 40% of the indent radius. Shift-click selects a contiguous result range in either circular history or square previews; selected results share the thumbnail action set with an added Save selected images action. Active-result copy and duplicate flatten only the active image, never its history. Duplicate creates an independent plain image on the canvas. Saves use a persistent image-0001.png, image-0002.png sequence across single and batch operations. Shared result-menu actions follow primary-image ordering while retaining user labels and action selections. Because right-click is still assigned to authoring, Try selected action in the Menu Studio executes the selected action for testing. Browser verified four selected results and the first sequential download; external image clipboard falls back to the Studio copy buffer if unsupported. Native Rust remains unchanged.

Batch save downloads a single ZIP containing individually numbered PNGs because the embedded browser blocks multiple automatic downloads. Verified images-0005-0008.zip contains image-0005.png through image-0008.png, with ZIP CRC validation passing. Single saves remain PNG downloads.

## Shared image/history refinement

Verified first image's in-window dialogue accepted an additional turn and retained previous messages. New downstream image inherited that effective history, accepted its own reply, and survived reload. Disconnect and reconnect retained messages and queued refreshed history for the next turn. Medium thumbnail selection showed visible previews with zero toolbar buttons. Five Node suites passed, including new snapshot/reconnection tests. Corrected an overlapping CSS rule that initially made wire hit paths visible; their final stroke is transparent. Native apron, seam and icon-fringe constraints are recorded in the contract.


## Simplified history interaction — 30 October 2026

One Preserve model history toggle enables continuous in-window dialogue and inherited history on output drag. Browser verified output drag creating a child with inherited conversation, selected history-wire Delete reducing four connections to three, and reconnection restoring four. Original prompt draft restored. Output port sits above the resize hit area. Composer uses continuous Chat Train styling. All five relevant Node suites passed. Native wire colors explicitly retained; no Rust edits or builds.


## v1.1 — dialogue parity, terminal cuts and grafted inputs

Reused Chat Train single-window typography, indentation and transient zero-gutter scrollbar behavior; open image dialogue captures wheel input. Browser verified unfocused dialogue composer scrolling from 0 to 140 with zoom unchanged at 100%, and the indicator hidden after idle. Preserve toggle remains open and visibly checked. Output drag produced a blank, unselected image with zero open editors. A terminal wire cut removed its connection, hid both history dots and left the menu unchecked; cuts archive old text outside active context. Interior-edge rejection is covered by tests. Two grafted style inputs showed a 16-unit capsule and generated two results (8 to 10) in one panel. Four-slot cap and matching edge cases pass tests. Six relevant Node suites passed. Source-image/text right-clicks and receiver ports reach dedicated graft menu targets. Updated research and contract; no Rust builds or edits.

Screenshots: images-dialogue-v11.png and images-graft-v11.png. Current tab retains the Inputs example for review. Grafting is a deterministic browser simulation; provider scheduling is documented for future native work.


Blank surface fix: the white SVG rect omitted the shared image mask. Applied that existing mask to the blank rect, restoring all ordinary port notches and the collapsed blister relief. Syntax and existing layout test pass. Browser checked the user-created blank 680 × 300 card at 84% and toggled its editor open/closed. Capture: blank-image-mask-fix.png. HTML/JS only.
