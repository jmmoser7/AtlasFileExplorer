//! Folder layout beside a saved `.slate` workbook.
//!
//! Generated pictures, clipboard pastes, packaged web pages, unbundled
//! document pages, and a reserved agent-build folder travel with the file
//! when the workbook moves to another computer. User-linked sources outside
//! this tree stay links (Article IX): copying them would be packaging.
//!
//! Nothing here reads file bytes until a caller has already refused a cloud
//! placeholder. Sidecar JSON names the prompt and the run; it never carries
//! an API key or any other secret.

use std::collections::BTreeMap;
use std::io;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

mod paths;
use paths::under;
pub use paths::{generated_provider_dir, same_path};
mod tree_copy;
pub use tree_copy::{
    copy_file, copy_tree, require_copy_destination, require_local_source, write_file,
};

/// Directory name beside the workbook file.
pub const ASSETS: &str = "assets";
pub const PASTED: &str = "pasted";
pub const GENERATED: &str = "generated";
pub const WEB: &str = "web";
pub const DOCUMENTS: &str = "documents";
pub const AGENT: &str = "agent";
pub const CAPTURES: &str = "captures";

/// App-data children that are caches, secrets, or other machine state.
/// A pasted or generated image never lives in one of these.
const DATA_DIR_INFRA: &[&str] = &[
    "thumbs",
    "secrets",
    "webview2",
    "web-stills",
    "document-pages",
    "document-previews",
    "canvas-exports",
    "model-posters",
    "home-covers",
    "themes",
    "tools",
    "session-log",
];

const RASTER_EXT: &[&str] = &["png", "jpg", "jpeg", "webp", "gif", "bmp"];

/// `<workbook folder>/assets`.
pub fn assets_root(workbook: &Path) -> PathBuf {
    workbook
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(ASSETS)
}

pub fn pasted_dir(workbook: &Path) -> PathBuf {
    assets_root(workbook).join(PASTED)
}

/// `assets/generated/<provider>/<yyyy-mm>`. `provider` and `year_month` are
/// single path components; separators in a provider id become hyphens.
pub fn generated_dir(workbook: &Path, provider: &str, year_month: &str) -> PathBuf {
    generated_provider_dir(workbook, provider).join(year_month_component(year_month))
}

pub fn web_dir(workbook: &Path) -> PathBuf {
    assets_root(workbook).join(WEB)
}

/// One packaged local page: `assets/web/<name>`. The stem keeps its case;
/// separators cannot escape the folder.
pub fn web_package_dir(workbook: &Path, name: &str) -> PathBuf {
    web_dir(workbook).join(single_component(name, "page"))
}

pub fn documents_dir(workbook: &Path) -> PathBuf {
    assets_root(workbook).join(DOCUMENTS)
}

/// Reserved for a chat train's "Just build" output (`assets/agent`).
pub fn agent_dir(workbook: &Path) -> PathBuf {
    assets_root(workbook).join(AGENT)
}

/// Where a clipboard bitmap is written. A saved workbook uses
/// `assets/pasted`. An unsaved one uses `<data_dir>/pasted` until Save.
pub fn paste_dir(workbook: Option<&Path>, data_dir: &Path) -> PathBuf {
    match workbook {
        Some(workbook) => pasted_dir(workbook),
        None => data_dir.join(PASTED),
    }
}

/// Where a generator writes a new picture. A saved workbook uses
/// `assets/generated/<provider>/<yyyy-mm>`. An unsaved one keeps today's
/// `<data_dir>/<provider>/<session>` folder so an in-flight engine does not
/// change location under itself.
pub fn generated_output_dir(
    workbook: Option<&Path>,
    data_dir: &Path,
    provider: &str,
    session_tag: &str,
    year_month: &str,
) -> PathBuf {
    match workbook {
        Some(workbook) => generated_dir(workbook, provider, year_month),
        None => data_dir.join(provider).join(session_tag),
    }
}

/// Unsaved "Just build" folder. The chat-train workstream calls the same
/// shape: `<data_dir>/assets/agent`.
pub fn unsaved_agent_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(ASSETS).join(AGENT)
}

/// Page images for one source document. Saved workbooks use
/// `assets/documents/<stem>-<hash>`. Unsaved workbooks keep
/// `<data_dir>/document-pages/<stem>-<hash>`, which is where those files
/// already land.
pub fn document_pages_dir(
    workbook: Option<&Path>,
    data_dir: &Path,
    stem: &str,
    source: &Path,
) -> PathBuf {
    let folder = format!("{stem}-{:08x}", path_hash(source));
    match workbook {
        Some(workbook) => documents_dir(workbook).join(folder),
        None => data_dir.join("document-pages").join(folder),
    }
}

/// UTC `yyyy-mm` for a Unix time. Asset folders use the month, not the day.
pub fn year_month(unix_secs: u64) -> String {
    let (y, m, _) = civil_date(unix_secs);
    format!("{y:04}-{m:02}")
}

/// Provenance written beside a generated image as `<id>.json`.
///
/// Fields are the prompt, the model, optional seed and engine params, when
/// it was created, and which run produced it. There is no slot for a key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedSidecar {
    pub prompt: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, String>,
    pub created: u64,
    pub source_run: String,
}

/// Write `<image stem>.json` next to a generated picture.
///
/// Refuses a machine-private destination. The serialized value is scrubbed
/// so a param map cannot smuggle a secret key in under another name.
pub fn write_generated_sidecar(image: &Path, sidecar: &GeneratedSidecar) -> io::Result<()> {
    if crate::secrets::is_machine_private(image) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "machine-private path",
        ));
    }
    let mut value = serde_json::to_value(sidecar).map_err(io::Error::other)?;
    scrub_sidecar(&mut value);
    let path = sidecar_path(image);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_vec_pretty(&value).map_err(io::Error::other)?;
    std::fs::write(path, json)
}

pub fn sidecar_path(image: &Path) -> PathBuf {
    image.with_extension("json")
}

/// Drop secret-shaped keys from a copied sidecar. Prompt text is left alone.
pub fn scrub_sidecar(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.retain(|key, child| {
                if secret_key(key) {
                    return false;
                }
                scrub_sidecar(child);
                true
            });
        }
        serde_json::Value::Array(items) => {
            for child in items {
                scrub_sidecar(child);
            }
        }
        _ => {}
    }
}

/// One file the collect / save migration copied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectedAsset {
    pub from: PathBuf,
    pub to: PathBuf,
    /// Workbook-relative locator with forward slashes (`assets/pasted/…`).
    pub locator: String,
}

/// One source the migration would not copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefusedAsset {
    pub path: PathBuf,
    pub reason: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CollectOutcome {
    pub collected: Vec<CollectedAsset>,
    pub refused: Vec<RefusedAsset>,
}

impl CollectOutcome {
    pub fn summary(&self) -> String {
        if self.collected.is_empty() && self.refused.is_empty() {
            return "No pasted or generated images to collect.".into();
        }
        let mut parts = Vec::new();
        if !self.collected.is_empty() {
            parts.push(format!(
                "Filed {} image{} under assets",
                self.collected.len(),
                if self.collected.len() == 1 { "" } else { "s" }
            ));
        }
        if !self.refused.is_empty() {
            parts.push(format!(
                "left {} in place ({})",
                self.refused.len(),
                refusal_counts(&self.refused)
            ));
        }
        parts.join("; ")
    }
}

/// Copy pasted/generated images and owned portal recordings under `data_dir` into the
/// workbook layout. Paths outside that directory, machine-private files,
/// infrastructure caches, and cloud placeholders are refused and not read.
///
/// File copies happen here. Callers on the frame loop must run this on a
/// worker. `sources` are the resolved filesystem paths the board links.
pub fn collect_data_dir_assets(
    data_dir: &Path,
    workbook: &Path,
    sources: &[PathBuf],
    now_secs: u64,
) -> CollectOutcome {
    let mut outcome = CollectOutcome::default();
    for source in sources {
        match plan_data_dir_asset(data_dir, workbook, source, now_secs) {
            Plan::Skip => {}
            Plan::Refuse(reason) => outcome.refused.push(RefusedAsset {
                path: source.clone(),
                reason,
            }),
            Plan::Copy {
                to,
                locator,
                generated,
            } => {
                if let Err(reason) = copy_asset(source, &to, generated) {
                    outcome.refused.push(RefusedAsset {
                        path: source.clone(),
                        reason,
                    });
                } else {
                    outcome.collected.push(CollectedAsset {
                        from: source.clone(),
                        to,
                        locator,
                    });
                }
            }
        }
    }
    outcome
}

/// Copy assets this workbook already references from its own `assets/` tree
/// into another workbook folder. This is Save As, not packaging: a linked
/// user file outside `assets/` is left where it is.
pub fn copy_referenced_assets(
    from_workbook: &Path,
    to_workbook: &Path,
    sources: &[PathBuf],
) -> CollectOutcome {
    let mut outcome = CollectOutcome::default();
    if same_path(from_workbook, to_workbook) {
        return outcome;
    }
    let from_assets = assets_root(from_workbook);
    for source in sources {
        let resolved = if source.is_absolute() {
            source.clone()
        } else {
            crate::workbook_assets::resolve_beside(from_workbook, source)
        };
        if crate::secrets::is_machine_private(&resolved) {
            outcome.refused.push(RefusedAsset {
                path: source.clone(),
                reason: "machine-private",
            });
            continue;
        }
        if !under(&from_assets, &resolved) {
            continue;
        }
        if cloud_blocked(&resolved) {
            outcome.refused.push(RefusedAsset {
                path: source.clone(),
                reason: "cloud placeholder",
            });
            continue;
        }
        let Some(rel) = strip_under(&from_assets, &resolved) else {
            continue;
        };
        let to = assets_root(to_workbook).join(&rel);
        if same_path(&resolved, &to) {
            continue;
        }
        let generated = rel.components().next().is_some_and(|c| {
            c.as_os_str()
                .to_string_lossy()
                .eq_ignore_ascii_case(GENERATED)
        });
        if let Err(reason) = copy_asset(&resolved, &to, generated) {
            outcome.refused.push(RefusedAsset {
                path: source.clone(),
                reason,
            });
        } else {
            outcome.collected.push(CollectedAsset {
                from: resolved,
                to,
                locator: locator_from_rel(&rel),
            });
        }
    }
    outcome
}

/// Join a workbook-relative locator onto the workbook's folder.
pub fn resolve_beside(workbook: &Path, locator: &Path) -> PathBuf {
    if locator.is_absolute() {
        return locator.to_path_buf();
    }
    workbook
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(locator)
}

enum Plan {
    Skip,
    Refuse(&'static str),
    Copy {
        to: PathBuf,
        locator: String,
        generated: bool,
    },
}

fn plan_data_dir_asset(data_dir: &Path, workbook: &Path, source: &Path, now_secs: u64) -> Plan {
    if !source.is_absolute() {
        return Plan::Skip;
    }
    if crate::secrets::is_machine_private(source) {
        return Plan::Refuse("machine-private");
    }
    if !under(data_dir, source) {
        return Plan::Refuse("outside the app data folder");
    }
    let Some(child) = first_component(data_dir, source) else {
        return Plan::Refuse("outside the app data folder");
    };
    if DATA_DIR_INFRA
        .iter()
        .any(|name| child.eq_ignore_ascii_case(name))
    {
        return Plan::Refuse("not a pasted or generated image");
    }
    if !is_raster(source) && !is_capture_video(data_dir, source, &child) {
        return Plan::Refuse("not a pasted or generated image");
    }
    if cloud_blocked(source) {
        return Plan::Refuse("cloud placeholder");
    }
    let file_name = source
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "image.png".into());
    let pasted =
        child.eq_ignore_ascii_case(PASTED) || file_name.to_ascii_lowercase().starts_with("paste-");
    let (to_dir, folder, generated) = if pasted {
        (pasted_dir(workbook), PASTED.to_string(), false)
    } else {
        let month = year_month(file_mtime_secs(source).unwrap_or(now_secs));
        let provider = provider_component(&child);
        (
            generated_dir(workbook, &provider, &month),
            format!("{GENERATED}/{provider}/{month}"),
            true,
        )
    };
    let to = unique_dest(&to_dir, &file_name, source);
    let stored_name = to
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or(file_name);
    Plan::Copy {
        to,
        locator: format!("assets/{folder}/{stored_name}"),
        generated,
    }
}

fn is_capture_video(data_dir: &Path, source: &Path, child: &str) -> bool {
    child.eq_ignore_ascii_case(CAPTURES)
        && source
            .parent()
            .is_some_and(|p| same_path(p, &data_dir.join(CAPTURES)))
        && source
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("capture-"))
        && source
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("mp4"))
}

fn copy_asset(from: &Path, to: &Path, generated: bool) -> Result<(), &'static str> {
    if crate::secrets::is_machine_private(from) || crate::secrets::is_machine_private(to) {
        return Err("machine-private");
    }
    if cloud_blocked(from) {
        return Err("cloud placeholder");
    }
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).map_err(|_| "could not copy")?;
    }
    if !same_path(from, to) && !files_identical(from, to) {
        std::fs::copy(from, to).map_err(|_| "could not copy")?;
    }
    if generated {
        let _ = copy_or_seed_sidecar(from, to);
    }
    Ok(())
}

fn copy_or_seed_sidecar(from: &Path, to: &Path) -> io::Result<()> {
    let src = sidecar_path(from);
    let dest = sidecar_path(to);
    if src.is_file() && !cloud_blocked(&src) && !crate::secrets::is_machine_private(&src) {
        let bytes = std::fs::read(&src)?;
        if let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(&bytes) {
            scrub_sidecar(&mut value);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let json = serde_json::to_vec_pretty(&value).map_err(io::Error::other)?;
            return std::fs::write(dest, json);
        }
    }
    if dest.is_file() {
        return Ok(());
    }
    write_generated_sidecar(
        to,
        &GeneratedSidecar {
            prompt: String::new(),
            model: String::new(),
            seed: None,
            params: BTreeMap::new(),
            created: file_mtime_secs(from).unwrap_or(0),
            source_run: from.to_string_lossy().into_owned(),
        },
    )
}

fn cloud_blocked(path: &Path) -> bool {
    if crate::cloud::is_dehydrated(path) {
        return true;
    }
    crate::cloud::copy_cloud_cost(std::slice::from_ref(&path.to_path_buf())).files > 0
}

fn files_identical(from: &Path, to: &Path) -> bool {
    if same_path(from, to) {
        return true;
    }
    if !to.is_file() || crate::cloud::is_dehydrated(to) {
        return false;
    }
    let len = |path: &Path| std::fs::metadata(path).map(|meta| meta.len()).ok();
    if len(from) != len(to) {
        return false;
    }
    std::fs::read(from).ok() == std::fs::read(to).ok()
}

fn unique_dest(dir: &Path, file_name: &str, source: &Path) -> PathBuf {
    let direct = dir.join(file_name);
    if !direct.exists() || files_identical(source, &direct) {
        return direct;
    }
    let path = Path::new(file_name);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "image".into());
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().into_owned())
        .unwrap_or_default();
    for n in 2..1000 {
        let name = if ext.is_empty() {
            format!("{stem}-{n}")
        } else {
            format!("{stem}-{n}.{ext}")
        };
        let candidate = dir.join(name);
        if !candidate.exists() || files_identical(source, &candidate) {
            return candidate;
        }
    }
    direct
}

fn file_mtime_secs(path: &Path) -> Option<u64> {
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?;
    modified
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

fn refusal_counts(refused: &[RefusedAsset]) -> String {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for item in refused {
        *counts.entry(item.reason).or_default() += 1;
    }
    counts
        .into_iter()
        .map(|(reason, n)| format!("{n} {reason}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn secret_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase().replace('-', "_");
    key == "api_key"
        || key == "apikey"
        || key == "authorization"
        || key == "secret"
        || key == "password"
        || key == "credential"
        || key == "access_token"
        || key == "refresh_token"
        || key.ends_with("_api_key")
        || key.ends_with("_secret")
        || key.ends_with("_password")
        || key.ends_with("_token")
}

fn provider_component(provider: &str) -> String {
    single_component(provider, "generator").to_ascii_lowercase()
}

fn single_component(name: &str, fallback: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch == '/' || ch == '\\' || ch == '\0' {
            if !out.ends_with('-') {
                out.push('-');
            }
        } else {
            out.push(ch);
        }
    }
    let out = out.trim_matches(|c| c == '-' || c == '.').to_string();
    if out.is_empty() || out == "." || out == ".." {
        fallback.into()
    } else {
        out
    }
}

fn year_month_component(value: &str) -> String {
    let bytes = value.as_bytes();
    if bytes.len() == 7
        && bytes[4] == b'-'
        && bytes[..4].iter().all(u8::is_ascii_digit)
        && bytes[5..].iter().all(u8::is_ascii_digit)
    {
        value.to_string()
    } else {
        "unknown".into()
    }
}

fn is_raster(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            RASTER_EXT
                .iter()
                .any(|known| ext.eq_ignore_ascii_case(known))
        })
}

fn path_hash(source: &Path) -> u32 {
    source
        .to_string_lossy()
        .bytes()
        .fold(0xcbf2_9ce4_u32, |hash, byte| {
            hash.wrapping_mul(0x0100_0193) ^ u32::from(byte)
        })
}

fn locator_from_rel(rel: &Path) -> String {
    format!("{}/{}", ASSETS, rel.to_string_lossy().replace('\\', "/"))
}

fn first_component(root: &Path, path: &Path) -> Option<String> {
    if !under(root, path) {
        return None;
    }
    let skip = root.components().count();
    match path.components().nth(skip)? {
        Component::Normal(raw) => Some(raw.to_string_lossy().into_owned()),
        _ => None,
    }
}

fn strip_under(root: &Path, path: &Path) -> Option<PathBuf> {
    if !under(root, path) {
        return None;
    }
    let skip = root.components().count();
    let mut rel = PathBuf::new();
    for component in path.components().skip(skip) {
        rel.push(component);
    }
    if rel.as_os_str().is_empty() {
        None
    } else {
        Some(rel)
    }
}

/// UTC calendar date (proleptic Gregorian) of a Unix time.
fn civil_date(secs: u64) -> (i64, u32, u32) {
    let z = (secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

#[cfg(test)]
mod tests;
