# Spec — groups, lock, hide (scene flags)

Stage-2 spec. Research inputs: `../research/rhino.md` §3–5.
Constitution: Art. VI (journaled patches), Art. IV (export honesty).

## Model (`crates/slate-doc/src/scene.rs`)

`Node` gains three serde-defaulted fields (old `.slate` files load
unchanged; `skip_serializing_if` keeps files clean):

```rust
pub struct GroupKey(pub u64);   // allocated like NodeId

pub struct Node {
    // …existing…
    #[serde(default)] pub locked: bool,
    #[serde(default)] pub hidden: bool,
    #[serde(default)] pub group: Option<GroupKey>,
}
```

Flat groups only (no nesting, no multi-parent — Rhino's cross-reference
pain, research §5e, is deliberately excluded). All flag changes are
ordinary `Patch` commands.

## Semantics matrix

| System | hidden | locked | grouped |
|--------|--------|--------|---------|
| Paint | skipped | normal paint | normal |
| Hit-test / click | skipped | skipped | click any member → select **all** members |
| Marquee | skipped | skipped | member inside rect → whole group selected |
| Ctrl+Shift+click | — | selects it (the escape hatch) | selects **only** that member |
| Drag/resize/rotate | n/a | excluded | group moves/scales as one (existing multi-sel bbox machinery) |
| Smart guides source | no | **yes** (Rhino: locked still snaps) | yes |
| Delete / Ctrl+X | n/a (not selectable) | excluded | whole group |
| Duplicate (Ctrl+D / Alt-drag) | n/a | excluded | duplicates whole group with a **fresh GroupKey** |
| Frames-as-slides / present | **excluded** | included | included |
| HTML artifact export | **excluded** | included | included (no wrapper element needed — flat) |
| Tab cycling | skipped | skipped | group counts as one stop |
| Select all (Ctrl+A) | excluded | excluded | included as groups |

Hidden + locked persist in the document (Rhino persists them; §3e).

## Commands & bindings

| Chord | Command | Behavior |
|-------|---------|----------|
| Ctrl+G | `board.group` | Selection (≥2 nodes) gets one fresh GroupKey (members leaving old groups). Journal: one group of Patches. |
| Ctrl+Shift+G | `board.ungroup` | Clear `group` on every selected member of any selected group. |
| Ctrl+H | `board.hide` | `hidden = true` on selection; selection clears. |
| Ctrl+Shift+H | `board.show_all` | `hidden = false` on all hidden nodes (P1 form; ShowSelected picker is P2). |
| Ctrl+L | `board.lock` | `locked = true` on selection; selection clears. |
| Ctrl+Shift+L | `board.unlock_all` | `locked = false` on all locked nodes. Rhino separates unlock-selected vs all; since locked nodes aren't selectable, **all** is the P1 form. |
| Ctrl+Shift+click | sub-object select | member inside a group; also reveals a locked node for one-off selection (grayed handles, then editable as usual — Rhino's practical escape). |

Context menu (right-click object) gains Group/Ungroup, Lock, Hide rows;
right-click empty canvas gains "Show all hidden (n)" / "Unlock all (n)"
when applicable — discoverability for states with no visible objects.

## Feedback

- Ctrl+H on a selection briefly ghosts the nodes out (150 ms fade) so the
  disappearance reads as intentional.
- A locked node force-selected via Ctrl+Shift+click paints grayed handles
  (theme token, shared palette — Art. X).
- Bottom readout shows "n hidden · n locked" chips when nonzero (click =
  show/unlock all) so state is never invisible chrome-wide.

## Cross-effects to audit in the build

- `duplicate_board_nodes` must remap GroupKeys.
- Alignment/distribute treat a group as one unit.
- Connectors may anchor to grouped/locked nodes (fine); anchors on hidden
  nodes render the connector end as Free-positioned until shown (derived
  geometry uses last rect; simplest: hide connectors whose anchor node is
  hidden — pick in build, document in COMMANDS.md).
- Present mode + artifact writer skip hidden (Art. IV honesty: export
  serializes what the board shows).

## Tests (slate-doc)

- Serde: old scene JSON without the fields loads with defaults.
- Group/ungroup/hide/lock patches invert cleanly through the journal.
- (App tests) marquee skips hidden/locked; group click selects members.
