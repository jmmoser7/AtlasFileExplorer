# Image Generator browser rebuild

Current implementation: v0.3, 30 October 2026. Static HTML/JS/CSS/SVG, no build step.

- `layout.js`: native attachment predicate, control rows, strips and Bezier wires.
- `view.js` and `style.css`: editable controls, prompt growth, icons, image/port silhouette, history/results.
- `app.js`: canvas navigation, instance persistence, connected inputs and history lineage.
- `icons.js` and `assets/pavilion.png`: local static reference assets.
- `catalog.js`: image provider capabilities, passed into the shared Chat Train picker.

Native sources and screenshots are read-only references. The discarded native renderer is not required or running. See the draft contract and browser validation for limits; visual approval is pending.

The v0.5 menu-studio.js/CSS restores the saved right-click recipe as a configurable Studio dialog. Wire experiments, borderless thumbnails and compact header are shared across all image fixtures. Non-agent native editing menus are explicitly excluded.
