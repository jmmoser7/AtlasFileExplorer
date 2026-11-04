# Frame interaction datum
Status: draft for owner review, 31 October 2026. No design changes are locked.
Family: canvas tool. Command: board.tool.frame. Inherits native P0/P1 transform, corner-grip and hit-region contracts; Studio S07/S08.

## Purpose
A lightweight HTML/CSS/JavaScript canvas for reviewing one Frame and iterating with the owner. Production Rust remains unchanged. The native baseline is the starting reference; the current browser reproduction is source-measured and has not been certified pixel-identical.

## Retained baseline
The canonical numeric data is [baseline.json](../frame/baseline.json); current native source fingerprints are [source-baseline.json](../frame/source-baseline.json). The pure query owner is [baseline.js](../frame/baseline.js). Source drift is a review event, never permission to silently recapture an expected value.

| Metric | Native datum |
|---|---|
| Default preset | Letter 612 × 792 world units |
| Other presets | Tabloid 1224 × 792; 16:9 960 × 540 |
| Default title/order | Slide 1 / zero-based order 0; displayed index is one-based |
| Plate | Theme card fill while unauthored; explicit authored white remains white in dark mode |
| Corner/stroke/shadow | Radius 8; no default stroke; no ordinary Frame shadow |
| Title | Ubuntu Light; 12 world units; inset 2 / lift 6 from visual upper edge |
| Tags | 10.5 world units, right end of visual upper edge; glyph U+2B26 |
| Selection | 0.16 fill; 1.5-world-unit silhouette outline |
| Hover | 1.25-world-unit outline; opacity 0.55; cubic easing; enter 120 ms / exit 220 ms |
| Fillet grip | Painted half-size 1.6 × projected prominence; minimum travel 10 world units |
| Property strip | Fill, Stroke, Corners, Prev, Next, Present, Deck; size 30 / gap 7 / object gap 14 |
| Dimensions | W/H only; 12-unit text; gap 24 or bottom lane 68 |
| Hit regions | Corner 7 / edge 6 / rotate 9 / fillet 9 screen pixels before prominence and proximity |
| Adaptive prominence | Resize smoothstep 12–48 projected px; rotate 3–16; wire 36–88 |
| Compact controls | 72-px target; actual authored geometry remains unchanged |
| Rotation | 45° multiples within 4° inclusive; Alt bypasses snapping |
| Navigation | Existing shared owner; zoom 1/128–128; exponential wheel coefficient 0.0015 |
| Text legibility | Hide below 4 projected pixels; geometry scales with zoom |

## Behavior matrix
These rows retain current source behavior. Browser limitations below remain outstanding work.

| ID | Dimension | Retained native behavior |
|---|---|---|
| D01 | Initiation | Existing Frame command; Studio has its own Frames tab |
| D02 | Stickiness/repeat | Existing native tool-arming owner; Studio preset arming is a local demonstration |
| D03 | Grammar | Click places preset centered; drag width is absolute horizontal travel, vertical travel selects the side |
| D04 | Threshold | Creation 4 screen pixels; ordinary selection 6; fillet native drag starts immediately |
| D05 | Modifiers | Corner resize proportional, Shift free; edge resize single axis, Shift proportional; Ctrl from center; Alt copy/snap override |
| D06 | Constraints | Native object/grid/smart snapping owners; Frame clamps rather than mirrors |
| D07 | Locks | Native command and transform contracts remain authoritative |
| D08 | Numeric | W/H positive values preserve center; corner grip supports native inline amount editing |
| D09 | Preview | Plate, upper-edge labels, selection, hover and external strip follow the metrics above |
| D10 | Cursor | Native arrows follow rotated local axes; rotation uses its native pointer affordance |
| D11 | Commit | One native SceneCmd journal transaction per completed mutation gesture |
| D12 | Cancel | Escape restores before-state; cancelled gesture contributes no mutation undo step |
| D13 | Selection | Silhouette feedback and adaptive fields; no eight visible resize squares |
| D14 | Post-edit | Move captures geometric members at press; Ctrl+D copies selected nodes only; Alt-move includes members; deleting Frame leaves content |
| D15 | Scope | Review prototype only; no production Rust, schema or command changes |
| D16 | Inheritance | Frame recipe defaults are independent of last shape style; legacy absent corner remains square |
| D17 | Pick | Original press owns gesture; topmost membership partition; broad phase uses rotated AABB, then authored unrotated rectangle |

Portal-only and media-only dimensions are not applicable. Membership is derived from content centers, never stored nesting. Hidden or locked members and connectors do not ride a move. The current creation Shift chord produces a square despite older recipe prose saying free aspect; this is recorded as a source quirk, not an approved change.

## Owners and fast validation
Native owners: slate-doc scene.rs / geom.rs / spatial/bounds.rs; slate-kit builtin/core.slatekit; Slate board/paint_node.rs, board/board_outline.rs, board_place.rs, board_snap.rs, board_handles and board_transform; atlas-shell canvas_scale, canvas_text, selection_tools, hit_regions, dimensions and ui-tokens.toml. Deck uses board_deck.rs and the existing frame-deck contract. Native implementation must extend these owners.

Run the source gate and geometry tests before building or comparing screenshots. FrameStudio.metrics() exposes the live specimen and canonical FrameBaseline.metricReport result. [Acceptance](../frame/ACCEPTANCE.md) defines the native candidate schema and the separate raster gate.

## Golden paths
1. Open Frames; select, move, resize and rotate; Undo restores each committed gesture; Escape restores a live drag.
2. Switch light/dark with unauthored fill, then authored white; inspect the retained color distinction.
3. Rotate through quarter turns; title stays on the visual upper edge.
4. Move a Frame with content; capture members at press. Alt-drag copies them; Ctrl+D copies only explicitly selected nodes.
5. Switch fixtures, zoom, pan and reset; Studio controls stay outside the element.
6. Query metrics, run source/geometry gates, then perform the independent paired raster pass.

## Outstanding parity work
Fill/Stroke/Corners now open concrete compact property editors; their measurements and transactions are in [PROPERTY_SPEC.md](../frame/PROPERTY_SPEC.md). Deck strokes use native segment intersection and first-contact ordering. Font bytes match, but vertical glyph layout and browser/egui rasterization are not yet independently matched. Native custom rotation cursor, tooltip timing, strip fade, complete inter-host hit arbitration, snapping/guide arbitration, group transforms, presentation flight, workbook/journal integrations and native export remain unverified or simplified. These are incomplete baseline coverage, not approved redesigns or waived acceptance requirements.

## Change control
No accepted-change rows are added to decisions.json. Apply the owner's next correction directly and keep the prototype fast. Record approved changes with metric IDs and dated amendments; keep the original datum available. Pixel equality is not claimed by a green JavaScript test.

## 1 November 2026 — baseline corrections
Dimension numbers now edit directly on their oriented leaders. The unit suffix remains visible; Enter or a valid outside click commits, Escape cancels, invalid values retain focus, and the editor follows pan and zoom. Width and height labels have no W/H prefix and use native three-decimal formatting. Studio controls now wrap below the top tab strip; the inspector opens on the left. These correct the prototype; they do not change Rust or certify the remaining native raster comparison.

Fill, Stroke and Corners now edit on the canvas with grouped preview/commit/cancel, numeric rails, RGB percentages and recent colors. The owner requested a squircle-to-blister transition, implemented from shared blister geometry. Current Rust excludes Frame from automatic media blister eligibility; this requested transition is therefore explicitly a Studio amendment, pending review, rather than a claim about that Rust branch. Duplicate and history shortcuts route through focused non-text controls. A Multiple slides specimen, deck stroke preview/intersections and undoable ordering make multi-slide evaluation concrete. No amendment is silently locked.

## 3 November 2026 — Frame presentation proposal

The owner requested JS toolbar refinement: **Fill, Stroke, Corners, Deck,
Settings, Present**, with six actions/215 world units closed. Remove Previous/
Next squircles and place the existing Present eye last. The historical native
seven-button/252 datum remains untouched; deck ordering and presentation keys
retain their current owners and behavior.

Settings proposes per-Frame Video start/loop overrides (default **Media setting**
for both), Web Saved view/Last view plus Save current view, and Interactive 3D
(requested default on). Inherit preserves authored VideoOpts flags. Web views
are supported URL/bookmark representations, not page DOM, cookies or a cloned
login/session. Interactive 3D changes the present native frozen-entry policy.
Fields, source boundaries and the existing preview/commit/cancel/undo contract
are in [the dated property proposal](../frame/PROPERTY_SPEC.md#3-november-2026--presentation-settings-proposal).

The phase-00 palette image is for visual review, not an approved contract or
permission to modify Rust. JS media specimens remain simulations; no live video,
website state capture, Rhino execution or native parity is established here.
Prior amendments, retained capabilities and independent acceptance stay intact;
no accepted-decision row is added without the user's actual design agreement.

## 3 November 2026 — v04 settings drawer applied for testing

The owner approved applying the v04 column proposal to the JS prototype. It replaces the accordion with three always-visible rows and normalized capsule edges in each control column. The six-action toolbar and all existing per-Frame settings/defaults remain unchanged.
Settings now uses an attached white drawer below the Frame: 136 world units high, matching the visual Frame width where practical, with a 460-unit readable minimum and viewport-width fallback. The existing 120 ms panel reveal clips and translates the drawer contents on open/close. A Settings-only camera pan exposes its lower edge when necessary; other property palettes retain their geometry.
This supersedes the earlier 420-by-194/284 Settings palette measurements only. Frame content/style, native fingerprints and raster acceptance remain unchanged; this is a testable JS implementation, not a native parity claim.

## 3 November 2026 — Frame Studio size toggle

The six-action JS Frame strip follows the media size policy in both states. A 456 × 40 world-unit collapsed band must fit the flat lower edge and use at most one fifth of the Frame footprint. Eligible Frames show a blister on the lower datum, with a 384 × 40 overlay when open. Other Frames keep their external squircles. The attached Settings drawer stays under the blister; the squircle drawer clears the external strip. These measurements supersede the earlier prototype strip counts.

## 3 November 2026 — Create Presentation in JS Studio

The numbered-list button opens the presentation drawer. The selector always reads **Select presentation**. Its menu shows deck names, a check beside the current deck, and **New presentation**. Hover or focus a deck name to preview the positions of its frames on the canvas.

The drawer uses the same placement and canvas scale as Settings. In blister mode it matches the host frame's width, carries the side highlight through the join, and rounds only at the palette's lower corners. In squircle mode it clears the toolbar. Open it from any member frame. A shared frame keeps the current deck if it belongs to that deck.

A click on blank canvas or Esc closes the drawer, including its selector and Add / remove mode, without changing the deck. A drag from blank canvas in Add / remove mode still selects frames along the stroke. Esc during a live canvas drag cancels that gesture first.

Presentations hold independent ordered lists of references to canvas frames. **Add / remove frames** highlights frames on hover and shows a plus or minus at each frame's upper right. Clicking a frame toggles its membership, and new slides append in pick order. The frame context menu also toggles membership. The thumbnail context menu removes that slide, and Delete removes selected thumbnails from the active deck.

Shift selects a range of thumbnails, and Ctrl toggles individual thumbnails. Dragging the selection into a gap inserts it there in relative order. The selected thumbnails slide into the preview position and settle on drop; their movement is the insertion cue. Small decks put every thumbnail in the toolbar when space allows. Large decks use a grid with horizontal row dividers.

**Sort on canvas** lays out frames in slideshow order, left to right and then top to bottom. It carries their movable contents and can be undone. Frames shared across presentations move on the same canvas, and each deck keeps its own playback order. The **Presentation · 5 slides** specimen includes an extra frame to add. **Presentation · 100 slides** exercises the complete reorder grid.

## Native follow-up — grip, zoom, and playback

- A Frame move and a Frame resize preserve the original grab point. Resizing one Frame carries its movable, geometrically owned contents from the start of the gesture; one Undo restores the Frame and those contents. Locked and hidden content keep their existing behavior.
- A held slide thumbnail stays under the original point of the cursor. Other thumbnails make room for the insertion and the moved group settles on release. Wheel over the palette still zooms the Board. The selected Frame's fill and outline follow the closed toolbar's blister cutout.
- In live presentation, Right/Down or scroll down advance, and Left/Up or scroll up go back. A focused live page owns its own keyboard input; interactive 3D or a live page under the cursor owns its own wheel input. Escape exits. Per-Frame media settings apply on slide entry; temporary 3D camera changes do not become authored Board state.

Live presentation changes cut instantly to the next or previous slide, with no animated slide or camera transition. The editing palette retains its thumbnail reorder animation, including insertion preview and settling on drop.

## Native follow-up — temporary Frame selection ownership

Global Window and Crossing sweeps can select Frames. A selected Frame temporarily represents its geometrically owned contents: a press over hosted content moves the Frame, child edge/corner/port hover is suppressed, and alignment/distribution operate on selected Frames as objects while carrying movable contents by the same delta. Group resize captures those contents with the Frames. Existing hidden/locked content exclusions remain in force.

Clicking blank canvas releases that temporary ownership, so hosted objects can be picked and edited individually again. Selection alone writes no document groups, locks, membership, or undo history. Actual geometry edits remain journaled and undo together. The existing contents-only sweep inside an unselected viewport-covering Frame and the explicit Ctrl+Shift sub-object gesture retain their established behavior.
