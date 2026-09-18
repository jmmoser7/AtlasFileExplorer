# Native fill coverage regression — September 18, 2026

The reported red trimmed corner contained only solid red and canvas pixels:
there were no blended edge pixels. Boolean fills were triangulated correctly,
but raw egui meshes bypass software path feathering and the native framebuffer
had multisampling disabled.

Both native apps now use `atlas_shell::NATIVE_MSAA_SAMPLES` (4), alongside
existing software feathering. The shared rule is in
[PAINT.md](../../crates/atlas-shell/PAINT.md). This changes raster coverage, not
document geometry, boolean operations, stroke widths, or exports.

## Native captures

The fixture calls the real vector-ink boolean subtraction and fill triangulation
and the production shared icon painter. Captures use the native Glow renderer at
1180 × 750, one physical pixel per logical point. The only difference between
before and after is multisampling; positions deliberately include a quarter-pixel
offset to exercise coverage.

- [Light, before](fill-before-light.png) / [light, after](fill-after-light.png)
- [Dark, before](fill-before-dark.png) / [dark, after](fill-after-dark.png)
- Existing feathered strokes with MSAA: [light](strokes-after-light.png) /
  [dark](strokes-after-dark.png). Visually checked curved dashes, round caps and
  joins, translucent curves, taper, and a closed capsule.

The opaque trimmed shape's crop has two colors before (fill and canvas) and five
after (fill, canvas, and three intermediate coverage levels). Each theme gains
1,129 blended boundary pixels. Curved holes and the concave cut also have coverage;
the translucent interior retains its uniform color with no internal mesh seams.
The inset-mask check covers 58,415 interior pixels per theme.

## Reproduce

Build `cargo build -p atlas-shell --example shape_palettes`, then launch the
fixture twice from PowerShell (it captures both themes and exits):

```powershell
$captureDir = Join-Path (Get-Location) 'design/render-quality-2026-09-18'
$fixture = Join-Path (Get-Location) 'target/debug/examples/shape_palettes.exe'
Start-Process -FilePath $fixture -ArgumentList @('--fill-quality', '--no-msaa', '--capture', "$captureDir/fill-before") -WindowStyle Hidden -Wait
Start-Process -FilePath $fixture -ArgumentList @('--fill-quality', '--capture', "$captureDir/fill-after") -WindowStyle Hidden -Wait
python design/render-quality-2026-09-18/verify.py
```

The pixel check requires Pillow. It verifies intermediate coverage at convex and
concave edges, holes, small curves, and translucent cuts, then checks a safely
inset interior mask for unwanted triangle seams. Capture comparison is a native
GPU check; a headless egui mesh test cannot detect disabled framebuffer AA.

These captures verify this machine's native renderer in both themes. They are not
a large-document GPU performance benchmark. Existing zoom-aware curve sampling
is still required: MSAA cannot repair coarse geometry or a low-resolution source
image.

Validation completed: the pixel check above, native stroke captures in both
themes, `cargo check -p slate -p native-file-atlas --tests`, targeted rustfmt and
diff checks, and `cargo build --release -p slate --features ui-tuner`. The latter
updated `target/release/slate.exe`; two existing unused-code warnings remain.
