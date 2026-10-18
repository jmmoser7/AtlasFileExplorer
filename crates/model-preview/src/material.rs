//! Renderer-neutral standard material factors and unresolved texture sources.
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub enum TextureSource {
    /// Optional app-worker output. GPU upload consumes these pixels directly.
    Decoded {
        rgba: Arc<[u8]>,
        width: u32,
        height: u32,
    },
    Embedded {
        bytes: Arc<[u8]>,
        mime_type: Option<String>,
    },
    /// Absolute/resolved model-relative location. The consumer must check cloud
    /// residency before reading; resolving a locator must never hydrate a file.
    LocalPath(PathBuf),
}

#[derive(Clone, Debug)]
pub struct PreviewMaterial {
    /// Linear RGB and straight alpha. Texture RGB is sRGB and multiplies this.
    pub base_color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    pub emissive: [f32; 3],
    pub base_color_texture: Option<TextureSource>,
}

impl Default for PreviewMaterial {
    fn default() -> Self {
        Self {
            base_color: [1.0; 4],
            metallic: 0.0,
            roughness: 0.8,
            emissive: [0.0; 3],
            base_color_texture: None,
        }
    }
}

impl PreviewMaterial {
    pub fn from_srgb(color: [u8; 3]) -> Self {
        Self {
            base_color: [linear(color[0]), linear(color[1]), linear(color[2]), 1.0],
            ..Self::default()
        }
    }
}

fn linear(byte: u8) -> f32 {
    let value = byte as f32 / 255.0;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}
