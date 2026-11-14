# Pinned eframe: no per-frame OpenGL context switch on Windows

This directory contains the unchanged contents of the published `eframe 0.31.1`
crate, except for the one source patch recorded in
[`atlas-gl-context-switch.patch`](atlas-gl-context-switch.patch) and these two
Atlas provenance files. The published package ships no license file; eframe is
licensed `MIT OR Apache-2.0` (see `Cargo.toml`).

The recorded diff uses zero context to avoid whitespace-only context markers.
Apply it to a clean upstream snapshot with `git apply --unidiff-zero` (from this
workspace root, add `--directory=vendor/eframe` before the patch filename).

- Package: https://crates.io/crates/eframe/0.31.1
- Repository: https://github.com/emilk/egui (`crates/eframe`)
- Packaged Git revision: `1669e52a7ccfc3489c1b0999b9ed48894a0b3887`
- Crate archive SHA256:
  `d0dfe0859f3fb1bc6424c57d41e10e9093fe938f426b691e42272c2f336d915c`

## What upstream 0.31.1 does

`change_gl_context` (`src/native/glow_integration.rs`) has an early-out for
"this surface is already current" and compiles it out on Windows. glutin's WGL
`Surface::is_current` compares only the rendering context, never the device
context, so with several viewports it reports every surface as current; an
earlier attempt at the early-out on Windows was reverted after viewports broke
([egui #4289](https://github.com/emilk/egui/issues/4289)). The consequence
([egui #4173](https://github.com/emilk/egui/issues/4173)) is that on Windows
every call does `wglMakeCurrent(NULL, NULL)` followed by
`wglMakeCurrent(hdc, hglrc)`: twice per painted frame with one viewport (before
`App::update` and before painting).

Upstream has since fixed this differently, by remembering which viewport the
context was last bound to: [egui #8461](https://github.com/emilk/egui/pull/8461),
"Don't switch the GL context twice per frame on Windows with one viewport",
merged 2026-10-06 and released in eframe 0.37.0. This patch has the same intent
for 0.31.1 with a smaller change. **Remove this directory when the workspace
moves to eframe 0.37.0 or later.**

## What the patch does

`change_gl_context` takes the early-out on every platform. On Windows the check
is `surface_is_current`: glutin's context comparison, plus the missing half -
it asks WGL for the bound device context (`wglGetCurrentDC`) and compares that
DC's window (`WindowFromDC`) with the surface's window. EGL surfaces keep
glutin's own comparison, which already checks the bound surfaces.

With one viewport the context is made current once and stays current. With
several (Slate's linked File Atlas window) it switches only when the target
window changes, as on the other platforms.

`ATLAS_GL_ALWAYS_SWITCH=1` in the environment restores upstream 0.31.1's
behavior for A/B comparison.

## Why Atlas carries it now

Two reasons, of different strength.

1. **Frame cost (established).** Each context switch is a synchronous round
   trip into the driver. Upstream treats the per-frame switch as a performance
   bug and removed it.

2. **A native crash on Intel graphics (plausible, unproven).** Both Windows
   crash dumps of Slate 1.0.7 from 2026-11-11 (Intel Iris Plus, driver
   30.0.101.3118) show:
   - the faulting thread is the driver's own command-stream worker
     (`ig11icd64.dll+0x65a690` calling its interpreter at `+0x4b7b0`). Its read
     offset is near the start of its ring buffer (the bad record begins at
     offset 0x45 in both dumps) while its cached write offset is near the end
     (0x79F0A8 / 0x79F355). That is consistent with the worker running past
     valid data after a wrap; the buffer itself is not in the dumps;
   - the main thread is in `GlowWinitApp::run_ui_and_paint -> change_gl_context
     (the first call of the frame) -> glutin make_not_current -> opengl32
     wglMakeCurrent -> ig11icd64 DrvReleaseContext`, inside the driver's
     flush-and-wait loop, waiting for the worker.

   What that does **not** establish is that the context release caused the
   fault. The main thread would be found in that wait loop if the worker had
   already gone wrong on commands queued at the end of the previous frame and
   the release was simply the next call that waits for it. The same driver
   flush routine is also reached from `SwapBuffers` and other GL entry points,
   which this patch does not touch.

   A third dump (Slate 1.0.8 development build, 2026-11-13 05:43, after 6 h 43 min)
   points at the same cycle from the other side. The worker crashes reading
   address 0 while executing a `glViewport` for a window-sized rectangle,
   because the driver context has no current draw framebuffer; the records
   queued behind it are the size of egui_glow's `clear` sequence, which eframe
   issues immediately after the frame's first `change_gl_context`. The main
   thread is in application code. That fits the worker running the first
   commands after a re-acquire before the surface was attached on its side.
   What cleared the pointer was not traced, and this reading has not been
   independently reviewed.

   So treat the patch as a mitigation to be measured, not as a proven fix: it
   removes four synchronous driver flushes per frame (two releases, two
   re-acquires) in single-window use. Whether that removes the crash, lowers
   its rate, or changes nothing can only be settled by an A/B soak on the Intel
   machine (default versus `ATLAS_GL_ALWAYS_SWITCH=1`), ideally with
   full-memory crash dumps enabled so the next dump contains the driver's ring
   buffer. The analysis is in `output/slate-audit-2026-11-13/REPORT.md` in the
   FileAtlas folder.

## Not covered

- Every real switch between two windows still releases and re-acquires the
  context (Slate with the linked File Atlas window open), as does viewport
  creation.
- The driver's other synchronous flushes (`SwapBuffers` every frame, readbacks).

## Status

Compile-checked in the workspace (`cargo check -p slate -p native-file-atlas`).
Not yet exercised in a native run: confirm on the Intel reference machine that a
single-window session renders normally, that opening and closing the linked
File Atlas window still paints both windows, and that window resize, minimize /
restore, presentation mode, a DPI or monitor change and display sleep / resume
are unaffected.

The workspace uses `[patch.crates-io]` at the existing version and excludes this
dependency snapshot from workspace membership. No unrelated upstream changes or
dependency upgrades are included.
