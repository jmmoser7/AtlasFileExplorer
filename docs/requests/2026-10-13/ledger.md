# Feedback fixes — 13 October 2026

Scope: all 25 bundles in `feedback/client/`, including the three October 10 regressions and the 22 reports submitted overnight and this morning. Original bundles and attachments remain untouched. Existing uncommitted implementation was preserved and checked before adding fixes.

## Implementation coverage

- **Oct 10 20:45 — masked-image crop:** existing fixed-mask/source-pixel crop and content-scale implementation retained; rotated-mask and peer-aspect undo regressions cover it.
- **Oct 10 20:46 — bring/send to front:** existing journaled reorder and cache invalidation retained; warm pick/paint/spatial-order and undo/redo regressions cover it.
- **Oct 10 20:50 — Rhino screenshot view restoration:** existing capture-relative lookup, preview, move/Alt-drop and camera-undo fixes retained.
- **Oct 12 23:33 — imported text editing:** existing double-click editor and workbook-local text override retained; linked originals are never written back.
- **Oct 12 23:36 — text double-click opens color:** existing caret ownership fix retained and tested by input-driven reentry regressions.
- **Oct 12 23:38 — generation count arrows:** existing stable child hit regions retained; added repeated clicks through the real image palette.
- **Oct 12 23:40 — edit current image without upstream image:** capture the explicitly edited image, crop and layers before releasing its chosen result. This supplies the image the user chose, without adding implicit selection/canvas context.
- **Oct 12 23:44 — text reentry overflow/wheel ownership:** existing editable-height growth, journaled exit and inactive-wheel fixes retained.
- **Oct 12 23:47 — browser image drop:** existing image bytes/HTML-image precedence over the hosting-page URL, bounded asynchronous fetch/decode and one-undo placement retained.
- **Oct 13 09:38 — object plus wire uniform resize:** exclude derived connector bounds from minimum object sizes and preserve the common scale after minimum-size constraints.
- **09:51 — light wire color:** shared default now RGB 204/207/207, matching the attachment.
- **09:58 — tiny-object hit priority:** the visible body moves before overlapping transform grips; explicit curve/sub-object editing remains available. Tiny hosts suppress overlapping wire targets. Wire grips use Grab rather than Move.
- **10:05 — generated-image history:** retain the existing shared album settle/arrival corrections; added real app regressions for idle, wrap and newly arriving results.
- **10:19 — boundary highlight:** filled-path hover now uses the actual path contours; selection already used the path boundary.
- **10:21 — tangent fillet cursor:** existing world-rotated edge direction retained; regression covers rotated hosts. Cursor directions use the native supported 45-degree directions.
- **10:26 — Rhino curve sub-selection:** Ctrl+Shift sequential clicks and directional window/crossing sweeps select segments; deleting a closed segment opens the curve.
- **10:31 — Trim/Split open versus closed:** prioritize genuinely divided open spans under filled cutters; retain cuts at internal polyline vertices; Ctrl removes picked cutters. Official Rhino research recorded; Slate's previously approved cutter-first Split flow remains.
- **10:32 — rectangular wire hosts:** new connections are offered only on rectangular node types; saved nonrectangular anchors continue resolving.
- **10:35 — shape-aware bottom toolbar:** shared strip layout sits below the lower silhouette across its entire width and can nest in concave notches; cached geometry avoids recomputing unchanged layouts. Horizontal stringers use a separate lower lane.
- **10:36 — Polyline Escape:** commit two or more placed vertices as one undo step; discard an incomplete draft without a phantom endpoint.
- **10:46 — palette, dimensions and typography:** a single object's open palette suppresses selection decoration and stringers, including text; multiple selected objects remain identified. Preferred typefaces are Replica, Arial, Courier New. Bold/italic/underline are journaled model data rendered by native text and artifact exports; old workbooks default to plain styling. Empty styled shape text preserves its settings through reentry. Hosted shape text now also uses the existing SVG text serializer in paint-layer exports instead of disappearing; style, inset, alignment, clipping and one host transform are covered by new regressions.
- **10:55 — primitive vertex edits:** moving/deleting a rectangle or polygon vertex promotes it to a closed path; deleting an edge opens it. Style and undo are preserved.
- **11:05 — rotation feedback:** one Windows-style two-headed 90-degree cursor arc replaces the separate geometry-attached arc.
- **11:08 — feedback window collapse/movement:** replace auto-sized anchored scroll windows with bounded usable heights and movable windows; regressions exercise reopen, resize and header drag.
- **11:10 — duplicate clipboard paste:** Unicode text uses one platform-event owner instead of racing native key polling across frames; bitmap/file fallback remains edge-triggered.

## Validation

- Cloud geometry validation: `cargo test --locked -p vector-ink`, exit 0; **149 passed, 0 failed, 1 ignored**. The internal-vertex regression passed.
- Windows `cargo check --locked --workspace --all-targets -j 2`: exit 0. A subsequent `cargo check --locked -p slate --all-targets -j 2` also passed after the final font and vertex-toolbar safeguards.
- Windows shared-shell tests compiled successfully, but execution was blocked before any test ran: `Access is denied (os error 5)` for `target/debug/deps/atlas_shell-4c6f6405ec5b0aa9.exe`. Build artifacts were preserved; no execution-policy workaround or repeated launch was attempted. Native GUI verification remains pending that machine approval.
- Final follow-up Windows `cargo check --locked -p slate --all-targets -j2`: exit 0, including the font-cache lifecycle regression and integration fixture repairs. Formatting and Rust size ceilings passed.
- Initial cloud Slate run: **1,321 passed, 60 failed, 53 ignored**. Full results are in `cloud-slate-final.md`. The picked-vertex test setup was corrected and its targeted rerun passed. Source review corrected stale fixtures and found additional narrow-toolbar, relative album-locator and font-cache defects. Follow-up cloud execution is in progress; failed behaviors remain unverified until their relevant checks pass.

## Known limits

The renderer exposes native resize directions in 45-degree increments and no native rotation cursor, so the existing pointer painter supplies the requested quarter arc. Fonts without an installed matching bold face retain their regular face. The existing paint-layer SVG text serializer is single-line; restoring hosted shape text retains that serializer's wrapping limitation. Ordinary shape HTML uses its existing wrapped text layout.
