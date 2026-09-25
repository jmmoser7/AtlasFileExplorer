# Media — interaction contract

Status: agreed
Family: tool
Reference: Existing Slate file import, PDF pages, Rhino viewer, and shared dock
Command: board.media.image / board.media.model / board.media.video / board.media.text · Key: none
Inherits: P0.* (all), P1.node; shared DOCK.md and TOOLBARS.md.

## Behavior matrix

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-------------------|--------|------|
| D01 | Initiation & arming | Add a primary Media icon with Image, 3D, Video, and Text sub-icons. Image includes flat/print media such as JPG, PDF, and PowerPoint. Text includes Word, spreadsheets, CSV, Excel, and source code, and shows an excerpt when the file can be read. 3D connects to the existing Rhino viewer. | stated | 100 |
| D02 | Stickiness & repeat | Each sub-icon runs a one-shot file-import command. After choosing files, use the existing placement flow. Repeat opens that category's picker again. | guess | 55 |
| D03 | Gesture grammar | Click a sub-icon, choose one or more files in a category-filtered native picker, and place them through the existing file-import path. External file drops use the same media resolution. No new drawing grammar. | guess | 55 |
| D04 | Click vs drag rule | The Media primary icon inherits the shared dock's click-to-open and double-click-to-pin behavior. The sub-icons are actions; existing file drag-and-drop behavior is reused. | pattern | 90 |
| D05 | Modifiers | P1.node.select, P1.node.move, and P1.node.transform apply after placement. No additional Media-specific modifiers. | pattern | 90 |
| D06 | Constraints & snapping | P1.node.osnap and P1.node.transform. Any placement point uses the existing resolved placement path; conversion never changes a placed node's geometry. | pattern | 90 |
| D07 | Direction / value locks | n/a: choosing linked media has no direction-lock gesture. | pattern | 85 |
| D08 | Numeric / manual entry | Use existing node sizing and inspector controls after placement. No new numeric-entry mode. | precedent | 85 |
| D09 | Preview & readouts | Keep a deck's available thumbnail while its PDF preview loads in the background. Show Loading, Ready, or a useful conversion error. Refresh stale derived previews without blocking the camera. | pattern | 85 |
| D10 | Cursor | Use the existing file picker, file-drop, selection, and transform cursors. The Media menu does not introduce an armed drawing cursor. | guess | 55 |
| D11 | Commit | P0.2 and P0.3: placement and authored page changes are journaled. Link the original PowerPoint; store its generated PDF only as a derived cache. Converting a file never writes back to the source. | pattern | 90 |
| D12 | Cancel | P0.1. Canceling the picker adds nothing. A late background result must not populate a different tab or resurrect a deleted item. | pattern | 90 |
| D13 | Selected presentation | P1.node.transform. Reuse existing image/PDF selection. A 3D model node paints only its render (or its gap card), as clean as a picture: no file-type badge (on the board or in the HTML export, which keeps the poster and the link), no padlock, no in-viewport tool strip. A single selected mesh model adds three squircles to the shared selection strip (P1.shape.properties): Viewport display (Model cube) opens one segmented default capsule with Shaded, Arctic, Material mask and Z-buffer; Measure (Ruler) is an action that stays highlighted while armed, like Deck; Screenshot (View) opens the export menu (D36). Enscape standalones and formats with no reader show neither. | stated | 100 |
| D14 | Post-edit | Selected PDF/PowerPoint images expose a Pages squircle. The same control opens a miniature `image_album` pallet over the document for the poster page (`board.media.page`) and Unbundle (`board.media.unbundle`) lays the deck on the board as a selected grid. Existing image controls are unchanged. A 3D model's display pass (`board.model_display`) commits on click as one undo step, live or frozen, following the Pages precedent; the frozen poster uses that pass and undo updates a live viewport. Measure (`board.model_measure`) enters a frozen viewport and arms point-to-point picking (Rhino Distance); one completed measurement, Esc, or the squircle again returns to Navigate, the resting tool. Completed measurements are derived: they stay drawn on the live viewport and clear on lock. On the Select tool, moving the pointer horizontally across a video node scrubs that node's full trim window without playing (Frame.io hover pan). The playhead stays where the pan stopped. A 96-frame filmstrip makes the pan a lookup; the playhead and timecode are derived and not journaled. A click with no modifiers toggles silent playback from that frame; click-drag still moves the node. One video decodes at a time. Windows Media Foundation plays mp4, m4v, mov, wmv, avi, mpg, and mpeg, and also webm, mkv, and ogv when that codec is installed. A file it cannot open, or a cloud placeholder, keeps the poster, play badge, and extension badge. Video HTML export is unchanged. | stated | 100 |
| D15 | Non-goals | PowerPoint renders as static PDF pages. No PowerPoint editing, transitions, or animation playback. No new 3D formats in this change. JPG and other supported image formats remain native previews. The board video viewer does not edit a timeline, cut clips, play audio, or shuttle with JKL. Photo filters on a 3D viewport and switchable saved views (Cover-Flow style) are later work. Viewport screenshots with embedded camera metadata are in scope via D36–D38. | stated | 100 |
| D16 | Create-style inheritance | Use existing linked-image node defaults and source aspect ratio; preserve the authored frame when derived previews refresh. | pattern | 85 |
| D17 | Hit-testing & pick | P1.node.select and existing media hit-testing. A 3D viewport follows P1.portal.contents-focus: double-click enters it (drag orbits, Shift+drag pans, the wheel zooms); a primary press outside it, or Esc, locks it and journals the pose as one camera change. Selection-strip chrome keeps the press. Esc peels one layer: the pending measure point, then Measure, then the shown measurements, then the viewport. 30 s idle still locks. A generator steering its wired model keeps that model until generator focus ends. | stated | 100 |
| D36 | Viewport screenshot | A selected mesh model adds a third squircle on the selection strip (Screenshot / View icon). Click opens a pointer-attached menu (P2) under the cursor: **Export to canvas** (default — centered under the pointer for rapid repeat) renders the current camera through the existing `render_capture` path, writes PNG with Slate XMP under `slate-outputs/<board>/…`, links the file, and places one image node to the right of the model in a single journaled add. **Export to folder…** uses the same async save-dialog path as other exports (`picker_rx` / `PickerMsg`) for PNG, JPEG, or WebP (TIFF deferred). Every format embeds one `slateview` XMP packet (`model-preview::view_meta`). | stated | 100 |
| D37 | View metadata | XMP namespace `https://slate.app/ns/view/1.0/` (`slateview`): version 1, camera fields mirroring `ModelCamera`, derived eye/up (ignored on read), projection, constant `fovY`, aspect, relative `modelPath`, `modelHash` (SHA-256 of model bytes), `modelName`, `modelSize`, `nodeId`, plus standard width/height and creator/date. Newer versions rejected; unknown fields ignored. Build/parse owner: `crates/model-preview/src/view_meta.rs`. | stated | 100 |
| D38 | View drop-back restore | Dropping a raster image (Explorer or an image node) onto a 3D model reads XMP on a worker thread. A `slateview` packet restores the camera as **one** journaled `ModelCamera` patch (undoable). Match `modelHash` when present; on mismatch still apply but toast that the model changed. No packet: toast “no saved Slate view” and do not place the file on the model target; empty canvas drops behave as today. | stated | 100 |

## Feel constants

Existing shared dock/icon tokens and media node transform constants. Video scrub budgets live in `atlas_core::video`: `STRIP_FRAMES` 96, `STRIP_EDGE` 320, `PLAY_EDGE` 960. Playhead and timecode use `PLAYHEAD_PX` 2 and `TIMECODE_PX` 12 through `canvas_scale`. No new gesture threshold: a click is still below the existing drag threshold.

## Golden paths

1. GP1: Open Media → Image → pick a JPG → existing image placement; cancel adds nothing.
2. GP2: Pick a PowerPoint → original stays linked, thumbnail remains during async PDF conversion, Pages album selects a rendered slide; source is never written. Unbundle keeps the original node id, places every page in a selected grid, and undoes in one step.
3. GP3: Open Media → 3D → choose .3dm → the node shows only its render. Double-click and orbit; select it → Viewport display → Arctic (one undo step); Measure → click two points → the length stays drawn and the viewport is back on Navigate; Esc clears it, Esc again freezes the pose.
4. GP4: Open Media → Video → place a clip. Hover across the node to scrub the full trim window without playing; the frame stays when the pointer leaves. Click plays from that frame and click again pauses. A cloud placeholder or a codec Windows cannot open stays a poster. Web-safe files still export as video.
5. GP5: Close/switch a tab or undo placement while conversion runs → completion does not modify another tab or restore deleted content.
6. GP6: Conversion fails or Office is unavailable → useful error and existing linked thumbnail; exported PDFs remain usable.
7. GP7: Open Media → Text → choose a Word, Excel, CSV, or source file → excerpt card on the board and in the HTML artifact. A package with no readable text stays a linked document card. PDF and PowerPoint stay on Image.

## Open questions

None. Proposed defaults approved by the user on 2026-09-15.

## Implementation notes

- Shared icon geometry: crates/atlas-shell/assets/tool-icons.json; painter: atlas_shell::icons. Media uses a stable family glyph; Image, 3D, Video use separate child glyphs.
- Slate menu: ui/tools.rs; commands.rs + dispatch.rs own registered actions; add_files_dialog owns picker plumbing. Preserve existing import/drop paths and tab ownership.
- PDF rendering and Office extraction owners: atlas-core::pdf / atlas-core::office. No conversion on thumbnail/scanner batch paths. Conversion is requested for placed/opened decks only and is bounded, asynchronous, and source-version keyed. Guard dehydrated sources before byte reads.
- Keep the PowerPoint as the source locator; derived PDF files live in local cache. Never silently hydrate cloud files or write back to the original. On machines without Office, show conversion unavailability and allow ordinary PDF input.
- Reuse apps/slate/src/app/pdf.rs for page browsing and preview.rs for resolution upgrades. Page count must be asynchronous too.
- Native and HTML artifact views use the same selected static page; link to the original and preserve source provenance. PDF cache data is not authored scene content.
- Rhino and Video share placement with the other media families. Video scrub and playback live in `atlas_core::video` (decode) and `apps/slate/src/app/board_video.rs` (the playhead). The playhead is not journaled. Cloud placeholders are not opened.
- Text documents reuse the existing snippet card. `atlas-core::office` extracts a capped excerpt from docx, xlsx, odt, ods, and rtf. CSV and source files are read as text. Legacy binary `.doc` / `.xls` with no excerpt use the linked document card.

## Validation — 2026-09-15

- Release compilation passed. The updated executable is installed at `target/release/slate.exe`; the previous executable was retained beside it.
- A local two-slide PowerPoint rendered through the conversion adapter and the repository's PDFium runtime. Both pages had the expected dimensions and text, and the source file's hash stayed unchanged.
- Contract consistency, media classification, per-page artifact posters, source overwrite protection, and shared native icon geometry checks passed. The first four Slate Media regressions passed before the final Grid/Venn compatibility and flyout-routing corrections.
- The final Slate test binary compiles, but Windows refuses to execute it with `Access is denied (os error 5)`. Status remains **agreed**, pending that final run; the last routing/Grid tests and the document-worker tests are not claimed as passed. No endpoint protection settings were changed.
- Pending focused check: `cargo test -p slate --lib -- media pdf::documents model_nodes_survive_headless_frames video_trim_settings_survive_save_and_reload export_renders_kind_specific_cards --test-threads=1`.

