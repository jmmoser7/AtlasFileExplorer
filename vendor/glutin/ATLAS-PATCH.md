# Pinned Glutin 0.32.3

This directory contains the published Glutin 0.32.3 crate and
[the Atlas WGL provider patch](atlas-wgl-provider.patch).

- [Published crate](https://crates.io/crates/glutin/0.32.3)
- [Upstream source](https://github.com/rust-windowing/glutin)
- Packaged Git revision `20d1c103172aa4025f02cc94ca16a3169bea789c`
- Crate archive SHA-256 `12124de845cacfebedff80e877bb37b5b75c34c5a4c89e47e1cdd67fb6041325`
- [Apache 2.0 license](LICENSE)

The patch uses zero context. Apply it to the published crate with
`git apply --unidiff-zero`. From the workspace root, add
`--directory=vendor/glutin` before the patch filename.

## Windows OpenGL provider

Glutin selected core OpenGL exports through a loadable DLL while WGL and
GDI calls for context creation and presentation remained linked to Windows.
A separate Mesa DLL could therefore receive OpenGL calls for a context
created by a different provider.

With `ATLAS_WGL_PROVIDER_DLL` set to an absolute Mesa wrapper path, the patch
uses the same provider to select pixel formats, create and bind contexts,
resolve OpenGL functions, and present. The loader canonicalizes the wrapper path
for Windows DLL resolution and verifies that `libgallium_wgl.dll` belongs to
the same directory. Each process selects the provider once. With no opt-in,
the original Windows GDI and WGL functions provide those operations. Glutin
also checks the bound device context when testing whether a WGL surface is
current, so eframe uses the same provider for that check.

When software WGL is selected explicitly, a WGL initialization failure is
reported with its original error code and setup step. The WGL-first selection
keeps its normal EGL fallback for the system provider.

Reassess this patch when the pinned Glutin version changes. Validate native
and Mesa startup, one and multiple windows, resize, and process shutdown.
