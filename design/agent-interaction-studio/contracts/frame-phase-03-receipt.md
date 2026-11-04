# Frames · Phase 03 run and verification

Run `cargo run --locked -p atlas-shell --example frame_canvas -- --large` from the workspace root. The specimen opens in its own window with the 100-slide palette already open. **5 slides** switches to the small fixture; select a Frame and use the numbered list icon to open its palette.

## Verified

- Five thumbnails fit in the toolbar. A wide Frame uses the blister and one continuous rail across the palette join. The small Frame uses the external squircle strip and the palette clears it. Blank canvas and Esc close the palette and membership mode.
- All 100 thumbnail rectangles fit at a 780 × 720 client viewport in ten rows and ten columns. The native window capture shows that the opaque palette covers underlying canvas frames.
- Real egui pointer/key input exercises Shift/Ctrl selection, live cross-row reorder, one journal commit on drop, Undo/Redo, and Delete of slide references. The selected Frames stay on the canvas.
- The scene model resolves shared named decks without changing the legacy default order. Sort moves Frames with their authored contents without changing either presentation's order. Preview playback follows the active deck and runs deterministic media entry and exit.

## Comparison and limits

The [small JS fixture](../?phase=01&tab=frame&sample=1&fixture=presentation-small) and [large JS fixture](../?phase=01&tab=frame&sample=1&fixture=presentation-large) were compared with the native captures for layout, icon order, palette join, chevron, and thumbnail grid. The native fixture has its own compact top bar and uses native font and icon rasterization; the capture is not a claim of exact pixel equivalence. The 100-slide evidence uses the live window because the software raster output shows clipped text in this fixture.

The specimen shares the Slate scene journal, frame fill primitive, deck stroke geometry, and shell selection controls. It exercises the Settings panel with simulated video, web views and 3D actions. Workbook save/export, actual media hosts, full Slate toolbar integration, production presentation flight, and the independent pixel gate remain for later phases.

## Build record

- Target: stable Windows MSVC, existing dev profile and target directory. Dependency tree excluded the full Slate app, WebView, Rhino and model providers.
- The first attempt warmed dependencies but ended on in-progress source errors. The first successful small build was **40.88 s**. The build after changing both shared crates took **85 s**; the final verified build took **48.66 s**. These are warm-cache runs; a fully cold build was not measured.
- Checks: `cargo check --locked -p atlas-shell --example frame_canvas`, `cargo build --locked -p atlas-shell --example frame_canvas --timings`, and `target/debug/examples/frame_canvas.exe --verify target/frame-phase03`.
- The consuming Slate library also passed `cargo check --locked -p slate --lib --jobs 2` in **49.84 s**. That was a type check, not a full Slate executable build.
- [Machine receipt and hashes](../assets/frame-proposals/20261103-phase03-native/receipt.json)
