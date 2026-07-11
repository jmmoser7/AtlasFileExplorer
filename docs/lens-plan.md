# The Lens — Slate's fourth view: a codebase-as-graph window

Status: implementation plan + frozen contracts. Written by the orchestrating
architect; implemented by a swarm of Composer agents. Do not change the
FROZEN contracts below without recording the deviation in your final report.

## 1. What we are building

A fourth Slate view mode, **Lens** (`ViewKind::Lens`), alongside Grid, Venn
and Board. A workbook tab points at a **code root** (a directory holding a
Cargo workspace or crate). Slate analyzes it deterministically and renders an
interactive dependency graph with semantic zoom — Grasshopper-style nodes and
wires over the existing Slate camera/canvas substrate.

Design principles (from the "Second Lens" research):

1. **Two-layer engine.** Layer 1 is deterministic static analysis (never
   hallucinates): Cargo workspace membership, crate dependency edges, module
   trees, `use` edges, trait-impl edges, LOC metrics. Layer 2 is an
   *LLM semantic overlay* — but we do not embed an LLM. The overlay is a
   **file contract**: Slate writes `graph.json` (ground truth) into the
   shared AI workspace, and Cursor agents write back `overlay.json`
   (subsystem names, summaries, cluster colors). Slate watches and renders
   the overlay on top of the deterministic graph. This reuses the existing
   `atlas-ai` live-link philosophy and is the Stage-2 "graph as agent context
   provider" from the research, done with files instead of MCP (MCP can bolt
   on later — the JSON contract is the API).
2. **Read-only lens first; evaluation-first UI.** Focus/neighbor
   highlighting, semantic zoom (workspace → crates → modules/files → items),
   degree-of-interest dimming, edge-kind filtering. No write-back to code in
   this iteration.
3. **Paradigm-aware visual grammar.** OO-family relations (trait
   implementations) render differently from dataflow-family relations
   (imports/uses) and coarse package dependencies.
4. **Maximally useful for THIS repo out of the box.** Auto-detects a Cargo
   workspace, splits `apps/` from `crates/`, layers packages by dependency
   depth (foundations left, apps right). A curated example overlay for the
   Atlas ecosystem repo ships in `docs/lens/`.

## 2. Repo integration constraints (read these, they are law)

- Shared chrome lives in `crates/atlas-shell`; the Lens is **canvas content**
  (like Grid/Venn/Board), so it is painted inside `apps/slate` — but all
  colors must derive from `atlas_shell::theme::Palette` fields (blending is
  fine, new hardcoded hex is not, mimic how `canvas.rs`/`board.rs` derive
  colors).
- Every user-facing binding goes into `apps/slate/src/app/commands.rs`
  `ENTRIES` and `COMMANDS.md`.
- New geometry lives in a **pure, UI-free crate** (pattern: `circle-pack`,
  `atlas-core::tree`): no egui deps, own `Rectf`/point types, unit tests
  in-file.
- Workspace dependency versions are pinned once in the root `Cargo.toml`
  `[workspace.dependencies]`; member crates use `{ workspace = true }`.
- Doc-model changes must be serde-backward-compatible (`#[serde(default)]`,
  `ViewKind::Unknown` + `normalized()` pattern).
- Analysis must never block paint: background thread + `crossbeam-channel`,
  drained once per frame (pattern: thumbs/previews in `apps/slate`).
- Linux CI reality: `cargo check/test --workspace`, `cargo fmt --all`,
  `cargo clippy --workspace --all-targets` must pass on Linux.

## 3. New crate: `crates/code-lens`

UI-free analysis backend. Layout:

```
crates/code-lens/
  Cargo.toml            # deps: serde, serde_json, toml, syn(full,visit), proc-macro2(span-locations)
  src/lib.rs            # mod decls + re-exports (FROZEN in wave 0 — do not edit later)
  src/model.rs          # FROZEN graph data model
  src/extract/mod.rs    # analyze_workspace entry
  src/extract/cargo.rs  # workspace membership + package dep edges (toml)
  src/extract/modules.rs# file/module tree from src/ walk
  src/extract/rust_src.rs # syn parse: items, use edges, impl-trait edges, LOC
  src/layout.rs         # semantic-zoom containment layout + edge rollup
  src/overlay.rs        # LensOverlay types + read/match
  src/beacon.rs         # graph.json writer + overlay poller (throttled)
  tests/fixtures/mini-ws/  # tiny committed cargo workspace used by unit tests
```

### 3.1 FROZEN — `model.rs`

```rust
pub type NodeId = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind { Struct, Enum, Trait, Function, Impl, TypeAlias, Const, Static, Macro }

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum NodeKind {
    Workspace,
    Package { is_app: bool },
    Module,          // directory-level module
    File,            // one .rs file (the module granularity for leaves)
    Item { item: ItemKind },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    PackageDep,   // intra-workspace Cargo dependency
    Use,          // use/import (dataflow family)
    ImplTrait,    // `impl Trait for Type` (OO family)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LensNode {
    pub id: NodeId,
    pub parent: Option<NodeId>,
    pub kind: NodeKind,
    pub name: String,          // display name ("atlas-core", "tree.rs", "Tree")
    pub path: std::path::PathBuf, // path relative to the analyzed root
    pub loc: u32,              // non-empty lines; containers = rollup of children
    pub children: Vec<NodeId>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LensEdge {
    pub from: NodeId,
    pub to: NodeId,
    pub kind: EdgeKind,
    pub weight: u32,           // aggregated count (e.g. number of use statements)
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct CodeGraph {
    pub root: NodeId,               // the Workspace node (0 when non-empty)
    pub nodes: Vec<LensNode>,       // index == NodeId
    pub edges: Vec<LensEdge>,       // cross-links only; containment via parent/children
    pub generated_at: u64,          // unix seconds
}

impl CodeGraph {
    pub fn node(&self, id: NodeId) -> &LensNode;
    pub fn is_empty(&self) -> bool;
    /// Direct cross-link neighbors (both directions), with edge kind + weight.
    pub fn neighbors(&self, id: NodeId) -> Vec<(NodeId, EdgeKind, u32)>;
    /// Walk up parents until `pred` holds; used for edge rollup.
    pub fn ancestor_where(&self, id: NodeId, pred: impl Fn(NodeId) -> bool) -> Option<NodeId>;
    /// Stable content fingerprint (ignores generated_at).
    pub fn fingerprint(&self) -> u64;
}

#[derive(Debug)]
pub enum LensError { NotACodeRoot(std::path::PathBuf), Io(std::io::Error) }
impl std::fmt::Display for LensError { /* human-readable */ }
```

### 3.2 FROZEN — extraction entry (`extract/mod.rs`)

```rust
/// Analyze `root` (dir containing Cargo.toml — workspace or single crate).
/// Deterministic; no panics on malformed source (skip + continue).
pub fn analyze_workspace(root: &std::path::Path) -> Result<crate::model::CodeGraph, crate::model::LensError>;
```

Extraction rules:

- **Packages**: root `Cargo.toml` `[workspace] members` (expand simple globs
  like `crates/*`); single-crate roots yield one Package under the Workspace
  node. `is_app` = package path is under an `apps/` directory OR the package
  has `src/main.rs`. Package `name` = the `[package] name` from its
  Cargo.toml.
- **PackageDep edges**: from each member's `[dependencies]`,
  `[dev-dependencies]`, `[build-dependencies]` — keep only deps whose name
  matches another workspace member. Weight 1.
- **Module tree**: walk each package's `src/`; every directory = `Module`
  node, every `.rs` file = `File` node (`mod.rs`/`lib.rs`/`main.rs` attach to
  their directory/package level sensibly — don't create a child `File` named
  `mod.rs` whose parent is the same module twice; simplest: every `.rs` file
  is a File node under its directory's Module node, and that is fine).
- **Items**: `syn::parse_file` each file; top-level items become `Item`
  nodes (struct/enum/trait/fn/impl/type/const/static/macro). Item `loc` from
  proc-macro2 span line ranges (`span-locations` feature). Nested inline
  `mod x {}` items may be flattened into the file or skipped — document the
  choice. Files that fail to parse: keep the File node (loc from line count),
  skip items, never fail the whole analysis.
- **Use edges**: for each `use` statement in a file, resolve the first path
  segment: `crate`/`self`/`super` → best-effort intra-package target
  (file/module if resolvable, else the package node); a workspace package
  name (hyphens ↔ underscores) → best-effort node inside that package (else
  the package node). External crates: ignore. Edge is File → target,
  duplicates aggregated into `weight`.
- **ImplTrait edges**: for `impl Trait for Type`, emit File → the workspace
  node defining `Trait` when it can be found by name (build a per-package
  symbol table of trait names in a first pass; resolve via the file's `use`
  imports, else same-package name match; if unresolvable, skip). Approximate
  resolution is acceptable and must be documented in code comments.
- **LOC**: count non-empty lines per file; containers roll up.

### 3.3 FROZEN — layout (`layout.rs`)

Pure geometry, no egui. World units are f32 "points".

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rectf { pub x: f32, pub y: f32, pub w: f32, pub h: f32 }

#[derive(Debug, Clone)]
pub struct PlacedNode {
    pub id: crate::model::NodeId,
    pub rect: Rectf,             // world-space; containers enclose children
    pub collapsed: bool,         // true when drawn as a chip (children hidden)
    pub depth: u8,               // 0 = workspace, 1 = package, ...
}

#[derive(Debug, Clone)]
pub struct LensWire {
    pub from: crate::model::NodeId,
    pub to: crate::model::NodeId,
    pub kind: crate::model::EdgeKind,
    pub weight: u32,
    pub from_pt: (f32, f32),     // attachment on `from` rect edge
    pub to_pt: (f32, f32),
}

#[derive(Debug, Clone, Default)]
pub struct LensLayout {
    pub placed: Vec<PlacedNode>, // paint order: parents before children
    pub wires: Vec<LensWire>,
    pub bounds: Rectf,
}

/// `expanded`: nodes whose children are shown. A node is visible when every
/// ancestor is in `expanded`. Edges roll up to the deepest visible ancestor
/// on each side; (from,to,kind) duplicates merge summing weight; self-loops
/// after rollup are dropped.
pub fn layout_graph(
    graph: &crate::model::CodeGraph,
    expanded: &std::collections::HashSet<crate::model::NodeId>,
) -> LensLayout;
```

Layout algorithm (deterministic):

- **Top level**: packages arranged in **dependency layers** — longest-path
  layering over `PackageDep` edges; layer 0 (no intra-workspace deps) is the
  leftmost column, apps end up rightmost. Within a column, sort `is_app`
  last, then by name. Fixed column gap and row gap.
- **Inside a container**: children laid out in a wrapped grid (rows), sorted
  by kind order (Module, File, Item) then name; container sized to fit
  children + a header strip (reserve ~28 world units of header height) +
  padding. Collapsed containers and leaves get a chip whose width scales
  gently with `log2(loc)` (clamped) so size encodes LOC.
- Wire attachment points: horizontal — right edge of `from`, left edge of
  `to` when `to` is to the right, else the nearer vertical edges. Endpoints
  spread along the edge so parallel wires don't coincide.

### 3.4 FROZEN — overlay + beacon (`overlay.rs`, `beacon.rs`)

```rust
// overlay.rs
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct LensOverlay {
    #[serde(default)] pub clusters: Vec<OverlayCluster>,
    #[serde(default)] pub generated_at: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OverlayCluster {
    pub id: String,
    pub title: String,               // e.g. "Shared chrome"
    #[serde(default)] pub summary: String,
    #[serde(default)] pub color: Option<[u8; 3]>,
    /// Selectors: "crate:<package-name>" or a root-relative path prefix
    /// ("crates/atlas-shell/src/theme.rs" or "crates/atlas-shell").
    #[serde(default)] pub members: Vec<String>,
}

/// <ai_workspace>/.atlas-ai/lens
pub fn lens_dir(ai_workspace: &std::path::Path) -> std::path::PathBuf;
pub fn read_overlay(ai_workspace: &std::path::Path) -> Option<LensOverlay>;
/// Deepest-selector-wins match of a node against overlay clusters.
pub fn match_cluster<'a>(
    overlay: &'a LensOverlay,
    graph: &crate::model::CodeGraph,
    node: crate::model::NodeId,
) -> Option<&'a OverlayCluster>;

// beacon.rs
#[derive(Debug, Default)]
pub struct LensBeacon { /* private throttle + fingerprint + overlay mtime state */ }
impl LensBeacon {
    pub fn new() -> Self;
    /// Throttled (>=1s, fingerprint-gated) atomic write of
    /// <ai_workspace>/.atlas-ai/lens/graph.json. Safe to call every frame.
    /// Returns true when a write happened.
    pub fn tick_write(
        &mut self,
        ai_workspace: &std::path::Path,
        source_root: &std::path::Path,
        graph: &crate::model::CodeGraph,
    ) -> bool;
    /// Polls overlay.json mtime (>=1s). Returns Some only when the file
    /// (re)appeared or changed since last successful load.
    pub fn tick_read(&mut self, ai_workspace: &std::path::Path) -> Option<LensOverlay>;
}
```

`graph.json` schema = serde of `CodeGraph` wrapped with metadata:

```json
{ "app": "slate", "source_root": "/abs/path", "generated_at": 1234567890,
  "graph": { "root": 0, "nodes": [...], "edges": [...] } }
```

Atomic writes: tmp file + rename (pattern: `atlas-ai/src/context.rs`).

## 4. Slate app integration (`apps/slate`)

- `crates/slate-doc/src/view.rs`: add `Lens` variant (serde `"lens"`),
  passthrough in `normalized()`. Older builds degrade to Grid via the
  existing `Unknown` path — acceptable.
- `crates/slate-doc/src/doc.rs`: `#[serde(default)] pub lens_root:
  Option<PathBuf>` on `SlateDoc` (v2 files without it load fine).
- New module `apps/slate/src/app/lens.rs`: owns `LensState` (runtime, on
  `SlateApp`):
  - background analysis worker: `crossbeam-channel` request/response,
    spawned thread runs `code_lens::analyze_workspace`; state machine
    Idle → Analyzing → Ready(CodeGraph)/Error(String); drained once per
    frame (`lens_pump`), repaint requested while Analyzing.
  - `expanded: HashSet<NodeId>` (default: workspace + all packages expanded
    → module level visible), `focus: Option<NodeId>`, cached
    `LensLayout` recomputed only when graph/expanded change,
    edge-kind filter flags, search string, `LensBeacon`, latest
    `LensOverlay`.
- `canvas.rs`: dispatch arm `ViewKind::Lens` → `lens_canvas(ui, rect)` in
  `lens.rs`. Reuse `tab.cam`, `world_to_screen`/`screen_to_world`,
  `zoom_at`, `fit_view`, turbo pan, `paint_dot_grid` — identical camera feel
  to Grid/Venn.
- Interactions: click = focus node (dim non-neighbors to ~25% alpha);
  double-click container = expand/collapse; double-click file/item = open in
  editor (`open_path` pattern); Esc = clear focus; `F` = fit view; hover
  tooltip = path, LOC, in/out degree, cluster summary when overlay matches.
- Painting: containers as rounded rects with header strips; chips for
  leaves/collapsed; wires as cubic beziers — `PackageDep` thick/ink, `Use`
  thin/accent, `ImplTrait` dashed/portal; stroke width `1 + log2(weight)`
  clamped; small arrowheads. Colors derived from `Palette` (blend helpers as
  in existing canvas code). Overlay clusters tint container headers and draw
  a title tag.
- Sidebar: new `ToolPanel::Lens` (bump `ChromeConfig<5,2>` → `<6,2>`, extend
  `ALL`/`label`), panel body via `atlas_shell::sidebar_section`: code-root
  picker (rfd folder dialog through the existing picker pattern), Rescan
  button, status line, edge-kind filter checkboxes, search field, overlay
  legend (cluster titles + colors), "expand to depth" quick buttons
  (Packages / Modules / Items).
- Menubar View menu + `display_body` combo gain Lens.
- `commands.rs` `ENTRIES` += Lens category (fit view, focus, expand/collapse,
  open source, clear focus, rescan) and `COMMANDS.md` updated.
- AI link: in `ai_context_frame` (or adjacent), when the Lens has a graph and
  the AI workspace is set: `beacon.tick_write(...)`, `beacon.tick_read(...)`.
  Also include `lens_root` in the existing `AiAppContext.selection`/`files`
  contract only if trivial — otherwise leave `AiAppContext` untouched.

## 5. Agent-communication contract (`docs/lens-agent-contract.md`)

A standalone doc that specifies:

- where `graph.json` lands and its schema (with a trimmed real example);
- the `overlay.json` schema and selector semantics;
- a recipe for a Cursor agent: read `graph.json`, cluster/summarize, write
  `overlay.json` atomically;
- plus a curated example overlay for THIS repository at
  `docs/lens/example-overlay-atlas.json` (clusters: Shared chrome, Document
  model & artifact, Geometry, Core index/scan/thumbs, AI link, Session
  bridge, Atlas app, Slate app) with real summaries drawn from AGENTS.md.

## 6. Work packages (swarm assignments)

| Wave | Agent | Owns (exclusively) |
|------|-------|--------------------|
| 0 | Scaffold | root `Cargo.toml`, `Cargo.lock`, whole `crates/code-lens` skeleton (frozen types + compiling stub fns + fixture dirs), `crates/slate-doc` (ViewKind::Lens, lens_root) |
| 1 | A Extractor | `crates/code-lens/src/extract/**` + its in-file tests + `tests/fixtures/mini-ws/**` |
| 1 | B Layout | `crates/code-lens/src/layout.rs` + in-file tests |
| 1 | C Slate UI | `apps/slate/**` (lens.rs, canvas dispatch, chrome.rs, ui/*, commands.rs, COMMANDS.md, ARCHITECTURE.md, mod.rs additions, Cargo.toml) |
| 1 | D Overlay/Beacon | `crates/code-lens/src/overlay.rs`, `src/beacon.rs` + tests, `docs/lens-agent-contract.md`, `docs/lens/example-overlay-atlas.json` |
| 2 | Integrator | anything needed to make `cargo fmt/clippy/test --workspace` green; AGENTS.md note; final polish |

Rules for wave-1 agents: do NOT edit `src/lib.rs` or `src/model.rs` of
`code-lens`; do NOT touch files owned by another agent. If a frozen contract
is impossible as written, implement the closest working shape and flag it in
your final report — the integrator adjudicates.

## 7. Acceptance criteria

1. `cargo test --workspace` and `cargo clippy --workspace --all-targets`
   pass on Linux; `cargo fmt --all` clean.
2. `code_lens::analyze_workspace` on THIS repo finds all 9+ workspace
   packages, the `slate → slate-doc` PackageDep edge, `apps` flagged
   `is_app`, and item nodes inside `crates/circle-pack` (self-analysis unit
   test guarded to run only when the enclosing workspace exists).
3. In Slate: View → Lens on a fresh workbook shows an empty-state prompt;
   picking this repo's root renders the layered package graph without
   blocking the UI; expand/collapse, focus dimming, fit view, tooltips,
   edge filters work; headless app test (`tests.rs`) drives a Lens frame
   without panicking.
4. With an AI workspace configured, `graph.json` appears under
   `<ws>/.atlas-ai/lens/` within ~1s of analysis completing, and dropping
   the example overlay in as `overlay.json` re-colors/labels clusters within
   ~1s.
