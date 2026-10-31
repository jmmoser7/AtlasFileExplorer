# Agent interaction studio

This is the design-prototype stage requested after the 78-pattern survey. It does not modify Slate's runtime agent implementations or call providers.

## Accepted direction

- Dashboard provider/model/effort/speed controls; chat train conversation flows; free-agent single-window, delivery and activity patterns; image result/editor patterns; right-click menu skin.
- Global persistent access across agents and app instances; explicit API-key submenu actions; interactive image mode off by default.
- A fourth continuity wire for image and dashboard tools; cutting it resets downstream context and preserves existing outputs and prompts.
- Effort may change during supported tasks; unsupported updates are visibly queued. Remember model choices in every mode.
- Replace isolated Single/Live labels with Generate on request / Regenerate when inputs change.
- Unselected survey rows are undecided, not rejected.

## Work and validation

1. Preserve submitted survey preferences and derive scenarios â€” complete; exact selections/comments in survey-preferences.json.
2. Build one shared menu and toolbar owner for every sample mode â€” complete.
3. Add sample conversation, output, continuity, activity and permission flows â€” complete; sample behavior only.
4. Browser-check interaction semantics, persistence, menu nesting, keyboard and narrow layouts â€” passed; results in validation/checks.json.
5. Independent review against the submitted preferences â€” complete; repaired queued delivery, continuity retention, shared credential status, source-chat binding, branch retention, and independent-mode keyboard parity.

## Simplification after user review

The user could not distinguish the product interface from the studio. Removed persistent studio navigation/inspector sidebars, repeated descriptions and status. A single blue-grey Studio controls strip chooses Interface and Example; Setup and Feedback start collapsed. One labelled Interface under review frame holds only the product preview. Access is now inside the product toolbar. Browser checks confirm the collapsed default, Access ownership, all six modes, and narrow layouts. The existing in-app tab was refreshed and visually verified; validation/studio-simplified-live.png records that view.

The right-click recipe is `design/right-click-menu-studio/drafts/2026-10-12-pass-03.json`, governed by `crates/atlas-shell/MENUS.md`: 256px menus, 33px rows, 18px icons, 8px inset, 12px divider region, 10px corners. These are reference metrics for the prototype, not a change to global production tokens.

## Reset to native chat train only â€” 30 October 2026

The user rejected the invented multi-modality preview and requested a faithful native chat-train baseline before any refinements. The active index now loads only chat-train/reference.js, model.js, app.js and style.css. Prior prototype and submitted survey files remain preserved, but are not part of the active preview.

Reference audit covered portal_header, portal_bound, portal_unbound, card_metrics, card_fit, composer, spawn, train, header_menu, results, bundle, and shared shell tokens. The existing cached CT5 renderer test was executed without a provider; its unmodified images and bundled Ubuntu font are included with source provenance in chat-train/BASELINE.md. A source-based browser rendition does not prove identical native rasterization; independent visual review identified small wrapping differences.

Implemented independent connected cards, tail composer, unsent continuation click/drag, explicit historical forks, native presentation rechunking, inline title/model editing, three fold states, authored resizing/Fit, local undo/redo, pan/zoom, streaming sample/Stop, and browser persistence. Native Full conversation retains its current checkpoint range; it does not silently invent ancestor expansion. Global agent redesign and production provider work remain for later review.

Validation: model behavior groups and focused Playwright interaction checks, plus independent visual comparison to native captures. User-facing tab refreshed only after the replacement was functional. Replies and schedule/access controls remain explicit local simulations; no native production files changed.

## Refinement 1 â€” agent picker and single window

The active picker now separates Provider, Model, Effort, and Speed into four compact rows. Each opens a regular choice page with Back navigation, following the dashboard generator pickerâ€™s navigation pattern. The provider list gives OpenAI API a split row: the main row selects the provider; its independent chevron opens Add / Change API key. A dedicated password field captures that review flow. Only status is saved, shared across studio instances; credential text is discarded. Provider capabilities and model names remain explicit fixtures.

Single-window mode now uses a readable 440-unit width and 340â€“560-unit fitted height. Its plain transcript scrolls independently, shows You/Agent captions, and retains one pinned composer and Send action. It no longer inherits a tiny preceding cardâ€™s height. Window transitions preserve configuration, logical drafts, branches, and checkpoints; the active window is brought into view. Browser-local legacy state migrates without the stale height cap. Changes to provider/model/effort/speed apply to the next message and do not rewrite historical settings.

The ellipsis menu groups chat presentations, Full conversation, and fork/access/schedule actions into separate sections. The review fixture now contains three substantial prompt/reply pairs. The two chevron hit regions are independent of the shared visual glyph, correcting a bug where a full-collapse click could hit the one-step control.

Model checks: 20 behavior groups passed. Browser checks through the supported in-app browser verified four-field selection, provider changes, effort/speed independence, the API chevron and masked editor, menu group boundaries, 440Ã—560 single-window overflow with one composer, and the actual pointer-driven fold sequence Open756 â†’ Partial197 â†’ Collapsed98 â†’ Open756. A separate headless Chrome launch was unavailable in this environment; do not interpret earlier green baseline UI checks as a full rerun of this refinement.

Additional browser checks on the long fixture: an unsent message survived Window â†’ Pairs â†’ Window, and Send/Stop retained eight visible messages in one window with exactly one composer. API key dialog cancellation did not change the selected provider. The final review tab uses the long sample; previous survey and note storage remains intact.

## Refinement 2 â€” compact anchored choices and persistent geometry

Provider/model/effort/speed submenus keep the selected section header at its original datum, reveal choices below it, hide subsequent sections, and toggle shut from that same header. They use the chat ellipsis menuâ€™s232-unit width. Provider marks are copied from the existing shell icon catalogue (ProviderCursor and ProviderCodex/OpenAI), preserving those source paths. Header feedback uses the same small chevron language, a speed lightning mark (paired in Fast), and a muted effort label.

The scroll indicator is an overlay thumb with no reserved text gutter. It appears during scrolling and fades after650ms idle; browser checks observed opacity0.65 while scrolling and0 afterward. Single-window user text is indented28 units and uses stronger ink; agent text uses secondary ink, following the native agentic transcriptâ€™s outgoing/incoming distinction.

Horizontal edge resizing stores width with automatic height, rather than freezing both dimensions. Explicit vertical resizing still records a chosen height. Both survive collapse/expand; Fit to text remains the explicit reset. Full conversation now invokes full expansion and preserves the message-pair divider. Legacy pair Full detail is migrated back to pair formatting.

Card-body gestures use click-to-select followed by text selection. An unselected body drag moves the card; a static click selects it. Controls and composer remain directly operable. Browser checks confirmed a50-pixel unselected drag moves the card and the following text drag keeps its position unchanged.

Validation:21 model groups plus syntax checks pass. Browser validation confirmed resized card320Ã—676 â†’474Ã—531, then474Ã—531 after collapse/reopen; transcript content485 exactly fits viewport485. Provider submenu anchor was identical before/after expansion, width232, with no Back row. Full conversation expanded to Open with the pair divider intact. No production Slate code changed.

Long-name stress case: the Cursor fixture includes an intentionally oversized198-character model name. Picker rows and the card header elide it independently, while preserving the complete value and hover text. Browser validation confirmed the card stays320 units wide, the label elides at121 units, and effort plus the dropdown chevron remain visible. The full raw name remains stored for the chosen model.

## Refinement 3 â€” capability contract and live-resize repairs

agent-catalog.js is the pure shared source for provider/model effort and speed options. An empty array means unsupported, a one-item array means fixed, and a longer array supplies only those choices. The same normalization runs in the picker, projected next-turn settings, setAgent, and send. Provider registration rejects omitted option arrays and unregistered metadata. registerSpeed/registerEffort permit explicit future labels; speed metadata includes the0/1/2 bolt count. Fixture profiles include limited effort (Luna), no effort/speed (Local), fixed speed, and Standard/Fast/Ultra (Sol). These are design fixtures, not live availability.

Corrected the cumulative live-resize bug: transcript bounds now come from fresh width measurements, not the previously resized height. Composer size and separator update during the gesture; streaming repaint and camera zoom cannot replace the active preview mid-drag. Horizontal resize fits all content and preserves width. Full conversation and full expansion remove fixed/manual and normal single-window height ceilings while keeping the width. A22nd model group covers full expansion and persistence.

Menus now share all three presentation choices (current checked), Full conversation, Fork chat here, Full access, Schedule, and Fit to text. Fork is disabled on an unsent draft with an explanation; Delete appears only on leaves. This preserves parity without pretending a checkpoint exists before a draft is sent.

Validation: capability tests and22 model groups passed. Browser checks observed Standard0/Fast1/Ultra2 complete bolts; Luna only Low/Medium; Local effort/speed disabled. Upstream and draft menus share sections. Three consecutive width gestures produced491Ã—531,297Ã—805,397Ã—611, with transcript content exactly matching its viewport each time. Full conversation changed the window from560/view458/content2180 to2286/view2184/content2184.

## Locked Studio baseline â€” 30 October 2026

Chat Train is now the first Chrome-style Studio tab. A new element tab starts with its own empty canvas; it cannot replace or overwrite Chat Train. Debrief reads the standalone contract and offers the Markdown download. Temporary tab creation/switch/removal was verified without changing the retained chat text.

contracts/chat-train.md locks the preserved native baseline and23 accepted changes, maps35 in-scope registry dimensions, lists feel constants and14 golden paths, and records the tested/prototype boundary. contracts/decisions.json mirrors the accepted rows. contracts/PATTERNS.md and TEMPLATE.md provide parity references for later elements. The scoped AGENTS.md and top-level AGENTS.md point future design iterations to this primary workflow.

Verification: contract row/decision parity and local references passed; browser verified Chat Train first/selected, independent blank canvas, retained conversation on return, and successful debrief heading/rendering. This locks the review design, not a claim of native production shipment.

## 2026-10-30 â€” Image Generator first pass

Added Image Generator as the second permanent Studio tab, opening its own canvas. Preserved native compact prompt/aspect/Single-Live/seed/count controls and responsive row thresholds. Reused Chat Train's picker via an optional per-open catalogue argument, with image-only native model fixtures and explicit unsupported capabilities. Kept the original Chat Train lock manifest; recorded the shared-code compatibility extension in the image draft and verification record. Added local sample output, status, Stop and history interactions for review. No production integrations or approved image-design decisions were created.

## 2026-10-30 â€” Image Generator native parity correction

The user rejected the first schematic preview and requested one-for-one native presentation. Replaced its active HTML/CSS element painter with a paint-only bridge around the existing Slate test Harness, production image painter, property UI, ports and shared layout owners. Kept the approved model picker as the explicit exception. Added independent small, medium, large, image-plus-text input and three-step generation-series scenes. The fourth history wire is a requested Studio addition with inspect/disconnect/restore, not a new production InputSlot. Native dropdowns, attachment eligibility, prompt metrics, selected decoration and viewport placement remain native-owned. The two retired first-pass painter files were removed; the original screenshot remains as historical evidence. Native build and visual validation are tracked in image-generator/REBUILD.md.

## 2026-10-30 â€” Browser-only Studio boundary restored

The user clarified that the Studio must remain a quick HTML/JS/CSS iteration platform. The native-renderer approach was the wrong tradeoff. Stopped the Rust build and reverted the Rust edits introduced for the preview, preserving pre-existing native changes. Removed the native-build launcher and recorded the browser-only boundary in the Studio instructions. Native source and captures are references, not a runtime dependency for subsequent iterations. The interim image canvas still needs conversion away from its native bridge; do not describe that conversion as complete.

## 30 October 2026 — Browser-only Image Generator rebuild

Replaced native.js/native.css and the port-8766 renderer bridge with app.js, layout.js, view.js, style.css and local photo/icon assets. Kept all five scenario groups, shared picker, editable prompts, attached/detached geometry and inspect/disconnect/restore history wires. No Rust source edits or builds. Stopped the obsolete image-studio-first renderer; the static server remains running. Updated the draft contract; visual lock remains pending.

## 30 October — Images interaction refinement

Generalized the image tab to Images. Added shared Slate navigation and its contract; fixed popup dismissal and focused-only prompt wheel ownership. Added compact top-datum circular history, Aspect Ratio/Custom, Mode/Settings overflow, rounded detached card, mutually exclusive square hover results, continuous hover bridge and image/result context menus from the native recipe. Added basic image-tool interaction specimens. Applied common code to all fixtures; no Rust edits or builds. Explicit requirements are recorded while pixel-level visual review remains open.

## Images v0.5 — 30 October

Shared wire-handle/cutout experiment, borderless results, wrapping square grid, all 12 small-fixture circles, permanent count and compact header. Recovered saved right-click Studio recipe; added independent menu targets and browser-persisted editor with export. Removed provisional non-agent panels from the prototype and specification. No Rust edits/builds.

## Images v0.6

Tightened model/ellipsis grouping; restored remembered agent panel on image reselection; made right-click a direct target-specific authoring gesture with capture-phase suppression of default context menus. Added editable targets beyond image body/results plus action and divider insertion/removal. Browser verified preserved prompt/count and independent target selection. No Rust work.

## Images v0.7

Clarified running-bond preview rows, restored rounded corners, retained previews while selected and cleared the previous image on a new selection. Added direct dragging of menu preview names/dividers, insertion indicators and keyboard movement; removed arrow buttons. Browser verified order changes and Cancel, selected persistence and deselection. Shared across image examples; native code untouched.


## Image action refinement — 30 October 2026

The native return grace is 1500 ms (board_properties/selection.rs). Reopening after longer absence no longer applies. Cutout shoulder fillet is 40% of the indent radius. Shift-click selects a contiguous result range in either circular history or square previews; selected results share the thumbnail action set with an added Save selected images action. Active-result copy and duplicate flatten only the active image, never its history. Duplicate creates an independent plain image on the canvas. Saves use a persistent image-0001.png, image-0002.png sequence across single and batch operations. Shared result-menu actions follow primary-image ordering while retaining user labels and action selections. Because right-click is still assigned to authoring, Try selected action in the Menu Studio executes the selected action for testing. Browser verified four selected results and the first sequential download; external image clipboard falls back to the Studio copy buffer if unsupported. Native Rust remains unchanged.

## Shared visual parity and history workflow

Capped result circles at 18; separated thumbnail selection from body selection; corrected attached-blister offsets, apron backing and wire strokes. Added optional in-image conversations, explicit history initiation and downstream branches. Context snapshots remain independent; detach preserves work and refresh/reconnect queues context for the next turn. Added history.js and focused state tests. No native build or edits.


## Simplified history interaction — 30 October 2026

One Preserve model history toggle enables continuous in-window dialogue and inherited history on output drag. Browser verified output drag creating a child with inherited conversation, selected history-wire Delete reducing four connections to three, and reconnection restoring four. Original prompt draft restored. Output port sits above the resize hit area. Composer uses continuous Chat Train styling. All five relevant Node suites passed. Native wire colors explicitly retained; no Rust edits or builds.


## v1.1 — dialogue parity, terminal cuts and grafted inputs

Reused Chat Train single-window typography, indentation and transient zero-gutter scrollbar behavior; open image dialogue captures wheel input. Browser verified unfocused dialogue composer scrolling from 0 to 140 with zoom unchanged at 100%, and the indicator hidden after idle. Preserve toggle remains open and visibly checked. Output drag produced a blank, unselected image with zero open editors. A terminal wire cut removed its connection, hid both history dots and left the menu unchecked; cuts archive old text outside active context. Interior-edge rejection is covered by tests. Two grafted style inputs showed a 16-unit capsule and generated two results (8 to 10) in one panel. Four-slot cap and matching edge cases pass tests. Six relevant Node suites passed. Source-image/text right-clicks and receiver ports reach dedicated graft menu targets. Updated research and contract; no Rust builds or edits.

Screenshots: images-dialogue-v11.png and images-graft-v11.png. Current tab retains the Inputs example for review. Grafting is a deterministic browser simulation; provider scheduling is documented for future native work.


## Dashboard Builder, Text Agent and Rhino 3D Studios

Added three separately selectable canvases with one shared agent runtime and independent preview modules. Reused model/capability picker, explicit API-key entry, compact menu patterns, Slate navigation and transient scrollbars. Added public activity/script previews, Stop, in-window dialogue and inherited chat-train branches. Text adds persistent selected-passage queries and range-validated Apply. Dashboard stages Apply/Discard revisions. Rhino provides a projected 3D model and generated Python demonstration, with no live host calls. Contracts and Studio-level debrief added; layout remains draft pending review. Browser flow checks and seven focused Node suites passed. Owner requested a repository commit of the Studio checkpoint; scope is design/agent-interaction-studio only.


## Native baseline correction for Dashboard, Text and Rhino

Owner rejected the first-pass application wrappers. Audited the native shared generator and property-strip owners, plus text-note and 3D display sources. Rebuilt the three host frames from ImageStudioLayout with native tool counts/order. Removed invented topbars, revision footers, mode/readiness bars and Progress/Script tabs. Native Settings actions and circular history share the compact top datum; folded public activity precedes the prompt. Kept only explicitly requested text selection/dialogue additions and renderer-only sample content. Canonical contracts rewritten for v0.2; source audit and replacement screenshots identify native facts versus inherited Studio refinements. No Rust edits/builds.

## Generator parity v0.3 — 30 October 2026

Applied the owner correction to use Images as the literal component precedent. Extracted the conversation/composer/count/model label into generator-ui.js and reused it in Images and the common Dashboard/Text/Rhino runtime. Removed the redundant native Settings commands and separate activity row; working feedback and Chat Train hover chevrons occupy the header datum. Preserved text selection targeting. Added connected source creation and output-to-input dragging to all three canvases, calling ImageGraft for shared matching/capsules and ImageMenuStudio for input recipes. Result count creates real simulated alternatives per job. Outside click dismisses pickers without closing the agent. Downstream output drag now creates blank, closed surfaces. Updated v0.3 contracts and explicit precedence over the rejected native Settings layout. No Rust edit or build.

## Preview and answer-link refinement v0.4 — 30 October 2026

Removed Dashboard Apply/Discard from the shared options menu. Fixed the card rebound check before selection and added remembered model/options popup rebound within 1500 ms. Reused Images grid geometry and 18-unit header sizing for actual miniature output rendering across Dashboard, Text and Rhino. Square selection stays distinct from body selection. Added exact text-to-response links and an in-place question/answer viewer; unrelated edits rebase anchors, while edited passages invalidate them. Updated the per-element contracts and shared patterns. HTML/CSS/JS only; native code and builds untouched.
