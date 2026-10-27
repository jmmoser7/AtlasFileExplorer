//! File-only promotion of a complete native capture batch. Does not contact Rhino.
//! promote_native_captures APPROVED_PROJECT MANIFEST NEW_OUTPUT_DIRECTORY
use atlas_connector::{
    capture_xmp, parse_capture_xmp, view_for_restore, CaptureMetadata, SourceScope,
};
use image::{ImageDecoder, ImageFormat, ImageReader, RgbaImage};
use model_preview::view_meta::{hash_bytes, write_png_with_xmp};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    error::Error,
    fs::{self, OpenOptions},
    io::{Cursor, Read, Write},
    path::{Component, Path, PathBuf},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const MAX_JSON: usize = 1024 * 1024;
const MAX_PNG: usize = 32 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    complete: bool,
    original_state_restored: bool,
    format: String,
    source: SourceScope,
    captures: Vec<Capture>,
    people_layers: Vec<String>,
    offset: usize,
    next_offset: Option<usize>,
    #[serde(default)]
    error: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Capture {
    view_name: String,
    view_index: usize,
    people_visible: bool,
    png: PathBuf,
    metadata: PathBuf,
    width_px: u32,
    height_px: u32,
}
struct Prepared {
    entry: Capture,
    png: PathBuf,
    sidecar: PathBuf,
    png_hash: String,
    sidecar_hash: String,
    metadata: CaptureMetadata,
    pixels: RgbaImage,
    xmp: String,
}

fn unaliased(path: &Path) -> Result<()> {
    for ancestor in path.ancestors() {
        let metadata = fs::symlink_metadata(ancestor)?;
        if metadata.file_type().is_symlink() {
            return Err("Symlink paths are not permitted".into());
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            // Reparse, offline, recall-on-open, recall-on-data-access.
            if metadata.file_attributes() & (0x400 | 0x1000 | 0x40000 | 0x400000) != 0 {
                return Err("Reparse/cloud paths must be made local first".into());
            }
        }
    }
    Ok(())
}
fn inside(base: &Path, supplied: &Path) -> Result<PathBuf> {
    if supplied
        .components()
        .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
    {
        return Err("Relative traversal is not permitted".into());
    }
    let joined = if supplied.is_absolute() {
        supplied.to_path_buf()
    } else {
        base.join(supplied)
    };
    unaliased(&joined)?;
    let actual = joined.canonicalize()?;
    if !actual.starts_with(base) || !actual.is_file() {
        return Err("Capture input must be a regular file inside its approved directory".into());
    }
    Ok(actual)
}
fn bounded(path: &Path, maximum: usize) -> Result<Vec<u8>> {
    let file = fs::File::open(path)?;
    if !file.metadata()?.is_file() || file.metadata()?.len() > maximum as u64 {
        return Err("Input exceeds its regular-file size bound".into());
    }
    let mut bytes = Vec::new();
    file.take(maximum as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > maximum {
        return Err("Input grew beyond its size bound".into());
    }
    Ok(bytes)
}
fn decode(bytes: &[u8], width: u32, height: u32) -> Result<RgbaImage> {
    if width == 0 || height == 0 || width > 4096 || height > 4096 {
        return Err("Capture dimensions exceed promotion bounds".into());
    }
    let mut reader = ImageReader::with_format(Cursor::new(bytes), ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let pixels = reader.decode()?.into_rgba8();
    if pixels.dimensions() != (width, height) {
        return Err("PNG and sidecar dimensions differ".into());
    }
    Ok(pixels)
}
fn prepare(
    project: &Path,
    manifest_path: &Path,
) -> Result<(PathBuf, String, Manifest, Vec<Prepared>)> {
    let path = inside(project, manifest_path)?;
    let bytes = bounded(&path, MAX_JSON)?;
    let manifest: Manifest = serde_json::from_slice(&bytes)?;
    if !manifest.complete
        || !manifest.original_state_restored
        || manifest.error.is_some()
        || manifest.format != "raw_png_and_native_metadata_sidecar"
        || manifest.captures.is_empty()
        || manifest.captures.len() > 4
        || manifest.people_layers.is_empty()
        || manifest.people_layers.len() > 64
    {
        return Err(
            "Only a complete, restored, bounded native capture batch can be promoted".into(),
        );
    }
    let parent = path.parent().ok_or("Manifest needs a parent")?;
    let mut prepared = Vec::new();
    let mut total = 0usize;
    let mut total_pixels = 0u64;
    let mut seen = std::collections::HashSet::new();
    for capture in &manifest.captures {
        if capture.view_name.is_empty()
            || capture.view_name.len() > 512
            || !seen.insert((capture.view_index, capture.people_visible))
        {
            return Err("Batch contains invalid or repeated capture identities".into());
        }
        let png = inside(parent, &capture.png)?;
        let sidecar = inside(parent, &capture.metadata)?;
        let png_bytes = bounded(&png, MAX_PNG)?;
        total += png_bytes.len();
        if total > 64 * 1024 * 1024 {
            return Err("Batch PNG byte budget exceeded".into());
        }
        let sidecar_bytes = bounded(&sidecar, MAX_JSON)?;
        let metadata: CaptureMetadata = serde_json::from_slice(&sidecar_bytes)?;
        if metadata.source != manifest.source
            || metadata.source != metadata.view.source
            || metadata.width_px != capture.width_px
            || metadata.height_px != capture.height_px
        {
            return Err("Capture, view, dimensions and batch source must match".into());
        }
        total_pixels += u64::from(metadata.width_px) * u64::from(metadata.height_px);
        if total_pixels > 16_777_216 {
            return Err("Batch decoded-pixel budget exceeded".into());
        }
        let xmp = capture_xmp(&metadata)?;
        view_for_restore(&metadata, &metadata.source, None)?;
        let pixels = decode(&png_bytes, metadata.width_px, metadata.height_px)?;
        prepared.push(Prepared {
            entry: Capture {
                view_name: capture.view_name.clone(),
                view_index: capture.view_index,
                people_visible: capture.people_visible,
                png: capture.png.clone(),
                metadata: capture.metadata.clone(),
                width_px: capture.width_px,
                height_px: capture.height_px,
            },
            png,
            sidecar,
            png_hash: hash_bytes(&png_bytes),
            sidecar_hash: hash_bytes(&sidecar_bytes),
            metadata,
            pixels,
            xmp,
        });
    }
    Ok((path, hash_bytes(&bytes), manifest, prepared))
}
fn output_path(project: &Path, supplied: &Path) -> Result<PathBuf> {
    if supplied
        .components()
        .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
    {
        return Err("Output traversal is not permitted".into());
    }
    let joined = if supplied.is_absolute() {
        supplied.to_path_buf()
    } else {
        project.join(supplied)
    };
    let parent = joined.parent().ok_or("Output needs an existing parent")?;
    unaliased(parent)?;
    let parent = parent.canonicalize()?;
    if !parent.starts_with(project) {
        return Err("Output must stay inside the approved project".into());
    }
    let output = parent.join(joined.file_name().ok_or("Output needs a name")?);
    if output.exists() {
        return Err("Output directory must be new".into());
    }
    Ok(output)
}
fn promote(project: &Path, manifest: &Path, output: &Path) -> Result<Value> {
    unaliased(project)?;
    let project = project.canonicalize()?;
    if !project.is_dir() {
        return Err("Approved project must be an existing local directory".into());
    }
    let (manifest_path, manifest_hash, manifest, captures) = prepare(&project, manifest)?;
    let output = output_path(&project, output)?;
    fs::create_dir(&output)?;
    let mut records = Vec::new();
    for (index, capture) in captures.into_iter().enumerate() {
        // The source bytes must remain the same through promotion; no live model is consulted.
        if hash_bytes(&bounded(&capture.png, MAX_PNG)?) != capture.png_hash
            || hash_bytes(&bounded(&capture.sidecar, MAX_JSON)?) != capture.sidecar_hash
        {
            return Err("Capture inputs changed during promotion".into());
        }
        let filename = format!("capture-{index:04}-native.png");
        let path = output.join(&filename);
        let staging = output.join(format!("{filename}.pending"));
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staging)?;
        write_png_with_xmp(
            &staging,
            capture.pixels.as_raw(),
            capture.metadata.width_px,
            capture.metadata.height_px,
            &capture.xmp,
        )?;
        let promoted = bounded(&staging, MAX_PNG)?;
        let mut decoder =
            ImageReader::with_format(Cursor::new(&promoted), ImageFormat::Png).into_decoder()?;
        let embedded = decoder.xmp_metadata()?.ok_or("Promoted PNG lost its XMP")?;
        if parse_capture_xmp(std::str::from_utf8(&embedded)?)?.as_ref() != Some(&capture.metadata)
            || decode(
                &promoted,
                capture.metadata.width_px,
                capture.metadata.height_px,
            )? != capture.pixels
        {
            return Err("Promoted pixels or native metadata did not roundtrip".into());
        }
        fs::rename(&staging, &path)?;
        records.push(json!({"view_name":capture.entry.view_name,"view_index":capture.entry.view_index,
            "people_visible":capture.entry.people_visible,"raw_png":capture.png,"raw_png_sha256":capture.png_hash,
            "sidecar":capture.sidecar,"sidecar_sha256":capture.sidecar_hash,"promoted_png":path,
            "promoted_png_sha256":hash_bytes(&promoted),"pixel_sha256":hash_bytes(capture.pixels.as_raw()),
            "bytes":promoted.len(),"width_px":capture.metadata.width_px,"height_px":capture.metadata.height_px}));
    }
    let evidence = json!({"schema":"slate-native-capture-promotion-v1","complete":true,"manifest":manifest_path,
        "manifest_sha256":manifest_hash,"source":manifest.source,"offset":manifest.offset,"next_offset":manifest.next_offset,
        "captures":records,"metadata":"Original native source/view metadata embedded unchanged; no new durable file fingerprint",
        "recall_scope":"Existing native camera/projection/display recall; people-layer state is represented by pixels, not added to view metadata"});
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output.join("promotion.json"))?;
    file.write_all(&serde_json::to_vec_pretty(&evidence)?)?;
    file.sync_all()?;
    Ok(evidence)
}
fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 3 {
        eprintln!("usage: promote_native_captures APPROVED_PROJECT MANIFEST NEW_OUTPUT_DIRECTORY");
        std::process::exit(2);
    }
    match promote(
        Path::new(&args[0]),
        Path::new(&args[1]),
        Path::new(&args[2]),
    ) {
        Ok(result) => println!(
            "{}",
            serde_json::to_string_pretty(&result).expect("JSON evidence")
        ),
        Err(error) => {
            eprintln!("Native capture promotion failed: {error}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scoped_batch_promotes_losslessly_and_refuses_escape_or_mismatch() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "slate-capture-promotion-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&root).unwrap();
        let pixels = image::RgbaImage::from_pixel(2, 1, image::Rgba([17, 31, 211, 255]));
        pixels.save(root.join("raw.png")).unwrap();
        let source = json!({"source_id":"fixture","revision":"r1"});
        let mut meta = json!({"source":source,"view":{"source":source,"schema":"org.slate.rhino.view","schema_version":1,
            "fidelity":"exact_native","state":{"camera":"fixture"}},"captured_at":"2026-10-26T15:00:00Z",
            "width_px":2,"height_px":1,"consistency":"atomic"});
        fs::write(
            root.join("raw.native.json"),
            serde_json::to_vec(&meta).unwrap(),
        )
        .unwrap();
        let manifest = json!({"complete":true,"original_state_restored":true,"format":"raw_png_and_native_metadata_sidecar",
            "source":source,"captures":[{"view_name":"Test","view_index":0,"people_visible":true,"png":"raw.png",
            "metadata":"raw.native.json","width_px":2,"height_px":1}],"people_layers":["People"],"offset":0,"next_offset":null});
        fs::write(
            root.join("manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        let original = fs::read(root.join("raw.png")).unwrap();
        assert!(promote(&root, Path::new("manifest.json"), Path::new("promoted")).is_ok());
        assert_eq!(fs::read(root.join("raw.png")).unwrap(), original);
        assert!(promote(&root, Path::new("manifest.json"), Path::new("promoted")).is_err());
        assert!(inside(&root.canonicalize().unwrap(), Path::new("../raw.png")).is_err());
        let outside = root.with_extension("outside.png");
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&outside)
            .unwrap();
        assert!(inside(&root.canonicalize().unwrap(), &outside).is_err());
        fs::remove_file(&outside).unwrap();
        meta["view"]["source"]["revision"] = json!("different");
        fs::write(
            root.join("raw.native.json"),
            serde_json::to_vec(&meta).unwrap(),
        )
        .unwrap();
        assert!(promote(&root, Path::new("manifest.json"), Path::new("rejected")).is_err());
        assert!(!root.join("rejected").exists());
        for path in [
            "promoted/capture-0000-native.png",
            "promoted/promotion.json",
            "raw.png",
            "raw.native.json",
            "manifest.json",
        ] {
            fs::remove_file(root.join(path)).unwrap();
        }
        fs::remove_dir(root.join("promoted")).unwrap();
        fs::remove_dir(root).unwrap();
    }
}
