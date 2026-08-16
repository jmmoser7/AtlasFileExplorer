# Text — interaction contract

Status: draft
Family: tool
Reference: Miro sticky / text place
Command: board.tool.text / board.tool.sticky · Key: T / N · Palette: text, sticky
Inherits: P0.*, P1.node, P2.GhostPlace — deviations flagged below.

## Behavior Matrix

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-----------------|--------|------|
| D01 | Initiation & arming | Dock Text icon opens a flyout submenu: (1) Text box · (2) Sticky note. Selecting a row arms that variant with a cursor-locked ghost. Hotkeys: T → Text box, N → Sticky. Palette: text / sticky (+ aliases). Space/Enter re-arms the last of the two (P0.4/P0.7). | stated | 100 |
| D02 | Stickiness & repeat | Text box: one-shot — after place+edit-blur (or Esc commit), return to Select. Sticky: stays armed after commit; Tab / Shift+Tab while editing spawns an adjacent sticky and keeps the tool armed. Esc while armed (no edit) disarms to Select. | research | 70 |
| D03 | Gesture grammar | Armed → GhostFollow (default-size ghost locked to cursor) → Press → (ClickPlace \| DragScale) → Commit → enter inline edit. Click-release places at default world size. Press-drag-release places and scales. Replaces P2.PlaceOnce for text/sticky (new L2: GhostPlace). | stated | 100 |
| D04 | Click vs drag rule | Cursor travel > draft.drag_threshold (4 screen px) before release = DragScale; release within threshold = ClickPlace at default size. Drag under MIN_DRAW discards. | stated | 100 |
| D05 | Modifiers | Held Shift during DragScale locks aspect: Sticky → square; Text box → lock to default aspect (280×48). No Alt/Ctrl create modifiers in v1. | research | 65 |
| D06 | Constraints & snapping | F9 grid snap applies to the ghost's anchor (Text: top-left of default rect; Sticky: center) and to both corners of a DragScale rect. F8 ortho does not apply to area placement. Object-snap off for v1. | guess | 50 |
| D07 | Direction / value locks | n/a during placement (no Tab direction lock). Tab while editing a sticky is the spawn-adjacent command (D02), not a place-time lock. | pattern | 80 |
| D08 | Numeric / manual entry | No digit-entry sizing during place (Art. III). After commit, font size / box size edit via inspector + bbox grips only. | guess | 70 |
| D09 | Preview & readouts | While Armed: ghosted Text box or sticky preview locked to the cursor at the default world size (text: 280×48 + sample Text; sticky: STICKY_SIZE + yellow fill), alpha from place.ghost_alpha. During DragScale: live sized ghost. Paint always uses world metrics × camera zoom (font = size×z, wrap = rect.w×z) with no screen-constant / auto-fit path and no relative-scale floor. | stated | 100 |
| D10 | Cursor | While Armed / DragScale the ghost is the cursor feedback (plus optional thin crosshair at the anchor). No system I-beam until inline edit starts after commit. | stated | 100 |
| D11 | Commit | Text box → TextNode (fill=None, default text Text, size 24 world). Sticky → TextNode with STICKY_FILL + STICKY_INK, empty text, size 24 world. Journal cmds board.tool.text / board.tool.sticky; one gesture = one undo. Immediately open inline edit. Creation DragScale sets rect from drag and scales font by mean scale vs default rect. Subsequent bbox resize of an existing node changes the frame only — authored font size stays put and text reflows. | stated | 90 |
| D12 | Cancel | Esc while GhostFollow / mid-DragScale → disarm to Select, no node (P0.1). Esc while inline-editing → commit text + peel edit (sticky stays armed per D02; text returns to Select). | pattern | 85 |
| D13 | Selected presentation | Standard node resize bbox + rotation (P1.node). Multi-select uses the shared group bbox; group uniform scale also scales TextNode.size. | pattern | 80 |
| D14 | Post-edit | Double-click enters inline edit. Inspector edits family / size / color / align / fill. Single-node bbox resize = frame-only reflow at fixed authored font size. No Miro Auto font size. | stated | 95 |
| D15 | Non-goals | Cut (Art. III): Miro Auto font size; sticky bulk-mode / spreadsheet paste; sticky stack; emoji reactions; S/M/L size picker chrome; screen-constant text / LOD that changes relative glyph size vs geometry. | stated | 100 |
| D16 | Create-style inheritance | Next Text box / sticky consumes the last single-node Text edit when present: family, size, color, align; sticky also inherits fill (else STICKY_FILL). Defaults when none: Text box fill=None size 24; sticky yellow fill size 24. | guess | 55 |
| D17 | Hit-testing & pick | AABB of the text/sticky rect (area pick), including the fill for stickies. Marquee intersects the rect. Not stroke-precise. | pattern | 85 |

## Feel Constants

| Token | Meaning | Initial value |
|-------|---------|---------------|
| draft.drag_threshold | Click vs drag split | 4 screen px |
| text.default_rect | Text-box default size | 280×48 world |
| text.default_size | Authored font size | 24 world |
| sticky.default_fill | Sticky fill | STICKY_FILL |
| place.ghost_alpha | Armed ghost opacity | Existing place token |

## Golden Paths

1. GP1: Press T, click empty board, release under threshold -> text box appears at default size and enters inline edit.
2. GP2: Press N, click empty board, type, press Tab -> current sticky commits and a new adjacent sticky begins editing.
3. GP3: Press T, drag beyond threshold, release -> text box frame matches the drag rect and text enters inline edit.
4. GP4: Press Esc while the ghost follows the cursor -> no node is created and Select is restored.

## Open Questions

Rows D05, D06, D08, and D16 remain proposed.
