#![cfg(not(windows))]

use atlas_connector_runtime::surface::{NativeSurface, SurfaceError};

#[test]
fn native_surfaces_explicitly_refuse_unsupported_platforms() {
    assert!(matches!(
        NativeSurface::attach(1, 1, 2),
        Err(SurfaceError::UnsupportedPlatform)
    ));
}
