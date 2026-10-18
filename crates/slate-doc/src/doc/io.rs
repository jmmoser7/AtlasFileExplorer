//! The one workbook encoder/decoder, also used by crash recovery.
use super::SlateDoc;
use crate::SlateLoadError;
use std::io;
use std::path::Path;

impl SlateDoc {
    /// Writes pretty JSON atomically (temp file in the same directory, then rename).
    pub fn save_to(&self, path: &Path) -> io::Result<()> {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let file_name = path
            .file_name()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"))?;
        let mut temp_name = file_name.to_os_string();
        temp_name.push(".tmp");
        let temp_path = parent.join(temp_name);

        let mut stored = self.clone();
        crate::locators::persist_asset_locators(&mut stored, path);
        let json = serde_json::to_string_pretty(&stored).map_err(io::Error::other)?;
        std::fs::write(&temp_path, json)?;
        std::fs::rename(temp_path, path)?;
        Ok(())
    }

    /// Loads a workbook from disk, rejecting unknown future format versions.
    pub fn load_from(path: &Path) -> Result<Self, SlateLoadError> {
        let bytes = std::fs::read(path).map_err(|err| SlateLoadError::Io {
            path: path.to_path_buf(),
            source: err.kind(),
        })?;
        Self::decode_bytes(&bytes, path)
    }

    /// The canonical workbook decoder, shared by ordinary files and recovery.
    pub(crate) fn decode_bytes(bytes: &[u8], path: &Path) -> Result<Self, SlateLoadError> {
        let mut value: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|err| SlateLoadError::Parse {
                path: path.to_path_buf(),
                message: err.to_string(),
            })?;
        // Repository Lens and Status Board are not part of the product. Drop
        // those nodes before decode so a mixed workbook still opens.
        drop_retired_portals(&mut value);
        let mut doc: SlateDoc =
            serde_json::from_value(value).map_err(|err| SlateLoadError::Parse {
                path: path.to_path_buf(),
                message: err.to_string(),
            })?;
        if doc.format_version > Self::CURRENT {
            return Err(SlateLoadError::UnsupportedVersion {
                found: doc.format_version,
            });
        }
        // Older documents upgrade in place: the next save writes CURRENT.
        doc.format_version = Self::CURRENT;
        doc.view.active_view = doc.view.active_view.normalized();
        // Legacy bbox lines convert to parametric two-point paths
        // (contracts/line.md migration decision).
        doc.scene.migrate_legacy_lines();
        doc.scene.migrate_agent_cards();
        crate::image_paint::sanitize_paint_layers(&mut doc.scene);
        Ok(doc)
    }
}

/// Removes board nodes whose portal kind is a retired subtype
/// (`repo_lens`, `status_board`). Other nodes in the same workbook stay.
fn drop_retired_portals(doc: &mut serde_json::Value) {
    let Some(nodes) = doc
        .get_mut("scene")
        .and_then(|scene| scene.get_mut("nodes"))
        .and_then(|nodes| nodes.as_array_mut())
    else {
        return;
    };
    nodes.retain(|node| {
        !matches!(
            node.pointer("/kind/portal/kind")
                .and_then(|kind| kind.as_str()),
            Some("repo_lens" | "status_board")
        )
    });
}
