# Tool kits — the `.slatekit` format

A board tool is two things:

- a **gesture grammar** — how pointer input is read. Code. Nine of them. A kit
  references one and can never define one.
- a **result recipe** — what the commit produces. Data. This is what a kit file
  holds.

That split is the whole design. It is why a tool can be authored without a
compiler, and why authoring one cannot introduce a new interaction model,
a new node kind, or anything `slate-artifact` would not know how to export
(Constitution Art. IV, Art. VII.3).

`builtin/core.slatekit` is the worked example: it holds the board's own tool
results and is read through this exact parser, with no privileged fields.

## Where kits live

| Scope | Location | Precedence |
|-------|----------|------------|
| Built-in | compiled in from `builtin/core.slatekit` | lowest |
| User | `data_dir()/tools/*.slatekit` | middle |
| Workbook | supplied with a `.slate` file | highest |

A kit **shadows** a lower-precedence one by reusing a tool's `id`. That is how
editing a built-in tool works: you write a new file, and the shipped one is
never rewritten. The shadowed definition stays visible in the registry so an
authoring interface can show you what you replaced.

`data_dir()` and not the install folder, because the program's own directory is
not writable on a normal install and a tool you authored has to survive an
upgrade.

## A minimal kit

```toml
format_version = 1
id = "mine"
name = "My tools"

[[tool]]
id = "redline"
name = "Redline pen"
grammar = "freehand"
sticky = "sticky"
aliases = ["markup"]

  [tool.recipe]
  kind = "shape"
  node = "path"
  create_style = "pinned"
  stroke = { width = 2.0, color = "#e8443a", cap = "round", join = "round" }
```

Inline tables cannot span lines in TOML, so a recipe with more than a couple of
fields wants the `[tool.recipe]` sub-table form shown above.

## Grammars

| `grammar` | Gesture | Produces nodes |
|-----------|---------|----------------|
| `select` | pick, marquee, handles, grips | no |
| `direct_select` | anchor / handle editing on paths | no |
| `drag_rect` | press-drag-release a box; click places a default size | yes |
| `two_point` | click-move-click with direction lock and typed magnitude | yes |
| `multi_point` | repeated clicks, Enter or double-click finishes | yes |
| `freehand` | a sampled stroke, fitted or variable-width | yes |
| `place_point` | one click places a thing | yes |
| `sweep` | continuous hit-test along a drag | no |
| `sample` | read a property from what is under the cursor | no |

Only the five that produce nodes can back a kit tool; the others have nothing
for a recipe to make. Naming a grammar this build does not implement costs that
one tool and nothing else — the rest of the file still loads.

## Recipes

### `kind = "shape"`

`node` is one of `frame`, `rect`, `ellipse`, `text`, `path`.

| Field | Meaning |
|-------|---------|
| `stroke` | see below; every sub-field defaults |
| `fill` | color reference, or omitted for unfilled |
| `corner` | `"square"`, `{ rounded = { radius = N } }`, `{ chamfer = { cut = N } }` |
| `create_style` | `"inherit"` (adopt the last style you edited) or `"pinned"` |
| `default_size` | `[w, h]` in world units, for a click rather than a drag |
| `text` | `{ text, size, color, align, family, fill }` for `node = "text"` |
| `frame` | `{ title, fill }` for `node = "frame"` |

`node = "path"` is style-only: path geometry comes from the gesture, so the
recipe contributes the stroke and nothing else. A frame title may contain `{n}`,
which becomes the 1-based slide number — a named substitution, not an
expression, and the only one there is.

### `kind = "portal"`

`portal` names an existing portal kind (`repo_lens` today) and the recipe
presets its `title`, `source`, and `query`. Five preset lenses over one
repository are five kit entries and no new code.

Leave `source` unset in a kit you intend to share: a locator is relative-first
(Art. IX.2) and a path from the author's machine is not a gift.

## Strokes

```toml
stroke = { width = 2.0, color = "accent", dash = "solid", cap = "round", join = "miter", profile = "uniform" }
```

Every field defaults, so `stroke = { color = "#e8443a" }` is a complete 2px
solid red stroke. `profile` may also be `{ taper = { start = 0.2, end = 1.0 } }`,
which is SVG-expressible as a filled outline and therefore allowed to be data.

## Colors

`#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, or a theme reference:

- `"accent"` — the palette's accent at its own alpha
- `"accent@60"` — the accent at alpha 60

Prefer the theme references. A kit that hardcoded its accent looks wrong in the
theme its author never had.

## Accelerators

`key` is optional and usually a mistake. Type-to-command already reaches any
tool by name, and bare letters are a scarce resource a kit cannot arbitrate: if
two tools claim one, the lower-precedence tool loses the binding and a finding
says so. Use `aliases` instead, for the names people arriving from other
software will type.

## What a kit may never do

Define a gesture grammar. Introduce a node kind or a style property. Carry
executable content of any kind — no expressions, no scripts, no image files.
Ship a path that only exists on its author's machine.

The load-bearing consequence: a kit tool produces **ordinary scene nodes**. A
missing kit costs you the ability to make more of them and costs the document
nothing, which is what makes installing a stranger's kit a reasonable thing to
do.

## Checking a kit

```
cargo xtask kits            # every .slatekit in the repository
cargo xtask kits <dir>      # a folder — your own kit while you write it
```

The audit uses this crate's parser and resolver, so it cannot disagree with what
the app will do at startup. It runs inside `cargo test --workspace`.
