# Atlas ecosystem — status dashboard handoff

**Audience:** an agent (or human) building or refining the interactive project
status dashboard. **You do not need the source repository.** Everything required
is in this folder.

| File | Role |
|------|------|
| **[`ATLAS-STATUS-BUNDLE.html`](ATLAS-STATUS-BUNDLE.html)** | **One-file deliverable** — interactive dashboard + handoff plan + embedded JSON. Open in a browser, or click **Copy all for agent**. |
| **[`ATLAS-STATUS-BUNDLE.md`](ATLAS-STATUS-BUNDLE.md)** | **Plain-text paste bundle** — same handoff + full JSON for chat/agent paste. |
| [`project-state.json`](project-state.json) | Canonical research snapshot (also embedded in the HTML). |
| [`index.html`](index.html) | Same content as the HTML bundle. |
| **This file** | Build brief source (included inside both bundles). |

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
