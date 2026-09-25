//! Slate viewport capture metadata (XMP `slateview` packet) embedded in PNG,
//! JPEG, and WebP exports. Pure Rust — no UI dependencies.

use image::{ImageDecoder, ImageEncoder};
use slate_doc::scene::{ModelCamera, ModelDisplay};
use std::path::Path;

/// Must match `apps/slate/src/app/model3d::FOV_Y`.
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

pub fn hash_file_bytes(path: &Path) -> Result<String, ViewMetaError> {
    use sha2::{Digest, Sha256};
    let mut file = std::fs::File::open(path).map_err(|e| ViewMetaError::Io(e.to_string()))?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher).map_err(|e| ViewMetaError::Io(e.to_string()))?;
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn build_xmp_packet(input: &ViewMetaInput) -> Result<String, ViewMetaError> {
    if path_looks_absolute(&input.model_path) {
        return Err(ViewMetaError::InvalidPath);
    }
    let display = display_token(input.camera.display);
    let target = fmt3(input.camera.target);
    let eye = fmt3(input.eye);
    let up = fmt3(input.up);
    let created = format_xmp_date(input.created_unix);
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
    slateview:modelName="{model_name}"
    slateview:modelPath="{model_path}"
    slateview:modelHash="{model_hash}"
    slateview:modelSize="{model_size}"
    slateview:nodeId="{node_id}"
    xmp:CreatorTool="{CREATOR}"
    xmp:CreateDate="{created}"
    tiff:ImageWidth="{w}"
    tiff:ImageLength="{h}"/>
 </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>"#,
        yaw = input.camera.yaw,
        pitch = input.camera.pitch,
        distance = input.camera.distance,
        fov = FOV_Y,
        aspect = input.aspect,
        model_name = xml_escape(&input.model_name),
        model_path = xml_escape(&input.model_path),
        model_hash = xml_escape(&input.model_hash),
        model_size = input.model_size,
        node_id = input.node_id,
        w = input.width,
        h = input.height,
    ))
}

pub fn parse_xmp_packet(xmp: &str) -> Result<ViewMetaParsed, ViewMetaError> {
    let _doc = roxmltree::Document::parse(xmp).map_err(|e| ViewMetaError::Parse(e.to_string()))?;
    let version = parse_u32(
        &pick_attr(xmp, "version").ok_or(ViewMetaError::MissingField("version"))?,
        "version",
    )?;
    let target = parse3(
        &pick_attr(xmp, "target").ok_or(ViewMetaError::MissingField("target"))?,
        "target",
    )?;
    let yaw = parse_f32(
        &pick_attr(xmp, "yaw").ok_or(ViewMetaError::MissingField("yaw"))?,
        "yaw",
    )?;
    let pitch = parse_f32(
        &pick_attr(xmp, "pitch").ok_or(ViewMetaError::MissingField("pitch"))?,
        "pitch",
    )?;
    let distance = parse_f32(
        &pick_attr(xmp, "distance").ok_or(ViewMetaError::MissingField("distance"))?,
        "distance",
    )?;
    let display = pick_attr(xmp, "display");
    let model_name = pick_attr(xmp, "modelName");
    let model_path = pick_attr(xmp, "modelPath").ok_or(ViewMetaError::MissingField("modelPath"))?;
    let model_hash = pick_attr(xmp, "modelHash").unwrap_or_default();
    let model_size = pick_attr(xmp, "modelSize")
        .map(|s| parse_u64(&s, "modelSize"))
        .transpose()?
        .unwrap_or(0);
    let node_id = pick_attr(xmp, "nodeId")
        .map(|s| parse_u64(&s, "nodeId"))
        .transpose()?
        .unwrap_or(0);
    if version > VIEW_VERSION {
        return Err(ViewMetaError::UnsupportedVersion(version));
    }
    if path_looks_absolute(&model_path) {
        return Err(ViewMetaError::InvalidPath);
    }
    let display = display
        .as_deref()
        .and_then(display_from_token)
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

/// Read XMP from a saved image file (single-file read, safe off the UI thread).
pub fn read_view_meta(path: &Path) -> Result<Option<ViewMetaParsed>, ViewMetaError> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let xmp = match ext.as_str() {
        "png" => read_png_xmp(path)?.or_else(|| read_xmp_via_image(path).ok().flatten()),
        "jpg" | "jpeg" => read_jpeg_xmp(path)?.or_else(|| read_xmp_via_image(path).ok().flatten()),
        _ => read_xmp_via_image(path)?,
    };
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

fn read_png_xmp(path: &Path) -> Result<Option<String>, ViewMetaError> {
    let bytes = std::fs::read(path).map_err(|e| ViewMetaError::Io(e.to_string()))?;
    let mut pos = 8;
    while pos + 12 <= bytes.len() {
        let len = u32::from_be_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
        if pos + 12 + len > bytes.len() {
            break;
        }
        let kind = &bytes[pos + 4..pos + 8];
        let data = &bytes[pos + 8..pos + 8 + len];
        if kind == b"iTXt" || kind == b"tEXt" {
            if let Some(text) = decode_png_text_chunk(data) {
                if text.contains("slateview:") || text.contains(VIEW_NS) {
                    return Ok(Some(text));
                }
            }
        }
        pos += 12 + len;
    }
    Ok(None)
}

fn decode_png_text_chunk(data: &[u8]) -> Option<String> {
    let keyword_end = data.iter().position(|&b| b == 0)?;
    let keyword = std::str::from_utf8(&data[..keyword_end]).ok()?;
    if keyword != "XML:com.adobe.xmp" {
        return None;
    }
    let mut i = keyword_end + 1;
    if i + 2 > data.len() {
        return None;
    }
    let compressed = data[i] != 0;
    i += 2; // compression flag + method
    if compressed {
        return None;
    }
    if !skip_png_text_field(data, &mut i) || !skip_png_text_field(data, &mut i) {
        return None;
    }
    (i <= data.len()).then(|| std::str::from_utf8(&data[i..]).ok().map(|s| s.to_string()))?
}

fn skip_png_text_field(data: &[u8], i: &mut usize) -> bool {
    if *i >= data.len() {
        return false;
    }
    while *i < data.len() && data[*i] != 0 {
        *i += 1;
    }
    if *i >= data.len() {
        return false;
    }
    *i += 1;
    true
}

fn read_jpeg_xmp(path: &Path) -> Result<Option<String>, ViewMetaError> {
    let jpeg = std::fs::read(path).map_err(|e| ViewMetaError::Io(e.to_string()))?;
    if jpeg.len() < 4 || jpeg[0] != 0xFF || jpeg[1] != 0xD8 {
        return Ok(None);
    }
    let mut pos = 2usize;
    while pos + 4 <= jpeg.len() {
        if jpeg[pos] != 0xFF {
            break;
        }
        let marker = jpeg[pos + 1];
        if marker == 0xDA || marker == 0xD9 {
            break;
        }
        let len = u16::from_be_bytes([jpeg[pos + 2], jpeg[pos + 3]]) as usize;
        if len < 2 || pos + 2 + len > jpeg.len() {
            break;
        }
        if marker == 0xE1 {
            let payload = &jpeg[pos + 4..pos + 2 + len];
            if let Some(text) = xmp_from_jpeg_app1(payload) {
                return Ok(Some(text));
            }
        }
        pos += 2 + len;
    }
    Ok(None)
}

fn xmp_from_jpeg_app1(payload: &[u8]) -> Option<String> {
    const NS: &[u8] = b"http://ns.adobe.com/xap/1.0\0";
    if payload.len() <= NS.len() || &payload[..NS.len()] != NS {
        return None;
    }
    std::str::from_utf8(&payload[NS.len()..])
        .ok()
        .map(|s| s.to_string())
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

pub fn png_xmp_precedes_idat(png: &[u8]) -> bool {
    let mut pos = 8; // signature
    let mut seen_xmp = false;
    while pos + 12 <= png.len() {
        let len = u32::from_be_bytes(png[pos..pos + 4].try_into().unwrap()) as usize;
        if pos + 12 + len > png.len() {
            break;
        }
        let kind = &png[pos + 4..pos + 8];
        if kind == b"IDAT" {
            return seen_xmp;
        }
        if kind == b"iTXt" || kind == b"tEXt" {
            let data = &png[pos + 8..pos + 8 + len];
            if data.starts_with(b"XML:com.adobe.xmp")
                || data.windows(19).any(|w| w == b"XML:com.adobe.xmp")
            {
                seen_xmp = true;
            }
        }
        pos += 12 + len;
    }
    false
}

fn encode_png_with_itxt(rgba: &[u8], w: u32, h: u32, xmp: &str) -> Result<Vec<u8>, ViewMetaError> {
    let mut base = encode_png_rgba(rgba, w, h)?;
    insert_png_xmp(&mut base, xmp)?;
    Ok(base)
}

fn pick_attr(xmp: &str, key: &str) -> Option<String> {
    let needle = format!("slateview:{key}=\"");
    let start = xmp.find(&needle)? + needle.len();
    let rest = &xmp[start..];
    let end = rest.find('"')?;
    Some(unxml(&rest[..end]))
}

fn unxml(s: &str) -> String {
    s.replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn encode_png_rgba(rgba: &[u8], w: u32, h: u32) -> Result<Vec<u8>, ViewMetaError> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, w, h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
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

fn insert_png_xmp(png: &mut Vec<u8>, xmp: &str) -> Result<(), ViewMetaError> {
    let chunk = build_itxt_chunk("XML:com.adobe.xmp", xmp);
    if png.len() < 8 {
        return Err(ViewMetaError::Image("png too short".into()));
    }
    let mut pos = 8;
    while pos + 12 <= png.len() {
        let len = u32::from_be_bytes(png[pos..pos + 4].try_into().unwrap()) as usize;
        if pos + 12 + len > png.len() {
            break;
        }
        let kind = &png[pos + 4..pos + 8];
        if kind == b"IDAT" {
            png.splice(pos..pos, chunk.clone());
            return Ok(());
        }
        pos += 12 + len;
    }
    Err(ViewMetaError::Image("png missing IDAT".into()))
}

fn build_itxt_chunk(keyword: &str, text: &str) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(keyword.as_bytes());
    data.push(0); // keyword terminator
    data.push(0); // compression flag (uncompressed)
    data.push(0); // compression method
    data.push(0); // language tag
    data.push(0); // translated keyword
    data.extend_from_slice(text.as_bytes());
    let len = data.len() as u32;
    let mut chunk = Vec::new();
    chunk.extend_from_slice(&len.to_be_bytes());
    chunk.extend_from_slice(b"iTXt");
    chunk.extend_from_slice(&data);
    let crc = crc32(&chunk[4..]);
    chunk.extend_from_slice(&crc.to_be_bytes());
    chunk
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffffffffu32;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xedb88320
            } else {
                crc >> 1
            };
        }
    }
    !crc
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
    let mut insert_at = 2usize;
    while insert_at + 4 <= jpeg.len() {
        if jpeg[insert_at] != 0xFF {
            break;
        }
        let marker = jpeg[insert_at + 1];
        if marker == 0xDA || marker == 0xD9 {
            break;
        }
        let len = u16::from_be_bytes([jpeg[insert_at + 2], jpeg[insert_at + 3]]) as usize;
        if len < 2 {
            break;
        }
        insert_at += 2 + len;
    }
    let mut segment = Vec::new();
    segment.extend_from_slice(b"http://ns.adobe.com/xap/1.0\0");
    segment.extend_from_slice(xmp.as_bytes());
    let seg_len = segment.len() + 2;
    let mut app1 = vec![0xFF, 0xE1];
    app1.extend_from_slice(&(seg_len as u16).to_be_bytes());
    app1.extend_from_slice(&segment);
    jpeg.splice(insert_at..insert_at, app1);
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

fn display_token(mode: ModelDisplay) -> &'static str {
    match mode {
        ModelDisplay::Shaded => "shaded",
        ModelDisplay::Arctic => "arctic",
        ModelDisplay::Material => "material",
        ModelDisplay::Depth => "depth",
    }
}

fn display_from_token(token: &str) -> Option<ModelDisplay> {
    match token {
        "shaded" => Some(ModelDisplay::Shaded),
        "arctic" => Some(ModelDisplay::Arctic),
        "material" => Some(ModelDisplay::Material),
        "depth" => Some(ModelDisplay::Depth),
        _ => None,
    }
}

fn path_looks_absolute(path: &str) -> bool {
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
        let parsed = read_view_meta(&path).unwrap().expect("xmp");
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
    fn png_xmp_chunk_before_idat() {
        let input = sample_input();
        let xmp = build_xmp_packet(&input).unwrap();
        let w = 2u32;
        let h = 2u32;
        let rgba = vec![255u8; (w * h * 4) as usize];
        let png = encode_png_with_itxt(&rgba, w, h, &xmp).unwrap();
        assert!(png_xmp_precedes_idat(&png));
        let dir = std::env::temp_dir().join(format!("slate-view-meta-dbg-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("shot.png");
        write_png_with_xmp(&path, &rgba, w, h, &xmp).unwrap();
        assert!(
            read_view_meta(&path).unwrap().is_some(),
            "read_view_meta should parse embedded iTXt"
        );
        let _ = std::fs::remove_dir_all(dir);
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
    fn build_rejects_absolute_model_path() {
        let mut input = sample_input();
        input.model_path = r"C:\Users\secret\model.3dm".into();
        assert_eq!(build_xmp_packet(&input), Err(ViewMetaError::InvalidPath));
    }
}
