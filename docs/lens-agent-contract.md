# Lens agent contract — `graph.json` and `overlay.json`

This document is the file contract between Slate's **Lens** view and Cursor
agents (or any tool that can read and write JSON in the shared AI workspace).
Slate supplies deterministic ground truth; agents supply semantic labels.

## 1. Purpose — two-layer engine

The Lens renders a codebase as an interactive dependency graph with two layers:

1. **Deterministic graph (Layer 1)** — built by `code-lens` from Cargo
   workspace membership, package dependencies, module trees, `use` edges,
   trait-impl edges, and LOC metrics. This layer never hallucinates; it is
   always derived from static analysis of the code root.
2. **Semantic overlay (Layer 2)** — cluster names, one- or two-sentence
   summaries, and RGB tint colors written by an agent. Slate watches
   `overlay.json` and tints matching nodes in the graph.

Slate does not embed an LLM. The overlay is a plain JSON file that agents
maintain alongside the graph, reusing the same live-link philosophy as
`atlas-ai` context beacons.

## 2. Where files live and when Slate reads/writes them

Given an AI workspace directory `<ai-workspace>` (configured in the AI panel):

| File | Path | Writer | Reader |
|------|------|--------|--------|
| Graph beacon | `<ai-workspace>/.atlas-ai/lens/graph.json` | Slate (`LensBeacon::tick_write`) | Agents |
| Semantic overlay | `<ai-workspace>/.atlas-ai/lens/overlay.json` | Agents | Slate (`LensBeacon::tick_read`) |
| Agent readme | `<ai-workspace>/.atlas-ai/lens/README.md` | Slate (once, on first graph write) | Humans / agents |

**When Slate writes `graph.json`:** after Lens analysis completes and an AI
workspace is configured, Slate calls `tick_write` each frame. Writes are
throttled to at most once per second and only when the graph fingerprint
(content hash ignoring `generated_at`) changed since the last successful
write. Writes are atomic (temp file + rename).

**When Slate reads `overlay.json`:** each frame, `tick_read` polls at most
once per second. It returns a new overlay only when the file appears or its
modification time changes since the last successful load.

## 3. `graph.json` schema

The file is pretty-printed JSON with a metadata wrapper around a `CodeGraph`:

```json
{
  "app": "slate",
  "source_root": "/absolute/path/to/cargo/root",
  "generated_at": 1710000000,
  "graph": {
    "root": 0,
    "nodes": [ /* LensNode[] */ ],
    "edges": [ /* LensEdge[] */ ],
    "generated_at": 1710000000
  }
}
```

### Wrapper fields

| Field | Type | Meaning |
|-------|------|---------|
| `app` | string | Always `"slate"` for Lens beacons |
| `source_root` | string | Absolute path to the analyzed Cargo root |
| `generated_at` | u64 | Unix seconds when the beacon was written |
| `graph` | object | The [`CodeGraph`] payload (see below) |

### `CodeGraph` fields

| Field | Type | Meaning |
|-------|------|---------|
| `root` | u32 | Node id of the workspace root node |
| `nodes` | array | All nodes; index equals `id` |
| `edges` | array | Cross-link edges only (containment is via `parent`/`children`) |
| `generated_at` | u64 | Unix seconds when analysis finished |

### `LensNode` fields

| Field | Type | Meaning |
|-------|------|---------|
| `id` | u32 | Stable index into `nodes` |
| `parent` | u32 or null | Containment parent |
| `kind` | object | Node kind tag (see below) |
| `name` | string | Display name (`atlas-core`, `tree.rs`, `Tree`, …) |
| `path` | string | Path relative to `source_root` (forward slashes) |
| `loc` | u32 | Non-empty lines (containers roll up children) |
| `children` | u32[] | Direct child node ids |

### Node kinds (`kind` tag)

| Tag | JSON shape | Meaning |
|-----|------------|---------|
| `workspace` | `{ "kind": "workspace" }` | Cargo workspace root |
| `package` | `{ "kind": "package", "is_app": bool }` | Workspace member crate; `is_app` when under `apps/` or has `main.rs` |
| `module` | `{ "kind": "module" }` | Directory-level module under `src/` |
| `file` | `{ "kind": "file" }` | One `.rs` source file |
| `item` | `{ "kind": "item", "item": "<item_kind>" }` | Top-level item inside a file |

Item kinds: `struct`, `enum`, `trait`, `function`, `impl`, `type_alias`,
`const`, `static`, `macro`.

### Edge kinds (`kind` field on `LensEdge`)

| Kind | Family | Meaning |
|------|--------|---------|
| `package_dep` | package | Intra-workspace Cargo dependency between packages |
| `use` | dataflow | `use` / import relationship (aggregated weight) |
| `impl_trait` | OO | `impl Trait for Type` link to the trait's defining node |

Each edge: `{ "from": <id>, "to": <id>, "kind": "...", "weight": <u32> }`.

### Minimal example snippet

```json
{
  "app": "slate",
  "source_root": "/home/dev/atlas",
  "generated_at": 1710000000,
  "graph": {
    "root": 0,
    "generated_at": 1710000000,
    "nodes": [
      {
        "id": 0,
        "parent": null,
        "kind": { "kind": "workspace" },
        "name": "atlas",
        "path": "",
        "loc": 120000,
        "children": [1]
      },
      {
        "id": 1,
        "parent": 0,
        "kind": { "kind": "package", "is_app": false },
        "name": "atlas-core",
        "path": "crates/atlas-core",
        "loc": 45000,
        "children": []
      }
    ],
    "edges": []
  }
}
```

## 4. `overlay.json` schema

```json
{
  "generated_at": 1710000100,
  "clusters": [
    {
      "id": "chrome",
      "title": "Shared chrome",
      "summary": "Tab strip, sidebar, theme, and command reference shared by Atlas and Slate.",
      "color": [120, 160, 220],
      "members": ["crate:atlas-shell", "crates/atlas-shell"]
    }
  ]
}
```

### `LensOverlay` fields

| Field | Type | Default | Meaning |
|-------|------|---------|---------|
| `clusters` | array | `[]` | Ordered list of semantic clusters |
| `generated_at` | u64 | `0` | Unix seconds when the overlay was written |

### `OverlayCluster` fields

| Field | Type | Default | Meaning |
|-------|------|---------|---------|
| `id` | string | required | Stable machine id (snake_case) |
| `title` | string | required | Short human label shown in the Lens legend |
| `summary` | string | `""` | One or two sentences for tooltips |
| `color` | `[u8; 3]` or null | null | RGB tint for cluster members |
| `members` | string[] | `[]` | Selectors (see below) |

Unknown fields are ignored. Clusters with empty `members` are kept but match
nothing.

### Selector semantics

Each `members` entry is either:

1. **`crate:<package-name>`** — matches the workspace `Package` node whose
   `name` equals `<package-name>` (from `[package] name` in Cargo.toml) and
   **all of its descendants**.
2. **Root-relative path prefix** — e.g. `crates/atlas-shell` or
   `crates/atlas-shell/src/theme.rs`. Matches any node whose `path` equals
   the prefix or starts with the prefix followed by `/`.

**Ancestor matching:** a node belongs to a cluster if **any** selector matches
the node itself **or any ancestor's** `path` (walking up the containment tree).
For `crate:` selectors, descendant matching is implicit via the package node.

**Deepest-selector-wins:** when multiple selectors match the same node, Slate
picks the match with the longest effective path prefix. A `crate:<name>`
selector normalizes to that package node's `path` for length comparison. When
two selectors tie on depth, the cluster that appears **earlier** in the
`clusters` array wins.

**No match:** nodes with no matching selector render with default graph colors.

## 5. Recipe for a Cursor agent

1. **Locate the beacon** — read `<ai-workspace>/.atlas-ai/lens/graph.json`
   (path comes from the user's AI workspace setting in Slate).
2. **Parse the graph** — load `graph.nodes` and `graph.edges`. Group packages
   and modules by responsibility (shared infrastructure, apps, geometry, AI
   bridge, etc.).
3. **Author clusters** — for each group, assign:
   - a stable `id` and short `title`;
   - a 1–2 sentence `summary` grounded in what the code actually does;
   - a distinct readable `color` as `[r, g, b]` bytes;
   - `members` selectors using `crate:<name>` and/or path prefixes that cover
     the group's nodes (prefer specific path prefixes when sub-clustering
     within a crate).
4. **Write atomically** — serialize pretty JSON to
   `<ai-workspace>/.atlas-ai/lens/overlay.json.tmp`, then rename to
   `overlay.json`. Set `generated_at` to the current Unix time.
5. **Iterate** — when `graph.json` changes (new `generated_at` or fingerprint),
   re-read and update overlays. Slate picks up changes within ~1 second.

A curated reference overlay for this repository ships at
`docs/lens/example-overlay-atlas.json`.

## 6. Forward compatibility

This JSON contract is the stable API surface. A future MCP server for Lens
will wrap the same read/write operations; agents that implement the file
contract today will work unchanged when MCP arrives.

[`CodeGraph`]: ../crates/code-lens/src/model.rs
