# Portal capture feedback refinement

User direction, 14 October 2026: video appeared inactive; show an exterior
timer with Pause leading to saving or placement on the board. Match the
existing blister menus. The durable interaction answers are in
[portal-capture](../contracts/portal-capture.md), D03/D09/D11/D12.

## Visual observations

- [Dynamic panels](../../../crates/atlas-shell/DYNAMIC_PANELS.md), capsule
  controls: the shared fillet capsule is 22.1 units high, with half-height
  corners, themed panel fill, an inside border and small shared canvas type.
  Stacked controls omit an enclosing tray, large header or footer.
- [Text controls](../../../crates/atlas-shell/src/selection_tools/text_controls.rs)
  combine compact segments on one datum; [layer palettes](../../../crates/atlas-shell/src/selection_tools/layer_palette.rs)
  reuse capsule presentation instead of introducing a separate menu skin.
- [Selection tools](../../../crates/atlas-shell/src/selection_tools.rs)
  own side-switching popup placement, top-layer popup input and painting,
  capsules, segments and theme colors. Applications supply state and commands.
- [Alignment](../../../crates/atlas-shell/src/selection_tools/alignment.rs)
  keeps control geometry in the shared shell. Capture follows that boundary.

## Resulting presentation

Reference inspection: the October image Layers and Filters references show a
continuous full-width apron, a low centered tab with bare glyphs, shallow
controls and no second surrounding tray. Layer/filter circles communicate
their preview content; they are not a license to make action icons circular.
The native wire-property fixtures in both themes confirm the shared segmented
control treatment, quiet borders, inline values and consistent hierarchy.
These are reference observations, not verification of the new capture UI.
The reference index explicitly warns that its old gray value rail and earlier
capsule thickness are superseded by later guide decisions; those historical
pixels must not override the current owners.

The launch controls occupy the ordinary full-width attached blister apron:
still destinations and Start video. The recording controls show a status dot,
elapsed time, Pause and Discard. Paused offers Save File, Place Board, Resume
and Discard. Responsive rows derive from the host's board-unit width; zoom
never changes the topology. Shared segments sit directly on the apron, with
no redundant outer capsule, context-menu tray or heading block. Saving retains
feedback until the worker completes.

Launch is `Panel::Capture` in the existing property pipeline. Runtime transport
is source-bound independently of selection and uses the same apron composition
owner. Host, panel, text, icons, spacing, borders and hit regions share the
camera transform. Source-body interactions and camera navigation retain the
editor. Secondary/middle drags are distinguished from completed clicks by the
existing property-dismiss owner. The chrome-pan owner also handles a pan that
starts over controls. Plain property controls permit board wheel navigation;
actual navigable dropdowns retain the shared wheel owner.

Earlier text claiming a screen-space capture exception, fixed exterior strip,
or automatic board-camera fitting was incorrect. It is superseded by P0.9,
P1.shape.properties and the newer rectangular blister rules. Offscreen controls
follow normal clipping and LOD; capture never changes the ordinary board camera
to keep a panel visible.

Starting stays distinct until an accepted frame arrives. The no-frame timeout
uses delivered frames rather than request cadence. Pause freezes both the
readout and the media timeline. File-picker cancellation leaves the draft
paused for another choice. Private draft encoding and final publication
remain asynchronous and bounded.

Native runtime verification remains pending at the user's request. The
contract stays agreed until its golden paths execute successfully.

## Proposed codification — awaiting visual approval

The governing style document already exists: `crates/atlas-shell/DYNAMIC_PANELS.md`.
Do not create another blister style master. These are proposed clarifications
to those existing owners, not ratified rules:

- State the existing mouse-button distinction explicitly: primary outside
  press may dismiss; secondary/middle navigation retains the editor; a
  completed no-drag outside click follows canonical dismissal.
- Describe capture-specific maximized allocation: fit source plus its
  maximum board-unit recording apron through the existing fit transform,
  using one scale for both and leaving authored geometry and board camera
  untouched. Reserve maximum phase height to avoid a source-scale jump at
  Pause/Save. Keep the established web caption behavior. This is an
  implementation choice for review, not a general screen-space exception.
- Extend dynamic-panel rule globs to the newer selection_tools submodules
  and capture adapters so the existing rules are discoverable at edit time.

Acceptance follows the existing style checklist: production controls in both
themes; host widths 220/360/680 board units at zoom 0.5/1/2; pan beginning
outside and on controls; host-body retention; viewport edges; source focus;
selection changes; lifecycle invalidation. Source checks and compilation do
not establish visual or runtime acceptance.
