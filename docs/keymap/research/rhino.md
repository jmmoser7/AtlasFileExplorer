# Rhino 3D — Stage-2 keymap research input

> **Purpose:** UX research on McNeel Rhinoceros’s command-driven interaction model, for adapting patterns to Slate (2D infinite canvas) and File Atlas (file-manager canvas).  
> **Scope:** Rhino 8 on Windows unless noted; Mac uses ⌘ where Windows uses Ctrl.  
> **Sources:** McNeel official help, User’s Guide / Level 1 training PDFs, keyboard-modifiers reference, McNeel Discourse threads (2020–2026).  
> **Date:** 2026-07-22

---

## How to read this document

Each feature section uses the same schema:

| Field | Meaning |
|-------|---------|
| **(a) Trigger** | Keys, clicks, status-bar toggles, command names |
| **(b) Immediate feedback** | What the user sees/hears on activation |
| **(c) Modifiers** | Shift / Ctrl / Alt / Tab / held-key inversions |
| **(d) Commit / cancel** | How the action finishes or aborts |
| **(e) Polish & edge cases** | Subtle behaviors power users rely on |
| **(f) Atlas recommendation** | How to adapt for a 2D infinite-canvas app with a registered-command registry |

---

## 1. The command paradigm *(most important)*

Rhino is **command-first**: almost every edit is a named, multi-step procedure driven from a persistent **command prompt** at the top or bottom of the window. The prompt shows the active command, options in parentheses, and selection counts. Users are trained to **watch the prompt** continuously.

### (a) Trigger

| Action | Trigger |
|--------|---------|
| Start command | Type name (autocomplete from first letters), toolbar icon, menu, alias, or paste macro |
| Accept default / advance step | **Enter**, **Spacebar**, or **RMB click** in a viewport (when not dragging) |
| Repeat last command | **Enter**, **Spacebar**, or **RMB click** when **no command is active** |
| Cancel | **Esc**; or click another toolbar/menu command (replaces current command) |
| Recent commands | Right-click **command-line area** → pop-up list (default ~20 entries, configurable) |
| Command history window | **F2** (separate floating/history view; see §2) |

**RMB disambiguation (critical):**

- **RMB down + drag** → viewport navigation (rotate in Perspective, pan in parallel views).
- **RMB click without movement** → same as Enter (accept prompt / repeat last command).
- **RMB click-and-hold** (default **250 ms** delay) → **context menu** (favorites + recent commands). Setting delay to **0 ms** makes RMB immediately open the menu and **stops** accidental “repeat last command.”

### (b) Immediate feedback

- Command name appears at prompt: e.g. `Line`, then `Start of line`, then options `(BothSides Vertical Tangent …)`.
- Options show **underlined shortcut letters**; clicking an option or typing its letter switches mode.
- Command **history** lines append below the prompt (e.g. `5 curves added to selection`).
- During point-pick phases, cursor marker jumps when snaps/ortho engage; status bar shows coordinates.

### (c) Modifiers

- **Enter ≡ Space ≡ RMB (static)** — hardwired; not fully remappable.
- Typed **numeric/coordinate input** at prompt bypasses pick phases (`@` relative, `r` relative, `w` world).
- **F1** while a command runs opens Help for that command.
- Starting a **new command from UI** while one is running **cancels** the current command immediately.

### (d) Commit / cancel

| Phase | Commit | Cancel |
|-------|--------|--------|
| Option choice | Click option or type letter | Esc |
| Object selection | Enter when done selecting | Esc (also clears selection — see below) |
| Point picks | Click or typed coordinate + Enter | Esc |
| Command complete | Returns to `Command:` idle prompt | — |

**Esc semantics (single key, many effects):** When Esc is pressed, Rhino **simultaneously**:

1. Cancels the running command (if any).
2. Clears the command prompt.
3. **Deselects all objects.**
4. Turns off control/edit point display.
5. Clears an open selection menu.
6. Exits fullscreen (if active).

There is **no staged Esc ladder** in the official model — one Esc is a full reset of command + selection + point display. (Individual commands may fail to respond mid heavy computation; this is treated as per-command bugs.)

**SelNone** deselects all but **does not run inside** an active command to clear pre-selection.

### Never-repeat list

Some commands **do not become “last command”** for Enter/Space/RMB repeat. Documented examples:

- **Undo** — repeating Undo via Space would be dangerous; repeat skips to the command *before* Undo.
- **Delete** — same rationale.

Users extend this list in **Options → General → Never repeat these commands** (free-text command names, one per line). Common user additions: **Zoom** (often the alias **`Z`**, not `Zoom`), **BringToFront**, navigation-only commands.

**Rules:**

- Only **command names** count; commands invoked **inside macros still repeat** even if listed.
- When repeat hits a never-repeat entry, Rhino **walks back** to the previous repeatable command.
- No native “repeat last-but-one”; workarounds: never-repeat list, command-line recent menu, or macros (`! _Line pause pause Sellast BringToFront SelNone`).

### Nested command states

- **Nestable commands** run inside another via macro prefix **`'`** (apostrophe), e.g. `'SelAll` during Move.
- **`Pause` / `_MultiPause`** in macros wait for user input without ending the parent command.
- **`*Command`** at macro start repeats that command until cancelled.
- **Scripted commands** (`-CommandName`) skip dialogs; options passed on command line.
- Command options can spawn sub-prompts; parent resumes after sub-command completes or Esc (cancels entire stack behavior is command-dependent).

### (e) Polish & edge cases

- Autocomplete: typing partial name completes most-used match; Enter starts it.
- **Space as Enter during string entry** (`GetString`) is a known pain point for scripters.
- RMB-as-Enter conflicts with pen tablets and orbit gestures — long-standing user frustration; mitigations are delay/context-menu tuning, never-repeat list, not separate Enter binding.
- **Launching a new command** from chrome cancels the old one without confirmation.
- View navigation (wheel, RMB drag) does **not** enter command history; explicit **Zoom** command does and breaks Space-repeat workflows unless `Z` is never-repeat.

### (f) Recommendation for Atlas / Slate

Adopt Rhino’s **prompt-centric loop** but **decouple** Enter/Space/RMB more cleanly for a 2D canvas:

1. **Command registry entry shape:** `{ id, display_name, repeat_policy: Always | Never | SkipToPrevious, phases[], default_options }`.
2. **Repeat policy defaults:** Mark `undo`, `redo`, `delete`, `zoom`, `fit_view`, `deselect_all` as **Never**; match Rhino’s skip-to-previous behavior.
3. **Single “reset” binding (Esc):** Cancel active command → clear pick state → deselect → collapse transient panels. Document explicitly in Advanced → Commands.
4. **Do not bind repeat-last to RMB** on a canvas that uses RMB for pan; use **Enter / Space only**, or a dedicated **`RepeatLast`** command on a chord.
5. **Command prompt UI:** Show `RunningCommand › phase › options`; mirror history into a scrollable strip (F2 analogue).
6. **Macros / nesting:** Support `pause` tokens in registered commands for multi-step flows (tag assignment, export pick folder).
7. **Pre-selection:** Allow selecting before command starts; Esc clears pre-selection only when idle (Rhino: SelNone blocked mid-command — consider being clearer in Atlas).

---

## 2. F2 — Command History window

### (a) Trigger

- **F2** toggles the Command History window (V8+ closes on second F2).
- Menu: Tools → Command History.
- Script: `-CommandHistory` with **File** option to export.

### (b) Immediate feedback

- Floating or dockable window listing **~500 lines** of command-line text from the **current session**.
- Distinct from the live prompt strip (history is read-mostly log).

### (c) Modifiers

- **Esc**, **Enter**, or **F2** again closes (V8+).
- Text is selectable/copyable.

### (d) Commit / cancel

- Read-only audit trail; closing does not affect model.
- Session ends → history discarded unless exported.

### (e) Polish & edge cases

- Limit not user-configurable (500 lines); scripts use `rs.CommandHistory()` / periodic file append.
- Right-click **command prompt** shows **recent commands** (default 20) — different from F2 full log but overlapping use.
- `ClearCommandHistory` for scripting.

### (f) Recommendation

- Ship **Command log panel** bound to **F2**: session transcript + filter + copy.
- Persist last N lines to disk optionally; link log lines to **command registry IDs** for agent/debug replay.
- Separate **“recent commands”** popover (Rhino right-click prompt) from full history.

---

## 3. Hide / Show / ShowSelected

| Command | Shortcut (Win) | Effect |
|---------|----------------|--------|
| **Hide** | Ctrl+H | Conceal selected objects |
| **Show** | Ctrl+Alt+H | Re-display **all** hidden objects |
| **ShowSelected** | Ctrl+Shift+H | Show only chosen hidden objects |
| **HideSwap** | — | Swap visible ↔ hidden sets |
| **Isolate** | — | Hide everything except selection (+ locked) |
| **Unisolate** | — | Restore Isolate |

Scripted **`-Hide`** / **`-Show`** support **named sets** (Hide Clusters workflow).

### (a) Trigger

Shortcuts above; `-Hide` for named set on command line.

### (b) Immediate feedback

- Hidden objects disappear from all normal display modes.
- **ShowSelected** temporarily **inverts the scene**: all hidden objects become visible (often wireframe in shaded views), normal objects hidden — a picking mode, not a permanent invert.

### (c) Modifiers

- None on shortcuts; named sets via `-Hide` / `-Show Name`.

### (d) Commit / cancel

- Immediate mode commands; no multi-step commit.
- Esc does not “un-hide”; use Show / ShowSelected.

### (e) Polish & edge cases

- Hidden state **persists in `.3dm` save** for the active document.
- Does **not** hide control/edit points (separate **HidePt** / **ShowPt**).
- **ShowSelected** does not affect hidden control points.
- Layout/detail visibility (HideInDetail) has separate persistence rules for linked/worksession files.
- **Snapshots** can capture locked/hidden state among other settings.

### (f) Recommendation

- Map to board item **visibility** (not delete): `hide`, `show_all`, `show_selected`.
- **ShowSelected inversion UX** is worth copying for dense boards — momentarily ghost the rest of the canvas.
- Persist hidden flags in document model; expose in Properties (§10).
- Optional named hide sets for presentation / export portals.

---

## 4. Lock / Unlock

| Command | Shortcut (Win) | Effect |
|---------|----------------|--------|
| **Lock** | Ctrl+L | Visible, snap-able, **not selectable** |
| **Unlock** | Ctrl+Alt+L | Unlock **all** locked objects |
| **UnlockSelected** | Ctrl+Shift+L | Unlock selected locked objects |
| **LockSwap** | — | Swap locked ↔ unlocked |
| **IsolateLock** | — | Lock everything except selection |

Layer lock locks all objects on a layer (related but separate appearance rules).

### (a) Trigger

Shortcuts; `-Lock` supports named sets like Hide.

### (b) Immediate feedback

- Locked objects render **grayed** by default (global **Appearance → Colors → Locked objects**, overridable per display mode: “use object attributes” vs specified lock color).
- Locked layer vs locked object: **object lock** changes appearance; **layer lock** behavior differs (blocks on locked layers may still gray).

### (c) Modifiers

- **SnapToLocked** command enables osnap on locked objects (default: snaps work on locked geometry).

### (d) Commit / cancel

- Immediate; Unlock vs UnlockSelected distinction matters on crowded files.

### (e) Polish & edge cases

- Cannot select locked objects for editing; **can snap** to them (background reference geometry pattern).
- SubD control points can be locked independently.
- Known confusion: BlockEdit can leave **duplicate locked orphans** — not intentional UX.

### (f) Recommendation

- **Lock** = position/style frozen, still visible, optionally snap-target (good for atlas frames / guide art).
- Separate **Unlock all** from **Unlock selected** shortcuts (Rhino’s Ctrl+Alt+L vs Ctrl+Shift+L).
- Grayed lock tint via theme token (shared chrome).
- Locked items: no move/resize/delete; allow snap; block marquee select unless Alt bypass.

---

## 5. Group / Ungroup

| Command | Shortcut | Effect |
|---------|----------|--------|
| **Group** | Ctrl+G | Named unit; default name `Group<N>` |
| **Ungroup** | Ctrl+Shift+G | Dissolve group on selection |
| **UngroupAll** | — | Ungroup nested hierarchy in one step |
| **SetGroupName** | — | Rename; can merge groups |
| **AddToGroup** / **RemoveFromGroup** | — | Membership edits |

### (a) Trigger

Shortcuts; **SetGroupName** for human-readable names; **SelGroup** to select by name.

### (b) Immediate feedback

- Clicking any member selects **entire group** (default).
- Group names case-sensitive; auto-incrementing default names.

### (c) Modifiers

- **Ctrl+Shift+click** (or window/crossing): **sub-object pick within group** without ungrouping.
- Same chord selects sub-curve faces etc.; with groups, may still invoke **selection menu** / sub-object UI (known rough edge).
- **Ctrl+Shift+window** selects whole objects inside groups without menu (forum tip).
- Advanced `Rhino.Options.General.ControlShiftSubObjectSelect` — reported inconsistently hooked up.

### (d) Commit / cancel

- Immediate; nested groups supported but **hierarchy is opaque** (objects can belong to multiple groups — cross-reference).

### (e) Polish & edge cases

- Extracting nested group from hierarchy often requires **ungroup repeatedly** or scripts (`Externalize_Group`).
- Imported SKP files create deep nested groups.
- **UngroupAll** (V8) reduces pain for nested cleanup.

### (f) Recommendation

- Groups as **selection affordance**, not tags (Slate tags stay in slate-doc per constitution).
- **Ctrl+Shift+click** to pick one board node inside a group; default click selects group.
- Named groups via rename command; show group name in Properties.
- Avoid multi-parent group cross-reference until needed; prefer tree/group IDs.

---

## 6. Join (Ctrl+J) and Trim (Ctrl+T)

### Join

- **Ctrl+J** — connects touching curves → polycurve; surfaces → polysurface/solid if edges within **2× tolerance**.
- Does **not** refit geometry — boolean meshing tag only for surfaces.
- **SelChain** selects tangent-connected strings.
- **JoinCopy** (V8): join duplicates, keep inputs.
- Curve join: end-to-end within tolerance; layer follows **first selected** object.

### Trim

- **Ctrl+T** — select **cutting objects**, then click parts to **remove**.
- **Shift+click** near curve end **extends** curve to cutter.
- **ApparentIntersections**: trim curves by **screen projection** in active viewport.
- Related: **Split** (keep both sides), **Untrim***, **ReplaceEdge**.

### (a–d) Triggers & flow

Both are **multi-step commands**: select inputs → Enter → further picks → Enter completes. Esc cancels.

### (e) Polish

- Join failure usually means gaps exceed tolerance — users run **JoinEdge** / **MatchSrf** separately.
- Trim vs Split: Trim deletes picked regions; Split keeps pieces.

### (f) Recommendation

- **Join** maps cleanly to **path/shape merge** on vector board (vector-ink polylines).
- Defer **Trim** until boolean/clip semantics exist; Split-before-delete may be clearer on 2D canvas.
- Expose tolerance in join command options; show preview of merged outline.

---

## 7. Ortho (F8)

### (a) Trigger

- **F8** toggle; status bar **Ortho** pane; command **Ortho** / alias **O**.
- **Shift+F8** (Mac docs) / **Shift hold** inverts mode while held.

### (b) Immediate feedback

- Cursor constrained to multiples of **OrthoAngle** (default **90°**) from **last created point**.
- **Hash marks** radiate at ortho angles when enabled (disable via `ShowOrthoHashMarks` advanced setting).
- Status bar **Ortho** label bold when on.

### (c) Modifiers

- **Shift** — **temporary invert**: if Ortho off → constrain while held; if Ortho on → free movement while held.
- **Tab** — locks **current tracking direction** (separate constraint; works with SmartTrack).
- **Ctrl** during pick — perpendicular to CPlane constraint (related modeling aid).
- **Osnap can override Ortho** while Shift-held (long debate; Rhino 9 adds **`Rhino.Options.ModelAid.DominantOrtho`**).

### (d) Commit / cancel

- Mode toggle persists across commands until toggled off.

### (e) Polish

- **OrthoAngle** nestable — change to 45° mid-session (`OrthoAngle 45`).
- Ortho measures from **last point created**, not last pick generically.
- Visual bugbear: ortho hash marks stay visible when osnap “derails” constraint — users want red/warning state.

### (f) Recommendation

- **F8** + **Shift invert** maps directly to 2D board: constrain drag to H/V (or 45° with shift-variant).
- Apply from **last anchor** of line/connector/frame move.
- When snap active, **project snap onto ortho line** (DominantOrtho behavior) — safer for diagram work.
- Show subtle axis hash marks on canvas when ortho active.

---

## 8. Grid Snap (F9) and Osnap

### Grid Snap (F9)

| | |
|-|-|
| **Toggle** | F9, status bar Grid Snap pane, alias **S**, `GridSnap` command |
| **Grid visibility** | **F7** (separate — grid can be hidden while snap stays on) |
| **Spacing** | Document Properties → Grid; **SnapSize** command |

- Snaps to **grid intersections only** (not along lines between intersections).
- **Overridden by Osnap** and by direct coordinate entry; partially by distance/angle constraints.
- **Alt** hold suspends Osnap to reach grid near geometry.

### Osnap

| Mode | Behavior |
|------|----------|
| **Persistent** | Checkboxes in Osnap panel (status bar); apply every pick |
| **One-shot** | **Shift+click** checkbox while command asks for point; overrides persistent for **one pick** |
| **Menu one-shot** | Tools → Object Snap → specific snap (one pick) |
| **Disable** | Panel “Disable” suspends all persistent; Alt inverts while picking |
| **Osnap off** | **Shift** during pick temporarily enables; **Ctrl** (Mac) for one-shot panel |

**Feedback:**

- Marker **jumps** to snap location within **snap radius** (pixels, not world units — Options → Modeling Aids).
- **Cursor tooltip** labels active snap (`End`, `Mid`, `Int`, …); optional **Project** prefix when projected to CPlane.
- **SmartTrack** lines/points extend osnap with inference geometry.
- Occluded snap: off by default (`SnapToOccluded`); locked objects need **SnapToLocked**.

### (a–d) Summary triggers

- **F9** grid vs **Osnap panel** object snaps — independent toggles.
- Alt suspends osnap; Shift enables one-shot from panel.

### (e) Polish

- “Stronger” snaps (End, Point) beat Near when competing.
- Grid+object combo requires workarounds (AlongLine + Int) — by design since v1.
- Hold **0.25 s** on LMB near snap before drag to **drag from snap point**.

### (f) Recommendation

- **Grid snap** → canvas **world grid** / alignment grid for board frames (F9).
- **Osnap** → node snap to **frame corners, edge midpoints, link anchors, path endpoints**; pixel radius setting.
- Tooltip + jump marker are mandatory feedback.
- One-shot snap chord: **Shift+click snap mode** in bottom aid bar.
- Alt = suspend snap (matches Rhino).

---

## 9. Selection

### Click & modifier selection

| Gesture | Result |
|---------|--------|
| Click object | Select (replaces selection unless Shift) |
| **Shift+click** | Add to selection |
| **Ctrl+click** | Remove from selection |
| Click empty | Deselect (Rhino 7); **Rhino 8:** Ctrl+click empty no longer clears — use **Esc** or **Alt+click** to clear all |
| **Ctrl+Shift+click/window** | Sub-object / in-group pick |

### Window vs crossing marquee

| Drag direction | Mode | Selects |
|----------------|------|---------|
| **Left → right** | **Window** | Wholly enclosed objects |
| **Right → left** | **Crossing** | Enclosed **or touched** |

- **Shift+marquee** adds; **Ctrl+marquee** removes.
- **Alt+marquee** forces **window** mode regardless of direction.
- During marquee, type **`W` + Enter** or **`C` + Enter** to force window/crossing (default aliases).

### Selection menu (depth pick)

When stacked objects ambiguous:

- Pop-up list at cursor; **wheel** cycles; **click** cycles at pick point.
- **RMB** accepts highlighted entry.
- **None** / click away cancels.

### (e) Polish

- Window vs crossing: **different rectangle colors** (advanced color tokens).
- Ctrl+click deselect on **front object** when stacked shaded — deselects rear wireframe edge case documented.
- **SelWindow** / **SelCrossing** commands for scripted selection.

### (f) Recommendation

- Keep **L→R window / R→L crossing** — industry standard Rhino users expect.
- Color-code marquee (window solid blue, crossing dashed green).
- Selection menu for overlapping thumbnails/cards on dense boards.
- Document **Esc = clear selection** prominently (Rhino 8 Alt clear is optional secondary).
- Shift/Ctrl additive/subtractive on marquee and click.

---

## 10. Properties panel (F3)

### (a) Trigger

- **F3** — toggle Properties panel (`Properties` command).
- Edit → Object Properties; panel tab when docked.

### (b) Immediate feedback

**Context-sensitive:**

- **Object(s) selected** → object properties (type-specific sections: Material, Curve Piping, Text, …).
- **Nothing selected** → **viewport properties** (camera, display mode, focal blur, …).
- Multi-select → intersection of applicable properties; mixed-type selection shows common fields.

### (c) Modifiers

- Panel persists; selection changes retarget content live.

### (d) Commit / cancel

- Most property edits apply immediately (some via Enter in numeric fields).

### (e) Polish

- **MatchProperties** command to pick source object.
- User’s Guide uses Properties for validation (closed solid, trimmed surface state).

### (f) Recommendation

- **F3** shared chrome panel: selection → node/tags/geometry; empty → **viewport/camera** (zoom, grid, snap toggles).
- Mirror Rhino’s “no selection = viewport mode” to reduce panel clutter.
- Type-specific sections for board nodes vs atlas file items.

---

## 11. Viewport navigation conventions

Rhino uses **view-type-dependent RMB semantics** (same button, different meaning):

### Perspective viewport

| Gesture | Action |
|---------|--------|
| **RMB drag** | Rotate / tumble |
| **Shift + RMB drag** | Pan |
| **Ctrl + RMB drag** (or wheel) | Zoom |
| **Wheel** | Zoom |
| **Shift** during rotate | Constrain rotation horizontal/vertical |

### Parallel viewports (Top, Front, …)

| Gesture | Action |
|---------|--------|
| **RMB drag** | Pan |
| **Ctrl + Shift + RMB drag** | Rotate view |
| **Ctrl + RMB drag** / wheel | Zoom |

### Other

- **Home / End** — step back/forward through view history (separate from model Undo).
- View zoom/pan via wheel **does not** pollute command repeat; explicit Zoom command does.
- Navigation remapping is **limited** — MMB “Manipulate view” option moves some behaviors; full Maya-style remaps not supported.

### (f) Recommendation for 2D infinite canvas

Atlas/Slate are **2D parallel** viewports:

| Gesture | Maps to |
|---------|---------|
| **RMB drag** | **Pan** (not rotate) |
| **Shift + RMB drag** | Optional alternate (rotate mini-map / tilt for 3D board viewport only) |
| **Ctrl + RMB drag** | Zoom (vertical drag) |
| **Wheel** | Zoom at cursor |
| **Do not** bind RMB click to Enter on canvas — reserve for pan chord start or context menu |

- Keep **view history** (Home/End analogue) separate from document undo.
- Slate 3D board viewport may adopt Perspective rules; default 2D tab uses parallel rules.

---

## Cross-cutting synthesis for Atlas keymap design

### Core loop to preserve

```
idle → (command | selection) → prompt phases → commit → idle → Enter/Space repeats
         ↑ Esc: cancel + deselect + clear prompt
```

### Hard-won Rhino lessons (avoid blind copy)

1. **RMB = Enter** causes chronic accidents with navigation — **split these** on touch/tablet-friendly canvas.
2. **Never-repeat list** is essential hygiene — ship sensible defaults, expose in Advanced.
3. **Esc is aggressive** — one key clears command, selection, and points; users love the certainty.
4. **ShowSelected invert** and **Lock still snaps** are high-value “pro” behaviors worth copying.
5. **Context-sensitive Properties** reduces panel noise.
6. **Marquee direction semantics** are muscle memory — do not invert without Alt/W/C overrides.

### Suggested default never-repeat entries (Atlas starter list)

```
undo, redo, delete, zoom, zoom_extents, fit_view, deselect_all,
clear_selection, properties_toggle, command_history
```

### Command registry fields implied by Rhino

| Field | Rhino analogue |
|-------|----------------|
| `repeat_policy` | Never-repeat list |
| `phases` | Prompt strings + option sets |
| `accept_keys` | Enter, Space (not RMB on canvas) |
| `cancel_behavior` | Esc full reset |
| `pre_select` | Select before command |
| `nestable` | Apostrophe commands in macros |
| `history_line` | Command history log entry |

---

## Primary sources

- [Keyboard and command-line modifiers (Rhino 8)](https://docs.mcneel.com/rhino/8/help/en-us/user_interface/keyboard%20modifiers.htm)
- [Object snaps (Rhino 8)](https://docs.mcneel.com/rhino/8/help/en-us/user_interface/object_snaps.htm)
- [Selection commands (Rhino 8)](https://docs.mcneel.com/rhino/8/help/en-us/commands/selection_commands.htm)
- [Hide / Lock / Group / Join / Trim / Ortho / Snap / Properties](https://docs.mcneel.com/rhino/8/help/en-us/) — respective command help pages
- [Mouse options (Rhino 8)](http://docs.mcneel.com/rhino/8/help/en-us/options/mouse.htm) — RMB context menu delay
- Rhino User’s Guide & Level 1 Training (McNeel PDFs) — repeat-last, Esc, selection, F-keys
- McNeel Discourse — never-repeat, RMB repeat, ortho/osnap interaction, Rhino 8 selection changes

---

*End of Stage-2 research input.*
