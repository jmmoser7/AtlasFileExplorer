# File Atlas Edit Mode Drag — interaction contract

Status: shipped
Family: tool
Reference: Windows File Explorer drag / context menu
Command: atlas.edit_move / atlas.edit_copy · Key: none · Palette: Mode dock
Inherits: P0.* — deviations flagged below.

## Behavior Matrix

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-----------------|--------|------|
| D01 | Initiation & arming | Mode dock exposes View and Edit. Edit arms real filesystem operations for the active tab only; View refuses them. | stated | 100 |
| D02 | Stickiness & repeat | Mode persists while the tab is active or parked, but fresh roots and launches reset to View. Drag operations are one-shot. | stated | 100 |
| D03 | Gesture grammar | Edit mode: LMB down on file/folder, drag past egui threshold, ghost follows cursor, folder hover validates target, release commits move/copy or cancels on blank/invalid. An LMB drag that starts on empty canvas is a rubber band, not an edit drag; RMB always pans. | stated | 100 |
| D04 | Click vs drag rule | Existing egui drag threshold disambiguates click/select from drag. Clicks keep current selection/open behavior. | pattern | 85 |
| D05 | Modifiers | Alt held at release copies; no Alt moves. Shift keeps rubber-band selection precedence. Ctrl is not an edit-drag modifier. | stated | 100 |
| D06 | Constraints & snapping | No grid, ortho, or object snap applies to filesystem placement. The target is the deepest folder whose bounds contain the cursor, so a folder's whole rectangle accepts a drop — including over the files it already holds. | stated | 100 |
| D07 | Direction / value locks | n/a: filesystem drag has no direction or numeric parameter lock. | stated | 100 |
| D08 | Numeric / manual entry | n/a for drag. Rename and new-folder names are typed in the anchored popup; Enter commits, Esc cancels. Every edit-mode prompt and confirmation opens at the cursor, never at a screen edge. | stated | 100 |
| D09 | Preview & readouts | Drag paints a semi-transparent ghost at the cursor naming the destination folder (or saying the release does nothing) and highlights the valid folder target. The bottom readout shows running filesystem operation progress. | stated | 100 |
| D10 | Cursor | Drag cursor is grabbing for move and copy cursor when Alt is held. Invalid/blank targets show no folder highlight. | stated | 100 |
| D11 | Commit | Release on valid folder dispatches a background FsOp. A copy is first accounted for cloud placeholders on a worker thread — files and whole folder subtrees, directory entries only — and confirms before downloading anything. Completed changes are journaled as FsRename, FsMove, FsCopy, or FsDelete and applied to the current index/tree. | stated | 100 |
| D12 | Cancel | Release on blank canvas or invalid target is a null action: no filesystem operation, no dialog, no journal entry. Esc closes prompt/dialog layers before canvas state. | stated | 100 |
| D13 | Selected presentation | Existing File Atlas selection presentation remains unchanged; edit drag does not add grips or bbox handles. | stated | 100 |
| D14 | Post-edit | Watcher/index reconciliation refreshes the canvas. Rename/move rewrite assignment keys so staged export metadata follows the file. | stated | 100 |
| D15 | Non-goals | No cut/paste file operations, no agent-initiated writes, and no Delete command outside Edit mode. Delete acts on the selection, or on the item under the cursor when nothing is selected — the only way to delete a folder from the keyboard, since folders are never part of a selection. | stated | 100 |
| D16 | Create-style inheritance | n/a: filesystem operations do not inherit visual style. | stated | 100 |
| D17 | Hit-testing & pick | Picking up uses the card hit-test (`Tree::hit_test`); dropping uses containment (`Tree::dir_at_point`), because a drop is aimed at a folder, not at a card. The target rejects self, the current parent, and descendants of dragged folders. | stated | 100 |

## Feel Constants

| Token | Meaning | Initial value |
|-------|---------|---------------|
| egui drag threshold | Click/drag split | inherited from egui |
| edit drag ghost opacity | Cursor-attached drag preview | 0.82 |
| target highlight stroke | Valid folder drop target | 2 px accent stroke |
| delete large branch files | Re-arm delete warning | 200 descendant files |
| delete large branch bytes | Re-arm delete warning | 1 GiB |
| delete large selection | Re-arm delete warning | 25 selected items |

## Golden Paths

1. GP1: In View mode, LMB drag a file over a folder and release -> no filesystem operation starts.
2. GP2: In Edit mode, LMB drag a file over a folder — anywhere inside it, including over its files — and release with Alt up -> move operation starts and a journal entry records the old and new paths.
3. GP3: In Edit mode, LMB drag a file or folder over a folder and release with Alt held -> the sources are walked off the frame loop for cloud placeholders (readout says "checking"), then the copy starts, or the download confirmation opens at the cursor first.
4. GP4: In Edit mode, drag a file and release over blank canvas -> no filesystem operation and no journal entry.
5. GP5: In Edit mode, right-click a file or folder -> Rename, Add subdirectory, and Delete are shown; in View mode they are hidden behind the mode hint.
6. GP6: Suppressed delete confirmation stays suppressed for small deletes, but any directory with subdirectories, at least 200 descendant files, at least 1 GiB, or a selection over 25 items always prompts.

## Open Questions

None outstanding.
