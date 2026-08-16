# ATLAS ECOSYSTEM — STATUS BUNDLE (self-contained handoff)
# Open the .html file in a browser for the interactive dashboard.
# Everything below is also embedded in that HTML.

# Atlas ecosystem — status dashboard handoff

**Audience:** an agent (or human) building or refining the interactive project
status dashboard. **You do not need the source repository.** Everything required
is in this folder.

| File | Role |
|------|------|
| [`project-state.json`](project-state.json) | Canonical research snapshot (thesis, law, roadmap, workplan, deviations, metrics, crates, next actions) |
| [`index.html`](index.html) | Working interactive dashboard — open in any browser (file:// or static host) |
| **This file** | Build brief, UX plan, acceptance criteria, update protocol |

**Snapshot date:** 2026-08-05 · **HEAD at capture:** `664e2e1`  
**Research verdict:** Wave 0 + ratification (G0) are complete. Wave 1 (convergent
journal) is the critical path and is unblocked. Roadmap Phase 2 core landed;
Phases 3–6 and workplan Waves 1–5 are largely ahead.

---

## 1. What this dashboard must do

Three jobs, one composition:

1. **Visualization** — architecture, products, crates, capability inventory.
2. **Roadmap** — Constitution phases (P1–P6) *and* audit workplan waves (W0–W5 + spikes), with dependency context.
3. **Progress evaluation** — scored phases/waves, open constitutional deviations, metrics baseline vs approx current, “what to do next.”

It is a **status instrument for orchestrators**, not a marketing site and not a
substitute for `CONSTITUTION.md`.

---

## 2. Non-negotiable content (already in JSON)

Do not invent progress. Read `project-state.json` and render it. If you rebuild
the UI, keep these sections:

1. **At a glance** — overall scores, HEAD, open deviations, format_version, gate status (G0 open for Wave 1).
2. **Thesis** — one-liner + 10% rule + horizon.
3. **Products** — File Atlas vs Slate capability lists.
4. **Roadmap phases P1–P6** — status, progress bar, open items.
5. **Workplan waves** — task cards with status; critical-path highlight on W1.
6. **Constitution articles** — I–XI short summaries (filterable).
7. **Decisions D1–D18** — one-line impact each.
8. **Deviations ledger** — open/closed filter; Art. + closes_with.
9. **Metrics** — baseline 2026-07-25 vs approx 2026-08-05; crate LOC bars.
10. **Architecture** — crate roles; missing planned crates; hard rules.
11. **Next actions** — prioritized critical path from `capabilities_inventory.next_critical`.
12. **Out of scope** — explicit cuts so agents do not re-propose them.
13. **Audit trail** — what to bring to audit №3.

---

## 3. UX / visual plan

### Composition

- One scrollable page with a sticky subnav (jump links), not a multi-page app.
- First viewport = **status hero**: product name “Atlas ecosystem”, one verdict
  sentence, overall progress ring/bars, four KPI chips (open DVs, format v,
  tests, crates). No marketing clutter.
- Below: sectioned instrument panels. Prefer dense information design over cards-for-cards’-sake.

### Visual direction (avoid AI-default looks)

- **Direction:** cartographic night board — charcoal `#0e1114`, ink `#d8d4c8`,
  signal teal `#3d9b8f`, caution amber `#c49a6c`, danger coral `#c45c4a`,
  muted grid lines. Not purple-gradient, not cream+terracotta, not broadsheet.
- Typography: distinctive pair — e.g. **IBM Plex Sans** + **IBM Plex Mono**
  (or Fraunces + JetBrains Mono). No Inter/Roboto/Arial/system-only stacks.
- Atmosphere: subtle dotted grid or topo-line background, not flat fill.
- Motion: 2–3 intentional motions only — progress bars fill on enter, phase
  nodes light on hover, filter chips cross-fade content. No glow spam.

### Interaction

- Filter chips: wave status / deviation status / article.
- Click a wave → expand task list with depends/closes badges.
- Click a deviation → show article + closing task.
- Hover crate bar → role tooltip.
- Optional: `?section=` deep link; print stylesheet that collapses filters.

### Technical constraints for the builder

- **Single-file or two-file** preferred (`index.html` + optional JSON). Works on
  `file://` — if fetch fails, embed JSON inline (current `index.html` does this).
- No build step, no React/Vite required unless you deliberately upgrade and
  document how to regenerate the static artifact.
- No network dependency for core function (fonts may CDN with local fallbacks).
- All copy that asserts facts must come from the JSON, not hard-coded prose that
  can drift.

---

## 4. Research conclusions the UI must surface

These are the evaluation findings — highlight them, do not bury them:

| Finding | Detail |
|---------|--------|
| **Critical path is Wave 1** | G0 ratified (`1b88e8e`); Wave 0 merged. Start T1.1a (ZOrder/v3) and T1.2 (atlas-ai split) in parallel. |
| **Journal still positional** | `SceneCmd` is still `Add{index}/Remove{index}/Patch` — DV-01 open. `format_version` still **2**. |
| **Collage half-done** | `crates/collage` shipped; command wiring is T2.2 (DV-05 still open). |
| **Portal groundwork early** | `NodeKind::Portal` + RepoLens exist before S1 taxonomy — risk of wrong assumptions; S1 is high-value. |
| **Agent surface is a seed** | Beacon + Cursor launch exist; no staging layer, no `atlas-mcp`, atlas-ai still renderer-bound (DV-02). |
| **Deviation debt** | **8 open / 4 closed.** DV-12 (dead Selection/Lens docks) opened after clippy sweep. |
| **Metrics baseline stale** | Last `cargo xtask metrics` is 2026-07-25; codebase grew (slate-kit, repo-graph, portal, kits). Re-run before audit №3. |
| **Stale doc trap** | `docs/audit/amendments/2026-07-25-amendments.md` still says UNRATIFIED — ignore it; constitution amendment log wins. |

### Progress scores (from JSON)

- Overall roadmap ≈ **28%**
- Workplan through Wave 2 ≈ **22%**
- P1 100% · P2 ~70% · P3 ~15% · P4 ~5% · P5/P6 0%
- W0 100% · W1 0% · Spikes 0% · W2 ~5%

---

## 5. Acceptance criteria for a “done” dashboard

- [ ] Opens offline; all sections render from embedded/linked state data.
- [ ] Verdict sentence matches research (Wave 1 unblocked / journal not convergent yet).
- [ ] Every open deviation appears; closed ones filterable but present.
- [ ] Every workplan task T0.1–T2.4 + spikes + W3–W5 sketches appears with status.
- [ ] Roadmap P1–P6 and workplan waves are both visible (not collapsed into one vague timeline).
- [ ] Next-actions list matches `next_critical` order.
- [ ] Out-of-scope list is visible.
- [ ] No fabricated ship dates or calendar ETAs (phases are dependency-ordered).
- [ ] Visual direction follows §3 (not purple SaaS / cream editorial / broadsheet).
- [ ] Handoff note in PR: how to refresh JSON after the next wave merges.

---

## 6. How to refresh after the repo moves

When you *do* have repo access later:

1. Re-read `ROADMAP.md`, `docs/workplan/README.md`, `docs/audit/deviations.md`.
2. Probe: `SlateDoc::CURRENT`, `SceneCmd` shape, presence of `order.rs` /
   `source.rs` / `atlas-mcp` / `docs/portal-contract.md`.
3. Run `cargo xtask metrics` and replace `metrics.baseline` or add a new dated
   snapshot column.
4. Update task `status` fields in JSON; recompute `progress_scores`.
5. Bump `meta.generated` / `meta.head_commit`.
6. Regenerate or re-embed JSON into `index.html`.

Without repo access: only edit JSON if a human supplies a new snapshot.

---

## 7. Suggested build modes for the implementing agent

**Mode A (default) — polish the shipped `index.html`.**  
Improve layout, accessibility, print CSS, and micro-interactions. Do not change
facts without updating JSON first.

**Mode B — rebuild in another stack.**  
Allowed if the static artifact remains one (or two) files and still embeds the
full JSON. React/Svelte/etc. must ship a built static result into this folder.

**Mode C — Miro / FigJam board.**  
Only if the user asks. Mirror the same section inventory; attach or paste the
JSON summary so the board is not source-dependent. (Miro MCP may require user
auth.)

---

## 8. Dispatch prompt (copy-paste)

```
You are building / refining the Atlas ecosystem status dashboard.
You do NOT have repository access.

Read in order:
  1. docs/status-dashboard/HANDOFF.md
  2. docs/status-dashboard/project-state.json
  3. docs/status-dashboard/index.html

Execute Mode A unless told otherwise. Preserve all facts from the JSON.
Meet every acceptance criterion in HANDOFF §5. Do not invent calendar
timelines. Open a PR that only touches docs/status-dashboard/ unless
asked to publish elsewhere.
```

---

## 9. Glossary (for agents new to the project)

| Term | Meaning |
|------|---------|
| **File Atlas** | Folder-tree visual organizer app |
| **Slate** | Workbook app (.slate) — tags, board, lens, export |
| **Board** | Authored infinite canvas; frames = slides |
| **Portal** | Journaled frame whose contents come from elsewhere |
| **Journal / SceneCmd** | Invertible command log; only legal mutation path |
| **Capability** | Everything outside the minimal canvas core |
| **Wave** | Swarm workplan batch from 2026-07-25 audits |
| **Phase** | Long-horizon ROADMAP.md sequence |
| **Deviation (DV-)** | Known constitution-vs-code gap |
| **G0** | User ratification of constitutional amendments |
| **Beacon** | JSON files under AI workspace for agent context |
| **10% rule** | Art. III — build the fraction actually used |


---

# project-state.json

```json
{
  "meta": {
    "generated": "2026-08-05",
    "head_commit": "664e2e1",
    "head_summary": "Declarative board tools: gesture grammar (code) + result recipe (data) (#38)",
    "head_date": "2026-08-03",
    "snapshot_note": "Self-contained research snapshot for agents without repo access. Counts marked 'approx' were recomputed 2026-08-05; baseline metrics are from cargo xtask metrics on 2026-07-25 (commit 4d40a33).",
    "repo": "Atlas ecosystem (native File Atlas + Slate)",
    "stack": "Rust + egui/eframe, Windows reference platform, Linux CI floor",
    "license": "MIT"
  },

  "thesis": {
    "one_liner": "One lean native daily driver that replaces a suite of subscription tools for viewing, composing, and eventually editing every kind of document its user works with.",
    "anti_bloat": "Professionals use ~10% of each monolith. Build that fraction excellently; everything else must earn its place (Constitution Art. III).",
    "product_loop": "Thought → action → completion, as tight as the substrate allows — for the human and for agents on the same canvas.",
    "horizon": "Two-plus years, vibe-coded. Invest first in contracts, schemas, and tests — implementations get cheaper to regenerate."
  },

  "products": [
    {
      "id": "file-atlas",
      "name": "File Atlas",
      "binary": "native-file-atlas",
      "path": "apps/file-atlas",
      "role": "Folder scanning, tree canvas, destination assignment / export workflow. Visual organization of tens of thousands of files.",
      "key_capabilities": [
        "Parallel streaming directory scan (~500k files/sec local)",
        "SQLite index + instant revisit",
        "Infinite pan/zoom branching folder tree with LOD (color blocks → cards)",
        "Thumbnails via Win32 shell + built-in PDF/Office/.3dm fallbacks",
        "Shared project cache tier under firm template path",
        "Structure-only map, overnight pre-warm dashboard",
        "Non-destructive staging/export with JSON manifest",
        "Journal undo/redo; activity timeline / date filter",
        "Linked-session tag offer from Slate via atlas-session"
      ]
    },
    {
      "id": "slate",
      "name": "Slate",
      "binary": "slate",
      "path": "apps/slate",
      "role": "Tag-driven workbooks (.slate) that link files (never copies). Grid, Venn, Board, Lens views. Presentation + HTML artifact export.",
      "key_capabilities": [
        "Faceted tag groups (mutex within group, free across)",
        "Grid + literal Venn presentations (circle-pack)",
        "Authored Board: frames-as-slides, shapes, text, images, paths, connectors",
        "Presentation mode + slate-artifact HTML export (serialization, not conversion)",
        "Lens: deterministic cargo/Rust code graph (code-lens) + agent overlay beacon",
        "Rhino .3dm 3D board viewports (rhino-mesh)",
        "Declarative board tools via slate-kit (grammar + recipe)",
        "Workbook lease / read-only mode when another holder is live",
        "Repository Lens portal node groundwork (NodeKind::Portal)",
        "Hosts File Atlas as second viewport in linked sessions"
      ]
    }
  ],

  "constitution": {
    "status": "Founding 2026-07-19; amendments A,B,C,E,F ratified 2026-07-25 (commit 1b88e8e)",
    "note": "docs/audit/amendments/2026-07-25-amendments.md still says UNRATIFIED — that file is stale; CONSTITUTION.md amendment log is authoritative.",
    "articles": [
      {
        "id": "I",
        "title": "Minimal core",
        "summary": "Core = canvas paradigm (camera, tabs, journal, chrome, command registry). All else is a capability. No document/geometry/capability logic may depend on egui or a model provider. Windows reference; pure crates must build on Linux. Local-first; self-hosted relay OK; no required accounts."
      },
      {
        "id": "II",
        "title": "Performance is a feature",
        "summary": "60fps canvas; no per-frame alloc/tessellation in paint; heavy work async + generation-tagged; glanceability matters."
      },
      {
        "id": "III",
        "title": "The 10% rule",
        "summary": "Implement the deliberately chosen fraction of a domain that is actually used. 'Photoshop has it' is not a reason."
      },
      {
        "id": "IV",
        "title": "Honest models",
        "summary": "Exports are serializations; graphs are extracted never hallucinated; board styling ceiling is SVG-expressible; egui painter and artifact writer are two interpreters of one model."
      },
      {
        "id": "V",
        "title": "One universe, portals",
        "summary": "Board is the only universe. Portals = journaled frames with non-journaled contents. Classes: Generated / Document / Host. Membership is derived geometry (topmost containing frame). Tab ViewKinds retire as portal parity is reached."
      },
      {
        "id": "VI",
        "title": "Journal-only mutation",
        "summary": "Every mutation is a named invertible journaled command with author. Address by stable identity; property-scoped. Derived state never journaled; bake promotes it."
      },
      {
        "id": "VII",
        "title": "Command parity (agent-native)",
        "summary": "Humans and agents share the command surface. Agents extend with data not code; proposal-by-default staging; skills are declarative command recipes; extension ladder: assets → out-of-process MCP → (future amendment) sandboxed scripts. No native in-process binary plugins."
      },
      {
        "id": "VIII",
        "title": "Bandwidth",
        "summary": "Canvas is the prompt. Every modality compiles to commands. Human never blocked on agent. Intent ink and presence are ephemeral, never journaled."
      },
      {
        "id": "IX",
        "title": "Slate is a linker, never a database",
        "summary": "Link via Source abstraction; relative-first locators; link health Ok/Missing/Unknown; packages are permanent forks with provenance; write-back is explicit, human-only, never agent."
      },
      {
        "id": "X",
        "title": "No chrome divergence",
        "summary": "All chrome painting lives in atlas-shell. Apps pass data and handle actions."
      },
      {
        "id": "XI",
        "title": "Agent conduct",
        "summary": "Pushback mandate on constitutional conflict. Amendments only by user edit to CONSTITUTION.md. Constitution wins over other docs."
      }
    ]
  },

  "roadmap_phases": [
    {
      "id": "P1",
      "name": "Ratify",
      "status": "done",
      "date": "2026-07-19",
      "summary": "CONSTITUTION, rules, ROADMAP, facet taxonomy. Law locked.",
      "progress": 1.0
    },
    {
      "id": "P2",
      "name": "Draw",
      "status": "core_landed",
      "date": "2026-07-19",
      "summary": "vector-ink, Path model, Polyline/Arc/Bezier/Pen tools, cached mesh paint, SVG artifact serialization.",
      "progress": 0.7,
      "open": [
        "Taper/cap/join inspector UI",
        "Intent-ink layer (Art. VIII) — feeds atlas-ai beacon",
        "Blend modes (deferred — epaint has no per-shape blend; revisit with GPU renderer in P6)"
      ]
    },
    {
      "id": "P3",
      "name": "Unify (portals)",
      "status": "early_groundwork",
      "summary": "Fold Grid → Venn → Lens into board portals; retire tab ViewKinds at parity. Multi-lens dashboards. Longer: folder portal unifying Atlas canvas.",
      "progress": 0.15,
      "done_early": [
        "NodeKind::Portal in slate-doc (Repo Lens unbound portal)",
        "BoardTool::RepoLens",
        "Draft portal-lens-repository contract + machine-checked contract system (#37)",
        "Portal groundwork shipped with dock UX (5a9ea0b)"
      ],
      "blocked_on": ["S1 portal taxonomy contract (not written)", "Wave 3 WI-4 Edge extraction as graph prerequisite"]
    },
    {
      "id": "P4",
      "name": "Speak (agent surface)",
      "status": "not_started",
      "summary": "MCP server on command surface; canvas-as-prompt two-way channel; agent-authored declarative assets.",
      "progress": 0.05,
      "partial": [
        "atlas-ai context beacon exists (one-way file write)",
        "Cursor launcher + AI panel body",
        "Lens overlay pattern is the staging precursor"
      ],
      "blocked_on": ["Wave 1 T1.2 Provider split", "Wave 2 T2.3 staging + T2.4 atlas-mcp"]
    },
    {
      "id": "P5",
      "name": "Reach (sources, facets, formats)",
      "status": "not_started",
      "summary": "Source abstraction beyond local; facet-matrix refactor; print-faithful PDF; AEC on-ramp (IFC, point clouds; USD later; glTF pragmatic).",
      "progress": 0.0,
      "blocked_on": ["Wave 2 T2.1 SourceUri identity"]
    },
    {
      "id": "P6",
      "name": "Ascend (substrate and beyond)",
      "status": "not_started",
      "summary": "GPU vector renderer (Vello) as port; Mac/iPad spikes; Miro-style multiplayer on journal streams; revisit script amendment.",
      "progress": 0.0,
      "blocked_on": ["Wave 4 live collab", "Article I hedge holding"]
    }
  ],

  "workplan": {
    "name": "Audit №1/№2 implementation swarm",
    "docs": [
      "docs/workplan/README.md",
      "docs/workplan/agent-brief.md",
      "docs/workplan/tasks/wave-0.md … wave-3-plus.md",
      "docs/workplan/tasks/spikes.md"
    ],
    "execution_model": "One agent per task card; own git worktree; file ownership table prevents conflicts; implementer ≠ reviewer; human/orchestrator merges.",
    "gates": [
      {
        "id": "G0",
        "name": "Ratify amendments A,B,C,E,F",
        "status": "done",
        "commit": "1b88e8e",
        "note": "Wave 1+ gate is open."
      }
    ],
    "waves": [
      {
        "id": "W0",
        "name": "Guardrails + first wins",
        "status": "done",
        "progress": 1.0,
        "theme": "CI, metrics, lease, collage solver, membership, palette tokens, spatial index, EXTENDING.md",
        "tasks": [
          {"id": "T0.1", "title": "GitHub Actions CI", "status": "done", "closes": []},
          {"id": "T0.2", "title": "cargo xtask metrics + baseline", "status": "done", "closes": []},
          {"id": "T0.3", "title": "Migration fixture harness", "status": "done", "closes": []},
          {"id": "T0.4", "title": "Workbook lease + read-only", "status": "done", "closes": ["DV-06"]},
          {"id": "T0.5", "title": "crates/collage pure solver", "status": "done", "closes": []},
          {"id": "T0.6", "title": "Membership determinism (one frame)", "status": "done", "closes": ["DV-04"]},
          {"id": "T0.7", "title": "Palette → ui-tokens.toml", "status": "done", "closes": ["DV-09"]},
          {"id": "T0.8", "title": "Spatial index over node AABBs", "status": "done", "closes": ["DV-10"]},
          {"id": "T0.9", "title": "EXTENDING.md + false-affordances", "status": "done", "closes": []}
        ],
        "done_means": "Two people can open same workbook without losing work; CI blocks broken PRs; metrics baseline exists; collage can be computed; one node one slide; theme is a text file; 10k-node hit-test viable; strangers can self-triage via EXTENDING.md."
      },
      {
        "id": "W1",
        "name": "Convergent foundations",
        "status": "not_started",
        "progress": 0.0,
        "gate": "G0 + Wave 0 (both satisfied — READY TO START)",
        "theme": "ZOrder + id-addressed + property-scoped journal; atlas-ai renderer-free + Provider trait",
        "tasks": [
          {"id": "T1.1a", "title": "ZOrder fractional index, format v3, journal cap", "status": "not_started", "closes": ["DV-08"], "size": "M"},
          {"id": "T1.1b", "title": "Id-addressed SceneCmd + SceneReject", "status": "not_started", "closes": [], "size": "M", "depends": ["T1.1a"]},
          {"id": "T1.1c", "title": "Property-scoped SetProp (PropKey ≤16)", "status": "not_started", "closes": ["DV-01"], "size": "L", "depends": ["T1.1b"]},
          {"id": "T1.2", "title": "atlas-ai renderer-free; Provider trait; AI panel → atlas-shell", "status": "not_started", "closes": ["DV-02"], "size": "M"},
          {"id": "T1.3", "title": "App marquee/pick on spatial index", "status": "partial", "closes": [], "size": "S", "note": "Commit 86d2498 exists on a branch; not confirmed on main. Pure-crate index (T0.8) is merged; app call sites may still linear-scan."}
        ],
        "done_means": "Two independently-authored command streams replay identically in either order; undo/redo unchanged; AI crate renderer-free.",
        "critical_path": true
      },
      {
        "id": "S",
        "name": "Design spikes",
        "status": "not_started",
        "progress": 0.0,
        "theme": "Contracts + prototypes only — no app wiring",
        "tasks": [
          {"id": "S1", "title": "Portal taxonomy contract → docs/portal-contract.md", "status": "not_started", "size": "M"},
          {"id": "S2", "title": "Canvas clock + crates/dynamics prototype", "status": "not_started", "size": "L"},
          {"id": "S3", "title": "Collab protocol + crates/atlas-collab prototype", "status": "not_started", "size": "L", "depends": ["T1.1c"], "gates": ["Wave 4"]}
        ]
      },
      {
        "id": "W2",
        "name": "Identity + collage shipping",
        "status": "not_started",
        "progress": 0.05,
        "gate": "Wave 1 merged",
        "theme": "SourceUri; collage/align commands; staging layer; atlas-mcp",
        "tasks": [
          {"id": "T2.1", "title": "SourceUri + ContentId + tri-state health, v4", "status": "not_started", "closes": ["DV-03"], "size": "L"},
          {"id": "T2.2", "title": "Collage + align/distribute as commands", "status": "not_started", "closes": ["DV-05"], "size": "M", "note": "crates/collage exists; board.align registered but no dispatch (DV-05)"},
          {"id": "T2.3", "title": "Staging layer (agent propose / human accept)", "status": "not_started", "size": "M"},
          {"id": "T2.4", "title": "atlas-mcp (commands_list, context_read, board_propose)", "status": "not_started", "size": "M"}
        ],
        "done_means": "Select 30 images → collage in one undo; MCP client can propose same command staged + attributed."
      },
      {
        "id": "W3",
        "name": "Share",
        "status": "sketched",
        "progress": 0.0,
        "gate": "Wave 2 + one week daily use",
        "tasks": [
          {"id": "T3.0", "title": "Delete SlateItem.path mirror", "status": "not_started"},
          {"id": "WI-5", "title": ".slatepack package (InDesign model / permanent fork)", "status": "not_started", "closes": ["DV-07"]},
          {"id": "WI-4", "title": "Extract Edge from Connector, format v5", "status": "not_started", "closes": ["DV-11"]},
          {"id": "WI-8", "title": "Product B shared-file (lease request, reload, authorship)", "status": "not_started"}
        ]
      },
      {
        "id": "W4",
        "name": "Live collaboration",
        "status": "sketched",
        "progress": 0.0,
        "gate": "Wave 3 + S3 adopted (D11: core feature)",
        "tasks": [
          {"id": "WI-9a", "title": "slate-relay binary", "status": "not_started"},
          {"id": "WI-9b", "title": "Session client (atlas-collab transport)", "status": "not_started"},
          {"id": "WI-9c", "title": "Presence UI (never journaled)", "status": "not_started"},
          {"id": "WI-9d", "title": "Session asset delivery", "status": "not_started"}
        ],
        "done_means": "Dozen hybrid participants edit via firm-run relay; see cursors + images; relay death leaves coherent local copies."
      },
      {
        "id": "W5",
        "name": "After audit №3",
        "status": "deferred",
        "progress": 0.0,
        "items": [
          "Web-view host portals (D15)",
          "Dynamics layer (D12/S2)",
          "Filmstrip video scrub (D13)",
          "Extension package format (D14)",
          "Generative image nodes (D16)",
          "Agent Mode 2 — agent nodes",
          "ControlSurface + PanelSpec (unratified Amendment D)",
          "Full portals migration (Roadmap P3)",
          "Canvas painting of staged proposals",
          "Journal persistence only if named use appears"
        ]
      }
    ]
  },

  "decisions": [
    {"id": "D1", "title": "Sharing is daily; meetings are large", "wave_impact": "Lease, relative locators, convergent journal, Product B promoted"},
    {"id": "D2", "title": "Autodesk / Tier-2 adapters out", "wave_impact": "No Source trait/opendal now; keep SourceUri identity model"},
    {"id": "D3", "title": "Frame membership stays geometric", "wave_impact": "No Membership edge; T0.6 one-frame rule; announce changes in collab"},
    {"id": "D4", "title": "First agent task = collage", "wave_impact": "collage crate + arrange commands = agent-surface proof harness; Mode 1 first"},
    {"id": "D5", "title": "Windows reference platform", "wave_impact": "Linux CI floor; no Mac/Linux port work planned"},
    {"id": "D6", "title": "Metrics baseline for audits", "wave_impact": "cargo xtask metrics"},
    {"id": "D7", "title": "Portals have three classes", "wave_impact": "Generated / Document / Host — S1 contract"},
    {"id": "D8", "title": "Derived state + canvas clock", "wave_impact": "S2 dynamics; bake command"},
    {"id": "D9", "title": "Write-back explicit, human-only", "wave_impact": "Agents never write back"},
    {"id": "D10", "title": "Package is a fork, not sealed snapshot", "wave_impact": "WI-5 simplified (InDesign Package)"},
    {"id": "D11", "title": "Live collaboration is a core feature", "wave_impact": "Wave 4 promoted; self-hosted relay; supersedes C0/C1 tiering as deferral"},
    {"id": "D12", "title": "Interactive nodes with forces", "wave_impact": "S2 / Wave 5 dynamics"},
    {"id": "D13", "title": "Video viewports (Frame.io style)", "wave_impact": "Wave 5 filmstrip scrub"},
    {"id": "D14", "title": "Extensions = declarative assets + MCP", "wave_impact": "No native plugins"},
    {"id": "D15", "title": "Host portals: web first", "wave_impact": "Wave 5; OS window embed cut"},
    {"id": "D16", "title": "Generative images are authored content", "wave_impact": "Wave 5 with provenance"},
    {"id": "D17", "title": "Nested workbooks: tab first, in-place later", "wave_impact": "S1 Document class"},
    {"id": "D18", "title": "Relay persists session log", "wave_impact": "S3 / Wave 4"}
  ],

  "deviations": {
    "open": 8,
    "closed": 4,
    "accepted": 0,
    "rows": [
      {"id": "DV-01", "article": "VI", "status": "open", "summary": "SceneCmd Add/Remove by usize index; Patch replaces whole Node — journal cannot converge", "closes_with": "WI-2 / T1.1c"},
      {"id": "DV-02", "article": "I", "status": "open", "summary": "atlas-ai depends on eframe/atlas-shell — agent surface renderer-bound", "closes_with": "WI-6a / T1.2"},
      {"id": "DV-03", "article": "IX", "status": "open", "summary": "SlateItem identity is absolute PathBuf — workbook not portable across mount points", "closes_with": "WI-3 / T2.1"},
      {"id": "DV-04", "article": "IV", "status": "closed", "summary": "members_of could put one node on two slides", "closed": "7ede2ae (T0.6)"},
      {"id": "DV-05", "article": "VII.1", "status": "open", "summary": "board.align registered but no dispatch; align/distribute have zero call sites", "closes_with": "T2.2"},
      {"id": "DV-06", "article": "II", "status": "closed", "summary": "No concurrent-write guard on .slate", "closed": "fd21497 (T0.4)"},
      {"id": "DV-07", "article": "IV.1", "status": "open", "summary": "No golden board-vs-artifact parity test", "closes_with": "T3.1 / WI-5"},
      {"id": "DV-08", "article": "II.3", "status": "open", "summary": "SceneJournal unbounded", "closes_with": "WI-2 / T1.1a"},
      {"id": "DV-09", "article": "X", "status": "closed", "summary": "Palette hardcoded vs ui-tokens", "closed": "96f2b03 (T0.7)"},
      {"id": "DV-10", "article": "II", "status": "closed", "summary": "No spatial index — linear hit-test", "closed": "41ef2bb (T0.8)"},
      {"id": "DV-11", "article": "IV", "status": "open", "summary": "Two wire routers (lens orthogonal vs connector_bezier)", "closes_with": "Wave 3 WI-4"},
      {"id": "DV-12", "article": "VII.1", "status": "open", "summary": "Selection + Lens dock panels toggle but never draw (~1.3k dead lines)", "closes_with": "Wave 2 chrome pass"}
    ]
  },

  "metrics": {
    "baseline": {
      "date": "2026-07-25",
      "commit": "4d40a33",
      "lines_total": 67962,
      "lines_code": 57920,
      "pure_lines_code": 16084,
      "renderer_lines_code": 41836,
      "pure_ratio": 0.278,
      "crates": 15,
      "tests": 470,
      "unsafe_blocks": 13,
      "direct_dependencies": 18,
      "format_version": 2,
      "node_kinds": 5,
      "scene_cmd_variants": 3,
      "commands_slate": 112,
      "commands_file_atlas": 40,
      "deviations_open": 7,
      "deviations_closed": 4
    },
    "approx_2026_08_05": {
      "note": "Recomputed by file walk / regex — not cargo xtask metrics. Re-run xtask for audit-grade numbers.",
      "commit": "664e2e1",
      "lines_code_sum": 68981,
      "crates": 17,
      "tests": 653,
      "format_version": 2,
      "node_kinds": 6,
      "node_kind_names": ["Frame", "Image", "Shape", "Text", "Connector", "Portal"],
      "scene_cmd_variants": 3,
      "scene_cmd_shape": "Add{index}/Remove{index}/Patch{before,after} — still positional (DV-01)",
      "commands_slate_approx": 122,
      "commands_file_atlas_approx": 27,
      "deviations_open": 8,
      "deviations_closed": 4,
      "new_crates_since_baseline": ["slate-kit", "repo-graph", "collage was in baseline"]
    },
    "crate_loc_approx": [
      {"name": "slate", "lines_code": 23841, "kind": "app"},
      {"name": "atlas-shell", "lines_code": 9367, "kind": "chrome"},
      {"name": "native-file-atlas", "lines_code": 9143, "kind": "app"},
      {"name": "atlas-core", "lines_code": 6250, "kind": "pure-ish"},
      {"name": "slate-doc", "lines_code": 3769, "kind": "pure"},
      {"name": "code-lens", "lines_code": 3541, "kind": "pure"},
      {"name": "slate-artifact", "lines_code": 2334, "kind": "pure"},
      {"name": "xtask", "lines_code": 2263, "kind": "tool"},
      {"name": "slate-kit", "lines_code": 1843, "kind": "pure"},
      {"name": "vector-ink", "lines_code": 1762, "kind": "pure"},
      {"name": "rhino-mesh", "lines_code": 1136, "kind": "pure"},
      {"name": "circle-pack", "lines_code": 987, "kind": "pure"},
      {"name": "collage", "lines_code": 803, "kind": "pure"},
      {"name": "atlas-commands", "lines_code": 717, "kind": "pure"},
      {"name": "repo-graph", "lines_code": 697, "kind": "pure"},
      {"name": "atlas-ai", "lines_code": 459, "kind": "renderer-bound"},
      {"name": "atlas-session", "lines_code": 69, "kind": "bridge"}
    ]
  },

  "architecture": {
    "workspace_members": [
      "crates/atlas-core", "crates/atlas-shell", "crates/atlas-session", "crates/atlas-ai",
      "crates/atlas-commands", "crates/slate-doc", "crates/slate-kit", "crates/slate-artifact",
      "crates/circle-pack", "crates/collage", "crates/vector-ink", "crates/code-lens",
      "crates/repo-graph", "crates/rhino-mesh", "apps/file-atlas", "apps/slate", "xtask"
    ],
    "missing_planned_crates": ["atlas-mcp", "atlas-stage", "atlas-collab", "dynamics", "slate-relay"],
    "crates": [
      {"name": "atlas-core", "role": "UI-free backend: scanner, SQLite index, thumbs, tree layout, journal, export, watcher, timeline", "renderer": false},
      {"name": "atlas-shell", "role": "Shared chrome: theme, top bar, tabs, dock, sidebar, widgets, timeline paint, command reference UI", "renderer": true},
      {"name": "atlas-session", "role": "In-process Slate⇄Atlas bridge (SharedSession)", "renderer": false},
      {"name": "atlas-ai", "role": "AI workspace config, Cursor launcher, context beacon — still has ui.rs + eframe (DV-02)", "renderer": true},
      {"name": "atlas-commands", "role": "Shared command registry types", "renderer": false},
      {"name": "slate-doc", "role": ".slate model: tags, items, scene graph, SceneCmd journal, lease, spatial index", "renderer": false},
      {"name": "slate-kit", "role": "Declarative board tools: 9 gesture grammars (code) + result recipes (data/.slatekit)", "renderer": false},
      {"name": "slate-artifact", "role": "HTML artifact writer: scene→slides, styles→CSS, JS runtime", "renderer": false},
      {"name": "circle-pack", "role": "Circle packing + Venn layout geometry", "renderer": false},
      {"name": "collage", "role": "Justified/grid/masonry layout solver (std only)", "renderer": false},
      {"name": "vector-ink", "role": "kurbo path engine: flatten, variable-width AA meshes, SVG outlines, hit-test, freehand fit", "renderer": false},
      {"name": "code-lens", "role": "Cargo/Rust graph extraction, semantic zoom layout, agent overlay contract", "renderer": false},
      {"name": "repo-graph", "role": "Repository history graph support for Lens portal", "renderer": false},
      {"name": "rhino-mesh", "role": "Pure-Rust .3dm cached render mesh reader", "renderer": false},
      {"name": "xtask", "role": "metrics, contracts check, kits check", "renderer": false}
    ],
    "hard_rules": [
      "Chrome only in atlas-shell (Art. X)",
      "Mutate board only via journaled helpers (Art. VI)",
      "Tagging is Slate-only; Atlas surfaces Slate tags in linked sessions",
      "AI panel body shared — currently atlas_ai::ui::ai_body (should move to atlas-shell per T1.2)",
      "Workspace dep versions pinned once; members use { workspace = true }",
      "Every user-facing binding registered in app commands.rs SPECS",
      "Never bulk-read dehydrated OneDrive/SharePoint files (cloud::is_dehydrated gate)"
    ]
  },

  "capabilities_inventory": {
    "shipped": [
      {"area": "Atlas scan/tree/thumbs", "detail": "Streaming scan, tidy tree, LOD, portals for large folders, multi-tier cache, cloud dehydration guard"},
      {"area": "Shared chrome", "detail": "Unified top bar, floating docks, readouts, themes via ui-tokens.toml, Advanced command reference"},
      {"area": "Slate tags + Grid/Venn", "detail": "Faceted tags, combination buckets, circle-pack Venn"},
      {"area": "Board authoring", "detail": "Frames, shapes, text, images, paths, brush/eraser, connectors, crop/adjust, snap, present mode"},
      {"area": "Vector draw (P2 core)", "detail": "vector-ink + Polyline/Arc/Bezier/Pen + mesh cache"},
      {"area": "Tool kits", "detail": "slate-kit grammar+recipe; builtin/core.slatekit (#38)"},
      {"area": "HTML export", "detail": "slate-artifact serialization of board as slides"},
      {"area": "Lens", "detail": "code-lens analysis + overlay beacon files"},
      {"area": "3D", "detail": "Rhino mesh viewports on board"},
      {"area": "Linked sessions", "detail": "egui multi-viewport Atlas hosted by Slate"},
      {"area": "AI seed", "detail": "Workspace config, Cursor launch, context JSON beacon"},
      {"area": "Lease", "detail": ".slate.lock heartbeat; read-only second opener"},
      {"area": "Collage math", "detail": "crates/collage — not yet wired to a command"},
      {"area": "Spatial index", "detail": "slate-doc query_rect/query_point"},
      {"area": "Portal groundwork", "detail": "NodeKind::Portal + RepoLens tool + draft contract"},
      {"area": "Contracts system", "detail": "docs/keymap/contracts + cargo xtask contracts"},
      {"area": "CI", "detail": "fmt, clippy -D warnings, test on Linux; check on Windows"}
    ],
    "next_critical": [
      {"priority": 1, "item": "T1.1a→c convergent journal (v3)", "why": "Gates collaboration math + closes DV-01/DV-08"},
      {"priority": 2, "item": "T1.2 atlas-ai split + Provider", "why": "Closes DV-02; unlocks staging/MCP"},
      {"priority": 3, "item": "S1 portal taxonomy", "why": "Prevent Phase 3 baking wrong determinism assumption"},
      {"priority": 4, "item": "T2.2 collage command", "why": "Named weekly use + agent proof harness"},
      {"priority": 5, "item": "T2.1 SourceUri", "why": "Shared workbooks across mount points (D1)"},
      {"priority": 6, "item": "T2.3 + T2.4 staging + MCP", "why": "Command parity becomes real"}
    ],
    "explicitly_out_of_scope": [
      "Autodesk ACC/APS and Tier-2 platform adapters",
      "Google Docs / iCloud native sources",
      "Source trait + opendal backend (deferred, no named use)",
      "Hyperedges",
      "Mac/Linux/iPad as supported products (CI Linux only)",
      "Native in-process binary plugins",
      "Agent-initiated write-back",
      "Ratifying Amendment D (control surfaces) before implementation"
    ]
  },

  "model_snapshot": {
    "format_version": 2,
    "format_ledger": [
      {"v": 2, "status": "current", "adds": "board scene"},
      {"v": 3, "status": "reserved T1.1a", "adds": "Node.z: ZOrder"},
      {"v": 4, "status": "reserved T2.1", "adds": "SlateItem.uri SourceUri + content ContentId"},
      {"v": 5, "status": "reserved WI-4", "adds": "Scene.edges Vec<Edge>"},
      {"v": "6+", "status": "unclaimed", "adds": "—"}
    ],
    "view_kinds_tab_level": ["Grid", "Venn", "Board", "Lens"],
    "board_tools": [
      "Select", "Pan", "Frame", "RectShape", "Ellipse", "Line", "Arc", "Polyline",
      "BezierSpan", "Pen", "Text", "Brush", "Eraser", "Eyedropper", "Sticky",
      "DirectSelect", "RepoLens"
    ],
    "facets": ["Raster", "Vector", "Text", "Paged", "Spatial3D", "Timeline", "Structured", "Composition"],
    "media_kind_note": "Still MediaKind-based; Phase 5 migrates to facets(path)->FacetSet"
  },

  "progress_scores": {
    "overall_roadmap": 0.28,
    "overall_workplan_through_w2": 0.22,
    "phase_scores": {
      "P1_Ratify": 1.0,
      "P2_Draw": 0.7,
      "P3_Unify": 0.15,
      "P4_Speak": 0.05,
      "P5_Reach": 0.0,
      "P6_Ascend": 0.0
    },
    "wave_scores": {
      "W0": 1.0,
      "G0": 1.0,
      "W1": 0.0,
      "Spikes": 0.0,
      "W2": 0.05,
      "W3": 0.0,
      "W4": 0.0,
      "W5": 0.0
    },
    "health": {
      "deviation_debt": 8,
      "pure_ratio_baseline": 0.278,
      "pure_ratio_trend": "watch — should not fall (Art. I canary)",
      "format_version_stuck_at": 2,
      "biggest_risk": "Swarm outruns review; or Wave 1 journal refactor regresses undo",
      "biggest_opportunity": "Wave 1 ready now (G0+W0 done) — unblocks collab + agent surface"
    }
  },

  "audit_trail": {
    "audit_1": "audit-2026-07-25-protocols-collaboration-agents-api.md (c808966)",
    "audit_2": "audit-02-2026-07-25-flexibility-stress-test.md",
    "decisions": [
      "docs/audit/2026-07-25-decisions.md (D1–D6)",
      "docs/audit/2026-07-25-decisions-flexibility.md (D7–D18)"
    ],
    "next_audit_needs": [
      "Fresh cargo xtask metrics vs 2026-07-25 baseline",
      "Deviation ledger delta",
      "Which cards shipped / escalated / were wrong",
      "Real daily-use answers: collage weekly? how many simultaneous editors? what broke first?",
      "If public: stranger request distribution by EXTENDING.md flexibility class"
    ]
  }
}

```
