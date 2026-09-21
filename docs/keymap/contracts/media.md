# Media — interaction contract

Status: agreed
Family: tool
Reference: Existing Slate file import, PDF pages, Rhino viewer, and shared dock
Command: board.media.image / board.media.model / board.media.video · Key: none
Inherits: P0.* (all), P1.node; shared DOCK.md and TOOLBARS.md.

## Behavior matrix

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-------------------|--------|------|
| D01 | Initiation & arming | Add a primary Media icon with Image, 3D, and Video sub-icons. Image includes flat/print media such as JPG, PDF, and PowerPoint. 3D connects to the existing Rhino viewer. | stated | 100 |
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
| D13 | Selected presentation | P1.node.transform. Reuse existing image/PDF selection and the existing Rhino node presentation. | pattern | 90 |
| D14 | Post-edit | Selected PDF/PowerPoint images expose a Pages squircle. The same control opens a miniature `image_album` pallet over the document for the poster page (`board.media.page`) and Unbundle (`board.media.unbundle`) lays the deck on the board as a selected grid. Existing image controls and Rhino camera controls are unchanged. Video keeps its current poster and trim controls; playback is currently in the HTML artifact. | stated | 100 |
| D15 | Non-goals | PowerPoint renders as static PDF pages. No PowerPoint editing, transitions, or animation playback. No new 3D formats or video decoder in this change. JPG and other supported image formats remain native previews. | stated | 100 |
| D16 | Create-style inheritance | Use existing linked-image node defaults and source aspect ratio; preserve the authored frame when derived previews refresh. | pattern | 85 |
| D17 | Hit-testing & pick | P1.node.select and existing media hit-testing. Rhino interaction stays with its current viewer and focus controls. | pattern | 90 |

## Feel constants

Existing shared dock/icon tokens and media node transform constants. No new gesture thresholds.

## Golden paths

1. GP1: Open Media → Image → pick a JPG → existing image placement; cancel adds nothing.
2. GP2: Pick a PowerPoint → original stays linked, thumbnail remains during async PDF conversion, Pages album selects a rendered slide; source is never written. Unbundle keeps the original node id, places every page in a selected grid, and undoes in one step.
3. GP3: Open Media → 3D → choose .3dm → existing Rhino viewer and camera controls.
4. GP4: Open Media → Video → existing poster/trim controls and artifact playback for supported web video.
5. GP5: Close/switch a tab or undo placement while conversion runs → completion does not modify another tab or restore deleted content.
6. GP6: Conversion fails or Office is unavailable → useful error and existing linked thumbnail; exported PDFs remain usable.

## Open questions

None. Proposed defaults approved by the user on 2026-09-15.

## Implementation notes

- Shared icon geometry: crates/atlas-shell/assets/tool-icons.json; painter: atlas_shell::icons. Media uses a stable family glyph; Image, 3D, Video use separate child glyphs.
- Slate menu: ui/tools.rs; commands.rs + dispatch.rs own registered actions; add_files_dialog owns picker plumbing. Preserve existing import/drop paths and tab ownership.
- PDF rendering and Office extraction owners: atlas-core::pdf / atlas-core::office. No conversion on thumbnail/scanner batch paths. Conversion is requested for placed/opened decks only and is bounded, asynchronous, and source-version keyed. Guard dehydrated sources before byte reads.
- Keep the PowerPoint as the source locator; derived PDF files live in local cache. Never silently hydrate cloud files or write back to the original. On machines without Office, show conversion unavailability and allow ordinary PDF input.
- Reuse apps/slate/src/app/pdf.rs for page browsing and preview.rs for resolution upgrades. Page count must be asynchronous too.
- Native and HTML artifact views use the same selected static page; link to the original and preserve source provenance. PDF cache data is not authored scene content.
- Rhino and Video retain existing behavior. No new board_*.rs module, portal host, or duplicated renderer is proposed.

## Validation — 2026-09-15

- Release compilation passed. The updated executable is installed at `target/release/slate.exe`; the previous executable was retained beside it.
- A local two-slide PowerPoint rendered through the conversion adapter and the repository's PDFium runtime. Both pages had the expected dimensions and text, and the source file's hash stayed unchanged.
- Contract consistency, media classification, per-page artifact posters, source overwrite protection, and shared native icon geometry checks passed. The first four Slate Media regressions passed before the final Grid/Venn compatibility and flyout-routing corrections.
- The final Slate test binary compiles, but Windows refuses to execute it with `Access is denied (os error 5)`. Status remains **agreed**, pending that final run; the last routing/Grid tests and the document-worker tests are not claimed as passed. No endpoint protection settings were changed.
- Pending focused check: `cargo test -p slate --lib -- media pdf::documents model_nodes_survive_headless_frames video_trim_settings_survive_save_and_reload export_renders_kind_specific_cards --test-threads=1`.

