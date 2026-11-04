# Frame Studio
Open http://127.0.0.1:8765/?phase=01&tab=frame. Existing static Studio; no build, service dependency or package installation.

Select a Frame to expose its controls. Drag/resize/rotate directly; use Inspect for quick title, geometry and styling experiments. Reset datum restores the specimen. Notes and experiments can be exported. Navigation reuses the existing shared canvas module.

The default is source-measured Letter / Slide 1. Fill, Stroke, and Corners open compact property panels from the frame's own menu. Duplicate is available at the top and through Ctrl+D. Ctrl+Z / Ctrl+Y undo and redo. The Multiple slides specimen provides three spaced slides for deck ordering and presentation. The [contract](../contracts/frame.md) records the baseline and outstanding parity work; native pixel equality remains unverified.

Fast checks, from this folder:

    node check-baseline.cjs
    node --test geometry.test.cjs

Browser check (Node 20+ with the existing Playwright installation):

    node check-ui.cjs
    node check-properties.cjs

Native metric candidate check:

    node check-baseline.cjs --candidate path/to/native.json

Query FrameStudio.metrics() in the browser; its canonical field uses FrameBaseline.metricReport(state,camera,theme). fixtures.json and reference-metrics.json supply deterministic Rust query inputs and outputs. source-baseline.json pins current source bytes; baseline.json pins tokens. Never regenerate those just to make a gate pass.

The interaction field adds live history depths, deck order/prefix, presentation, property-panel placement and menu geometry. [PROPERTY_SPEC.md](PROPERTY_SPEC.md) describes property controls, transactions, dimensions and the requested blister transition.

[Acceptance](ACCEPTANCE.md) requires independent native interaction evidence and paired screenshots after cheap metrics pass. HTML behavior tests cannot certify Rust or pixel equality. This folder is a draft for rapid co-design; no native code was changed or built.

## 3 November — presentation controls proposal

The requested JS strip has six actions: **Fill, Stroke, Corners, Deck, Settings,
Present**. Previous/Next squircles are removed; the existing Present eye moves
to the end. The closed strip is 215 world units; the original seven-button
252-unit native datum and its source fingerprints remain historical references.

Settings proposes per-Frame Video start/loop overrides, Web entry view and
Interactive 3D. Both Video controls default to **Media setting**, preserving
authored playback flags; 3D defaults interactive as explicitly requested.
Web saved view means a URL or supported view bookmark, not cookies, page DOM
or a portable login/session. [PROPERTY_SPEC.md](PROPERTY_SPEC.md#3-november-2026--presentation-settings-proposal)
records fields, transactions and native limits.

The palette image in phase 00 is a proposal for visual review, not a locked
design or Rust implementation permission. Media/Web/model specimen feedback
is simulated; no external playback, website capture or Rhino call is claimed.
