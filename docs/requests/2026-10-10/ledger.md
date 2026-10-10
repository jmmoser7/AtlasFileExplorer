# Request ledger — 2026-10-10 Slate feedback

Every request the owner made on 2026-10-10 has one row. This file is the
contract between the owner and the agents building it.

## How this ledger works

1. **Nothing is dropped silently.** An agent reports every id it owns as
   `done`, `blocked: <reason>`, or `needs-decision: <question>`. "I chose not
   to" is not a status. If an item turns out larger than expected, finish the
   others and report it `blocked` with what remains — do not quietly skip it.
2. **Look first, then rigor.** Every item marked *sheet* gets a review sheet
   before polish: an `#[ignore]`d test in `apps/slate/src/app/tests/review/`
   that calls `review_shot(h, raster, out, "<id>", "<state>")` for each
   state named in the row, writing `target/review/<id>/<state>.png`. Shoot
   both themes when color is involved and three zooms (0.5, 1, 2) when size
   or text is involved. The coordinator looks at every PNG before merge.
3. **Preview before main.** Look-and-feel branches merge into a
   `preview/2026-10-10` branch; the owner tries that release build. Only
   after the owner approves does it reach `main`. Pure bug fixes with a test
   that fails before the fix may go straight to `main`.
4. **Acceptance is the row, not the agent's judgment.** If the row and the
   code disagree, the row wins or the agent asks.
5. **Two working modes; the owner says which.**
   - *At desk:* show direction within about 15 minutes. A look-and-feel item
     first sends one screenshot (a non-visual item, a short plan) and waits
     for a yes or no before it is built out. Ask about ambiguity at once
     instead of guessing. Items reach the preview as each one passes, and
     the owner gets a build at least hourly. An item with nothing reviewable
     after about 45 minutes stops and reports.
   - *Away:* grind to finished. Make the reasonable call on an ambiguity,
     report it as `decided: <what and why>`, and keep going.
6. **Light review.** A test that fails without the fix plus a screenshot that
   matches the row ships. The coordinator goes back to an agent only when the
   screenshot shows a defect.

Decisions the owner made today: streaming replaces the model name in the
chat top bar; a highlight dragged out of an image becomes a cut-out sticker;
a pack that only lacks an API key shows in its chooser as "add API key"; the
selection-grow gesture is a proposal plus prototype, not production code.

## Chat train card — workstream A

| Id | Request | Acceptance | Sheet |
|---|---|---|---|
| CT1 | Chevrons step one level at a time across collapsed, partial, and maximized. | From maximized: single chevron goes to partial, double goes to collapsed. From collapsed: single goes to partial, double goes to maximized. From partial: both directions work. A test drives every transition. | collapsed, partial, maximized, each with chevrons hovered |
| CT2 | A chevron owns the pointer over its hit zone. | Over either chevron there is no resize cursor and a press never starts a frame resize. | — |
| CT3 | Minimal chevron glyphs. | Thinner strokes, glyphs packed close; the double chevron is no more than ~1.4× the single's height. | close-up at zoom 2 |
| CT4 | No hover text on chevrons. | No "expand fully" / "partial expand" tooltip; hover is a highlight only. | hovered |
| CT5 | Top bar as in the owner's reference: left `● Program · Model`, right `⌄  •••  ●`. | Nothing else in the bar (move anything else into the `•••` menu). While a reply streams, the model name is replaced by an animated `Responding 1.2k` (streamed token count) and reverts when done. | idle, streaming, light and dark |
| CT6 | Partial state scrolls. | Wheel over a partial card scrolls its transcript (the card owns the wheel per P0.10; the board does not zoom). Pinned to the bottom while streaming until the person scrolls up; earlier messages are reachable. | partial at bottom, partial scrolled up |
| CT7 | Same text datums in all three states. | Left text inset, right inset, and wrap width are identical in collapsed, partial, and maximized. A test asserts it. | the same transcript in all three states |
| CT8 | Composer gap is one line. | Space between the last text line and the `Message` composer is one line height (today about three). | partial, maximized |
| CT9 | Subtle input-dot split. | When an input dot splits, the gap between its halves is 50% of the dot radius. | split at zoom 2 |
| CT10 | Link dots sit on the same offset. | The top and bottom link dots are inset from the top and bottom edges by the same distance the side dots are inset from the side edges. | card with all dots visible |

## Text — workstream B

| Id | Request | Acceptance | Sheet |
|---|---|---|---|
| TX1 | Text never reflows when the board zooms — chat trains and every other canvas text (text nodes, shape text, snippet and sheet cards, labels). | Line breaks are computed once in world units and the whole block scales uniformly with zoom. A test sweeps zoom 0.25 to 4 and asserts identical break positions and proportional line offsets for a chat card (static and while streaming), a text node, shape text, and a snippet card. Find and fix every paint path that still wraps at screen size; `canvas_text::world_layout` landed earlier but the owner still sees jumping. | each of the four objects at zooms 0.5, 1, 2, 4, cropped and scaled to the same size so breaks can be compared |

## Board regressions — workstream C

| Id | Request | Acceptance | Sheet |
|---|---|---|---|
| BR1 | Placed and pasted images keep the source aspect ratio. | File drop, clipboard paste (including snippet bitmaps), and generated images all keep width/height of the source. This was fixed about a month ago: find the regressing commit and name it. Test fails before the fix. | — |
| BR2 | PDFs keep each page's own size and orientation. | A portrait letter page drops as portrait. A multi-page PDF with mixed page sizes gives each page (card and unbundled pages) its own dimensions. Test with a fixture of mixed sizes. | mixed-size PDF unbundled |
| BR3 | One hit order for the whole board. | The topmost painted node receives the press, whatever its kind. An image lying on a chat card can be grabbed. Same rule for portals and agent cards. Test covers image-over-chat and chat-over-image. | — |
| BR4 | Pasted images feed image generators. | An image pasted from the clipboard can be wired into an image generator input exactly like a placed file. Test fails before the fix. | — |
| BR5 | Cursor feedback on wire handles. | On every node kind, the cursor changes (grab style) inside the wire-grab hit zone, in addition to the handle appearing. | — |

## Image highlight — workstream D

| Id | Request | Acceptance | Sheet |
|---|---|---|---|
| SG1 | Hover highlights; click opens the tag. | Hover only draws the highlight. Clicking the highlight opens a minimal squircle tag just above the cursor, in the existing squircle style. | hover, tag open, light and dark |
| SG2 | First tag option: Create layer from highlight. | Creates a layer through the existing image-layer system (journaled); the layer is then reachable from the layer squircle at the top of the image frame. | after creating, layer menu open |
| SG3 | Drag the highlight out of the frame for a standalone sticker. | Dragging the highlight outside the image frame places a cut-out sticker node: the image pixels clipped by the traced outline (SVG-expressible clip path, same in the board painter and the HTML export). Journaled, undoable. | mid-drag, placed sticker |
| SG4 | The Layers option is missing from the image squircle menu. | Restore it; name the commit that removed it. Test that the menu lists it. | image menu open |
| SG5 | Grow-selection gesture (owner wants to brainstorm). | `docs/proposals/segment-grow.md`: 2–3 options for slow-drift-grows, quick-retreat-undoes, and zoom-sets-feature-scale, plus edge-quality options, each with cost; one rough prototype behind a debug flag. No production behavior. | prototype frames |

## Chrome — workstream E

| Id | Request | Acceptance | Sheet |
|---|---|---|---|
| CH1 | Bottom toasts are readable. | Wrapped at a sensible width (never one word per line), always above the bottom tool palette with clear spacing, never behind it; both apps (atlas-shell). | short and long toast, light and dark |
| CH2 | Suggestion box matches the tool palette. | Bigger, moved slightly in from the corner, same height and baseline as the bottom tool palette. | bottom edge of the window |
| CH3 | Suggestion box clears the readout chevron. | Never overlaps the readout bar's expand/collapse chevron, readouts open or closed. | readouts open, closed |

## Packs — workstream F

| Id | Request | Acceptance | Sheet |
|---|---|---|---|
| PK1 | Pack registry, phases 1–3, per `pack-registry.md` in this folder. | The brief's "Tests that define done" list. Phase 4 is out of scope. | — |
| PK2 | ChatGPT image generation is always offered. | OpenAI image shows in the Generate chooser even without a key, dimmed as `OpenAI · add API key`; clicking opens key entry (stored through `atlas_core::secrets`) with a link to https://platform.openai.com/api-keys. Packs that need an install, not just a key, stay out of the chooser and appear in Connections. | chooser without key, key entry |

## Browser drop — workstream G

| Id | Request | Acceptance | Sheet |
|---|---|---|---|
| BD1 | Spec for dropping browser elements, per `browser-drop-brief.md` in this folder. | `docs/keymap/specs/browser-element-drop.md` exists and covers every point in the brief. No code, contract, or `decisions.json` changes. | — |

## Status

The coordinator fills this in after review. Agents report status per id in
their final reply and do not edit this table (parallel edits conflict).

| Id | Status | Branch / commit | Notes |
|---|---|---|---|
