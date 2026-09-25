//! Slate viewport capture metadata (XMP `slateview` packet) embedded in PNG,
//! JPEG, and WebP exports. Pure Rust — no UI dependencies.

use image::{ImageDecoder, ImageEncoder};
use slate_doc::scene::{ModelCamera, ModelDisplay};
use std::path::Path;

/// Vertical field of view recorded for Slate perspective captures.
pub const FOV_Y: f32 = 0.6108652;
pub const VIEW_VERSION: u32 = 1;
pub const VIEW_NS: &str = "https://slate.app/ns/view/1.0/";
const CREATOR: &str = "Slate";

#[derive(Debug, Clone, PartialEq)]
pub struct ViewMetaInput {
    pub camera: ModelCamera,
    pub eye: [f32; 3],
    pub up: [f32; 3],
    pub aspect: f32,
    pub width: u32,
    pub height: u32,
    pub model_name: String,
    /// Workbook-relative locator; must not be absolute.
    pub model_path: String,
    pub model_hash: String,
    pub model_size: u64,
    pub node_id: u64,
    pub created_unix: i64,
    /// Optional `ImageAdjust::cache_hash()` baked into the export pixels (D39).
    pub image_adjust_hash: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ViewMetaParsed {
    pub camera: ModelCamera,
    pub model_name: String,
    pub model_path: String,
    pub model_hash: String,
    pub model_size: u64,
    pub node_id: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ViewMetaError {
    UnsupportedVersion(u32),
    MissingField(&'static str),
    InvalidPath,
    Parse(String),
    Io(String),
    Image(String),
}

impl std::fmt::Display for ViewMetaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedVersion(v) => write!(f, "unsupported slateview version {v}"),
            Self::MissingField(k) => write!(f, "missing slateview field {k}"),
            Self::InvalidPath => write!(f, "model path must be relative"),
            Self::Parse(s) => write!(f, "{s}"),
            Self::Io(s) => write!(f, "{s}"),
            Self::Image(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for ViewMetaError {}

pub fn hash_bytes(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

pub fn build_xmp_packet(input: &ViewMetaInput) -> Result<String, ViewMetaError> {
    if !input.model_path.is_empty() && path_looks_absolute(&input.model_path) {
        return Err(ViewMetaError::InvalidPath);
    }
    let display = input.camera.display.key();
    let target = fmt3(input.camera.target);
    let eye = fmt3(input.eye);
    let up = fmt3(input.up);
    let created = format_xmp_date(input.created_unix);
    let adjust_hash = input
        .image_adjust_hash
        .map(|h| format!(r#" slateview:imageAdjustHash="{h}""#))
        .unwrap_or_default();
    let model_path_attr = if input.model_path.is_empty() {
        String::new()
    } else {
        format!(
            r#"
    slateview:modelPath="{}""#,
            xml_escape(&input.model_path)
        )
    };
    Ok(format!(
        r#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
    xmlns:slateview="{VIEW_NS}"
    xmlns:xmp="http://ns.adobe.com/xap/1.0/"
    xmlns:tiff="http://ns.adobe.com/tiff/1.0/"
    slateview:version="{VIEW_VERSION}"
    slateview:target="{target}"
    slateview:yaw="{yaw}"
    slateview:pitch="{pitch}"
    slateview:distance="{distance}"
    slateview:display="{display}"
    slateview:eye="{eye}"
    slateview:up="{up}"
    slateview:projection="perspective"
    slateview:fovY="{fov}"
    slateview:aspect="{aspect}"
    slateview:modelName="{model_name}"{model_path_attr}
    slateview:modelHash="{model_hash}"
    slateview:modelSize="{model_size}"
    slateview:nodeId="{node_id}"
    xmp:CreatorTool="{CREATOR}"
    xmp:CreateDate="{created}"
    tiff:ImageWidth="{w}"
    tiff:ImageLength="{h}"{adjust_hash}/>
 </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>"#,
        yaw = input.camera.yaw,
        pitch = input.camera.pitch,
        distance = input.camera.distance,
        fov = FOV_Y,
        aspect = input.aspect,
        model_name = xml_escape(&input.model_name),
        model_path_attr = model_path_attr,
        model_hash = xml_escape(&input.model_hash),
        model_size = input.model_size,
        node_id = input.node_id,
        w = input.width,
        h = input.height,
    ))
}

fn slateview_attrs(xmp: &str) -> Result<std::collections::HashMap<String, String>, ViewMetaError> {
    let doc = roxmltree::Document::parse(xmp).map_err(|e| ViewMetaError::Parse(e.to_string()))?;
    let mut out = std::collections::HashMap::new();
    for node in doc.descendants() {
        for attr in node.attributes() {
            if attr.namespace() != Some(VIEW_NS) {
                continue;
            }
            out.insert(attr.name().to_string(), attr.value().to_string());
        }
    }
    Ok(out)
}

fn attr_field<'a>(
    attrs: &'a std::collections::HashMap<String, String>,
    key: &'static str,
) -> Result<&'a str, ViewMetaError> {
    attrs
        .get(key)
        .map(|s| s.as_str())
        .ok_or(ViewMetaError::MissingField(key))
}

pub fn parse_xmp_packet(xmp: &str) -> Result<ViewMetaParsed, ViewMetaError> {
    let attrs = slateview_attrs(xmp)?;
    let version = parse_u32(attr_field(&attrs, "version")?, "version")?;
    let target = parse3(attr_field(&attrs, "target")?, "target")?;
    let yaw = parse_f32(attr_field(&attrs, "yaw")?, "yaw")?;
    let pitch = parse_f32(attr_field(&attrs, "pitch")?, "pitch")?;
    let distance = parse_f32(attr_field(&attrs, "distance")?, "distance")?;
    let display = attrs.get("display").map(|s| s.as_str());
    let model_name = attrs.get("modelName").cloned();
    let model_path = attrs.get("modelPath").cloned().unwrap_or_default();
    let model_hash = attrs.get("modelHash").cloned().unwrap_or_default();
    let model_size = attrs
        .get("modelSize")
        .map(|s| parse_u64(s, "modelSize"))
        .transpose()?
        .unwrap_or(0);
    let node_id = attrs
        .get("nodeId")
        .map(|s| parse_u64(s, "nodeId"))
        .transpose()?
        .unwrap_or(0);
    if version > VIEW_VERSION {
        return Err(ViewMetaError::UnsupportedVersion(version));
    }
    if !model_path.is_empty() && path_looks_absolute(&model_path) {
        return Err(ViewMetaError::InvalidPath);
    }
    let display = display
        .as_deref()
        .and_then(ModelDisplay::from_key)
        .unwrap_or(ModelDisplay::Shaded);
    Ok(ViewMetaParsed {
        camera: ModelCamera {
            target,
            yaw,
            pitch,
            distance,
            display,
        },
        model_name: model_name.unwrap_or_default(),
        model_path,
        model_hash,
        model_size,
        node_id,
    })
}

/// Read XMP from one local image file.
///
/// The caller must establish that `path` is not a dehydrated cloud placeholder
/// before calling. This pure crate deliberately has no platform cloud-policy
/// dependency.
pub fn read_view_meta(path: &Path) -> Result<Option<ViewMetaParsed>, ViewMetaError> {
    let xmp = read_xmp_via_image(path)?;
    let Some(xmp) = xmp else {
        return Ok(None);
    };
    parse_xmp_packet(&xmp).map(Some)
}

fn read_xmp_via_image(path: &Path) -> Result<Option<String>, ViewMetaError> {
    use image::ImageReader;
    let reader = ImageReader::open(path).map_err(|e| ViewMetaError::Io(e.to_string()))?;
    let mut decoder = reader
        .into_decoder()
        .map_err(|e| ViewMetaError::Image(e.to_string()))?;
    match decoder.xmp_metadata() {
        Ok(Some(bytes)) => {
            let text =
                std::str::from_utf8(&bytes).map_err(|e| ViewMetaError::Parse(e.to_string()))?;
            Ok(Some(text.to_string()))
        }
        Ok(None) => Ok(None),
        Err(e) => Err(ViewMetaError::Image(e.to_string())),
    }
}

pub fn write_png_with_xmp(
    path: &Path,
    rgba: &[u8],
    w: u32,
    h: u32,
    xmp: &str,
) -> Result<(), ViewMetaError> {
    let buf = encode_png_with_itxt(rgba, w, h, xmp)?;
    std::fs::write(path, &buf).map_err(|e| ViewMetaError::Io(e.to_string()))
}

pub fn write_jpeg_with_xmp(
    path: &Path,
    rgb: &[u8],
    w: u32,
    h: u32,
    quality: u8,
    xmp: &str,
) -> Result<(), ViewMetaError> {
    let mut jpeg = encode_jpeg_rgb(rgb, w, h, quality)?;
    jpeg = splice_jpeg_xmp(jpeg, xmp);
    std::fs::write(path, &jpeg).map_err(|e| ViewMetaError::Io(e.to_string()))
}

pub fn write_webp_with_xmp(
    path: &Path,
    rgba: &[u8],
    w: u32,
    h: u32,
    xmp: &str,
) -> Result<(), ViewMetaError> {
    let bytes = encode_webp_rgba(rgba, w, h, xmp)?;
    std::fs::write(path, &bytes).map_err(|e| ViewMetaError::Io(e.to_string()))
}

fn encode_png_with_itxt(rgba: &[u8], w: u32, h: u32, xmp: &str) -> Result<Vec<u8>, ViewMetaError> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, w, h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .add_itxt_chunk("XML:com.adobe.xmp".to_owned(), xmp.to_owned())
        .map_err(|e| ViewMetaError::Image(e.to_string()))?;
    {
        let mut writer = encoder
            .write_header()
            .map_err(|e| ViewMetaError::Image(e.to_string()))?;
        writer
            .write_image_data(rgba)
            .map_err(|e| ViewMetaError::Image(e.to_string()))?;
    }
    Ok(out)
}

fn encode_jpeg_rgb(rgb: &[u8], w: u32, h: u32, quality: u8) -> Result<Vec<u8>, ViewMetaError> {
    use image::codecs::jpeg::JpegEncoder;
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, quality)
        .write_image(rgb, w, h, image::ExtendedColorType::Rgb8)
        .map_err(|e| ViewMetaError::Image(e.to_string()))?;
    Ok(out)
}

fn splice_jpeg_xmp(mut jpeg: Vec<u8>, xmp: &str) -> Vec<u8> {
    if jpeg.len() < 4 || jpeg[0] != 0xFF || jpeg[1] != 0xD8 {
        return jpeg;
    }
    let mut segment = Vec::new();
    segment.extend_from_slice(b"http://ns.adobe.com/xap/1.0/\0");
    segment.extend_from_slice(xmp.as_bytes());
    let seg_len = segment.len() + 2;
    let mut app1 = vec![0xFF, 0xE1];
    app1.extend_from_slice(&(seg_len as u16).to_be_bytes());
    app1.extend_from_slice(&segment);
    jpeg.splice(2..2, app1);
    jpeg
}

fn encode_webp_rgba(rgba: &[u8], w: u32, h: u32, xmp: &str) -> Result<Vec<u8>, ViewMetaError> {
    use image_webp::WebPEncoder;
    let mut out = Vec::new();
    let mut enc = WebPEncoder::new(&mut out);
    enc.set_xmp_metadata(xmp.as_bytes().to_vec());
    enc.encode(rgba, w, h, image_webp::ColorType::Rgba8)
        .map_err(|e| ViewMetaError::Image(e.to_string()))?;
    Ok(out)
}

pub fn path_looks_absolute(path: &str) -> bool {
    let p = path.trim();
    if p.is_empty() {
        return false;
    }
    if p.starts_with('/') || p.starts_with('\\') {
        return true;
    }
    // Windows drive letter
    p.as_bytes()
        .get(1)
        .is_some_and(|b| *b == b':' && p.as_bytes()[0].is_ascii_alphabetic())
}

fn fmt3(v: [f32; 3]) -> String {
    format!("{},{},{}", v[0], v[1], v[2])
}

fn parse3(text: &str, field: &'static str) -> Result<[f32; 3], ViewMetaError> {
    let mut parts = text.split(',');
    let x = parts
        .next()
        .ok_or(ViewMetaError::MissingField(field))?
        .parse()
        .map_err(|e| ViewMetaError::Parse(format!("{field}: {e}")))?;
    let y = parts
        .next()
        .ok_or(ViewMetaError::MissingField(field))?
        .parse()
        .map_err(|e| ViewMetaError::Parse(format!("{field}: {e}")))?;
    let z = parts
        .next()
        .ok_or(ViewMetaError::MissingField(field))?
        .parse()
        .map_err(|e| ViewMetaError::Parse(format!("{field}: {e}")))?;
    Ok([x, y, z])
}

fn parse_f32(text: &str, field: &'static str) -> Result<f32, ViewMetaError> {
    text.parse()
        .map_err(|e| ViewMetaError::Parse(format!("{field}: {e}")))
}

fn parse_u32(text: &str, field: &'static str) -> Result<u32, ViewMetaError> {
    text.parse()
        .map_err(|e| ViewMetaError::Parse(format!("{field}: {e}")))
}

fn parse_u64(text: &str, field: &'static str) -> Result<u64, ViewMetaError> {
    text.parse()
        .map_err(|e| ViewMetaError::Parse(format!("{field}: {e}")))
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn format_xmp_date(unix: i64) -> String {
    use chrono::{TimeZone, Utc};
    Utc.timestamp_opt(unix, 0)
        .single()
        .map(|t| t.format("%Y-%m-%dT%H:%M:%S").to_string())
        .unwrap_or_else(|| "1970-01-01T00:00:00".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_input() -> ViewMetaInput {
        ViewMetaInput {
            camera: ModelCamera {
                target: [1.0, 2.0, 3.0],
                yaw: -0.7,
                pitch: 0.4,
                distance: 12.5,
                display: ModelDisplay::Arctic,
            },
            eye: [4.0, 5.0, 6.0],
            up: [0.0, 0.0, 1.0],
            aspect: 1.6,
            width: 800,
            height: 500,
            model_name: "chair.3dm".into(),
            model_path: "assets/chair.3dm".into(),
            model_hash: "abc123".into(),
            model_size: 4096,
            node_id: 42,
            created_unix: 1_700_000_000,
            image_adjust_hash: None,
        }
    }

    fn round_trip_format(
        write: fn(&Path, &[u8], u32, u32, &str) -> Result<(), ViewMetaError>,
        rgba: &[u8],
        w: u32,
        h: u32,
        ext: &str,
    ) {
        let input = sample_input();
        let xmp = build_xmp_packet(&input).unwrap();
        assert!(!path_looks_absolute(&input.model_path));
        let dir = std::env::temp_dir().join(format!("slate-view-meta-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(format!("shot.{ext}"));
        write(&path, rgba, w, h, &xmp).unwrap();
        let parsed = read_view_meta(&path)
            .unwrap()
            .unwrap_or_else(|| panic!("{ext} xmp"));
        assert_eq!(parsed.camera, input.camera);
        assert_eq!(parsed.model_hash, input.model_hash);
        assert_eq!(parsed.model_path, input.model_path);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn xmp_round_trip_png_jpeg_webp() {
        let w = 4u32;
        let h = 4u32;
        let mut rgba = Vec::new();
        for i in 0..(w * h) {
            let v = (i * 17 % 255) as u8;
            rgba.extend_from_slice(&[v, v / 2, 255 - v, 255]);
        }
        let rgb: Vec<u8> = rgba.chunks(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
        round_trip_format(
            |p, rgba, w, h, xmp| write_png_with_xmp(p, rgba, w, h, xmp),
            &rgba,
            w,
            h,
            "png",
        );
        round_trip_format(
            |p, rgb, w, h, xmp| write_jpeg_with_xmp(p, rgb, w, h, 90, xmp),
            &rgb,
            w,
            h,
            "jpg",
        );
        round_trip_format(
            |p, rgba, w, h, xmp| write_webp_with_xmp(p, rgba, w, h, xmp),
            &rgba,
            w,
            h,
            "webp",
        );
    }

    #[test]
    fn rejects_newer_version() {
        let xmp = build_xmp_packet(&sample_input())
            .unwrap()
            .replace("slateview:version=\"1\"", "slateview:version=\"2\"");
        let err = parse_xmp_packet(&xmp).unwrap_err();
        assert_eq!(err, ViewMetaError::UnsupportedVersion(2));
    }

    #[test]
    fn malformed_xmp_reports_xml_error() {
        assert!(matches!(
            parse_xmp_packet("<x:xmpmeta"),
            Err(ViewMetaError::Parse(_))
        ));
    }

    #[test]
    fn build_rejects_absolute_model_path() {
        let mut input = sample_input();
        input.model_path = r"C:\Users\secret\model.3dm".into();
        assert_eq!(build_xmp_packet(&input), Err(ViewMetaError::InvalidPath));
    }

    #[test]
    fn hash_bytes_is_stable() {
        let a = hash_bytes(b"model-bytes");
        assert_eq!(a, hash_bytes(b"model-bytes"));
        assert_ne!(a, hash_bytes(b"other"));
    }
}
