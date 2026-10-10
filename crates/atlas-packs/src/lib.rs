//! Pack catalog. A manifest names one contract and a probe. Health is resolved
//! by the caller on a worker; this crate never paints and never calls a vendor.

mod catalog;
mod manifest;
mod probe;

pub use catalog::{Catalog, Offer, PackRow};
pub use manifest::{ContractId, PackId, PackManifest, ProbeSpec};
pub use probe::PackHealth;

/// Manifests shipped with the app. A week-long trial stays a JSON file beside
/// these, not a new crate (Article III).
pub fn shipped_manifests() -> Vec<PackManifest> {
    SHIPPED
        .iter()
        .map(|text| serde_json::from_str(text).expect("shipped pack manifest"))
        .collect()
}

const SHIPPED: &[&str] = &[
    include_str!("../packs/cursor.json"),
    include_str!("../packs/codex.json"),
    include_str!("../packs/ollama.json"),
    include_str!("../packs/comfy.json"),
    include_str!("../packs/openai-image.json"),
    include_str!("../packs/pdfium.json"),
    include_str!("../packs/sam.json"),
];
