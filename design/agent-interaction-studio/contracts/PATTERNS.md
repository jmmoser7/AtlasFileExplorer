# Studio shared interaction patterns

Status: agreed in the Chat Train review, 30 October 2026.

These are design precedents for later Studio elements. Reuse an applicable pattern; record an explicit, user-approved deviation instead of creating a slightly different control. A pattern does not grant production authority or require every element to use every control.

## S01 Compact menus

Use the chat action menu's width, row alignment, icon column, low-contrast hover and section rules. Each row is a short label, optional current value, and optional chevron/check. Explanations belong in hover help, not paragraphs. Subselection opens below its stationary header and replaces lower rows; the same header/chevron closes it. Keep the label, icon and selected-value datum fixed. Related actions share a section; distinct meanings use dividers. Corresponding states keep the same section vocabulary, disabling inapplicable actions with a reason.

## S02 Explicit agent configuration

Provider, model, effort and speed are distinct choices. One catalogue owns each model's allowed options and validation before a request. Empty arrays mean unsupported; fixed/unsupported settings are visible but not selectable. A new provider declares capabilities rather than inheriting imaginary options. Provider-key editing uses a separate chevron and masked Add/Change action, never the chat composer. Settings changes do not send a message.

## S03 Continuous conversation feedback

Use subtle indent and ink differences to distinguish the person's text from the agent's. Avoid decorative role bubbles or redundant role headings. The composer remains pinned and independently editable. An overlay scroll thumb appears during scrolling and fades after idle without reserving a gutter. This also applies to scrollable dropdown menus; no persistent scrollbar or text offset is permitted. Show meaningful running status and an explicit Stop control; do not rely solely on a spinner.

## S04 Content-driven geometry

Authoring width reflows text and computes height from that width. Recompute preview bounds from the current input, never from a previously mutated preview. Keep width through folding and reload. Full expansion fits all content; normal bounded conversation windows may scroll. Fit to text explicitly clears authored size. Zoom scales world geometry without changing line breaks solely because the camera moved.

## S05 Direct manipulation before text selection

Dragging an unselected card's ordinary body moves it. A stationary click selects the card; a subsequent text gesture highlights text. Controls and composers retain their own actions. One gesture commits one undoable action; cancellation restores pre-gesture geometry.

## S06 Compact selection feedback

Use established small single/double chevron geometry and the shared header datum. Show selected effort beside the model in muted text. Standard speed has no bolt; Fast one complete bolt; Ultra two complete bolts. Future labels and bolt counts come from explicit metadata. Long labels elide while retaining their full value and hover text; nearby effort/speed/chevron controls remain accessible.

## S07 One element per review tab

The Studio shell and reviewed element remain visibly separate. Chat Train is first. Each later tab owns an independent canvas for one element. Switching tabs does not rewrite Chat Train's history or decisions. Debrief opens the element's standalone contract. New elements start without approved decisions; approval is earned through the conversation and recorded explicitly.

## S08 Slate canvas navigation

Locked by owner request in the Images review, 30 October 2026. Use the same navigation in every Studio canvas: right-drag, middle-drag or Space+left-drag pans; wheel zooms about the pointer; Shift+wheel pans horizontally. A stationary right-click opens the target's context menu. Crossing the 4-screen-pixel drag threshold suppresses that click's menu. Zoom uses Slate Board's exponential coefficient 0.0015 and range 1/128 to 128. Hovering a text box does not consume zoom; a focused text editor owns ordinary scrolling while actively editing. Ctrl+wheel remains explicit zoom. Outside click and Escape dismiss menus. Reuse canvas-navigation.js and its profile rather than inventing another set of controls. Native reference owners are app/board.rs and atlas-core/src/display.rs.

## S09 Image result hover and selection continuity

Requested shared image behavior, 30 October 2026. The generator's circular result history and the canvas's square result thumbnails are mutually exclusive. Circular history belongs to the generator's top datum. With image panels closed, hovering the image exposes square thumbnails without opening the toolbar. The image, gap and thumbnail strip form one hover region; retain it while moving onto a thumbnail, selecting it, or opening its context menu. Context menus pin the strip until dismissed. Use independently configurable image-body and result-thumbnail action sets, scoped to the right-clicked result and rendered with shared menu chrome. Result images have no border or internal padding. Square results wrap after eight columns in a running bond with alternate rows offset by half an image; the 24-unit thumbnails have 4-unit fillets. The hover bridge includes every row. Selected images retain their square previews until another image or the canvas is selected, unless the open generator is showing circular history. Visual review of the implementation remains open.

## Promotion and amendments

Each contract names its S-patterns and deviations. An additional confirmed shared pattern receives the next S-ID and a dated decision. Reconcile these precedents with native owners during implementation; they are not a parallel production command/theme system.


### S08 image dialogue exception — 30 October 2026

An image-agent dialogue already explicitly opened by the user captures ordinary wheel input over both transcript and composer, without a second focus click. Ctrl+wheel still zooms. Ordinary unopened or unfocused image prompt fields keep the existing canvas behavior. This exception does not change Chat Train navigation.

## S10 Shared generator assembly — 30 October 2026

Images, Dashboard Builder, Text Agent and Rhino reuse one dialogue/composer/count/model-label implementation (`generator-ui.js`), with the existing image layout, grafting, model picker and menu editor. The owner explicitly rejected per-modality reinvention. One Preserve model history toggle governs in-place dialogue and automatic downstream context. New downstream surfaces start blank with controls closed. Grafting uses the same source channels, capped capsule and longest-list matching in every applicable host. The working indicator and hover chevrons share the header datum; public updates belong in the dialogue. Host-specific property tools remain local; the dashboard three-dot menu is Preserve model history only. See the v0.3 per-element contracts for complete behavior and limits.

### S09/S10 extension — non-image previews and return grace

The owner extends Images result previews to Dashboard, Text and Rhino: each circle and square is a thumbnail of its saved output. Closed-agent square previews keep the same geometry, running bond, hover bridge, persistence and separate preview/body selection behavior. Returning to a dismissed card or the last-open model/options menu within 1500 ms restores it; later selection does not. Native production styling is unchanged.

### S11 Linked text answers

After a query about selected text completes, that passage links to the recorded response. Hover/focus shows the question and answer without moving the reader; activation scrolls the agent dialogue to that response. Offsets rebase through unrelated edits and invalidate when the linked passage changes. Links and prior answers do not alter model-history preservation settings.
