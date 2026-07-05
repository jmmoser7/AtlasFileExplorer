# Bottom readout bar — design rules

Rules for the persistent bottom bar (`ui/readouts.rs`) and its sub-dashboard
strip. Read `ARCHITECTURE.md` Layer 1 before adding panels.

## Two vertical zones

The bottom `TopBottomPanel` (`readouts`) is split into two zones, top to bottom:

| Zone | Role | Always visible? |
|------|------|-----------------|
| **Sub-dashboard strip** | Capsule cards (activity heatmap today; more later) | When a sub-dashboard readout is enabled via gear |
| **Metrics ticker** | File/folder counts, scan status, zoom, root path | When metrics readout is enabled (default on) |

The metrics ticker sits at the **absolute bottom** of the window context. Sub-dashboards
never push it downward — they occupy the strip directly above it.

Temporary overlays (e.g. `prewarm_dashboard`) stack above the entire readout panel.

## Sub-dashboard capsule anatomy

Each sub-dashboard is a bordered capsule via `readout_dashboard_capsule` in
`readout_dashboard.rs`. Match sidebar card tokens (`SidebarTokens` / 6px radius):

```
┌───────────────────────────────┐
│ −  Activity        drag edges │  ← header (18px); +/− toggles body
├───────────────────────────────┤
│  panel body…                  │  ← when expanded
└───────────────────────────────┘
  ↑                         ↑
  left edge drag            right edge drag
```

### Tokens (`ReadoutDashboardTokens`)

| Token | Value | Notes |
|-------|-------|-------|
| Corner radius | 6px | Same as sidebar cards |
| Border | 1px `theme.border` | Inside stroke on card fill |
| Fill | `theme.card` | Elevated from panel background |
| Inner padding | 8×6 px | Horizontal × vertical |
| Strip gap | 6px | Between capsules in the strip |
| Header height | 18px | Toggle + title |
| Toggle glyph | 8px | `+` contracted, `−` expanded |
| Edge handle | 6px | Left/right resize hit zones |
| Default width | 62% of bar | Per-tab `readout_width_frac` |
| Min / max width | 25% / 100% | Clamped on edge drag |

Build theme from the app palette:

```rust
let p = app.palette();
let theme = ReadoutDashboardTheme {
    card: p.card,
    border: p.border,
    ink: p.ink,
    sub: p.sub,
};
```

## Interaction

| Control | Effect |
|---------|--------|
| Gear menu (`ChromeConfig.readouts[]`) | Show or hide a readout panel |
| Header `+` / `−` | Fully contract (header only) or expand body |
| Left / right edge drag | Resize capsule width along the bottom bar |
| Header title click | Same as toggle |

Per-tab session state (not persisted to disk yet):

- `readouts_expanded[]` — body visible for each sub-dashboard
- `readout_width_frac[]` — width as a fraction of the bottom bar

## Adding a new sub-dashboard

1. Add a variant to `ReadoutPanel` in `chrome.rs` (`ALL`, `label`, `default_on`).
2. Extend `readouts[]`, `readouts_expanded[]`, and `readout_width_frac[]` in `ChromeConfig`.
3. Implement body content in its module (keep drawing inside the capsule — no custom `Frame` in `readouts.rs`).
4. Register a capsule in `sub_dashboard_strip` inside `readouts.rs`.
5. The gear menu picks up the new panel automatically via `ReadoutPanel::ALL`.

**Do not** put sub-dashboard bodies in the metrics ticker row. **Do not** add custom
border styling in panel modules — extend `readout_dashboard.rs` instead.

## Related files

| File | Role |
|------|------|
| `ui/readout_dashboard.rs` | Capsule frame, resize handles, header toggle |
| `ui/readouts.rs` | Strip layout, metrics ticker, gear menu |
| `ui/activity_heatmap.rs` | Activity heatmap body (first sub-dashboard) |
| `chrome.rs` | Panel registry and visibility state |
