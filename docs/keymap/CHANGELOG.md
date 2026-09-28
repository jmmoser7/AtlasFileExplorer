# Canvas command project — change log

## 2026-09-27 — The board paints every rotated node again (review r19)

- `brush` D11: the board again paints every rotated node, as it did
  before review r18, so a rotated path's miter spikes, blur, square caps
  and wide tween tips no longer vanish at the view edge. The review r18
  entry below that said the board stops painting rotated paths outside
  the view no longer holds.
- `brush` D11: a band over a rotated stroke counts as in view, and asks
  for frames, only while the stroke's rotated ink box meets the view. That
  box is the one the tiles use for the stroke's ink (`tiles::ink_rect`).
  A band over a rotated stroke out of view still asks for no frames.

## 2026-09-27 — An eraser band outlasts Delete, and rotated strokes cull (review r18)

- `brush` D11: deleting a stroke while an eraser band's pass is applied
  keeps the band, unpainted and asking for no frames, and a preview still
  settling for the removed stroke turns into such bands. Ctrl+Z that
  brings the stroke back never paints the un-erased stroke. The band
  ends when its tab closes or its pass leaves the undo history.
- `brush` D11: a rotated stroke is in view only while the box around its
  rotated frame, padded as the board pads strokes, meets the view. A band
  over a rotated stroke out of view asks for no frames, and the board
  stops painting rotated paths outside the view. Other rotated nodes, and
  rotated paths with an arrowhead or hosted text, still count as in view.
- The rotated-frame box that the tiles and culling use is now
  `WorldRect::rotated_bounds` in both places.

## 2026-09-27 — An undone eraser band survives settling and re-adds (review r17)

- `brush` D11: undoing an eraser pass while its preview still settles
  keeps its band, and every later band over that stroke, unpainted. A
  redo brings them back, so it never paints the un-erased stroke. A band
  whose pass is undone also stays while the undo takes its stroke out of
  the scene, so a double redo brings it back.
- `brush` D11: a later freehand eraser pass released before its preview
  exists keeps an earlier flick's band, as a straight pass already did.
- A band over a hidden, off-view, or undone stroke reads the stroke's
  content only after the scene changed, so it hashes nothing per frame.
- A band counts as in view, and asks for frames, only while its stroke's
  own ink meets the view as the board culls strokes. A band whose reach
  meets the view while its stroke is culled no longer repaints the board
  forever.

## 2026-09-27 — An eraser band outlasts hiding, panning and undo (review r16)

- `brush` D11: an eraser band stays while its stroke is hidden or out of
  view, until the stroke shows its raster for its content in view. A
  hidden stroke's band does not paint and asks for no frames. It paints
  again with the hide ghost and once the stroke is shown, so neither
  shows the un-erased stroke. A flick band now also stays when the stroke
  is panned away and back.
- `brush` D11: undoing an eraser pass keeps its band, unpainted, while
  the pass can be redone. A redo brings the band back, including over a
  stand-in whose cut the workers gave up on.
- `brush` D11: once an eraser stand-in starts a stroke's chain, each new
  bitmap keeps up to two bitmaps of other contents behind it. A redo after
  the restored bitmap was rebuilt at another zoom paints the erased bitmap
  again. A late raster of other content no longer replaces a bitmap that
  shows the stroke's current content. D11 now says that an eviction by the
  bitmap memory budget is not covered.
- `brush` D11: closing a tab frees its queued stroke raster jobs, its
  landed rasters, and every cached bitmap of its strokes, including nested
  boards' and removed strokes'. A raster a worker is still building for it
  is dropped when it lands.
- A nested board's landed stroke raster that nobody takes within about
  120 frames is dropped; a portal still on screen asks again. Pruning
  landed rasters compares node ids and hashes nothing per frame.

## 2026-09-27 — A refused eraser pass takes no ink check over (review r15)

- `brush` D11: an eraser pass on a read-only tab no longer takes over an
  earlier pass's ink check. The earlier pass's answer stays with it and
  removes the emptied stroke in that pass's undo step once the tab
  accepts edits.
- A stroke a later eraser pass removes leaves the selection, as it does
  when its own pass removes it.
- The board keeps asking for frames while an ink check waits, so the
  answer is taken in without further input.
- `brush` D11: the "nothing is removed" sentence states that answers are
  taken in only between gestures, and that undoing the earlier pass
  restores its ink.

## 2026-09-27 — An erased stroke stays erased across edits, tabs and redo (review r14)

- `brush` D11: an eraser stand-in is sealed by the undo step of the pass
  that made it. Deleting a picked vertex, or any edit that keeps the pass,
  no longer brings back the stroke's bitmap from before the pass; undoing
  the pass does, and a redo paints the erased stand-in again.
- `brush` D11: cached stroke bitmaps belong to one document and one
  nesting. A stroke with the same node id in another tab, or in another
  portal of the same nested board, never paints or replaces another's
  stand-in. Closing a tab frees its eraser settles, line jobs, and bitmaps
  at once, and a settled pass in a tab not shown keeps only its stand-in.
- `brush` D11: a band with no preview stays while its own reach or the
  stroke's ink is in view, not only the stroke's centerline. A band over a
  stand-in, including one whose cut the workers gave up on, stays off view
  until the stroke's raster is current. Hidden strokes no longer keep the
  board asking for frames.
- `brush` D11: the image paint session sentence says layer marks are
  decided by the budgeted check alone (they have no live preview), and
  that the layer composite still re-stamps on the frame loop (open).
- Nested board strokes no longer clone their node on every frame.
- `brush` D03: the Shift-inverts-Ortho sentence from review r7 is
  labelled as agent text. `brush` D11: the image paint session sentence
  from review r9 is labelled as agent text (was listed under the
  "Undo never skips a step" entry).

## 2026-09-27 — Quick eraser passes remove every stroke they empty (review r13)

- `brush` D11: an eraser pass that commits while an earlier pass's
  "is any ink left?" check is still out takes that check over. A stroke
  the earlier pass emptied now leaves in the later pass's undo step
  instead of staying in the scene, invisible. Undoing the later pass
  brings it back as the earlier pass left it, fully erased; undoing the
  earlier pass restores its ink.
- A waiting ink check no longer costs anything per frame; the stroke is
  checked once, when the answer lands.
- An eraser preview standing in for a big stroke follows the stroke when
  it is moved before the new bitmap lands (already true since review
  r12; now covered by a test).

## 2026-09-27 — A settling eraser preview leaves only to a newer one (review r12)

- `brush` D11: a released Shift eraser pass whose cut is still on the
  workers keeps its preview and band until something shows the same
  strokes or newer ones. A second pass released unchanged, a flick
  released before its preview exists, Esc, a tab switch and back, and a
  move of the stroke no longer drop it to the uncut stroke; a move
  carries the preview with the stroke. When the workers give up on the
  cut, the preview stands in with the band over its uncut part.
- `brush` D11: a settling preview takes its cut even out of view, a band
  with no preview ends once its stroke leaves the view, and the board
  stops asking for frames once no cut is on the workers.
- `brush` D11: a nested board's strokes keep their own bitmaps and never
  touch the host's eraser settle, even under a colliding node id.
- `brush` D11: an eraser stand-in paints only while the stroke keeps the
  passes it shows; after an undo the stroke's earlier bitmap paints
  until the restored one lands.
- `brush` D11: the eraser release decides image paint layer marks from
  the preview or within the frame's raster budget, as on the board; past
  the budget the mark commits and stays, even when fully erased.
- `brush` D11: states that a Shift eraser pass continues the last pass
  only in the tab that made it (review r11).

## 2026-09-27 — Undo never skips a step; an emptied board keeps its tab (review r12)

- Board undo / redo: a step that cannot be undone stays on the undo stack
  with its Ctrl+Z mark, the board is left as it was, and a toast says
  "Couldn't undo that step". The next Ctrl+Z tries the same step again
  instead of silently undoing the older step under it. Redo keeps a failed
  step on the redo stack the same way ("Couldn't redo that step").
- An untitled tab whose board was emptied by delete or undo is not blank
  while it still has board undo or redo history. Opening a workbook takes
  a new tab, so that history (possibly the only copy of a drawing) is kept.

## 2026-09-27 — A Shift release before the rebuild lands keeps the segment (review r11)

- `brush` D11: a Shift segment released while the rebuilt live canvas
  still waits for the stroke's stamp no longer vanishes. The canvas adds
  the segment as its vector mesh over the stroke as it was, until the
  extended stroke's own raster lands; nothing stamps on the frame loop.
  The tile budget and the paint ask the canvas one question.
- `brush` D11: a stroke's previous bitmap maps onto its current frame only
  when its placement alone changed; after a content change it paints at
  its own old place, so an extended stroke is never stretched.
- `brush` D11: a canvas that gives up on lost line jobs drops its pixels
  and is never resumed, so the scene's copy of the chain paints once. A
  release that adds a stroke beside the anchor leaves the canvas standing
  in for nothing.
- `brush` D11: the sentences review r10 added are labelled as agent text.

## 2026-09-27 — An eraser pass that empties a big stroke removes it (review r11)

- `brush` D11: when a release cannot tell within the frame's
  `SYNC_STAMP_PX` budget whether a pass left a stroke any ink, the raster
  workers run the same coarse check instead of the stroke being treated
  as changed and not removed. A stroke with no ink left is removed in the
  pass's own undo step: one undo restores it with its ink, one redo
  removes it again. This holds for Shift and freehand passes. If an undo,
  another edit, or a document switch comes first, nothing is removed and
  the fully erased stroke stays; its erased pixels do not pick. Nothing
  stamps on the frame loop for the check.

## 2026-09-27 — A board drawing keeps its untitled tab

- An untitled tab whose only content is on its board is no longer treated
  as blank. Opening a workbook takes a new tab instead of silently
  replacing the drawing, and the tab stays in the strip on Home.
- `brush` D03: opening a workbook into a new tab no longer empties another
  tab's Shift chain; only a reused tab starts fresh.
- Eraser: a Shift pass starts from the last pass's end only in the tab that
  erased it; another tab starts at its press instead of erasing along a
  line from a point of a different document. Esc on a pass also drops its
  cuts still on the raster workers (review r11).

## 2026-09-27 — Eraser Shift release stamps nothing on the frame loop; Shift+wheel pans (review r10)

- `brush` D11: an Eraser Shift release takes in a cut that landed after
  the last paint instead of discarding it and stamping the segment again.
  When the final cut is still on the workers, the release commits the
  erase mark at once and the preview and band stand in until the cut
  lands. A flick that crossed a stroke before its preview existed keeps
  the stroke's tiles under the band until its erased bitmap is current.
  The release stamps on the frame loop only within the frame's
  `SYNC_STAMP_PX` budget. The committed erase mark is unchanged.
- `brush` Textures: the eraser band's approximation lasts until the exact
  cut lands after the release as well as after the pointer stops.
- `canvas.pan_scroll`: Shift + scroll wheel pans the board sideways again.
  egui delivers a Shift wheel as a horizontal delta, which the board had
  been ignoring. Plain wheel zoom and Ctrl + wheel are unchanged; the
  binding text is unchanged.

## 2026-09-27 — Shift rebuilds off the frame loop, honest preview (review r10)

- `brush` D11: when the camera or the stroke changed, the next Shift press
  rebuilds the live canvas by stamping the stroke on the raster workers;
  the scene keeps painting the stroke until that stamp lands, so the press
  frame stamps nothing on the frame loop. A pan is named among the camera
  changes. A line job lost on a worker is asked again twice, then the
  scene paints the stroke.
- `brush` D11 and Textures: the moving Shift preview is stated as an
  approximation of the exact stamp. It starts square on a stamped chain,
  fades over softness times the radius, and is premultiplied as the stamp
  is, but ahead of the joint it still covers the chain's last dab until
  the exact stamp lands. Textures no longer claims the moving preview
  paints the stamp's pixels.

## 2026-09-27 — Shift segments start only from marks they can reach (review r10)

- `brush` D03: the Brush's newest mark belongs to the workbook that drew
  it. After another tab draws a mark, or another workbook is opened into
  the same tab, the next Shift segment starts at its own press and never
  extends a stroke that only shares the mark's number.
- `brush` D03: a newest mark the segment cannot extend (locked, closed,
  not a brush stamp, or on a layer the image paint session is not
  painting) still gives the segment its start. The segment is a new
  stroke, and the preview no longer takes that mark into its canvas, so
  what the drag shows is what the release commits.
- Eraser: one pass that removes a vector mark on a paint layer and cuts
  brush marks above it on the same layer now commits every edit as one
  undo step. Before, the batch stopped partway, leaving the removal
  applied with no undo step and dropping the cuts. No binding changed.

## 2026-09-27 — Agent-authored D11 sentences marked as proposals

- `brush` D11: the Shift segment's vector-mesh stand-in and worker
  stamping, and the pointer's return to the last sample after a mid-stroke
  HUD, are labelled as agent proposals. The user asked for a live preview
  every frame and no big-stroke raster on the frame loop, not for this
  particular stand-in.
- `board.ortho` help: Shift always takes 45° steps on Brush and Eraser,
  whatever F8 says (`brush` D03/D06).
- Agent Stop on a bundled card whose folder has moved writes nothing and
  says so ("stop it in Cursor"), rather than reporting a stop that never
  reached the run (`docs/agent-link-contract.md`). No binding changed.

## 2026-09-27 — Shift segments stay on the painted layer (review r9)

- `brush` D11: a Brush Shift segment extends an image's layer mark only
  while the image paint session paints that mark's layer. After the
  session ends (the image deselected, Brush still armed) or once another
  layer is active, the segment is a new mark, on the board or on the
  active layer, and the earlier mark is untouched. The Eraser already
  kept to the active layer; both now edit layer marks through one
  session-guarded owner.

## 2026-09-27 — Tip HUD on brush strokes, painted opacity (review r7)

- `specs/direct-selection.md`, `specs/brush-color.md`, PATTERNS
  P1.curve.vertex-style: the tip HUD reaches committed brush strokes. With
  nothing picked, size and softness scale every stamped tip in proportion,
  color recolors and a texture choice retextures every tip; a picked or
  hovered anchor edits only its tip (softness included). Picks on painted
  strokes are Direct Select anchors or hover (the Select tool shows no
  grips on painted ink). The painter and the HTML export read the same
  tips (Art. IV).
- A vertex's opacity reads and writes as the opacity it paints at (node
  opacity × its alpha), stored by the share rule, so the other vertices
  keep painting as before.
- A uniform freehand brush stroke commits the tip it was drawn with, even
  after a chord that followed its last sample.
- PATTERNS P1.curve.create-style: a curve tool's wheel no longer moves the
  brush foreground (no user statement asks for a shared color).
- PATTERNS P1.curve.tip-chord, `brush` D11: a chord that closes mid-stroke
  warps the pointer back to the last drawn sample and the stroke resumes
  there.
- `brush` D03 and D06, `KEYMAP.md` F8: on painted ink Ortho (F8) never
  cancels the Shift line's 45° steps (Brush and Eraser). Vector tools keep
  Rhino's Shift-inverts-Ortho.
- No binding changed.

## 2026-09-27 — Brush Shift start follows the journal; Eraser Shift pass off the frame loop

- `brush` D03: the Shift segment's start follows the journal. After an
  undo, redo, or delete, the next Shift press starts at the end the newest
  remaining brush mark has now (and extends it), or at the press when no
  mark is left. Before, an undone segment's end stayed the start and the
  release added a new node. The anchor holds mark identities only
  (`board_color::BrushChain`) and resolves against the scene on each press.
- `brush` D11: the Eraser's Shift pass cuts its segment out of each reached
  stroke on the raster workers (the brush Shift segment's line jobs, one
  lane per stroke), newest first. Move frames paint the eraser's band as
  the stand-in; the release takes in the exact cut. The committed erase
  mark is unchanged. No binding changed.

## 2026-09-27 — Agent Stop before the link folder exists

- Agent output Stop writes `cancel.json` even when pressed before a worker
  has published the workspace link folder (it creates the folder first).
  A bundled card whose folder has moved is not recreated
  (`docs/agent-link-contract.md`). No binding changed.

## 2026-09-27 — One Alt+right-drag entry; Smooth in the tip keys; HUD naming

- `board.stroke.width_hud` is removed: it had no dispatch owner and
  listed Alt+right-drag a second time in the Advanced window. Its
  mid-draw sentence (and its "width" / "thickness" aliases) moved into
  `board.brush.size_hud`. `line`, `arc`, `polyline`, `bezier-span`, and
  `pen` D05 now cite `board.brush.size_hud`. No binding changed.
- `board.brush.width_down/up` and `board.brush.softness_up/down` help
  name Smooth, as the dispatch already did. `COMMANDS.md` E bullet: painted
  strokes are spot-erased (one `Patch` per reached stroke), vector strokes
  removed whole; the `[ / ]` bullet names Smooth.
- `brush` D09 and implementation notes: the size HUD's style row and the
  color wheel are transient input chrome opened at the right-button press
  point for the life of the right-drag (P0.9's pointer-attached chrome
  exception), not P2.GhostFollow.

## 2026-09-27 — Brush Shift segment stamps off the frame loop (tip18)

- `brush` D11: the Shift segment's stamp builds on the raster workers,
  newest request wins. Each move frame paints the segment's tipped vector
  mesh, the exact stamp replaces it within a few frames of the pointer
  stopping, and the release keeps that stamp. Move frames used to re-stamp
  the whole segment on the frame loop: 560 000–980 000 stamp pixels and
  27–50 ms per move frame at a 207 px brush, 150 % zoom, 1.5 px/pt; now
  none. An undo, a zoom, or moving the stroke still makes the next Shift
  press rebuild the canvas.
- In an image paint session, a Shift segment from the last mark extends
  that layer mark (one layer patch, one undo step per segment) instead of
  adding a second mark, so its joint does not double the opacity. D03
  already said so; the image session did not do it.
- A brush path's cached stamp is keyed on each tip's texture, and on each
  erase mark tip's texture, so a texture edit re-stamps it. No binding
  changed.

## 2026-09-27 — Tip HUD help, tip-chord wording, Eraser row

- `KEYMAP.md` Ctrl+right-drag: curve tools share the wheel, the disk
  snaps to exact white and black, and the gap before the hue ring is dead
  (keeps the last value, never samples; `brush` D09). Alt, Shift, and
  Ctrl+right-drag rows name the committed-curve targets: picked vertices,
  else the hovered vertex, else the whole curve under Direct Select.
- `KEYMAP.md` E: painted brush strokes spot-erase (one Patch per pass);
  vector strokes are removed whole. `[ ]` and Shift+`[ ]` name Eraser and
  Smooth as well as Brush (`smooth` D05).
- `board.brush.size_hud`, `board.brush.opacity_hud`, and
  `board.brush.color_wheel` help (and `COMMANDS.md`) add Smooth where it
  arms and the committed-curve targets. No binding changed.
- `line`, `arc`, `polyline`, `bezier-span`, and `pen` D16, and `pen` D10:
  "width chord" becomes the tip chord (P1.curve.tip-chord: width, color,
  and opacity).
- The size HUD's style row is described as transient input chrome at the
  right-button press point (P0.9 pointer-attached exception), not
  P2.GhostFollow, in `specs/brush-color.md` and `board_tip_hud.rs`.
- Superseded notes added to the earlier "Tip HUD for every open curve"
  entry (dot radius 130 px; the style row drives softness to hardest).

## 2026-09-27 — KEYMAP brought in line with the brush and draft contracts

- `KEYMAP.md` Tab / Shift+Tab: Tab locks the pending segment's direction
  for every drawn segment (P2.RhinoDraft.tab): Line, Polyline, Arc,
  Bézier span, the Brush Shift drag, and the Eraser Shift pass. It
  previously cited only `line.md` D07.
- `KEYMAP.md` Alt+right-drag: 2 screen px of diameter per pixel of travel
  (was 1), softness for Brush, Eraser, and Smooth, and the style row
  entered at maximum hardness, kept on release (`brush.md` D05, D08).
- `KEYMAP.md` Shift+right-drag: Brush opacity, Eraser and Smooth strength,
  and curve tools' opacity, down to 0 % (was "Brush only"; `brush.md`
  D05).
- `KEYMAP.md` adds Shift+left-drag (Brush) and Shift+click (Brush) rows:
  the straight segment from the last stroke's end with 45° steps and the
  Tab lock, and the connect click within 8 screen px (`brush.md` D03, D04,
  D07). Neither steps opacity.

## 2026-09-27 — Brush Shift press starts from the live canvas (tip18)

- `brush` D11: a Shift press that continues the last brush mark starts its
  preview from the live canvas that mark left, while that canvas still
  shows the stroke under the same camera. It no longer re-stamps the whole
  stroke on the frame loop, a cost that grew with every chained segment
  (110–183 ms per press at a 207 px brush, 150 % zoom, 1.5 px/pt). The
  gesture is unchanged: D03 and D04 stand as written.

## 2026-09-27 — Curves drawn at 0 % commit clear; cached draft previews

- P1.curve.tip-chord: a Line, Arc, Polyline, Bézier span or Pen drawn
  with every tip at 0 % opacity now commits a node at 0 %, with each
  vertex keeping its full alpha. It used to commit fully opaque, against
  the 27 September decision that opacity reaches 0 % on every tool. Mixed
  tips still normalize to the most opaque tip. The HTML export writes
  `opacity:0` for such a node, and it is still picked by its geometry.
- The vector draft previews no longer tessellate on a frame where the
  draft, its tips and the zoom are unchanged. A live Pen stroke paints as
  pieces of 64 samples that meet with butt ends mid-segment, so a pointer
  move rebuilds only the last piece. The preview is otherwise unchanged
  (`board_path::DraftInkCache`, Art. II). No new command.

## 2026-09-27 — Agent chat: stable chooser, bare Stop, drafts join a switch

- `portal-agent-link` D13: the Codex conversation chooser keeps a fixed
  column count from its row count (one up to 3 rows, two up to 8, otherwise
  three) and places every row by its index, so no row changes column or
  row while zooming or scrolling. It always reserves its scroll bar's
  width, and the bar and scroll offset scale with the board (P0.9). The
  project chooser stays one column. The narrative's "wrap into columns" is
  superseded.
- `portal-agent-link` D22: while a reply streams, Stop is a bare small gray
  square on the output circle's place, with no disc or ring; the place,
  press reach and hand-back are unchanged.
- `portal-agent-link` D14: an unsent draft joins a presentation switch.
  In message pairs and one message per card it stays a draft card in the
  new form, in line with the train; in a single chat window the window's
  composer takes its text and wires. Nothing is sent, and one Undo returns
  the draft to its card and place with its text. Any card, a draft
  included, can start the switch. Replaces "An unsent draft stays on its
  card."
- `portal-agent-link` D11: pasting a copied chat train is an independent
  fork (user decision, 25 September 2026): a new linked source replaying
  the copied transcript, fresh session ids, titled `Forked from <name> ·
  replayed`; the original is untouched.
- `portal-agent-link` narrative: "Train presentation is the default" is
  marked superseded; message pairs are the default since 25 September
  (D22).

## 2026-09-27 — Tip HUD on picked vertices, strip at the picks, handle snap, vertex Delete

- Corrects the earlier 2026-09-27 line "Direct Select: the same HUD edits
  the target curve's width, color, opacity". User decision, 2026-09-26 and
  2026-09-27: with vertices picked (Direct Select anchors or Select-tool
  grip picks), Alt/Ctrl/Shift+right-drag edit only those vertices' width,
  color and opacity (vertex color alpha). With nothing picked, hovering a
  vertex arms the HUD for that vertex. Elsewhere under Direct Select it
  edits the whole curve, and a whole-curve color now recolors existing
  vertex tips, which previously hid it. One undo step per HUD; Esc
  restores. The HTML export writes per-vertex opacity as `stop-opacity`.
- `brush-color.md`: removed the unratified claim that per-vertex color and
  opacity are not SVG-expressible (Art. IV). The `linearGradient` export
  already expresses them.
- With vertices picked, the shape property strip sits beside the picked
  points under Direct Select and Select alike, and its edits apply to those
  points.
- Handle drags (Select grips, Direct Select, and a Bézier span being
  drafted) keep the grab offset and snap the handle tip, never onto the
  handle's own anchor.
- Delete with picked vertices, under any tool, removes those vertices and
  rejoins the neighbors. The curve is removed only when fewer than two
  (open) or three (closed) vertices would remain. One Ctrl+Z restores
  either way.

## 2026-09-27 — Tip HUD feel: gain, style band, 0 % opacity, arrows, wheel gap

- Size HUD gain is 2 screen px of diameter per px of travel
  (`SIZE_DRAG_GAIN`), still scaling about the press point (`brush` D05, D08).
- The way down to the style row now drives softness to hardest and the
  release keeps it; this reverses the earlier restore-at-the-row entry
  below (user: "intentionaly put the brushe types at the bottome so that
  user wouldenter each type with maximum hardnes"). The row's band spans
  the full width under the circle: size and softness never scrub inside
  it, so the row holds still. Swatches are 42 px, screen-sized, and drawn
  from cached stamps and meshes that show what each choice produces.
- Opacity reaches 0 % everywhere (HUD scrub, curve opacity, eraser
  strength); a 0 % stroke is still picked by its geometry.
- The arrow style aims its head along the last head-length of the curve,
  not the last tiny segment, and the body tucks under it across short
  segments, on the board and in the HTML export (one owner in
  `slate_doc::geom`). Narrow-at-both-ends swells along straight spans.
- Color wheel: an 8 px dead gap between the saturation/value disk and the
  hue ring (it keeps the value and never samples), a disk mapping that
  reaches pure white, snaps that return exactly #FFFFFF and #000000, and
  the pointer warp lands on the painted dot's center each time a dot is
  entered (`brush` D09, D17).

## 2026-09-27 — Commits never stall on big strokes; lighter brush textures

- `brush` D11 (user: "ran f4 erasor lock up interface on commit of
  comand"): releasing an eraser pass, a brush stroke, or a smooth pass no
  longer rasterizes big strokes on the frame loop. Bitmaps larger than
  `SYNC_STAMP_PX` (256 × 256) build on the brush raster workers and the
  newest wins. Until then the eraser preview, the live brush canvas, or the
  stroke's previous bitmap stands in, and the un-erased stroke never paints
  again. The eraser release reads the pass's result from its live preview
  instead of stamping each stroke again. On six screen-wide strokes (dev
  profile) the erase release frame went from 36 ms (75 ms blurred) to 1 ms,
  the brush release from 10 ms to 1 ms, and the worst smooth drag frame
  from 66 ms to 1 ms. No new command.
- `brush` Textures (new section; user: "brush texres are very heavy ...
  look closly at prior art from tools like photoshop"): the grain is applied
  once to each finished pixel, reading its depth into the stroke and baked
  world-tiling paper fields. No noise is evaluated per dab. Graphite, Pencil,
  Ink, and Watercolor each follow their Photoshop counterpart: Watercolor
  gains wet edges and granulation, Ink a slight wet rim. A textured stamp
  costs about half what it did (Watercolor 112 → 49 ms on the bench). The
  board, tiles, live previews, and HTML export share the one model.

## 2026-09-27 — Shift 45° steps and the Tab direction lock on every drawn segment

- P2.RhinoDraft.ortho / .tab now cover every drawn segment: Line,
  Polyline, Arc, Bézier span, and the Brush / Eraser Shift straight line
  (user: "hold shift to lock orientatin f drawn segment or position of
  vertecie relitive to last drawn verticie. tap tab to hard lock curet
  orientation or trajectory"). Held Shift puts the next point on a 45° step
  from the last placed one. Tab locks the direction from the last point
  toward the pointer; the pointer then changes only length; Tab again
  releases; placing the point, Esc, or a tool change ends it. One owner
  (`toggle_segment_lock` / `resolve_segment_point`) replaces the Line-only
  `line_toggle_lock`. `polyline` D07 no longer deviates; `arc` D07,
  `bezier-span` D07, `brush` D07 gain the lock; `pen` D07 notes that
  freehand has no pending segment.
- Under a Tab lock, point snaps now land where they project onto the locked
  ray instead of being suspended (`line` D07, `object-snap` D06 / D07).
- The board keeps the Tab it uses for the lock: egui's focus navigation no
  longer moves keyboard focus into a panel field, which had made "Tab again
  releases" unreachable from the keyboard.
- Brush (tip18): Shift+drag previews a straight segment from the last
  stroke's end and commits on release; a drag past 8 screen px takes 45°
  steps, a Shift+click still connects to the click point at any angle
  (tip19). The dead "Shift+click steps opacity" owner is removed
  (`step_opacity`, `step_brush_opacity`, the stale `brush_chain`); Shift
  never steps opacity, Shift+right-drag scrubs it (`brush` D03 / D04 / D05 /
  D06 / D09, `brush-color.md`, the dock tooltip, `board.tool.brush` help).
- New reference entry `board.draft.direction_lock` (Tab while drawing).

## 2026-09-27 — Mid-draw tip changes tween (P1.curve.tip-chord)

- P1.curve.width-chord becomes P1.curve.tip-chord: each placed point or
  sample records the tool's whole tip (width, color and opacity), not
  only its width. Line, Arc, Polyline, Bézier span, Pen and Brush.
- The draft previews paint the committed tips in their real colors and
  show the blend; commit keeps them per vertex. Equal tips commit a plain
  stroke. Curve opacity stays node-level: the node takes the most opaque
  tip and each vertex's alpha carries its share. One Ctrl+Z still removes
  the curve.
- Easing: straight on lines and sharp or chamfered polylines, along the
  sweep on arcs, smoothstep on Bézier spans, Pen curves and filleted
  polylines. Stamped strokes ease per segment. A filleted polyline eases
  between its polyline vertices, and Trim, Split, and smoothing cuts on
  it take the eased value (supersedes the straight filleted taper of
  2026-09-26).
- Pen and Brush stop sampling while a HUD is up (the Brush used to keep
  painting under the scrub) and blend into the new tip by smoothstep over
  24 screen px. Brush strokes now keep per-sample stamped tips.
- HTML export grades the outline with per-vertex `stop-opacity`, matching
  the board.
- Rows: `line` D05, `arc` D05, `polyline` D05, `bezier-span` D05, `pen`
  D05, `brush` D11.

## 2026-09-27 — Tip HUD for every open curve, styles, and anchor editing

- Alt+right-drag's size circle is pinned to the canvas at the press point
  (world anchored) and grows about that center.
- The color wheel's saved-color dots sit clear of the hue ring (ring edge
  106 px, dots at 128 px with a 10 px pick), so the two no longer collide.
  Superseded 2026-09-27: the ring's outer edge is 112 px and the dots sit
  at 130 px (`WHEEL_SLOT_RADIUS`); see "Tip HUD feel: gain, style band,
  0 % opacity, arrows, wheel gap" above and `brush` feel constants.
- Line, Polyline, Arc, Pen, and Bezier take Alt+right-drag (size),
  Ctrl+right-drag (color wheel), and Shift+right-drag (opacity). They edit
  the open-curve create style. Softness stays Brush and Eraser only.
- Under the size circle, a row of style icons: Brush and Eraser pick a
  texture (Smooth, Graphite, Pencil, Ink, Watercolor); vector tools pick
  flat/square, round/round, arrow at the end, narrow at the start, or
  narrow at both ends. Textures are world-anchored grain in the shared
  stamp, so the board and the HTML export match.
- Direct Select: the same HUD edits the target curve's width, color,
  opacity, and style as one undo step (outdated: picked vertices are
  edited alone, see the entry above). Dragged anchors and handles snap by
  the anchor itself (not the cursor), to board snaps and to the curve's
  other anchors. Delete removes the selected anchors and rejoins the
  neighbors.
- Reaching the style row freezes the size and restores the softness from
  the press, so picking a texture does not harden the tip. Superseded
  2026-09-27: the way down drives softness to hardest and the release
  keeps it; see "Tip HUD feel: gain, style band, 0 % opacity, arrows,
  wheel gap" above (`brush` D05). The color
  wheel's saturation disk paints under the hue ring so no backdrop
  hairline shows between them.

## 2026-09-27 — Selection outline follows the authored corner

- P1.node.corner-grip records the 25 September request: the selection
  outline and contents-focus highlight of a corner-capable node, including
  a 3D viewport's live ring and portal frames, follow its fillet or
  chamfer. Behavior was already correct; tests now pin the chamfer case.
- DYNAMIC_PANELS.md marks the Fill, Stroke, and wire-properties reference
  images: their second rail is saturation, not the value rail they show.

## 2026-09-26 — Per-vertex style survives editing

- `trim` D16 and `split` D16: each piece keeps the per-vertex widths,
  colors and corner overrides of the source vertices it keeps, and a cut
  vertex takes the stroke's width and color at the cut
  (P1.curve.vertex-style). No new command.
- Direct Selection Join keeps each vertex's style; a merge drops only the
  merged end's entry.
- `pen` D05: the live preview draws each sample at its own width.
- `smooth` D14: smoothing resamples per-vertex widths and colors onto the
  refit vertices, so a tapered pen stroke stays tapered.
- Direct Selection arrow-key nudges keep every vertex's width, color and
  corner override.
- `join` D16: joining open paths carries each source's per-vertex widths,
  colors and corner overrides in joined order, reversed with a reversed
  source.
- `trim` / `split` D16: an open curve with per-vertex tips is cut in curve
  parameter space and keeps its curves, so pieces paint the widths and
  colors their source painted between vertices (new token
  `trim.tip_tolerance`).

## 2026-09-26 — Agent chat: Stop on the output circle, switch re-chunks

- `portal-agent-link` D22: while a card's reply streams, Stop takes the
  place of its top output circle, so it no longer moves as the card grows.
  A click stops that card only; it neither continues nor forks.
- `portal-agent-link` D14: switching between message pairs, one message
  per card, and the single chat window mid-conversation re-chunks every
  linear path of the whole conversation, keeps forks at their branch
  points, and moves wires to the card that now shows their message. No
  provider runs; one Undo returns the previous presentation.

## 2026-09-26 — Menus own the wheel and stay on screen

- New pattern P0.10 (owner `atlas_shell::menu_wheel`): an open list,
  dropdown, or scroll menu takes the wheel while the pointer is over it;
  the board does not zoom underneath (typeface and height lists in `text`
  D14, the agent model dropdown).
- DYNAMIC_PANELS.md / P1.shape.properties: popups paint on
  `selection_tools::POPUP_ORDER`, above stringers, the strip, grips, and
  nodes. `selection_tools::place_popup` fits a popup inside the viewport
  while avoiding the selected geometry, and keeps the default position
  when nothing fits.

## 2026-09-26 — Paint layer palette and drop capsules

- `image-paint-layers` D01 / D09 / D13: the layer palette hangs centered
  10 units below the image while a hosting tool paints it; the `+` sits
  past its right end; recent-color dots are smaller and live inside the
  palette. Painting hides the blue cast, stringers, and fillet grip. The
  photo-filter capsule holds filters only, and agent pictures offer it.
- `image-paint-layers` D14: the Replace / Add as layer capsules are
  reachable by dragging onto them; the dragged image is never its own
  target.

## 2026-09-26 — 3D screenshots match the viewport; drop-back preview

- `media` D36: Export to canvas renders what the viewport shows (display
  pass, filter, camera, aspect, background) through the live render target.
- `media` D38: dragging a picture with a saved Slate view over a model
  previews that camera with a short tween and draws the picture into the
  viewport; releasing sets it, moving away restores the original view.

## 2026-09-26 — Crop toggles packed; saturation rail keeps its color

- `image-crop` D01: Off/Crop packs into the one Corners capsule with
  Fillet/Chamfer and %/u, without changing the capsule or slider size.
- `shape-selection-toolbar` D13 / DYNAMIC_PANELS.md: the saturation rail
  paints gray to full hue at value 1 (the field's top edge), so it does not
  darken with the selected color.

## 2026-09-25 — Crop repeat, multi-crop, and first-grab handles

- `image-crop` D02: a finished crop joins repeat-last (P0.4); a Space tap
  or Enter re-enters crop on whatever image is selected.
- `image-crop` D09 / D10 / D17: corner-first hit zones never smaller than
  8 px, the very first press reaches the handle, and crop mode keeps every
  selected image.
- `media` D40: a 3D model's View wires attach like any node's: no port at
  rest, the shared wire snap highlights the edge during a drag.

## 2026-09-25 — Color editor reverted; value rail becomes saturation

- `shape-selection-toolbar` D13 / GP8 and DYNAMIC_PANELS.md (user
  decision): the original saturation/value field at the current hue comes
  back, with opacity, saturation, and hue rails. Only the neutral value
  rail changed, to saturation. The interim hue-by-value square with two
  rails is withdrawn.

## 2026-09-25 — Sign-in pop-ups in web portals

- `portal-web-embed` D15 / D22 (and the D32 deny list) amended, user-ratified:
  a page's `window.open` with a size or position — Google and Microsoft
  sign-in — opens in a small Slate-owned window on the portal's own profile,
  so the opener survives and the sign-in lands in that portal's cookies. It
  is titled with its origin, closes itself when the page calls
  `window.close()`, and closes with its portal. Links and featureless
  `window.open` still navigate the portal in place. No new command.

## 2026-09-25 — 3D viewport declutter, screenshots, and saved views

- `media` D13: display modes (Shaded, Arctic, Material mask, Z-buffer)
  move to circle chips in a chips-only filter capsule on the selection
  strip, and Measure becomes a strip action. The viewport paints only its
  render: no file-type badge on the board or in the HTML export, no
  padlock, no in-viewport tool strip.
- `media` D36 / D37 / D38: Screenshot exports a PNG to the canvas under
  the pointer (default) or a folder, with the camera in Slate XMP;
  dropping it back on the model restores that view in one undo step.

## 2026-09-25 — Paint layers and Replace / Add as layer

- `image-paint-layers` D01: painting on a selected image creates a layer
  in the same undo step; layers are not filters.
- `image-paint-layers` D14: dropping an image onto another offers
  Replace or Add as layer capsules; one undo restores both.

## 2026-09-25 — Message pairs are the default

- `portal-agent-link` D22: a new conversation's train presents message
  pairs; one message per card and the single chat window stay menu
  commands. Legacy workbooks keep their saved presentation.

## 2026-09-24 — Spot eraser and color wheel gap

- The Eraser erases painted brush ink under the pass instead of deleting
  the stroke. Soft, partial, dab, and Shift-straight passes work like the
  Brush, with the same size, softness, and strength controls and Ctrl+Z.
  Vector strokes are still removed whole.
- The color wheel's hue ring meets the saturation/value disc. Anywhere on
  the wheel keeps the current color; only moving past the wheel samples.

## 2026-09-24 — Bumper cars (optional tool) and Preferences → Snaps

- New contract `bumper-cars`. Off until Preferences → Configure → Tools →
  Bumper cars. Shapes and sticky notes get a Bumper squircle (On/Off, buffer,
  friction from Puck to Anchor). Filled closed shapes collide as solids,
  unfilled ones as rings, open curves as walls. Pushes and the release glide
  journal with the drag as one undo; the glide itself is a replay.
- Every snap row moves into Preferences → Snaps, with the same commands and
  F9. The shared top-bar menu gains nested flyouts for both apps.

## 2026-09-23 — Brush opacity and color dots

- Shift+right-drag scrubs brush opacity and keeps the right button from panning.
  Ctrl and Alt still win when they are held with Shift.
- Choosing a recent color leaves the wheel where it is and moves the pointer
  onto that color. Strokes composite in Normal mode, opacity on the whole stroke.

## 2026-09-23 — Brush stamp

- A brush stroke is a radial bitmap of its centerline, shared by the board
  and the HTML artifact. Softness fades inside the diameter. Overlaps keep
  the maximum, so the stroke matches one dab of the tip.
- Alt+right-drag, Ctrl+right-drag, and Shift+click read the modifiers on
  the press and keep the right button from panning.

## 2026-09-23 — Sticky note place

- A sticky places once and returns to Select. The place ghost does not
  stay, and the text/color capsule does not open.
- The note is center-aligned. Place and double-click open a black blinking
  caret in the middle. Clicking off commits the text.
- The default fill is white, with a subtle drop shadow.
- Sticky text stays center-aligned and shrinks so a long note still fits
  inside the card. The authored size is the ceiling.

## 2026-09-22 — Frame deck

- Select a frame and the Deck squircle appears on its strip. Click it,
  then click frames or draw a stroke through them to set the presentation
  order. It is not a tools-dock icon.
  New frames use an 8-unit fillet and no border.

## 2026-09-22 — Slate board portal

- A document portal loads another workbook's board into a frame.
  Place it from the Portals flyout. Double-click or Enter opens that
  workbook as a tab.
- Dropping a `.slate` file on a blank board still opens it. Dropping one
  on a board that already has content asks whether to open it or insert it.

## 2026-09-22 — Brush tip mesh and Ctrl+right-drag color wheel

- The size circle stays on the press point and scales about that center.
- The in-progress stroke previews at the brush width instead of a hairline.
- Soft tips use a multi-band fringe and a rounder cap, so the halo is not a spike.
- Ctrl+right-drag opens the color wheel. Alt+right-drag still scrubs size and softness.

## 2026-09-22 — Brush size, softness, and color wheel

- Alt+right-drag scrubs brush diameter and softness. Shift+Alt+right-drag
  opens a color wheel centered so the pointer starts on the current color.
- The document stores 24 recent colors. They fill equal slots clockwise from
  6 o'clock; a repeat moves to 6 o'clock; a full ring drops the oldest.
  Fill, Stroke, and text editors still show the first six.
- Shift+[ / Shift+] step softness. A soft stroke feathers on the board and
  exports as an SVG gaussian blur.

## 2026-09-22 — Wire drop on empty canvas

- Dragging a new wire onto blank canvas commits it with a free end at
  the release point. The tool-search palette no longer opens.

## 2026-09-21 — File Atlas portal fill, stroke, formatting, marquee

- File Atlas Fill authors the portal window. Unauthored fill follows
  `Palette::card` (slightly lighter than the board) so the outline is visible;
  theme switches do not mutate the scene. Legacy `[16,18,22,255]` fills keep
  that theme-relative behavior.
- Stroke is a portal capability. The Stroke squircle authors `portal.stroke`
  (width 0 = none) and paints the window border; hover/focus chrome yields to
  an authored stroke.
- A Formatting squircle (catalog Display icon) opens the File Atlas filter
  menu in `selection_tools`: search, type radios, Ghost/Hide, Zoom to matches,
  and Zoom to fit (`portal.atlas.fit`). Filter/search/camera stay view-state.
- Name/type matching lives in `atlas-core::filter`; the standalone app and
  the portal both call it.
- Contents-focus: drag a box on empty canvas, or Shift-drag, marquees files
  (`Tree::files_in_rect`; Ctrl additive). A drag on a file still carries.

## 2026-09-20 — PDF Pages album and unbundle

- A selected multi-page PDF or PowerPoint gains a Pages squircle on the
  shared property strip. The same control browses pages and unbundles.
- Page browse is a thin `image_album` pallet hovering over the document,
  not a hover grid or a second Cover Flow painter.
- `board.media.unbundle` keeps the original node id, lays the full deck
  on the board as a selected grid, and journals one undo group.
- Hover page-picker chrome and the old explode path are removed.

## 2026-09-18 — Image photo-filter capsule

- Image selections gain a Filters squircle on the shared property strip.
- The editor is the fillet-height capsule: colored radios (B&W, Invert,
  Clarendon, Juno, Lark) and one intensity slider. Hover previews; click
  or scrub journals `ImageAdjust` through `board.shape.edit`.
- Recipes live on `slate-doc::scene::PhotoFilter` and compile to the
  existing CSS-filter model. `invert` is now an amount (legacy bool
  documents still load). 3D model viewports omit the control.

## 2026-09-18 — Shared property strip beyond shapes

- Fill, Stroke and Corners follow scene capabilities, so the same strip
  serves frames, text sticky-note fills, portals, images and wires.
- The forked frame popup is gone. Deck order, tags, add-images and present
  use the geometry-node squircles.
- Selecting expands the icons; one empty-canvas click commits, collapses
  and deselects (no second click).
- The fillet capsule is 30% taller than the 17-unit wire capsule
  (`CORNER_HEIGHT` / `CAPSULE_HEIGHT`).

## 2026-09-18 — Dock family primaries open the flyout only

- Clicking Frame, Shapes, Portals, Text, Media, or Actions on the dock
  opens that palette. It does not arm a subtype. Nested flyout icons
  still arm or run the chosen command.
- Frame primary and size glyphs share one page-and-dog-ear sheet at
  the proposed size's true aspect (Letter 8.5×11, Tabloid 11×17,
  16:9, Custom 1:1).

## 2026-09-18 — Selection yields to property previews

- Opening a Fill, Stroke, Corners or Wire adjustment fades selection/hover
  decoration out over 120 ms. Switching editors keeps it hidden; closing or
  cancelling restores it. Property controls and stringers remain visible.
- The shared shell selection painter handles silhouettes, path highlights and
  endpoint grips without changing authored paint, selection, or undo history.
- GP14 covers native board paint output for rectangles and wires in both themes.

## 2026-09-17 — Curved dashes and stroke mesh quality

- Dashes retain every intervening curve sample instead of becoming straight endpoint chords, including runs crossing a closed path's seam.
- Round caps advance along noncrossing boundary rails; round and bevel joins hold their inner intersection fixed. This removes internal triangle overlap and translucent buildup. Cap and join detail is tolerance-driven.
- Tapered strokes retain adaptive curve samples instead of resampling to a maximum of 64 stations. SVG stroke outlines use the same cap/join/dash boundary owner as the native mesh.
- Path fills, path strokes and wires refine with zoom to keep curve error below 0.15 logical px. Meshes stay cached by zoom bucket.
- Regression targets cover dash curvature, closed seams, odd dash arrays, capsule/join area, tapered samples, export outlines and zoom-dependent fill caching.

## 2026-09-17 — Trim follows authored outlines

- Trim uses the actual fillet/chamfer boundary of both rectangles, including percentage corners, instead of substituting square boxes. The board painter and trim share the pure outline owner in `slate-doc::scene`.
- Ellipse and curve sampling use bounded geometric error rather than a fixed 48-sided ellipse. Cutter highlights follow their actual outlines.
- Trim/Split results bake world-space rotation once; legacy rotated line endpoints and rotated text/image clips now use the correct coordinate space.
- Added rounded/chamfered overlap, rotation, serialization and undo regressions. Slate, slate-doc and slate-artifact test targets compile; Windows denied execution of the slate-doc regression executable (`os error 5`), so test execution is not claimed.

## 2026-09-17 — Document settings palette

- Document settings is a toggle palette (icon strip) like the other board
  tools — grid, snaps, reach, and kinds stay on the strip instead of a
  stacked form that ignored palette mode.

## 2026-09-17 — Wire property palettes and short stringers

- Dimension labels that cannot fit between their ticks move beyond the stringer end and remain editable in place.
- Wires share the shape property palette, with capsule routing, weight, dash and arrow controls. Routing is authored per wire and exported faithfully; legacy wires retain their default until edited.
- Shift/Ctrl selection and crossing marquee support wire batches. Marquee checks the actual route, avoiding both full-AABB containment and empty-box false hits.

## 2026-09-17 — Selection, arc bulge, last style, stringer clearance

- Shift+click (and Shift+marquee) add to the selection; rectangles no longer
  lose the set because hover-resize stole the press. Empty Shift+click keeps
  the current set. `P1.node.select`.
- Selection chrome is a per-shape silhouette (faint fill + outline, or the
  path itself) instead of a painted union bounding box.
- Arc grammar is start → end → middle. The last pick is the through-point, so
  dragging it changes curvature only; endpoints stay put. `arc` D03.
- Inherit-style creates (`CreateStyle::Inherit`, the kit default) take the
  last single-node stroke and fill. Stroke-only creates do not wipe fill
  memory. `P1.shape.style` / `P1.curve.create-style`.
- The align widget's bottom cluster sits past the width stringer
  (`STRINGER_GAP × zoom`) so the two do not overlap at some zoom levels.

## 2026-09-17 — Trim stroke and slimmer corner slider

- Miter stroke cross-sections follow the corner bisector; bevels and miter-limit fallbacks preserve both edge normals. Trimmed paths retain uniform-width edges instead of triangular slivers. Shared geometry fix; authored stroke settings and native SVG export stay unchanged.
- Fillet/Chamfer capsule height is halved to 17 board units. Its slider now has the reference's enclosing capsule and outlined pill thumb in both themes; metrics remain transient at the pointer.

## 2026-09-17 — Closed polyline hits the stroke

- Unfilled closed paths pick, marquee, Near-snap, wire ports, and
  smart guides on the path itself (including the closing seam), not the
  AABB. Stroke hit-testing walks each contour separately and honors
  `ClosePath`, so a phantom segment cannot fire outside the box (the
  classic close-to-origin ghost). Bounding-box resize chrome stays off
  for unfilled paths even after select. `P1.curve.pick` / `P1.wire.ports`.

## 2026-09-17 — Approved compact shape palettes

- Fill/Stroke share a low-profile picker with full-width buffers, transient cursor metrics, inline RGB percentages and document-local recent colors.
- Squircle selection controls and a single-row Fillet/Chamfer strip use shared light/dark theme slots.
- Exterior stringers edit at their rotated labels. The entire assembly follows board pan/zoom without screen-position caching or viewport relocation.
- Recent-color usage metadata records successful committed RGB changes and persists separately from scene undo/export.

## 2026-09-17 — Sharp web viewers and unified idle chrome

- Monitor/DPI-sized capture tiers restore legible text on enlarged and
  maximized web viewers while retaining valid stills during upgrades.
- Mouse coordinates track the physical capture scale.
- Plain centered title bar; native scrollbars hide with it without reflow.
  Fullscreen chrome uses the Slate index top-bar dimensions.
- Windows denied the File Atlas dependency build script and contract checker
  (OS error 5). Slate compilation, runtime tests, and a new executable remain
  blocked; these source changes have not been visually verified.


## 2026-09-16 — File Atlas portal follows board scale

- Keep the inner folder camera in portal-local units and compose it with
  the board transform for painting, LOD, and picking. Leaving contents
  preserves the local view while board zoom/pan carries the entire map.
- Initial fit is independent of board zoom; focused pan/zoom converts
  pointer input to local units. Maximized Atlas uses the same input handler.
- Corrected `portal-atlas-lens` D23's contradictory clip-only scaling rule
  and added GP9 regression coverage for rendered cards and navigation.
- Validation: release build and final regression test compilation passed.
  Windows denied launching the tests and `cargo xtask contracts` (OS error 5),
  so execution and live interaction remain unverified.

## 2026-09-16 — Retain valid web portal stills

- Low-resource portals retain the last valid frame at full opacity. Reject
  transparent and uniform black/white clears at native capture and upload.
- Eviction, same-source viewport regeneration, and recapture preserve the
  texture; rebinding to a different source still discards the old image.
- Validation: release build, test compilation, and contract audit passed.
  Windows denied launching the focused test executable (OS error 5), so
  executable tests and live zoom behavior remain unverified.


## 2026-09-15 — Retracting web portal chrome

- Slimmed and inset the URL blister. The bar overlays the page and retracts
  after 1.2 seconds of idle time; interaction or the top edge reveals it.
- Page bounds remain full-frame when chrome hides, reveals, or folds.


## 2026-09-15 — Web portal stability and hover preferences

- Camera zoom no longer resizes live browser captures or destroys a visible
  portal's session. Captures are bounded; GPU readback never waits for a copy.
- Animation sampling targets 30 fps without focus; texture uploads reuse the
  existing texture. Native Escape restores maximized portals through the
  command cancel stack.
- `board.hover_highlight`: local passive-outline preferences for the seven
  primary node types, under Preferences → Advanced settings.
- Validation: `cargo check --release -p slate --tests` passed, and the release
  test executable built. Windows denied launching both that executable and
  `cargo xtask contracts` (OS error 5); test outcomes and live animation feel
  remain unverified.

## 2026-08-24 — File Atlas portal drag-out

- Contents-focus left-drag on a File Atlas card calls `atlas_core::shell_drag`
  (same CF_HDROP as standalone File Atlas / Explorer). Copy or link, never
  move. `portal.atlas.drag_out`.

## 2026-08-22 — Article XII (one owner of knowledge)

- Constitution **Art. XII**: knowledge has one owner; incidental copies
  wait for the third; parallel interpreters stay two. Agents must not
  paste a working portal to emulate another.
- Pattern **P2.PortalHost** (locators, empty CTA, bake, focus prelude,
  paint shell, `border_hit_px`, folder-map session). Host contracts
  inherit it.
- Agent rule: `.cursor/rules/dry.mdc`. Known twins: DV-18, DV-19, DV-20.

## 2026-08-22 — File Atlas portal reuses folder_map; contents-focus click-out

- **`atlas-shell::folder_map`** is the one folder map (camera, orthogonal
  leaders, collapse grips, cards). File Atlas and the Slate File Atlas
  portal both call it. Slate still must not import `AtlasApp`.
- **`P1.portal.contents-focus`**: a primary click outside the focused
  host-portal body peels focus; entering one portal peels any other;
  wheel/pan never reach an unfocused portal. `portal-atlas-lens` D12 /
  D17 / D22 / D29 updated.

## 2026-08-21 — File Atlas lens portal

- **`portal-atlas-lens`** agreed. Host portal on the Slate board over
  `atlas-core` (not a File Atlas app feature). Portals flyout +
  `board.portal.atlas`. Contents-focus, maximize, bake poster, Open in
  File Atlas (existing hosted viewport).
- **`P1.portal.folder-drop`**: dropping a folder opens a chooser (File
  Atlas default, other honest lenses, or place files on the board). Alt
  keeps today's drop.

One entry per delivery wave. The governing docs are `KEYMAP.md` (what is
bound and why) and `ARCHITECTURE.md` (how it is built); per-app binding
tables live in each app's `commands.rs` (`SPECS`) and render in
**Advanced → Commands & shortcuts**.

## 2026-08-16 — Trim / Split / Join geometry honesty

- Boolean results snap back onto source vertices, H/V lines, and edges
  (`vector-ink` `clean.rs`). Uncut axis-aligned borders stay axis-aligned;
  collinear mid-edge vertices drop. Overlay is the topology oracle only.
- **Join** of objects that do not share area is a no-op ("Objects do not
  touch"). It no longer packs disjoint islands into one uneditable
  compound path. Group (Ctrl+G) is the grouping command. Connected
  components still union independently.

## 2026-08-22 — Dock strip paint is shared

- Canvas `DockStrip` nodes paint through `atlas_shell::dock::paint_icon_strip_card`
  — the same fieldset strip as the docked flyout, title in the outer
  border. Resize contain-scales the measured card; it does not reflow
  icons. `P1.dock-strip.chrome` / `.select` updated.

## 2026-08-16 — Dock strip (dropped toolbar)

- **`P1.dock-strip`** in `PATTERNS.md`. A canvas `DockStrip` click arms
  the command; click-hold-drag anywhere on the node moves it. Icon strip
  keeps the vertical four-dot column; stacked captions use a horizontal
  three-dot ellipsis. Icon-strip bodies use fieldset groups of circular
  secondary icons; tertiary toggles stack two-high on that datum and
  the knob slides. Hover chips sit above the dots. Selection chrome
  follows the painted fillet.

## 2026-08-16 — Split (keep every piece)

- **`board.tool.split`** on **Ctrl+Shift+T**. Same pick-cutters-then-click
  syntax as Trim; the click keeps every span/face as its own Path.
  Type "split" / Actions dock row. Contract: `contracts/split.md`.

## 2026-08-16 — Join (open paths + region union)

- **`board.path.join`** on **Ctrl+J** (already the chord) now also unions
  closed shapes and treats an open curve in that set as a stroke-weight
  ribbon. Type "join" / Actions dock row. Contract: `contracts/join.md`.
- Open+open is unchanged: nearest endpoints, first selected style, one undo.

## 2026-08-16 — Trim (2D Rhino subset)

- **`board.tool.trim`** on **Ctrl+T**. New workbook tab is **Ctrl+N** only.
  Contract: `contracts/trim.md` (`P2.RhinoTrim`). Actions dock chip between
  objects and properties.
- Pick cutters, Enter, click the dying piece. Each click is one undo.
  Line cutters are infinite. No Untrim — geometry is rewritten.
- Closed shapes become compound even-odd paths (a circle punch is a hole).
  Text and images keep the node and store the remaining region in
  `Node.clip` (SVG/CSS `clip-path`). Frames and portals are never targets.

## 2026-08-16 — Status Board portal (second generated portal)

- **`portal-status-board`** lands as a generated portal: journaled frame +
  local `project-state.json` source + section query; contents come from
  `crates/status-board` (`layout_status`) and are never journaled. Placement
  reuses `drag_rect` (click = 960×720, drag free-aspect, Shift locks 16:9).
  Commands: `board.portal.status_board`, `portal.status.source` /
  `refresh` / `bake`. No single-key chord.
- **`P1.portal` extended.** Generated-portal rules (place, bind, pick, bake,
  sync) sit beside the host-portal rules promoted with `portal-web-embed`
  (health, enter, determinism, export-honesty).

## 2026-08-08 — Web portal hardening (post-ship)

- **Deferred WebView2 admit no longer sticks on Loading.** Admission is
  re-issued every frame while a portal is eligible, so an environment that
  finishes creating after the first admit still starts the page; an environment
  that fails reports `NoRuntime` instead of pretending forever.
- **Local source probes left the UI thread.** `metadata` for Missing/mtime runs
  on a worker and is generation-tagged; a live portal whose file changes on disk
  reloads in place (D21) rather than waiting to be evicted.
- **`index.htm`-only folders bind correctly.** Drop/bind records the entry the
  folder actually holds.
- **Popups and downloads are denied** in the composition host (D15, D32).
- **`portal.web.source` accepts a detail** (URL or path) so agents share the
  human command path (GP11 / D27). `live_min_px` is 160 so a normally zoomed
  board page is live rather than looking broken.

## 2026-08-08 — Atlas mouse buttons: left acts, right navigates

- **Right-drag pans from anywhere, cards included.** It previously pans only on
  empty canvas, because a right-drag off a card handed the files to Windows —
  which meant pan failed wherever the folder was full, exactly where it is
  needed most. Ctrl+right-drag turbo pan and middle-drag pan are unchanged.
- **The shell drag-out moved to the left button**, joining the other things the
  left button already did to the card under the cursor: filesystem move/copy in
  Edit mode, and the carry-to-Slate in a linked session.
- **Left-drag on empty canvas now rubber-band selects** instead of panning.
  Shift+left-drag still forces a band from on top of a card, which is the only
  way to start one in a dense folder.
- Tests: `right_drag_pans_even_when_it_starts_on_a_card` and
  `left_drag_on_empty_canvas_sweeps_a_selection`.

## 2026-08-01 — One time axis: the activity timeline

- **The stacked pair became one control.** File Atlas' contribution graph and
  its date-window slider were two widgets with two independent scales for one
  piece of state; they are now `atlas_shell::timeline::ActivityTimeline`, where
  cells, handles, and ticks are all placed by the same `x(t)`. The Filters
  dock's duplicate slider is gone, replaced by a readout plus *clear
  selection*. Spec: `specs/activity-timeline.md`.
- **Semantic zoom instead of a scrub bar.** Wheel pans, Ctrl+wheel zooms at the
  cursor, and the 7×N weekday block morphs — staggering into per-day slots
  around a month of span, then expanding the focused day into an adaptive
  bucket strip and finally per-file dashes down to seconds. Thresholds, LOD,
  and wheel feel are tokens, not constants.
- **Discrete picks generalized to the grain in force** (`TimePicks`, a
  normalized disjoint interval set): Ctrl+click toggles the bucket you are
  looking at — a day zoomed out, an hour or a minute zoomed in — so punching a
  hole in a range is possible at any depth. Timeline reset earns its own cancel
  layer (`CancelLayer::Readout`) below canvas selection, so Esc walks the
  canvas first and the time window last.

## 2026-08-01 — Type-to-command vs bare-letter shortcuts

- **Board type-to-command**: typing opens the canvas palette with the query
  pre-seeded (same UI as double-click empty board). Bare A–Z shortcuts hold
  ~700 ms before committing so a following character can promote into
  command entry instead of stealing the first letter of a typed name
  (`B` vs `brush`). Esc cancels the hold; pointer-down / other chords
  commit early so tool-then-click stays snappy.

## 2026-08-01 — Repository Lens portal on the board

- **Board portal ships a usable v1**: `NodeKind::Portal` (generated /
  repo_lens) with journaled source + query; palette / tool placement
  (`board.portal.repo_lens`); empty-state bind; async `repo-graph`
  extract→layout paint; focus dimming; refresh / bake; inspector controls.
  Git write-back commands remain stubbed (toast) until IX.5 wiring lands.
- Earlier the same day: contract moved to **agreed**, `repo-graph` scaffolded,
  and SPECS registered.

## 2026-07-30 — The contract system covers portals (first portal contract)

- **`portal-lens-repository.md`** — the first contract for something that is
  not a canvas tool: a **generated** portal (Art. V.3 / decision D7) of type
  **lens**, subtype **repository**, drawing one git repository's branching,
  merging, and forking over time. Status: **draft** — all 31 rows are
  `proposed` in `decisions.json` and four open questions are live (time-axis
  default, fork-surface scope, placement binding, extraction backend).
- **Two constitutional refusals recorded in the contract, not silently
  complied with** (Art. XI): the source is a local git worktree, never a
  hosted account (Art. I.4), and the fork surface drawn is the one a clone can
  prove — configured remotes — with hosted fork networks left to an optional
  out-of-process enrichment rather than inferred (Art. IV.2, false-affordance
  register row 4).
- **`DIMENSIONS.md` grows a Scope column and D18–D31.** Dimensions now declare
  which contract families must answer them (`tool` / `portal` / `any`), so a
  gesture tool is not made to write `n/a` about export serialization and a
  portal is not made to invent a numeric-entry story. The fourteen new axes
  are the portal questions: class and authority, source binding, query,
  regeneration, contents interaction, level of detail, export, bake,
  collaboration, agent surface, determinism, performance envelope, failure
  states, and view-state ownership.
- **`PATTERNS.md` gains `P1.portal`** as a named but empty class: with one
  portal contract, its rules stay L3 by the promotion rule.
- **`docs/keymap/research/git-history.md`** — source research: GitKraken and
  the GitLens commit graph, GitHub's network graph, `git log --graph`'s
  first-parent lane rule, and what those tools do that this portal will not.
- **`cargo xtask contracts`** — the framework's rule ("silence is not an
  answer") becomes machine-checked: every contract answers every dimension its
  family is scoped to, every matrix row is mirrored in `decisions.json`, and a
  contract may claim `agreed`/`shipped` only when no row is proposed and no
  open question remains. Runs in `cargo test --workspace`; contracts now carry
  a `Family:` header line that the check reads.

## 2026-07-23 — Tool interaction contracts (method, not code)

- New project skill **`.cursor/skills/tool-contract`**: the codified
  communication method for pinning a tool's interaction and feel before
  building. Flow: one-line user prompt → agent research → **behavior
  matrix** in chat (best-guess defaults, row IDs, sources) → terse
  corrections by row ID → contract doc → golden-path tests.
- New catalog **`docs/keymap/contracts/`**:
  - `DIMENSIONS.md` — the **permanent matrix**: an append-only registry
    of every behavior dimension ever used (`D01`–`D15` seeded from the
    Line request). Stable IDs, never renumbered; per-tool matrices must
    account for every dimension (answer, pattern reference, or `n/a`).
    New axes discovered during any tool request are appended and persist
    for all future requests.
  - `PATTERNS.md` — hierarchical pattern vocabulary (L0 universal →
    L1 object-class → L2 archetypes → L3 tool-specific) with the
    promotion rule: a rule appearing in two contracts moves up, never
    duplicates down.
  - `TEMPLATE.md` — the contract template; matrix rows come from
    `DIMENSIONS.md` in registry order.
  - `line.md` — first worked contract (Rhino Line, status: draft).
    Flags the gap: today's Line is a drag-only bbox shape; the contract
    specifies a parametric two-point line with endpoint grips under the
    new `P2.RhinoDraft` archetype.
- **Volatile matrix canvas** — per tool request, the matrix now renders
  as an interactive Cursor canvas beside the chat
  (`<tool>-tool-contract.canvas.tsx`): Accept / Alter / Reject per
  dimension, option pills for open questions, a "propose new dimension"
  input feeding the permanent registry. Decisions persist to the canvas
  data sidecar, which the agent reads back to update the contract.
  First instance: `line-tool-contract`.
- **Decisions database** — `decisions.json`: every tool × dimension
  decision (behavior, source, confidence, verdict, date). Approved rows
  are **precedent**: a future overlapping tool (e.g. bezier after line)
  seeds its matrix from them at 85–95% confidence instead of re-guessing.
  Rows flip `proposed → approved` as completion bookkeeping, alongside
  appending user-added dimensions to `DIMENSIONS.md`.
- **Confidence column** — every matrix row (canvas, contract, database)
  carries a score: 100 stated by the user · 85–95 approved precedent ·
  75–90 cataloged pattern · 60–80 source-app research · <60 guess. Open
  questions are drawn from the lowest-confidence rows.
- **Line contract agreed** (same day): the user accepted all 15 matrix
  rows as proposed and resolved all four open questions (dock readouts ·
  45° ortho · length-only numeric entry · legacy bbox lines convert to
  parametric on load). `line.md` → Status: agreed; all 15 `decisions.json`
  rows → approved (now precedent for arc/polyline/bezier); no new
  dimensions proposed, so `DIMENSIONS.md` is unchanged at D01–D15.
  `KEYMAP.md` gains the **L** binding (🟢 adopt) and the Tab
  direction-lock note. Next step: implementation to contract (golden
  paths GP1–GP6 become headless input-script tests).
- **Line tool shipped to contract** (same day): new
  `apps/slate/src/app/board_line.rs` — draft state machine (both
  grammars, `draft.drag_threshold` disambiguation), Tab direction lock,
  typed-length numeric entry (digits/Backspace mid-gesture, Enter
  commits), F8 ortho (Shift inverts) + F9 grid + endpoint object snap,
  dock length/angle readout, crosshair + lock glyph, fg-color commit as
  one journaled Add. Committed lines are open single-segment **Path**
  nodes, so Direct Selection, Ctrl+J join, and stroke picking work
  unchanged (D14); selected lines show endpoint grips instead of a
  resize bbox (D13), grip drags journal one point-edit Patch. Legacy
  bbox lines (`ShapeKind::Line` + `flip`) migrate to parametric paths
  on load (`Scene::migrate_legacy_lines`). Feel constants pinned in
  `board_line::draft_tokens` (P0.6). Golden paths GP1–GP6 are headless
  tests (`line_gp1`–`line_gp6`); GP3's expected point corrected to
  (97,0) — the board's ortho projection convention, not a rotation.
  `line.md` + `decisions.json` → Status: shipped; `KEYMAP.md` L row →
  ✅ exists. Palette alias "segment" registered in SPECS.
- **Tool-contract skill hardened** (same day): the volatile canvas's
  "Send decisions to agent" button now dispatches `openAgent` at the
  building conversation (focuses the working agent on the taskbar —
  never `newComposerChat`, which lost context in a fresh chat), and
  step 7 (Implement + pin) is explicitly not optional: a contract
  flipping to agreed triggers implementation in the same task unless
  the user defers it.
- **Line contract amendments** (2026-07-24): Square end caps on draft
  curves (`default_curve_stroke`, distinct from round expressive ink);
  **P1.curve.create-style** — last single-node edit seeds stroke +
  opacity on the next Line commit (`board_style.rs`); D13 extended to
  multi-select (endpoint grips on every simple line, no per-line or
  group bbox). Golden paths GP7–GP8; registry gains **D16** (create-style
  inheritance).
- **Line stroke-precise pick** (2026-07-24): open curves (including simple
  lines and legacy `ShapeKind::Line`) click- and marquee-select on stroke
  geometry via `board_path::hit_shape_stroke` / `marquee_hits_node` — never
  the node AABB alone (**P1.curve.pick**, D17). Registry + template updated;
  GP9 / `line_pick_stroke_not_bbox` test.

## 2026-07-22 — P1 delivery

### New crate

- **`crates/atlas-commands`** (pure, zero-dependency — Art. I): commands as
  data. `CommandSpec` (id, name, category, chord, repeat policy,
  availability, palette aliases), `Registry` with chord lookup +
  collision validation, `History` (500-entry ring, author-attributed per
  Art. VI) with Rhino-style `last_repeatable` (never-repeat entries are
  skipped over), the `CancelLayer` cancel-stack contract
  (ActiveOperation → Draft → Mode → Selection → Chrome), and
  `palette_query` fuzzy search. This registry is the Phase-4 MCP command
  surface arriving early (Art. VII).

### Document model (`slate-doc`, with full `slate-artifact` parity — Art. IV)

- Nodes gained `hidden`, `locked`, and `group` (serde-defaulted; old
  `.slate` files load unchanged).
- New `NodeKind::Connector` — wires between board nodes. Endpoints anchor
  to a node side at a fraction (`Anchored{node, side, t}`) or float free;
  the bezier is **derived at paint/export time** from live node rects,
  never stored stale. Arrowheads, midpoint labels, Default/Faint display.
  Exports as SVG path + triangles + text.
- `TextNode.fill` (the sticky-note base) and `ImageAdjust.invert`
  (CSS `invert(1)`, mirrored in `imagefx.rs` pixel math and the artifact).

### Geometry (`vector-ink`)

- New `edit` module: anchor/handle model over bezier paths —
  `anchors_from_bezpath`/back (lossless), move anchor/handle (Alt breaks
  smooth symmetry), angle-preserving segment translation (Illustrator
  "constrain path dragging"), corner↔smooth conversion, `join_endpoints`
  (merge / close / bridge), anchor and segment hit-testing.

### Shared chrome (`atlas-shell` — Art. X)

- **Minimap** (`minimap.rs`): squircle overlay, content rendered to a
  generation-keyed cached texture (Art. II), viewport rectangle,
  click/drag/scroll navigation. Both apps.
- **Canvas palette** (`palette.rs`): anchored fuzzy-search popup, keyboard
  navigation, zero cost while closed.
- **History window** (`history_ui.rs`): read-only command log with author
  chips and copy-to-clipboard.
- New `[minimap]` and `[palette]` token sections in `ui-tokens.toml`.

### Slate

- **Registry migration**: `ENTRIES` → `SPECS`; keys dispatch through the
  registry; every dispatch and major mutation pushes attributed history.
  Space (tap) / Enter (idle) = **repeat last command**; Esc = formal
  cancel stack; F1 help, F2 history window, Ctrl+Shift+P preferences,
  Ctrl+N new tab.
- **Tools**: **B** Brush (fg color, sticky tool, Shift+click straight
  chain, `[`/`]` Photoshop-tier width stepping, width-circle cursor),
  **E** Eraser (whole-stroke, live 30% preview, one undo group),
  **I** Eyedropper (+ spring-loaded Alt from Brush; Alt+click samples
  background), **N** Sticky note (Tab-while-editing spawns the next
  sticky), **A** Direct Selection (anchors/handles/segments via
  vector-ink, double-click toggles corner/smooth), **D**/**X** color
  reset/swap with persisted fg/bg state + dock chips, **C** enter crop.
- **Wires**: hover-edge grips; drag to connect (snap-solid preview);
  Shift = add, **Ctrl = detach/rewire**, **Ctrl+Shift = move all wires**
  (Grasshopper grammar); release on empty opens the palette and
  auto-connects the placed node; labels via double-click; arrowhead/faint
  controls in the context menu.
- **Flags**: Ctrl+G/Ctrl+Shift+G group/ungroup (click selects the group,
  Ctrl+Shift+click picks a member), Ctrl+H/Ctrl+Shift+H hide/show-all,
  Ctrl+L/Ctrl+Shift+L lock/unlock-all (locked nodes still feed smart
  guides — Rhino), readout chips for hidden/locked counts.
- **Constraints**: **F8** ortho (45° steps; held Shift *inverts* it —
  Rhino), F9 snap, G/F7 grid; ortho feeds moves, drafts, wires, anchor
  drags with DominantOrtho snap projection.
- **Overlays**: **M** minimap, double-click empty board = canvas palette,
  **Ctrl+F** board search (dim non-matches, Enter cycles, camera flight),
  Tab/Shift+Tab reading-order object cycling with camera follow.
- **Clipboard**: Ctrl+C/X/V (paste at pointer, +24 stepping, connector
  bridging with anchor degradation), **Ctrl+Shift+V paste in place**.
- **Misc**: PageUp/PageDown/Ctrl+B z-order, Ctrl+J join paths, Ctrl+U
  image-adjust popover, Ctrl+I invert image, F3 inspector toggle,
  arrows pan when nothing is selected, **Z** zoom tool (click in,
  Alt+click out, drag = zoom window).
- **Join** (Ctrl+J): merge coincident endpoints, close open paths, bridge
  nearest endpoints across two paths (first path's style wins).

### File Atlas

- Registry migration + Space/Enter repeat + Esc cancel stack (existing
  order preserved), history log surfaced in Advanced.
- **M** minimap over the folder tree (avg-color file tints), **Ctrl+F**
  focuses the filter search, Tab/Shift+Tab cycles filtered files with
  camera follow, **Z** zoom tool, arrows pan (Shift ×4), **Ctrl+C** copies
  selected file paths, Ctrl+N new tab, F1/F3/Ctrl+Shift+P.
- Unchanged by design: F2 = Assign, Shift+click = range select,
  double-click empty = zoom-to-point, Ctrl+right-drag = turbo pan.

### Deliberately rejected (see `KEYMAP.md` for reasons)

Ctrl+RMB zoom (turbo pan wins), Ctrl+T trim (new-tab + Art. III),
Ctrl+W zoom window (Z-drag covers it), F11 attributes (fullscreen),
F12 DigClick, Ctrl+P print (deferred to Roadmap Phase 5), Delete/Ctrl+S
in Atlas.

### Deferred to P2 (specced in `specs/`, not built)

Radial menu (middle-click), scale tool (S), rulers/guides (Ctrl+R),
brush preset cycling (,/.) + F6 color panel, graphic styles (Shift+F5),
Shift+letter tool-family cycling, segment-splitting eraser, image-pixel
eyedropper, nested groups, show-hidden picker, Atlas command palette,
shared fg/bg chrome primitive, connector relations in the AI beacon.

## 2026-09-15 — Agent portal refinement

Approved minimal icon picker, hover chrome, semantic input wires, in-node image albums and journaled Unbundle. Added `portal.agent.unbundle` and `portal.agent.stop`. No new shortcuts. See `contracts/portal-agent-link.md`.


## 2026-09-17 — native shape property implementation

Implemented geometry-gated circular selection palettes; transaction previews; desktop RGB sampling; centered dimension stringers; persistent percent/absolute fillet/chamfer geometry with export parity. Ordered input preserves moving line/polyline/arc picks and all freehand event samples; draft polyline endpoints/segments participate in snapping and start closure. Added command registry entries and regression coverage. Shared ownership passed Article XII review. See the shape contracts and desktop sampler acceptance notes.
