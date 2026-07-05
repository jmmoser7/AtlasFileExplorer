# Slate — UI architecture

Slate mirrors File Atlas's shell exactly (same `atlas-shell` chrome) with a
different workspace model: instead of a filesystem root per tab, each tab owns
one **workbook** (`.slate` document — links to files plus a faceted tag
system, never file copies).

## Layer 0 — Top chrome (`ui/tabs.rs`)

Browser-style workbook tabs. All painting comes from `atlas_shell::tabs`;
this module only adapts `SlateTab` state to `TabSpec`s and applies actions.

## Layer 1 — Tab workspace

| Region | Module | Role |
|--------|--------|------|
| Left tools rail | `ui/tools.rs` | **Tags** (hierarchical group editor), **Display** (Grid/Venn, cell size, theme), **Workbook** (open/save, add files, Atlas link) |
| Canvas | `canvas.rs` | Grid + Venn presentations, selection, right-click tag assignment |
| Bottom readouts | `ui/readouts.rs` | Item/tag counts, link health, zoom |
| Advanced | `ui/advanced.rs` | Floating window (workbook info, commands reference) |
| Commands | `commands.rs` | Canonical bindings; see `COMMANDS.md` |

## The tag model (`slate-doc`)

- A workbook holds **tag groups** (facets). Tags **within** a group are
  mutually exclusive on a file (Big/Medium/Small); tags **across** groups
  combine freely (Big + Red). `SlateItem.assignments: BTreeMap<GroupId, TagId>`
  enforces this structurally.
- Items with no assignments are **uncategorized**: they render in a separate
  tray and never appear inside Venn circles.
- `SlateDoc::combination_buckets` drives both presentations: grid sections are
  tag-combination buckets; Venn regions are subsets of focused tags.

## Presentations

- **Grid** (`canvas.rs::grid_layout`) — sections per tag combination,
  uncategorized last.
- **Venn** (`canvas.rs::venn_layout_now`) — literal circles per focused tag
  (`crates/circle-pack::venn_layout`); thumbnails render as circle-cropped
  textured meshes packed inside their set circles, shared files sit in the
  lens overlaps. Tag focus is toggled from the Tags panel.
- New presentations should follow the same pattern: pure geometry in a crate,
  a `*_layout` builder producing `Placed` items, painting + hit-testing on the
  shared camera.

## Linked Atlas sessions (`session.rs`)

"Open File Atlas" hosts Atlas as a second **viewport of the Slate process**
(`show_viewport_immediate`), bridged by `crates/atlas-session`:

1. Slate publishes the active workbook's tag groups + its window rect.
2. Atlas shows those tags in its right-click menu (multi-assign per menu
   instance) and supports click-hold-drag of thumbnails toward Slate.
3. Slate drains the assignment inbox into the document each frame and resolves
   released drags by screen-point-in-window; untagged drops arrive
   uncategorized.

Thumbnails are never re-extracted: `SessionFile.cache_key` reuses Atlas's
thumbnail cache (`atlas_core::thumbs`), which both apps read.

## Workbook lifecycle invariants

1. `tabs` is never empty; `active_tab` always in bounds.
2. Every document mutation goes through `SlateApp::doc_mut()` (sets `dirty`).
3. Dirty tabs refuse to close (toast, no data loss).
4. `selection` only holds live `ItemId`s; tag/group removal strips
   assignments inside `slate-doc`, and dead ids are dropped on use.
5. Saves are atomic (temp file + rename in `slate-doc`).

`tests.rs` drives the real frame loop headlessly over these invariants.
