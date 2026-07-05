use eframe::egui::Color32;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Family {
    Image,
    Video,
    Audio,
    Doc,
    Design,
    Cad,
    Code,
    Archive,
    Data,
    Other,
}

pub const FAMILIES: [Family; 10] = [
    Family::Image,
    Family::Video,
    Family::Audio,
    Family::Doc,
    Family::Design,
    Family::Cad,
    Family::Code,
    Family::Archive,
    Family::Data,
    Family::Other,
];

/// A filterable sub-classification within a [`Family`] (e.g. JPEG under Images).
#[derive(Clone, Copy, Debug)]
pub struct ExtGroup {
    pub label: &'static str,
    pub exts: &'static [&'static str],
}

impl Family {
    pub fn idx(self) -> usize {
        FAMILIES.iter().position(|f| *f == self).unwrap()
    }

    /// Fine-grained extension buckets shown under the family row when applicable.
    pub fn ext_groups(self) -> &'static [ExtGroup] {
        match self {
            Family::Image => &[
                ExtGroup {
                    label: "JPEG",
                    exts: &["jpg", "jpeg"],
                },
                ExtGroup {
                    label: "PNG",
                    exts: &["png"],
                },
                ExtGroup {
                    label: "TIFF",
                    exts: &["tif", "tiff"],
                },
                ExtGroup {
                    label: "GIF",
                    exts: &["gif"],
                },
                ExtGroup {
                    label: "WebP",
                    exts: &["webp"],
                },
                ExtGroup {
                    label: "SVG",
                    exts: &["svg"],
                },
                ExtGroup {
                    label: "HEIC / AVIF",
                    exts: &["heic", "avif"],
                },
                ExtGroup {
                    label: "RAW",
                    exts: &["dng", "raw", "cr2", "nef", "arw"],
                },
                ExtGroup {
                    label: "Other",
                    exts: &["bmp", "ico", "tga", "exr", "hdr"],
                },
            ],
            Family::Video => &[
                ExtGroup {
                    label: "MP4",
                    exts: &["mp4", "m4v"],
                },
                ExtGroup {
                    label: "MOV",
                    exts: &["mov"],
                },
                ExtGroup {
                    label: "MKV",
                    exts: &["mkv"],
                },
                ExtGroup {
                    label: "WebM",
                    exts: &["webm"],
                },
                ExtGroup {
                    label: "AVI",
                    exts: &["avi"],
                },
                ExtGroup {
                    label: "MPEG",
                    exts: &["mpg", "mpeg", "mts", "m2ts"],
                },
                ExtGroup {
                    label: "WMV / FLV",
                    exts: &["wmv", "flv"],
                },
            ],
            Family::Audio => &[
                ExtGroup {
                    label: "MP3",
                    exts: &["mp3"],
                },
                ExtGroup {
                    label: "WAV",
                    exts: &["wav"],
                },
                ExtGroup {
                    label: "FLAC",
                    exts: &["flac"],
                },
                ExtGroup {
                    label: "AAC / M4A",
                    exts: &["aac", "m4a"],
                },
                ExtGroup {
                    label: "OGG",
                    exts: &["ogg"],
                },
                ExtGroup {
                    label: "Other",
                    exts: &["aiff", "aif", "wma", "mid"],
                },
            ],
            Family::Doc => &[
                ExtGroup {
                    label: "PDF",
                    exts: &["pdf"],
                },
                ExtGroup {
                    label: "Word",
                    exts: &["doc", "docx", "odt"],
                },
                ExtGroup {
                    label: "Excel",
                    exts: &["xls", "xlsx", "ods", "numbers"],
                },
                ExtGroup {
                    label: "PowerPoint",
                    exts: &["ppt", "pptx", "odp", "key"],
                },
                ExtGroup {
                    label: "Text",
                    exts: &["txt", "md", "rtf"],
                },
                ExtGroup {
                    label: "eBook",
                    exts: &["epub"],
                },
                ExtGroup {
                    label: "Other",
                    exts: &["pages", "one"],
                },
            ],
            Family::Design => &[
                ExtGroup {
                    label: "Photoshop",
                    exts: &["psd", "psb"],
                },
                ExtGroup {
                    label: "Illustrator",
                    exts: &["ai", "eps"],
                },
                ExtGroup {
                    label: "InDesign",
                    exts: &["indd"],
                },
                ExtGroup {
                    label: "Figma",
                    exts: &["fig"],
                },
                ExtGroup {
                    label: "Sketch",
                    exts: &["sketch"],
                },
                ExtGroup {
                    label: "Adobe XD",
                    exts: &["xd"],
                },
                ExtGroup {
                    label: "Affinity",
                    exts: &["afdesign", "afphoto"],
                },
                ExtGroup {
                    label: "Corel",
                    exts: &["cdr"],
                },
            ],
            Family::Cad => &[
                ExtGroup {
                    label: "Rhino",
                    exts: &["3dm", "3dmbak"],
                },
                ExtGroup {
                    label: "AutoCAD",
                    exts: &["dwg", "dxf"],
                },
                ExtGroup {
                    label: "SketchUp",
                    exts: &["skp"],
                },
                ExtGroup {
                    label: "Blender",
                    exts: &["blend"],
                },
                ExtGroup {
                    label: "Mesh / exchange",
                    exts: &["obj", "stl", "fbx", "gltf", "glb", "3ds"],
                },
                ExtGroup {
                    label: "STEP / IGES",
                    exts: &["step", "stp", "iges", "igs"],
                },
                ExtGroup {
                    label: "BIM",
                    exts: &["rvt", "rfa", "ifc"],
                },
                ExtGroup {
                    label: "Other",
                    exts: &[
                        "gh", "ghx", "sldprt", "sldasm", "sat", "usd", "usdz", "max", "c4d",
                    ],
                },
            ],
            Family::Code => &[
                ExtGroup {
                    label: "Rust",
                    exts: &["rs"],
                },
                ExtGroup {
                    label: "JavaScript / TS",
                    exts: &["js", "ts", "jsx", "tsx"],
                },
                ExtGroup {
                    label: "Python",
                    exts: &["py"],
                },
                ExtGroup {
                    label: "Web",
                    exts: &["html", "htm", "css", "scss"],
                },
                ExtGroup {
                    label: "C / C++",
                    exts: &["c", "cpp", "h", "hpp"],
                },
                ExtGroup {
                    label: "Other",
                    exts: &[
                        "cs", "java", "go", "rb", "php", "sh", "ps1", "bat", "cmd", "lua", "swift",
                        "kt", "vb", "sql",
                    ],
                },
            ],
            Family::Archive => &[
                ExtGroup {
                    label: "ZIP",
                    exts: &["zip"],
                },
                ExtGroup {
                    label: "RAR / 7z",
                    exts: &["rar", "7z"],
                },
                ExtGroup {
                    label: "Tarballs",
                    exts: &["tar", "gz", "bz2", "xz"],
                },
                ExtGroup {
                    label: "Disk images",
                    exts: &["iso", "dmg"],
                },
                ExtGroup {
                    label: "Other",
                    exts: &["cab"],
                },
            ],
            Family::Data => &[
                ExtGroup {
                    label: "CSV / TSV",
                    exts: &["csv", "tsv"],
                },
                ExtGroup {
                    label: "JSON / XML",
                    exts: &["json", "xml"],
                },
                ExtGroup {
                    label: "YAML / TOML",
                    exts: &["yaml", "yml", "toml"],
                },
                ExtGroup {
                    label: "Config / logs",
                    exts: &["ini", "log"],
                },
                ExtGroup {
                    label: "Database",
                    exts: &["db", "sqlite", "parquet"],
                },
            ],
            Family::Other => &[],
        }
    }

    /// Returns the sub-group label for `ext` within this family, if any.
    pub fn ext_group_label(self, ext: &str) -> Option<&'static str> {
        for group in self.ext_groups() {
            if group.exts.contains(&ext) {
                return Some(group.label);
            }
        }
        None
    }

    /// Stable id for persisting sub-type filter state in the UI.
    pub fn ext_group_id(self, group: &ExtGroup) -> String {
        format!("{}:{}", self.idx(), group.label)
    }

    pub fn label(self) -> &'static str {
        match self {
            Family::Image => "Images",
            Family::Video => "Video",
            Family::Audio => "Audio",
            Family::Doc => "Documents",
            Family::Design => "Design",
            Family::Cad => "3D / CAD",
            Family::Code => "Code",
            Family::Archive => "Archives",
            Family::Data => "Data",
            Family::Other => "Other",
        }
    }

    pub fn color(self) -> Color32 {
        match self {
            Family::Image => Color32::from_rgb(0x4f, 0xc3, 0xf7),
            Family::Video => Color32::from_rgb(0xba, 0x68, 0xc8),
            Family::Audio => Color32::from_rgb(0x4d, 0xb6, 0xac),
            Family::Doc => Color32::from_rgb(0xff, 0xb7, 0x4d),
            Family::Design => Color32::from_rgb(0xf0, 0x62, 0x92),
            Family::Cad => Color32::from_rgb(0xae, 0xd5, 0x81),
            Family::Code => Color32::from_rgb(0x90, 0xa4, 0xae),
            Family::Archive => Color32::from_rgb(0xa1, 0x88, 0x7f),
            Family::Data => Color32::from_rgb(0xdc, 0xe7, 0x75),
            Family::Other => Color32::from_rgb(0x8d, 0x8d, 0x8d),
        }
    }

    pub fn from_ext(ext: &str) -> Family {
        match ext {
            "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "tif" | "tiff" | "svg" | "heic"
            | "avif" | "ico" | "tga" | "exr" | "hdr" | "dng" | "raw" | "cr2" | "nef" | "arw" => {
                Family::Image
            }
            "mp4" | "mov" | "avi" | "mkv" | "webm" | "m4v" | "mpg" | "mpeg" | "wmv" | "flv"
            | "mts" | "m2ts" => Family::Video,
            "mp3" | "wav" | "flac" | "aac" | "ogg" | "m4a" | "aiff" | "aif" | "wma" | "mid" => {
                Family::Audio
            }
            "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "md" | "rtf"
            | "odt" | "ods" | "odp" | "pages" | "key" | "numbers" | "one" | "epub" => Family::Doc,
            "psd" | "psb" | "ai" | "indd" | "sketch" | "fig" | "xd" | "afdesign" | "afphoto"
            | "eps" | "cdr" => Family::Design,
            "3dm" | "3ds" | "obj" | "stl" | "fbx" | "blend" | "skp" | "dwg" | "dxf" | "step"
            | "stp" | "iges" | "igs" | "gh" | "ghx" | "sldprt" | "sldasm" | "sat" | "gltf"
            | "glb" | "usd" | "usdz" | "ifc" | "rvt" | "rfa" | "max" | "c4d" | "3dmbak" => {
                Family::Cad
            }
            "rs" | "js" | "ts" | "tsx" | "jsx" | "py" | "html" | "htm" | "css" | "scss" | "c"
            | "cpp" | "h" | "hpp" | "cs" | "java" | "go" | "rb" | "php" | "sh" | "ps1" | "bat"
            | "cmd" | "lua" | "swift" | "kt" | "vb" | "sql" => Family::Code,
            "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" | "iso" | "cab" | "dmg" => {
                Family::Archive
            }
            "csv" | "tsv" | "json" | "xml" | "yaml" | "yml" | "toml" | "ini" | "db" | "sqlite"
            | "parquet" | "log" => Family::Data,
            _ => Family::Other,
        }
    }
}

#[derive(Clone, Debug)]
pub struct FileEntry {
    pub path: PathBuf,
    /// Path relative to the scan root, backslash separated.
    pub rel: String,
    pub name: String,
    pub name_lc: String,
    pub ext: String,
    pub size: u64,
    pub mtime: i64,
    /// Platform creation / birth time when available.
    pub ctime: i64,
    /// Owner account name (e.g. `jmoser`), empty when unavailable.
    pub owner: String,
    pub family: Family,
    /// Tombstone set by the filesystem watcher when the file disappears.
    pub dead: bool,
}

impl FileEntry {
    pub fn from_abs(
        root: &Path,
        path: PathBuf,
        size: u64,
        mtime: i64,
        ctime: i64,
        owner: String,
    ) -> Option<FileEntry> {
        let rel = path.strip_prefix(root).ok()?.to_string_lossy().into_owned();
        // `rel` is backslash-separated everywhere (tree building, cache keys,
        // and the SQLite index all assume it), so normalize on non-Windows.
        #[cfg(not(windows))]
        let rel = rel.replace('/', "\\");
        let name = path.file_name()?.to_string_lossy().into_owned();
        Some(Self::build(path, rel, name, size, mtime, ctime, owner))
    }

    pub fn from_rel(
        root: &Path,
        rel: String,
        size: u64,
        mtime: i64,
        ctime: i64,
        owner: String,
    ) -> FileEntry {
        #[cfg(windows)]
        let path = root.join(&rel);
        #[cfg(not(windows))]
        let path = root.join(rel.replace('\\', "/"));
        let name = rel
            .rsplit(['\\', '/'])
            .next()
            .unwrap_or(rel.as_str())
            .to_string();
        Self::build(path, rel, name, size, mtime, ctime, owner)
    }

    fn build(
        path: PathBuf,
        rel: String,
        name: String,
        size: u64,
        mtime: i64,
        ctime: i64,
        owner: String,
    ) -> FileEntry {
        let ext = match name.rsplit_once('.') {
            Some((stem, e)) if !stem.is_empty() => e.to_ascii_lowercase(),
            _ => String::new(),
        };
        let family = Family::from_ext(&ext);
        let name_lc = name.to_lowercase();
        FileEntry {
            path,
            rel,
            name,
            name_lc,
            ext,
            size,
            mtime,
            ctime,
            owner,
            family,
            dead: false,
        }
    }
}

pub const SECS_PER_DAY: i64 = 86_400;
pub const SECS_PER_HOUR: i64 = 3_600;
pub const SECS_PER_MINUTE: i64 = 60;

/// Days since Unix epoch (UTC, day precision).
pub fn day_index(secs: i64) -> i64 {
    secs.div_euclid(SECS_PER_DAY)
}

pub fn day_start(secs: i64) -> i64 {
    day_index(secs) * SECS_PER_DAY
}

pub fn hour_start(secs: i64) -> i64 {
    secs.div_euclid(SECS_PER_HOUR) * SECS_PER_HOUR
}

pub fn snap_to_step(secs: i64, step: i64) -> i64 {
    if step <= 0 {
        return secs;
    }
    ((secs as f64 / step as f64).round() as i64) * step
}

/// Calendar parts from unix seconds (UTC, days precision).
pub fn ymd_from_secs(secs: i64) -> (i32, u32, u32) {
    let s = date_string(secs);
    let y = s.get(0..4).and_then(|x| x.parse().ok()).unwrap_or(1970);
    let m = s.get(5..7).and_then(|x| x.parse().ok()).unwrap_or(1);
    let d = s.get(8..10).and_then(|x| x.parse().ok()).unwrap_or(1);
    (y, m, d)
}

const MONTH_SHORT: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

pub fn month_short(m: u32) -> &'static str {
    MONTH_SHORT
        .get(m.saturating_sub(1) as usize)
        .copied()
        .unwrap_or("???")
}

/// Human-readable label for a timeline tick at the given step width.
pub fn timeline_tick_label(secs: i64, step_secs: i64) -> String {
    let (y, m, d) = ymd_from_secs(secs);
    if step_secs >= 365 * SECS_PER_DAY {
        return format!("{y}");
    }
    if step_secs >= 28 * SECS_PER_DAY {
        return format!("{} {y}", month_short(m));
    }
    if step_secs >= SECS_PER_DAY {
        return format!("{} {}", month_short(m), d);
    }
    if step_secs >= SECS_PER_HOUR {
        let h = (secs - day_start(secs)) / SECS_PER_HOUR;
        return format!("{h:02}:00");
    }
    let h = (secs - day_start(secs)) / SECS_PER_HOUR;
    let min = (secs % SECS_PER_HOUR) / SECS_PER_MINUTE;
    format!("{h:02}:{min:02}")
}

/// Range readout under the timeline rail.
pub fn timeline_range_caption(lo: i64, hi: i64, snap_secs: i64) -> String {
    if snap_secs >= SECS_PER_DAY {
        if lo == hi {
            date_string(lo)
        } else {
            format!("{} — {}", date_string(lo), date_string(hi))
        }
    } else {
        let fmt = |t: i64| {
            if snap_secs >= SECS_PER_HOUR {
                format!(
                    "{} {}",
                    date_string(t),
                    timeline_tick_label(t, SECS_PER_HOUR)
                )
            } else {
                format!("{} {}", date_string(t), timeline_tick_label(t, 900))
            }
        };
        format!("{} — {}", fmt(lo), fmt(hi))
    }
}

pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{} {}", bytes, UNITS[u])
    } else {
        format!("{:.1} {}", v, UNITS[u])
    }
}

/// Days-precision date from unix seconds ("2026-07-02"), no chrono dependency.
pub fn date_string(secs: i64) -> String {
    let days = secs.div_euclid(86400);
    // Howard Hinnant's civil_from_days algorithm.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{:04}-{:02}-{:02}", y, m, d)
}

pub fn age_string(mtime: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let dt = (now - mtime).max(0);
    if dt < 3600 {
        format!("{}m ago", dt / 60)
    } else if dt < 86_400 {
        format!("{}h ago", dt / 3600)
    } else if dt < 32 * 86_400 {
        format!("{}d ago", dt / 86_400)
    } else {
        date_string(mtime)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ext_group_labels_cover_common_image_formats() {
        assert_eq!(Family::Image.ext_group_label("jpg"), Some("JPEG"));
        assert_eq!(Family::Image.ext_group_label("jpeg"), Some("JPEG"));
        assert_eq!(Family::Image.ext_group_label("png"), Some("PNG"));
        assert_eq!(Family::Image.ext_group_label("tiff"), Some("TIFF"));
    }

    #[test]
    fn ext_group_id_is_stable() {
        let group = &Family::Doc.ext_groups()[0];
        assert_eq!(Family::Doc.ext_group_id(group), "3:PDF");
    }

    #[test]
    fn snap_to_step_rounds_to_nearest_hour() {
        // 20 minutes past → snaps down; 40 minutes past → snaps up.
        // (Exact midpoints are avoided: `f64::round` ties away from zero.)
        let base = day_start(20_000);
        assert_eq!(snap_to_step(base + 20 * 60, SECS_PER_HOUR), base);
        assert_eq!(
            snap_to_step(base + 40 * 60, SECS_PER_HOUR),
            base + SECS_PER_HOUR
        );
    }

    #[test]
    fn timeline_tick_label_scales_with_step() {
        let noon = day_start(20_000) + 12 * SECS_PER_HOUR;
        assert_eq!(timeline_tick_label(noon, SECS_PER_HOUR), "12:00");
        assert_eq!(
            timeline_tick_label(noon + 15 * SECS_PER_MINUTE, 15 * SECS_PER_MINUTE),
            "12:15"
        );
        assert_eq!(timeline_tick_label(noon, 365 * SECS_PER_DAY), "1970");
    }
}
