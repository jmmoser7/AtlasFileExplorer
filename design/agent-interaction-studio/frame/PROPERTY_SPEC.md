# Frame property and multi-slide interaction specification

Draft, 1 November 2026. Browser behavior is implemented. Native raster equality is pending. This extends the original Frame datum and does not authorize a Rust change.

## Source owners

Read `crates/atlas-shell/src/selection_tools.rs`, `selection_tools/{color_editor,color_field,popup_placement,blister}.rs`, `apps/slate/src/app/board_properties/{palette_ui,apron}.rs`, `board_properties.rs`, `board_deck.rs`, `board/board_sheet.rs`, and `crates/slate-doc/src/scene.rs`. Reuse those production owners. The owner-requested blister transition extends the current Frame branch: automatic rectangular media blister eligibility currently excludes Frame. Do not misreport it as an unchanged Rust behavior.

## Measurable geometry

All dimensions below are world units. Screen projection is `world * zoom + camera`. The compact editor stays horizontal as the frame rotates, and follows camera changes. Property popups never shrink to fit.

The table retains the 1 November/native reference. The dated proposal below
supersedes the JS toolbar order/closed width only; baseline files stay unchanged.

| Region | Required geometry |
|---|---|
| Closed squircle menu | Seven 30 × 30 buttons; gap 7; total width 252; silhouette gap 14 |
| Order | Fill, Stroke, Corners, Previous, Next, Present, Deck |
| Blister eligibility | Flat lower edge spans 504; band footprint ≤20% of silhouette bounds |
| Blister | Height 40; shoulder 84; width 432 when editor open; inset 42; icons 31.2 |
| Transition | 120 ms; icons interpolate from squircles to blister; faded squircle surfaces cannot intercept clicks |
| Fill / Stroke / Corners editor | Width 420; heights 174 / 190 / 22.1 |
| SV field | Local rect (5, 5, 396, 70); white-to-hue horizontally, black vertically |
| Rails | x=12, width=396, height=7, first y=96, pitch=16 |
| Rail order | Opacity, Saturation, Hue; Stroke adds Width |
| Rail handles | Diameter 9; no permanent captions or numeric value; transient drag readout |
| Footer | y=height−20; eyedropper center x=21 / size 23; recent centers 51+26i / radius 8 / max 6 |
| RGB | Percent values 0–100; centers 244+65i; font 12; underline numbers |
| Corners treatment | x=2.6, width=126; Fillet / Chamfer; inner y=2.6, height=16.9, font 9 |
| Corners amount | x=134, width=210; handle 20 × 11 |
| Corners units | x=350, width=68; % / u |
| Popup gap / viewport margin | 8 / 8 |
| Duplicate | Each selected node offset (+24,+24); retain title, style and order; select copies |

Popup placement tries the preferred side, then below/right/left of the host, then below/right/left of the strip. Prefer a fully visible candidate intersecting neither host nor strip; the native above-host fallback can overlap the host while clearing the strip. If none fits, retain the preferred location. Do not replace this with arbitrary viewport clamping. The open blister anchor is stable throughout animation.

## Interaction and transaction requirements

| Action | Observable result |
|---|---|
| Press and release Fill/Stroke/Corners | Opens the corresponding canvas panel; no Studio inspector substitution |
| Edit panel control | Live preview on the actual selected frame; suppress single-frame selection decoration/dimensions |
| Choose another property | Commit the previous preview once; open a new preview transaction |
| Click outside / repeat active icon | Commit panel edits once and dismiss |
| Escape | Restore complete pre-panel frame state; no undo entry for a cancelled preview |
| Ctrl+Z during panel preview | Commit then undo that preview; no stranded live edits |
| Drag a rail | Continuous preview; transient value readout; single enclosing panel transaction |
| Click rail handle | Inline typed value; Enter/outside commits locally; Escape cancels that typed edit |
| Enter a numeric value | Clamp finite value to rail range; reject nonfinite input |
| Color controls | HSV preserves hue on grey/black; alpha changes preserve RGB; RGB changes preserve alpha |
| Stroke width | Fraction=`ln(1+width)/ln(10001)`; range 0–10000 |
| Recent color | Commit-local RGB MRU, deduplicated, max six; click applies RGB preserving alpha |
| Eyedropper | Use desktop EyeDropper when supported; otherwise sample a visible frame/canvas color |
| Switch % / u corners | Preserve effective corner amount; percent 100 means half the shorter frame side |
| Duplicate / Ctrl+D | Copies explicitly selected nodes; frame contents stay on canvas unless selected; Alt-move separately copies members |
| Undo / Redo | Ctrl+Z, Ctrl+Shift+Z, Ctrl+Y and top buttons; selectors/buttons do not swallow scene shortcuts; text inputs retain text undo |
| Reload | Retain experiment and undo/redo history; never restore an orphaned open preview |
| Previous / Next | Swap native order values with adjacent sorted frame; preserve the native equal-order no-op after duplication |
| Deck click | Move clicked frame to end of touched prefix; untouched visible frames follow in previous order |
| Deck stroke | Preview only while held; on release compute every segment intersection, including frames skipped between pointer events; first contact wins, ties favor later scene index |
| Deck undo / Escape | Undo restores frame order and touched prefix; Escape during stroke cancels preview without a mutation |
| Deck icon | Toggles Deck / Select; Escape leaves Deck mode |
| Present | Start from selected slide; arrows/PageUp/PageDown change slide; Escape restores canvas camera |

Native deck intersection tests use authored axis-aligned rectangles, even for rotated frames; retain that source quirk. Native previous/next currently patches the two nodes separately; the Studio groups the swap as one review gesture. Presentation currently switches views immediately; native presentation flight remains an explicit outstanding delta.

## Cheap query gate

`FrameStudio.metrics().interaction` reports current selected IDs, undo/redo depths, deck order/prefix, presentation ID, active panel geometry and `propertyTokens`. `panel.geometry` includes host bounds, menu rectangle, icon rectangles, eligibility, reveal fraction, projected popup position and panel height. At a settled open state require reveal=1, eligible menus=432×40, and popup width=420×zoom. Corner panels have height=22.1×zoom; Fill 174×zoom; Stroke 190×zoom. Compare exact colors/enums/action order, finite geometry and source constants before the expensive paired screenshot pass. Existing canonical fixture gates continue to apply.

Run `node check-properties.cjs`, `node check-ui.cjs`, `node --test geometry.test.cjs`, and `node check-baseline.cjs`. Property checks use actual pointer/keyboard actions and save screenshots plus `validation/property-report.json`. Passing browser tests proves these HTML paths only; independent Rust metrics, native interaction journals and paired raster review are still required by ACCEPTANCE.md.

## 3 November 2026 — presentation settings proposal

Status: owner-requested JS iteration and phase-00 image proposal for visual
review; no visual lock, decisions-database change or Rust permission follows.

- Strip order: **Fill, Stroke, Corners, Deck, Settings, Present**; six 30×30
  actions with five 7-unit gaps = **215 world units**. Remove Previous/Next
  squircles; retain the Present eye at the end, deck ordering and presentation
  keyboard navigation. Keep the original seven-button/252 datum for comparison.
- Settings belongs to the existing `FramePropertyPanels` owner. Use the same
  world projection, host/strip placement and stable open-blister anchor; never
  substitute the Studio inspector or a separate settings controller.
- Draft palette: **420 world units wide**, **194 high** with Video open;
  Webpages adds 60 and 3D models adds 30, so all open is **284 high**. These are
  current JS measurements for review, not a user-approved spacing lock.
- Edits preview on the selected Frame. Repeating Settings/outside click commits
  once; Escape restores the complete prior state without an undo step. Switching
  property commits once; Ctrl+Z commits then undoes. Persist per Frame, include
  values in duplicate/undo/redo, and never reload an orphaned preview.

The JS draft stores `presentationMedia` per Frame:

| Field | Proposed choices/default | Meaning |
|---|---|---|
| `video.start` | `inherit` (Media setting, default), `on-click`, `automatic` | Follow media autoplay, wait for a video click, or request playback on Frame entry. |
| `video.loop` | `inherit` (Media setting, default), `true`, `false` | Follow the authored loop flag or explicitly override it for this Frame. |
| `web.onEntry` | `saved-view` (default), `last-view` | Open the saved URL/supported bookmark or retain the latest available session view. |
| `web.savedView` | Nullable URL/view-bookmark fixture | Save current view captures the supported locator/view representation only. |
| `model.interactive` | `true` (requested default) / `false` | Proposed interactive versus frozen 3D presentation; not existing native entry policy. |

Native capability boundaries, inspected 3 November:

- `slate-doc::VideoOpts` already owns trim, autoplay, loop, mute and controls;
  native defaults are autoplay/loop/muted **true**, controls **false**. Inherit
  preserves these flags; this proposal adds Frame entry/override policy only.
- `WebPortalRef` owns authored locator/viewport intent; last URL, scroll/history,
  focus, consent and sign-in remain derived per-peer/private as specified by
  [Web D31/D32](../../../docs/keymap/contracts/portal-web-embed.md). Saved view
  cannot promise arbitrary cross-origin DOM/session cloning (including Maps).
- `ModelCamera` owns camera/display/materials and existing saved-view captures.
  `present.rs::start_present` currently calls `lock_all_models` to freeze the
  committed pose; interactive-by-default is an explicit proposed policy change.

PowerPoint distinguishes automatic entry, clicking the video, and click-sequence
playback; this proposal does not add a click-sequence mode. See [Microsoft's video
options](https://support.microsoft.com/en-us/powerpoint/insert-and-play-a-video-file-from-your-computer).
Actual browser autoplay may be blocked, particularly with audible media; retain
a usable play action rather than claim success. See [MDN autoplay guidance](https://developer.mozilla.org/en-US/docs/Web/Media/Guides/Autoplay).
The present JS media fixtures are simulations with no live external I/O; these
settings and image references establish neither native playback/Web/Rhino support
nor native parity. Existing independent acceptance gates remain applicable.
The local `presentation-media` fixture has two slides with authored autoplay/
loop respectively on and off to expose inheritance. Last view is session-local;
Save current view records only its local bookmark fixture.
