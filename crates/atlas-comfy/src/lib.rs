//! Loopback-only ComfyUI adapter. Generate, Vary, and Render graphs built from
//! core ComfyUI nodes against weights the user already installed. No renderer,
//! no scene types, and no weight downloads.
//!
//! TWIN: crates/atlas-ollama/src/lib.rs server lifecycle 2026-09-22 (DV-22)
use atlas_agent::{AgentRequest, AgentSession, AgentStatus, ImageBundle, ImageOutput};
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
};

const BASE: &str = "http://127.0.0.1:8188";
/// How far a wired picture may move away from itself.
pub const VARY_DENOISE: f64 = 0.7;
/// A render starts from the shaded view, so materials and light can change
/// while strong depth and edge control hold the massing, members, and camera.
pub const RENDER_DENOISE: f64 = 0.9;
pub const RENDER_DEPTH: f64 = 0.9;
pub const RENDER_EDGE: f64 = 0.6;
pub const VARY_EDGE: f64 = 0.6;
/// Weight of a wired style picture's tokens in the positive prompt. At 1.0 the
/// T2I adapter also carries the picture's subject; 0.6 keeps the prompt's.
pub const STYLE_STRENGTH: f64 = 0.6;
const EDGE_LOW: f64 = 0.2;
const EDGE_HIGH: f64 = 0.5;
const POLL: Duration = Duration::from_millis(100);
/// History check while the event socket is expected to announce completion.
const EVENT_FALLBACK: Duration = Duration::from_secs(1);
const CATALOG_TTL: Duration = Duration::from_secs(10);

static SERVER: Mutex<Option<Child>> = Mutex::new(None);

use atlas_curl::{encode_component, hidden, Failure, Field, Request};

pub fn executable() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("COMFY_BIN")
        .map(PathBuf::from)
        .filter(|p| p.is_file())
    {
        return Some(path);
    }
    let paths: Vec<_> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    if let Some(found) = paths.iter().find_map(|dir| {
        let desktop = dir.join("ComfyUI.exe");
        if desktop.is_file() {
            return Some(desktop);
        }
        let python = dir.join(if cfg!(windows) {
            "python.exe"
        } else {
            "python"
        });
        if python.is_file() && main_py_near(&python).is_some() {
            return Some(python);
        }
        None
    }) {
        return Some(found);
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let local = PathBuf::from(local);
        for root in [local.join("ComfyUI"), local.join("Programs/ComfyUI")] {
            if let Some(found) = search_install(&root, 4) {
                return Some(found);
            }
        }
    }
    None
}

fn search_install(root: &Path, depth: u8) -> Option<PathBuf> {
    if depth == 0 || !root.is_dir() {
        return None;
    }
    let python = root.join("python.exe");
    if python.is_file() && main_py_near(&python).is_some() {
        return Some(python);
    }
    let desktop = root.join("ComfyUI.exe");
    if desktop.is_file() {
        return Some(desktop);
    }
    let skip = ["models", "output", "input", "custom_nodes", "temp", ".git"];
    let mut dirs: Vec<_> = std::fs::read_dir(root)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter(|path| {
            path.file_name()
                .and_then(|n| n.to_str())
                .is_none_or(|name| !skip.iter().any(|s| name.eq_ignore_ascii_case(s)))
        })
        .collect();
    dirs.sort();
    for dir in dirs {
        if let Some(found) = search_install(&dir, depth - 1) {
            return Some(found);
        }
    }
    None
}

fn is_desktop(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.eq_ignore_ascii_case("ComfyUI.exe"))
}

fn main_py_near(python: &Path) -> Option<PathBuf> {
    let parent = python.parent()?.parent()?;
    let candidates = [parent.join("ComfyUI/main.py"), parent.join("main.py")];
    candidates.into_iter().find(|p| p.is_file())
}

fn local(endpoint: &str) -> String {
    format!("{BASE}{endpoint}")
}

fn get_json(endpoint: &str) -> Result<Value, String> {
    let body = Request::get(local(endpoint))
        .loopback()
        .timeout(Duration::from_secs(10))
        .send()
        .map_err(|failure| match failure {
            Failure::Transport(e) => format!("ComfyUI transport unavailable: {e}"),
            Failure::Status(_) => "ComfyUI is not responding on the local port.".into(),
        })?;
    serde_json::from_slice(&body).map_err(|e| e.to_string())
}

fn post_json(endpoint: &str, body: &Value, timeout_secs: u64) -> Result<Value, String> {
    let body = Request::post_json(local(endpoint), body)
        .loopback()
        .timeout(Duration::from_secs(timeout_secs))
        .send()
        .map_err(|failure| match failure {
            Failure::Transport(e) => format!("ComfyUI transport unavailable: {e}"),
            Failure::Status(detail) if detail.trim().is_empty() => {
                "ComfyUI rejected the request.".into()
            }
            Failure::Status(detail) => detail.trim().to_string(),
        })?;
    serde_json::from_slice(&body).map_err(|e| e.to_string())
}

fn names(endpoint: &str) -> Result<Vec<String>, String> {
    Ok(get_json(endpoint)?
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_string))
        .filter(|n| !n.is_empty())
        .take(64)
        .collect())
}

/// Installed checkpoint filenames. Does not launch the server and does not download.
pub fn checkpoints() -> Result<Vec<String>, String> {
    let names = names("/models/checkpoints")?;
    if names.is_empty() {
        return Err(
            "ComfyUI has no installed checkpoints. Add a model file to its checkpoints folder, then choose it again."
                .into(),
        );
    }
    Ok(names)
}

fn ensure_server(cancel: &AtomicBool) -> Result<(), String> {
    if get_json("/models/checkpoints").is_ok() {
        return Ok(());
    }
    let exe = executable().ok_or("Install ComfyUI before using local image generation.")?;
    let mut command = Command::new(&exe);
    hidden(&mut command);
    if !is_desktop(&exe) {
        let main =
            main_py_near(&exe).ok_or("ComfyUI's main.py was not next to the Python executable.")?;
        command.arg("-s").arg(&main).args([
            "--listen",
            "127.0.0.1",
            "--port",
            "8188",
            "--disable-api-nodes",
        ]);
        if let Some(dir) = main.parent() {
            command.current_dir(dir);
        }
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut server = SERVER.lock().unwrap_or_else(|e| e.into_inner());
    if server
        .as_mut()
        .and_then(|c| c.try_wait().ok())
        .flatten()
        .is_some()
    {
        *server = None;
    }
    if server.is_none() {
        *server = Some(
            command
                .spawn()
                .map_err(|e| format!("Could not start ComfyUI: {e}"))?,
        );
    }
    drop(server);
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("ComfyUI start cancelled.".into());
        }
        if get_json("/models/checkpoints").is_ok() {
            return Ok(());
        }
        if Instant::now() > deadline {
            return Err("ComfyUI did not become ready.".into());
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// Stable Diffusion families differ in cross-attention width. A ControlNet only
/// steers a checkpoint of its own family.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Sd1,
    Sd2,
    Sdxl,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arch {
    Sd(Family),
    /// A transformer model (FLUX, SD3, Qwen-Image) these graphs cannot drive.
    Other,
    Unknown,
}

/// Reads the architecture from a safetensors header, never from the weights.
pub fn arch_from_header(header: &Value) -> Arch {
    let Some(map) = header.as_object() else {
        return Arch::Unknown;
    };
    for (key, value) in map {
        if !key.ends_with("attn2.to_k.weight") {
            continue;
        }
        let width = value
            .get("shape")
            .and_then(Value::as_array)
            .and_then(|s| s.get(1))
            .and_then(Value::as_u64);
        return match width {
            Some(768) => Arch::Sd(Family::Sd1),
            Some(1024) => Arch::Sd(Family::Sd2),
            Some(2048) => Arch::Sd(Family::Sdxl),
            _ => Arch::Unknown,
        };
    }
    if map
        .keys()
        .any(|k| k.contains("double_blocks") || k.contains("joint_blocks"))
    {
        return Arch::Other;
    }
    Arch::Unknown
}

fn arch_from_name(name: &str) -> Arch {
    let n = name.to_ascii_lowercase();
    if ["flux", "qwen", "sd3", "hidream", "hunyuan", "wan2"]
        .iter()
        .any(|s| n.contains(s))
    {
        Arch::Other
    } else if n.contains("xl") {
        Arch::Sd(Family::Sdxl)
    } else if n.contains("sd21") || n.contains("sd_turbo") || n.contains("v2-1") {
        Arch::Sd(Family::Sd2)
    } else if n.contains("sd15") || n.contains("sd14") || n.contains("v1-5") || n.contains("v11") {
        Arch::Sd(Family::Sd1)
    } else {
        Arch::Unknown
    }
}

fn read_header(path: &Path) -> Option<Value> {
    if !path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("safetensors"))
    {
        return None;
    }
    let mut file = std::fs::File::open(path).ok()?;
    let mut len = [0u8; 8];
    file.read_exact(&mut len).ok()?;
    let len = u64::from_le_bytes(len);
    if len == 0 || len > 64 * 1024 * 1024 {
        return None;
    }
    let mut buf = vec![0u8; len as usize];
    file.read_exact(&mut buf).ok()?;
    serde_json::from_slice(&buf).ok()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Installed {
    pub name: String,
    pub arch: Arch,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Catalog {
    pub checkpoints: Vec<Installed>,
    pub controls: Vec<Installed>,
    /// Preview decoders in ComfyUI's `vae_approx` folder.
    pub previews: Vec<String>,
    /// Style adapters in `style_models` and the image encoders they read.
    pub styles: Vec<Installed>,
    pub vision: Vec<String>,
}

/// TAESD shows each step as a near-final image; latent2rgb needs no weights.
pub fn preview_method(catalog: &Catalog, family: Option<Family>) -> &'static str {
    let decoder = match family {
        Some(Family::Sd1 | Family::Sd2) => "taesd_decoder",
        Some(Family::Sdxl) => "taesdxl_decoder",
        None => return "latent2rgb",
    };
    if catalog.previews.iter().any(|p| p.starts_with(decoder)) {
        "taesd"
    } else {
        "latent2rgb"
    }
}

fn load_catalog() -> Result<Catalog, String> {
    let checkpoints = checkpoints()?;
    let controls = names("/models/controlnet").unwrap_or_default();
    let folders = get_json("/internal/folder_paths").ok();
    let locate = |folder: &str, name: &str| -> Option<PathBuf> {
        folders
            .as_ref()?
            .get(folder)?
            .as_array()?
            .iter()
            .filter_map(Value::as_str)
            .map(|dir| Path::new(dir).join(name))
            .find(|p| p.is_file())
    };
    let classify = |folder: &str, name: String| {
        let arch = match locate(folder, &name).and_then(|p| read_header(&p)) {
            Some(header) => match arch_from_header(&header) {
                Arch::Unknown => arch_from_name(&name),
                arch => arch,
            },
            None => arch_from_name(&name),
        };
        Installed { name, arch }
    };
    Ok(Catalog {
        checkpoints: checkpoints
            .into_iter()
            .map(|n| classify("checkpoints", n))
            .collect(),
        controls: controls
            .into_iter()
            .map(|n| classify("controlnet", n))
            .collect(),
        previews: names("/models/vae_approx").unwrap_or_default(),
        styles: names("/models/style_models")
            .unwrap_or_default()
            .into_iter()
            .map(|n| classify("style_models", n))
            .collect(),
        vision: names("/models/clip_vision").unwrap_or_default(),
    })
}

pub use atlas_agent::ImageTask as Task;

/// Files a run reads, resolved from the wired inputs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sources {
    pub node: u64,
    pub image: Option<String>,
    pub depth: Option<String>,
    /// A picture on the Style port and the node it came from.
    pub style: Option<(u64, String)>,
}

pub fn sources(request: &AgentRequest) -> Result<Sources, String> {
    use atlas_agent::InputSlot;
    let styles: Vec<(u64, &String)> = request
        .inputs
        .on(InputSlot::Style)
        .flat_map(|item| item.images.iter().map(move |i| (item.node, i)))
        .filter(|(_, image)| !image.is_empty())
        .collect();
    if styles.len() > 1 {
        return Err("Style reads one picture. Disconnect the extra style wire.".into());
    }
    let wired: Vec<_> = request.inputs.on(InputSlot::Media).collect();
    let mut found = media(&wired)?;
    found.style = styles.first().map(|(n, i)| (*n, (*i).clone()));
    Ok(found)
}

fn media(wired: &[&atlas_agent::ContextItem]) -> Result<Sources, String> {
    let models: Vec<_> = wired.iter().filter(|i| i.depth.is_some()).collect();
    if models.len() > 1 {
        return Err("Connect one 3D model to this generator at a time.".into());
    }
    if let Some(item) = models.first() {
        if wired
            .iter()
            .any(|i| i.depth.is_none() && i.images.iter().any(|p| !p.is_empty()))
        {
            return Err(
                "Render reads one 3D model. Disconnect the picture, or wire it to its own generator."
                    .into(),
            );
        }
        return Ok(Sources {
            node: item.node,
            image: item.images.first().cloned(),
            depth: item.depth.clone(),
            style: None,
        });
    }
    let images: Vec<(u64, &String)> = wired
        .iter()
        .flat_map(|item| item.images.iter().map(move |i| (item.node, i)))
        .filter(|(_, image)| !image.is_empty())
        .collect();
    if images.len() > 1 {
        return Err(format!(
            "This generator uses one source image. Disconnect {} extra image{}.",
            images.len() - 1,
            if images.len() == 2 { "" } else { "s" }
        ));
    }
    Ok(Sources {
        node: images.first().map(|(n, _)| *n).unwrap_or(0),
        image: images.first().map(|(_, i)| (*i).clone()),
        depth: None,
        style: None,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    pub task: Task,
    pub checkpoint: String,
    pub family: Option<Family>,
    pub depth_control: Option<String>,
    pub edge_control: Option<String>,
    /// Style adapter and the CLIP vision encoder it reads, when Style is wired.
    pub style: Option<(String, String)>,
}

fn style_model(catalog: &Catalog, family: Family) -> Option<String> {
    catalog
        .styles
        .iter()
        .find(|s| s.arch == Arch::Sd(family))
        .map(|s| s.name.clone())
}

/// T2I style adapters read CLIP ViT-L image tokens.
fn vision_encoder(catalog: &Catalog) -> Option<String> {
    let large = |n: &str| {
        let n = n.to_ascii_lowercase();
        n.contains("vit-l") || n.contains("vit_l") || n.contains("large")
    };
    catalog
        .vision
        .iter()
        .find(|n| large(n))
        .or_else(|| catalog.vision.first())
        .cloned()
}

fn fast(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("lcm") || n.contains("turbo") || n.contains("lightning")
}

fn control(catalog: &Catalog, family: Family, kind: &str) -> Option<String> {
    catalog
        .controls
        .iter()
        .find(|c| c.arch == Arch::Sd(family) && c.name.to_ascii_lowercase().contains(kind))
        .map(|c| c.name.clone())
}

/// Auto prefers one fast checkpoint whose family has installed controls, so the
/// same weights stay resident for Generate, Vary, and Render. A wired style
/// picture first asks for a family with a style adapter.
fn auto_checkpoint(catalog: &Catalog, styled: bool) -> Option<&Installed> {
    catalog
        .checkpoints
        .iter()
        .filter(|c| c.arch != Arch::Other)
        .max_by_key(|c| {
            let (styles, controlled) = match c.arch {
                Arch::Sd(f) => (
                    styled && style_model(catalog, f).is_some(),
                    control(catalog, f, "depth").is_some() as u8 * 2
                        + control(catalog, f, "canny").is_some() as u8,
                ),
                _ => (false, 0),
            };
            (
                styles,
                controlled,
                fast(&c.name),
                std::cmp::Reverse(c.name.clone()),
            )
        })
}

pub fn plan(catalog: &Catalog, requested: Option<&str>, sources: &Sources) -> Result<Plan, String> {
    let task = Task::for_inputs(sources.depth.is_some(), sources.image.is_some());
    let chosen = match requested.filter(|m| !m.is_empty()) {
        Some(name) => catalog.checkpoints.iter().find(|c| c.name == name).ok_or(
            "The selected checkpoint is not installed. Add it in ComfyUI, or choose Auto.",
        )?,
        None => auto_checkpoint(catalog, sources.style.is_some())
            .ok_or("ComfyUI has no Stable Diffusion checkpoint installed.")?,
    };
    if chosen.arch == Arch::Other {
        return Err(format!(
            "{} is not a Stable Diffusion checkpoint. Choose Auto, or a Stable Diffusion 1.5, 2, or XL checkpoint.",
            chosen.name
        ));
    }
    let family = match chosen.arch {
        Arch::Sd(f) => Some(f),
        _ => None,
    };
    let depth_control = family.and_then(|f| control(catalog, f, "depth"));
    let edge_control = family.and_then(|f| control(catalog, f, "canny"));
    if task == Task::Render && depth_control.is_none() && edge_control.is_none() {
        return Err(format!(
            "No depth or edge ControlNet matches {}. Choose Auto, or add a matching ControlNet to ComfyUI.",
            chosen.name
        ));
    }
    let style = match &sources.style {
        None => None,
        Some(_) => {
            let adapter = family.and_then(|f| style_model(catalog, f)).ok_or(format!(
                "Style needs a style adapter that matches {}. Choose Auto, or add the Stable Diffusion 1.5 T2I style adapter to ComfyUI's style_models folder.",
                chosen.name
            ))?;
            let encoder = vision_encoder(catalog).ok_or(
                "Style needs a CLIP vision model. Add CLIP ViT-L to ComfyUI's clip_vision folder.",
            )?;
            Some((adapter, encoder))
        }
    };
    Ok(Plan {
        task,
        checkpoint: chosen.name.clone(),
        family,
        depth_control: (task == Task::Render).then_some(depth_control).flatten(),
        edge_control: (task != Task::Generate).then_some(edge_control).flatten(),
        style,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Preset {
    pub steps: u64,
    pub cfg: f64,
    pub sampler: &'static str,
    pub scheduler: &'static str,
    /// Latent consistency checkpoints need LCM sampling.
    pub lcm: bool,
    /// Native side length. Output area stays near its square.
    pub side: u32,
}

pub fn preset(checkpoint: &str, family: Option<Family>) -> Preset {
    let n = checkpoint.to_ascii_lowercase();
    let side = if family == Some(Family::Sdxl) && !n.contains("turbo") {
        1024
    } else {
        512
    };
    if n.contains("lcm") {
        Preset {
            steps: 6,
            cfg: 1.5,
            sampler: "lcm",
            scheduler: "sgm_uniform",
            lcm: true,
            side,
        }
    } else if n.contains("turbo") {
        Preset {
            steps: 4,
            cfg: 1.0,
            sampler: "euler",
            scheduler: "normal",
            lcm: false,
            side,
        }
    } else if n.contains("lightning") {
        Preset {
            steps: 4,
            cfg: 1.0,
            sampler: "euler",
            scheduler: "sgm_uniform",
            lcm: false,
            side,
        }
    } else {
        Preset {
            steps: 20,
            cfg: 6.5,
            sampler: "dpmpp_2m",
            scheduler: "karras",
            lcm: false,
            side,
        }
    }
}

/// Names ComfyUI gave the uploaded source files.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Uploaded {
    pub image: Option<String>,
    pub depth: Option<String>,
    pub style: Option<String>,
    /// Render latent size, taken from the depth capture.
    pub size: Option<(u32, u32)>,
    /// Shape of a generated picture. Vary and Render keep their source's.
    pub aspect: atlas_agent::Aspect,
}

/// A latent of about `side`² pixels at `ratio`, in multiples of 8.
pub fn generate_size(side: u32, ratio: f32) -> (u32, u32) {
    let root = ratio.clamp(0.25, 4.0).sqrt();
    let w = (side as f32 * root).round() as u32;
    let h = (side as f32 / root).round() as u32;
    (round8(w), round8(h))
}

struct Graph {
    nodes: serde_json::Map<String, Value>,
    next: u32,
}

impl Graph {
    fn add(&mut self, class: &str, inputs: Value) -> String {
        self.next += 1;
        let id = self.next.to_string();
        self.nodes
            .insert(id.clone(), json!({"class_type": class, "inputs": inputs}));
        id
    }
}

fn out(id: &str, slot: u32) -> Value {
    json!([id, slot])
}

fn control_chain(
    g: &mut Graph,
    (positive, negative): (Value, Value),
    control: &str,
    image: Value,
    strength: f64,
    end: f64,
) -> (Value, Value) {
    let loader = g.add("ControlNetLoader", json!({"control_net_name": control}));
    let apply = g.add(
        "ControlNetApplyAdvanced",
        json!({
            "positive": positive,
            "negative": negative,
            "control_net": out(&loader, 0),
            "image": image,
            "strength": strength,
            "start_percent": 0.0,
            "end_percent": end,
        }),
    );
    (out(&apply, 0), out(&apply, 1))
}

fn round8(v: u32) -> u32 {
    (v / 8).max(8) * 8
}

/// A graph of core ComfyUI nodes only. The result is a preview, so ComfyUI's
/// output folder never fills with copies of what Slate already stores.
pub fn workflow(plan: &Plan, prompt: &str, seed: u64, up: &Uploaded) -> Result<Value, String> {
    let preset = preset(&plan.checkpoint, plan.family);
    let mut g = Graph {
        nodes: serde_json::Map::new(),
        next: 0,
    };
    let ckpt = g.add(
        "CheckpointLoaderSimple",
        json!({"ckpt_name": plan.checkpoint}),
    );
    let model = if preset.lcm {
        let m = g.add(
            "ModelSamplingDiscrete",
            json!({"model": out(&ckpt, 0), "sampling": "lcm", "zsnr": false}),
        );
        out(&m, 0)
    } else {
        out(&ckpt, 0)
    };
    let pos = g.add(
        "CLIPTextEncode",
        json!({"text": prompt, "clip": out(&ckpt, 1)}),
    );
    let neg = g.add("CLIPTextEncode", json!({"text": "", "clip": out(&ckpt, 1)}));
    let mut cond = (out(&pos, 0), out(&neg, 0));
    if let Some((adapter, encoder)) = &plan.style {
        let picture = up
            .style
            .as_ref()
            .ok_or("The style image was not uploaded.")?;
        let vision = g.add("CLIPVisionLoader", json!({"clip_name": encoder}));
        let image = g.add("LoadImage", json!({"image": picture}));
        let encoded = g.add(
            "CLIPVisionEncode",
            json!({"clip_vision": out(&vision, 0), "image": out(&image, 0), "crop": "center"}),
        );
        let style = g.add("StyleModelLoader", json!({"style_model_name": adapter}));
        let applied = g.add(
            "StyleModelApply",
            json!({
                "conditioning": cond.0,
                "style_model": out(&style, 0),
                "clip_vision_output": out(&encoded, 0),
                "strength": STYLE_STRENGTH,
                "strength_type": "multiply",
            }),
        );
        cond.0 = out(&applied, 0);
    }
    let (latent, denoise) = match plan.task {
        Task::Render => {
            let (w, h) = up.size.ok_or("The 3D view was not captured.")?;
            if let (Some(control), Some(depth)) = (&plan.depth_control, &up.depth) {
                let image = g.add("LoadImage", json!({"image": depth}));
                cond = control_chain(&mut g, cond, control, out(&image, 0), RENDER_DEPTH, 1.0);
            }
            match &up.image {
                Some(view) => {
                    let image = g.add("LoadImage", json!({"image": view}));
                    if let Some(control) = &plan.edge_control {
                        let edges = g.add(
                            "Canny",
                            json!({"image": out(&image, 0), "low_threshold": EDGE_LOW, "high_threshold": EDGE_HIGH}),
                        );
                        cond =
                            control_chain(&mut g, cond, control, out(&edges, 0), RENDER_EDGE, 1.0);
                    }
                    let latent = g.add(
                        "VAEEncode",
                        json!({"pixels": out(&image, 0), "vae": out(&ckpt, 2)}),
                    );
                    (latent, RENDER_DENOISE)
                }
                None => {
                    let latent = g.add(
                        "EmptyLatentImage",
                        json!({"width": round8(w), "height": round8(h), "batch_size": 1}),
                    );
                    (latent, 1.0)
                }
            }
        }
        Task::Vary => {
            let source = up
                .image
                .as_ref()
                .ok_or("The source image was not uploaded.")?;
            let image = g.add("LoadImage", json!({"image": source}));
            let megapixels =
                ((preset.side * preset.side) as f64 / 1_000_000.0 * 100.0).round() / 100.0;
            let scaled = g.add(
                "ImageScaleToTotalPixels",
                json!({"image": out(&image, 0), "upscale_method": "area", "megapixels": megapixels, "resolution_steps": 8}),
            );
            if let Some(control) = &plan.edge_control {
                let edges = g.add(
                    "Canny",
                    json!({"image": out(&scaled, 0), "low_threshold": EDGE_LOW, "high_threshold": EDGE_HIGH}),
                );
                cond = control_chain(&mut g, cond, control, out(&edges, 0), VARY_EDGE, 0.9);
            }
            let latent = g.add(
                "VAEEncode",
                json!({"pixels": out(&scaled, 0), "vae": out(&ckpt, 2)}),
            );
            (latent, VARY_DENOISE)
        }
        Task::Generate => {
            let (width, height) = generate_size(preset.side, up.aspect.ratio().unwrap_or(1.0));
            let latent = g.add(
                "EmptyLatentImage",
                json!({"width": width, "height": height, "batch_size": 1}),
            );
            (latent, 1.0)
        }
    };
    let sampler = g.add(
        "KSampler",
        json!({
            "seed": seed,
            "steps": preset.steps,
            "cfg": preset.cfg,
            "sampler_name": preset.sampler,
            "scheduler": preset.scheduler,
            "denoise": denoise,
            "model": model,
            "positive": cond.0,
            "negative": cond.1,
            "latent_image": out(&latent, 0),
        }),
    );
    let decode = g.add(
        "VAEDecode",
        json!({"samples": out(&sampler, 0), "vae": out(&ckpt, 2)}),
    );
    g.add("PreviewImage", json!({"images": out(&decode, 0)}));
    Ok(Value::Object(g.nodes))
}

fn seed_from(id: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in id.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash >> 1
}

/// Width and height from a PNG header.
pub fn png_size(path: &Path) -> Option<(u32, u32)> {
    let mut head = [0u8; 24];
    std::fs::File::open(path).ok()?.read_exact(&mut head).ok()?;
    if &head[..8] != b"\x89PNG\r\n\x1a\n" || &head[12..16] != b"IHDR" {
        return None;
    }
    let w = u32::from_be_bytes(head[16..20].try_into().ok()?);
    let h = u32::from_be_bytes(head[20..24].try_into().ok()?);
    Some((w, h))
}

/// Only the newest frame of the current live run is replaceable. Returns the
/// file of the frame it removed.
pub fn settle_live(bundle: &mut ImageBundle, run: Option<&str>) -> Option<String> {
    let run = run?;
    if bundle.images.last()?.live != run {
        return None;
    }
    bundle.images.pop().map(|old| old.source)
}

fn upload_image(path: &Path, name: &str) -> Result<String, String> {
    if !path.is_file() {
        return Err(format!(
            "Connected image is unavailable: {}",
            path.display()
        ));
    }
    let shown = path.to_string_lossy();
    if shown.contains(';') || shown.contains(',') || shown.contains('"') {
        return Err(format!(
            "Rename the connected image so its path has no ; , or \" characters: {}",
            path.display()
        ));
    }
    let out = Request::post_form(
        local("/upload/image"),
        vec![
            (
                "image".into(),
                Field::File {
                    path: path.to_path_buf(),
                    filename: Some(name.to_string()),
                },
            ),
            ("overwrite".into(), Field::Text("true".into())),
        ],
    )
    .loopback()
    .timeout(Duration::from_secs(120))
    .send()
    .map_err(|failure| match failure {
        Failure::Transport(e) => format!("Could not send the source image to ComfyUI: {e}"),
        Failure::Status(_) => "ComfyUI did not accept the source image.".into(),
    })?;
    let value: Value = serde_json::from_slice(&out).map_err(|e| e.to_string())?;
    let uploaded = value["name"]
        .as_str()
        .filter(|n| !n.is_empty())
        .ok_or("ComfyUI did not name the uploaded image.")?;
    Ok(
        match value["subfolder"].as_str().filter(|s| !s.is_empty()) {
            Some(sub) => format!("{sub}/{uploaded}"),
            None => uploaded.to_string(),
        },
    )
}

fn download_image(filename: &str, subfolder: &str, kind: &str, dest: &Path) -> Result<(), String> {
    let endpoint = format!(
        "/view?filename={}&subfolder={}&type={}",
        encode_component(filename),
        encode_component(subfolder),
        encode_component(kind)
    );
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let sent = Request::get(local(&endpoint))
        .loopback()
        .timeout(Duration::from_secs(60))
        .output(dest)
        .send();
    if sent.is_err() || !dest.is_file() {
        return Err("ComfyUI finished without a readable image.".into());
    }
    Ok(())
}

struct FinishedImage {
    filename: String,
    subfolder: String,
    kind: String,
}

fn finished_images(history: &Value, prompt_id: &str) -> Result<Option<Vec<FinishedImage>>, String> {
    let entry = match history.get(prompt_id) {
        Some(entry) if !entry.is_null() => entry,
        _ => return Ok(None),
    };
    let status = entry["status"]["status_str"].as_str().unwrap_or("");
    if status == "error" {
        let message = entry["status"]["messages"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|m| m.get(1))
            .find_map(|d| d["exception_message"].as_str())
            .unwrap_or("ComfyUI reported an execution error.");
        return Err(message.trim().to_string());
    }
    if !entry["status"]["completed"].as_bool().unwrap_or(false) && status != "success" {
        return Ok(None);
    }
    let mut images = Vec::new();
    if let Some(outputs) = entry["outputs"].as_object() {
        for node in outputs.values() {
            for image in node["images"].as_array().into_iter().flatten() {
                let Some(filename) = image["filename"].as_str() else {
                    continue;
                };
                images.push(FinishedImage {
                    filename: filename.to_string(),
                    subfolder: image["subfolder"].as_str().unwrap_or("").to_string(),
                    kind: image["type"].as_str().unwrap_or("output").to_string(),
                });
            }
        }
    }
    if images.is_empty() {
        return Err("ComfyUI finished without an image.".into());
    }
    Ok(Some(images))
}

fn cancel_prompt(prompt_id: &str) {
    let Ok(queue) = get_json("/queue") else {
        return;
    };
    let running = queue["queue_running"]
        .to_string()
        .contains(&format!("\"{prompt_id}\""));
    if running {
        let _ = post_json("/interrupt", &json!({}), 10);
        return;
    }
    let _ = post_json("/queue", &json!({"delete": [prompt_id]}), 10);
}

/// One WebSocket frame: opcode, FIN, and unmasked payload.
fn read_frame<R: Read>(r: &mut R) -> std::io::Result<(u8, bool, Vec<u8>)> {
    let mut head = [0u8; 2];
    r.read_exact(&mut head)?;
    let (fin, opcode) = (head[0] & 0x80 != 0, head[0] & 0x0f);
    let mut len = u64::from(head[1] & 0x7f);
    if len == 126 {
        let mut b = [0u8; 2];
        r.read_exact(&mut b)?;
        len = u64::from(u16::from_be_bytes(b));
    } else if len == 127 {
        let mut b = [0u8; 8];
        r.read_exact(&mut b)?;
        len = u64::from_be_bytes(b);
    }
    if len > 64 * 1024 * 1024 {
        return Err(std::io::Error::other("oversized frame"));
    }
    let mask = if head[1] & 0x80 != 0 {
        let mut m = [0u8; 4];
        r.read_exact(&mut m)?;
        Some(m)
    } else {
        None
    };
    let mut payload = vec![0u8; len as usize];
    r.read_exact(&mut payload)?;
    if let Some(m) = mask {
        for (i, b) in payload.iter_mut().enumerate() {
            *b ^= m[i % 4];
        }
    }
    Ok((opcode, fin, payload))
}

/// The encoded image in a ComfyUI binary preview message, if it is one.
fn preview_image(payload: &[u8]) -> Option<&[u8]> {
    const PREVIEW_IMAGE: u32 = 1;
    let event = u32::from_be_bytes(payload.get(..4)?.try_into().ok()?);
    (event == PREVIEW_IMAGE && payload.len() > 8).then(|| &payload[8..])
}

/// ComfyUI's event socket for the client that queued a prompt: progress and
/// preview frames. Loopback and server-to-client only, so a small reader
/// replaces a WebSocket dependency.
struct Events {
    stream: std::net::TcpStream,
    reader: std::io::BufReader<std::net::TcpStream>,
    partial: Vec<u8>,
    partial_op: u8,
}

impl Events {
    fn connect(client_id: &str) -> Option<Self> {
        use std::io::BufRead;
        let addr = "127.0.0.1:8188".parse().ok()?;
        let mut stream =
            std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(2)).ok()?;
        stream.set_nodelay(true).ok()?;
        stream.set_read_timeout(Some(Duration::from_secs(3))).ok()?;
        write!(
            stream,
            "GET /ws?clientId={} HTTP/1.1\r\nHost: 127.0.0.1:8188\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n",
            encode_component(client_id)
        )
        .ok()?;
        let mut reader = std::io::BufReader::new(stream.try_clone().ok()?);
        let mut line = String::new();
        reader.read_line(&mut line).ok()?;
        if !line.contains(" 101") {
            return None;
        }
        loop {
            line.clear();
            if reader.read_line(&mut line).ok()? == 0 || line == "\r\n" {
                break;
            }
        }
        Some(Self {
            stream,
            reader,
            partial: Vec::new(),
            partial_op: 0x2,
        })
    }

    /// The next whole message, or `None` once `wait` passes with nothing to read.
    fn next(&mut self, wait: Duration) -> std::io::Result<Option<(u8, Vec<u8>)>> {
        use std::io::BufRead;
        self.stream
            .set_read_timeout(Some(wait.max(Duration::from_millis(1))))?;
        match self.reader.fill_buf() {
            Ok([]) => return Err(std::io::ErrorKind::UnexpectedEof.into()),
            Ok(_) => {}
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                return Ok(None)
            }
            Err(e) => return Err(e),
        }
        // A frame has begun; finish it without the poll timeout.
        self.stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        loop {
            let (opcode, fin, payload) = read_frame(&mut self.reader)?;
            match opcode {
                0x8 => return Err(std::io::ErrorKind::ConnectionAborted.into()),
                0x9 => {
                    // Client frames are masked; a zero key leaves the payload as is.
                    let mut pong = vec![0x8A, 0x80 | payload.len().min(125) as u8, 0, 0, 0, 0];
                    pong.extend(payload.iter().take(125));
                    self.stream.write_all(&pong)?;
                }
                0xA => {}
                0x0 => {
                    self.partial.extend(payload);
                    if fin {
                        return Ok(Some((self.partial_op, std::mem::take(&mut self.partial))));
                    }
                }
                _ if fin => return Ok(Some((opcode, payload))),
                _ => {
                    self.partial_op = opcode;
                    self.partial = payload;
                }
            }
            if !fin {
                continue;
            }
            return Ok(None);
        }
    }
}

/// Newest preview of the run the client is waiting on.
pub type PreviewSink = std::sync::Arc<Mutex<Option<atlas_agent::ImagePreview>>>;

/// Drain events for up to `wait`, keeping the newest progress and frame.
fn pump_events(
    events: &mut Option<Events>,
    wait: Duration,
    request: &str,
    prompt_id: &str,
    sink: Option<&PreviewSink>,
) -> bool {
    let Some(ev) = events.as_mut() else {
        std::thread::sleep(wait);
        return false;
    };
    let until = Instant::now() + wait;
    loop {
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return false;
        }
        let message = match ev.next(left) {
            Ok(Some(message)) => message,
            Ok(None) => continue,
            Err(_) => {
                *events = None;
                std::thread::sleep(left);
                return false;
            }
        };
        if message.0 == 0x1 && run_ended(&message.1, prompt_id) {
            return true;
        }
        let Some(sink) = sink else {
            continue;
        };
        let mut slot = sink.lock().unwrap_or_else(|e| e.into_inner());
        let preview = slot.get_or_insert_with(Default::default);
        if preview.request != request {
            *preview = atlas_agent::ImagePreview {
                request: request.to_string(),
                ..Default::default()
            };
        }
        match message {
            (0x1, text) => {
                let Ok(value) = serde_json::from_slice::<Value>(&text) else {
                    continue;
                };
                let data = &value["data"];
                if value["type"] == "progress" && data["prompt_id"] == prompt_id {
                    preview.step = data["value"].as_u64().unwrap_or(0) as u32;
                    preview.steps = data["max"].as_u64().unwrap_or(0) as u32;
                }
            }
            (0x2, bytes) => {
                if let Some(image) = preview_image(&bytes) {
                    preview.image = std::sync::Arc::new(image.to_vec());
                    preview.frame += 1;
                }
            }
            _ => {}
        }
    }
}

/// ComfyUI reports the end of a prompt as `executing` with no node, or as a
/// terminal execution event.
fn run_ended(text: &[u8], prompt_id: &str) -> bool {
    let Ok(value) = serde_json::from_slice::<Value>(text) else {
        return false;
    };
    let data = &value["data"];
    data["prompt_id"] == prompt_id
        && match value["type"].as_str() {
            Some("executing") => data["node"].is_null(),
            Some("execution_success" | "execution_error" | "execution_interrupted") => true,
            _ => false,
        }
}

fn safe_tag(value: &str) -> String {
    value
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .take(48)
        .collect()
}

pub struct Client {
    output_dir: PathBuf,
    tag: String,
    catalog: Option<(Instant, Catalog)>,
    preview: Option<PreviewSink>,
}

impl Client {
    pub fn start(cancel: &AtomicBool) -> Result<Self, String> {
        ensure_server(cancel)?;
        Ok(Self {
            output_dir: PathBuf::new(),
            tag: String::new(),
            catalog: None,
            preview: None,
        })
    }

    /// Intermediate frames of each run are published here while it samples.
    pub fn set_preview_sink(&mut self, sink: PreviewSink) {
        self.preview = Some(sink);
    }

    /// Results land here, outside the synced link folder. `tag` keeps this
    /// portal's uploads apart from every other portal's in ComfyUI.
    pub fn set_output_dir(&mut self, dir: PathBuf, tag: &str) {
        self.output_dir = dir;
        self.tag = safe_tag(tag);
    }

    fn catalog(&mut self) -> Result<Catalog, String> {
        if let Some((at, catalog)) = &self.catalog {
            if at.elapsed() < CATALOG_TTL {
                return Ok(catalog.clone());
            }
        }
        let catalog = load_catalog()?;
        self.catalog = Some((Instant::now(), catalog.clone()));
        Ok(catalog)
    }

    pub fn run(
        &mut self,
        request: &AgentRequest,
        session: &mut AgentSession,
        cancel: &AtomicBool,
        mut changed: impl FnMut(&AgentSession),
    ) -> Result<(), String> {
        if self.output_dir.as_os_str().is_empty() {
            return Err("This image portal has no output folder.".into());
        }
        let prompt = request.prompt.trim();
        if prompt.is_empty() {
            return Err("Connect a note with a prompt, or type one.".into());
        }
        let sources = sources(request)?;
        let catalog = self.catalog()?;
        let plan = plan(&catalog, request.model.as_deref(), &sources)?;
        let mut up = Uploaded::default();
        if let Some(path) = &sources.image {
            let path = Path::new(path);
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("png");
            let role = if sources.depth.is_some() {
                "view"
            } else {
                "image"
            };
            let name = format!("slate-{}-{}-{role}.{ext}", self.tag, sources.node);
            up.image = Some(upload_image(path, &name)?);
        }
        if let Some(path) = &sources.depth {
            let path = Path::new(path);
            up.size = png_size(path);
            let name = format!("slate-{}-{}-depth.png", self.tag, sources.node);
            up.depth = Some(upload_image(path, &name)?);
        }
        if let Some((node, path)) = &sources.style {
            let path = Path::new(path);
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("png");
            let name = format!("slate-{}-{node}-style.{ext}", self.tag);
            up.style = Some(upload_image(path, &name)?);
        }
        let params = request.image.clone().unwrap_or_default();
        up.aspect = params.aspect;
        // Live replaces one frame; otherwise each picture lands as it finishes.
        let count = if params.live.is_some() { 1 } else { params.images() };
        let base = params.seed.unwrap_or_else(|| seed_from(&request.id));
        session.status = AgentStatus::Thinking;
        changed(session);
        for pass in 0..count {
            let seed = base.wrapping_add(u64::from(pass));
            let graph = workflow(&plan, prompt, seed, &up)?;
            let id = if count == 1 {
                request.id.clone()
            } else {
                format!("{}-{pass}", request.id)
            };
            self.run_graph(request, &id, graph, &catalog, &plan, cancel, session, &mut changed)?;
        }
        session.status = AgentStatus::Idle;
        changed(session);
        Ok(())
    }

    /// Queue one graph, follow it to the end, and add what it made to the album.
    #[allow(clippy::too_many_arguments)]
    fn run_graph(
        &mut self,
        request: &AgentRequest,
        id: &str,
        graph: Value,
        catalog: &Catalog,
        plan: &Plan,
        cancel: &AtomicBool,
        session: &mut AgentSession,
        changed: &mut impl FnMut(&AgentSession),
    ) -> Result<(), String> {
        let prompt = request.prompt.trim();
        let params = request.image.clone().unwrap_or_default();
        if cancel.load(Ordering::Relaxed) {
            return Err("Generation stopped.".into());
        }
        // Subscribe before queueing: ComfyUI sends frames only to a connected client.
        let mut events = self
            .preview
            .as_ref()
            .and_then(|_| Events::connect(&request.id));
        let queued = post_json(
            "/prompt",
            &json!({
                "prompt": graph,
                "client_id": request.id,
                "extra_data": {"preview_method": preview_method(catalog, plan.family)},
            }),
            30,
        )?;
        if let Some(errors) = queued
            .get("node_errors")
            .filter(|e| !e.as_object().is_some_and(|o| o.is_empty()))
        {
            return Err(format!("ComfyUI rejected the workflow: {errors}"));
        }
        let prompt_id = queued["prompt_id"]
            .as_str()
            .ok_or("ComfyUI did not accept the workflow.")?
            .to_string();
        let deadline = Instant::now() + Duration::from_secs(20 * 60);
        let mut next_history = Instant::now();
        let images = loop {
            if cancel.load(Ordering::Relaxed) {
                cancel_prompt(&prompt_id);
                return Err("Generation stopped.".into());
            }
            if Instant::now() > deadline {
                cancel_prompt(&prompt_id);
                return Err("ComfyUI did not finish within 20 minutes.".into());
            }
            if Instant::now() >= next_history {
                let history = get_json(&format!("/history/{prompt_id}"))?;
                if let Some(images) = finished_images(&history, &prompt_id)? {
                    break images;
                }
                // With the event socket, history is only a fallback check.
                next_history = Instant::now()
                    + if events.is_some() {
                        EVENT_FALLBACK
                    } else {
                        POLL
                    };
            }
            if pump_events(
                &mut events,
                POLL,
                &request.id,
                &prompt_id,
                self.preview.as_ref(),
            ) {
                next_history = Instant::now();
            }
        };
        let live = params.live.as_deref();
        for (index, image) in images.iter().enumerate() {
            let ext = Path::new(&image.filename)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("png");
            let id = if images.len() == 1 {
                id.to_string()
            } else {
                format!("{id}-{index}")
            };
            let dest = self.output_dir.join(format!("{id}.{ext}"));
            download_image(&image.filename, &image.subfolder, &image.kind, &dest)?;
            if session.bundle.images.iter().any(|v| v.id == id) {
                continue;
            }
            if let Some(old) = settle_live(&mut session.bundle, live) {
                if Path::new(&old).starts_with(&self.output_dir) {
                    let _ = std::fs::remove_file(old);
                }
            }
            session.bundle.images.push(ImageOutput {
                id,
                source: dest.to_string_lossy().into_owned(),
                request: request.id.clone(),
                prompt: prompt.to_string(),
                model: plan.checkpoint.clone(),
                task: plan.task.label().to_string(),
                live: live.unwrap_or_default().to_string(),
            });
            changed(session);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sd(name: &str, family: Family) -> Installed {
        Installed {
            name: name.into(),
            arch: Arch::Sd(family),
        }
    }

    fn catalog() -> Catalog {
        Catalog {
            checkpoints: vec![
                sd("sd_turbo.safetensors", Family::Sd2),
                sd("DreamShaper8_LCM.safetensors", Family::Sd1),
                Installed {
                    name: "flux1-schnell.safetensors".into(),
                    arch: Arch::Other,
                },
            ],
            controls: vec![
                sd("control_v11f1p_sd15_depth_fp16.safetensors", Family::Sd1),
                sd("control_v11p_sd15_canny_fp16.safetensors", Family::Sd1),
            ],
            previews: vec!["taesd_decoder.safetensors".into()],
            styles: vec![sd("t2iadapter_style_sd14v1.pth", Family::Sd1)],
            vision: vec!["clip_vit_large_patch14.safetensors".into()],
        }
    }

    #[test]
    #[ignore = "needs ComfyUI with a Stable Diffusion checkpoint installed"]
    fn live_run_makes_each_picture_in_the_chosen_aspect() {
        let dir = std::env::temp_dir().join(format!("atlas-comfy-live-{}", std::process::id()));
        let cancel = AtomicBool::new(false);
        let mut client = Client::start(&cancel).unwrap();
        client.set_output_dir(dir.clone(), "live-test");
        let mut req = request(json!([{"node": 2, "text": "park", "images": [], "slot": "prompt"}]));
        req.id = "live-count".into();
        req.image = Some(atlas_agent::ImageParams {
            count: 2,
            aspect: atlas_agent::Aspect::Wide,
            ..Default::default()
        });
        let mut session: AgentSession = serde_json::from_str(r#"{"provider":"comfy"}"#).unwrap();
        let mut arrivals = 0;
        client
            .run(&req, &mut session, &cancel, |s| arrivals = arrivals.max(s.bundle.images.len()))
            .unwrap();
        assert_eq!(session.bundle.images.len(), 2);
        assert_eq!(arrivals, 2);
        let (w, h) = png_size(Path::new(&session.bundle.images[0].source)).unwrap();
        assert!(w > h, "16:9 comes out wide: {w}x{h}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    #[ignore = "needs a running ComfyUI on 127.0.0.1:8188"]
    fn live_server_accepts_the_event_socket() {
        let mut events = Events::connect("slate-probe").expect("handshake");
        let (op, _) = events
            .next(Duration::from_secs(2))
            .unwrap()
            .expect("status");
        assert_eq!(op, 0x1, "ComfyUI greets with a text status frame");
    }

    #[test]
    fn socket_frames_and_previews_parse_and_the_decoder_picks_the_method() {
        // Short binary frame: event 1 (preview), image type 1 (JPEG), bytes.
        let mut payload = 1u32.to_be_bytes().to_vec();
        payload.extend(1u32.to_be_bytes());
        payload.extend(b"jpeg");
        let mut frame = vec![0x82, payload.len() as u8];
        frame.extend(&payload);
        let (op, fin, got) = read_frame(&mut std::io::Cursor::new(frame)).unwrap();
        assert_eq!((op, fin), (0x2, true));
        assert_eq!(preview_image(&got), Some(&b"jpeg"[..]));
        assert_eq!(preview_image(&3u32.to_be_bytes()), None);
        // 16-bit length and a masked payload.
        let body = vec![7u8; 300];
        let mut frame = vec![0x81, 0x80 | 126];
        frame.extend((body.len() as u16).to_be_bytes());
        let mask = [1u8, 2, 3, 4];
        frame.extend(mask);
        frame.extend(body.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
        let (op, _, got) = read_frame(&mut std::io::Cursor::new(frame)).unwrap();
        assert_eq!(op, 0x1);
        assert_eq!(got, body);

        assert!(run_ended(
            br#"{"type":"executing","data":{"node":null,"prompt_id":"p1"}}"#,
            "p1"
        ));
        assert!(!run_ended(
            br#"{"type":"executing","data":{"node":"3","prompt_id":"p1"}}"#,
            "p1"
        ));
        assert!(!run_ended(
            br#"{"type":"execution_success","data":{"prompt_id":"p2"}}"#,
            "p1"
        ));

        let mut c = catalog();
        assert_eq!(preview_method(&c, Some(Family::Sd1)), "taesd");
        assert_eq!(preview_method(&c, Some(Family::Sdxl)), "latent2rgb");
        c.previews.clear();
        assert_eq!(preview_method(&c, Some(Family::Sd1)), "latent2rgb");
    }

    fn request(wired: Value) -> AgentRequest {
        AgentRequest {
            id: "r".into(),
            prompt: "pavilion in a park".into(),
            model: None,
            at: 0,
            inputs: serde_json::from_value(json!({"revision": "1", "context": [], "wired": wired}))
                .unwrap(),
            history: vec![],
            image: None,
            output_dir: None,
            oneshot: false,
        }
    }

    fn classes(graph: &Value) -> Vec<String> {
        let mut out: Vec<String> = graph
            .as_object()
            .unwrap()
            .values()
            .map(|n| n["class_type"].as_str().unwrap().to_string())
            .collect();
        out.sort();
        out
    }

    #[test]
    fn headers_name_the_family_and_refuse_transformers() {
        let key = |width: u64| json!({"model.diffusion_model.input_blocks.1.1.transformer_blocks.0.attn2.to_k.weight": {"dtype": "F16", "shape": [320, width], "data_offsets": [0, 1]}});
        assert_eq!(arch_from_header(&key(768)), Arch::Sd(Family::Sd1));
        assert_eq!(arch_from_header(&key(1024)), Arch::Sd(Family::Sd2));
        assert_eq!(arch_from_header(&key(2048)), Arch::Sd(Family::Sdxl));
        let diffusers_control = json!({"down_blocks.0.attentions.0.transformer_blocks.0.attn2.to_k.weight": {"shape": [320, 768]}});
        assert_eq!(arch_from_header(&diffusers_control), Arch::Sd(Family::Sd1));
        let flux = json!({"double_blocks.0.img_attn.qkv.weight": {"shape": [9216, 3072]}});
        assert_eq!(arch_from_header(&flux), Arch::Other);
        assert_eq!(arch_from_name("flux1-dev.safetensors"), Arch::Other);
        assert_eq!(arch_from_name("mystery.ckpt"), Arch::Unknown);
    }

    #[test]
    fn wired_inputs_choose_the_task_and_auto_keeps_controlled_weights() {
        let c = catalog();
        let text = sources(&request(json!([{"node": 2, "text": "park", "images": []}]))).unwrap();
        let generate = plan(&c, None, &text).unwrap();
        assert_eq!(generate.task, Task::Generate);
        assert_eq!(generate.checkpoint, "DreamShaper8_LCM.safetensors");
        assert!(generate.edge_control.is_none());

        let picture = sources(&request(
            json!([{"node": 3, "text": "", "images": ["photo.jpg"]}]),
        ))
        .unwrap();
        let vary = plan(&c, None, &picture).unwrap();
        assert_eq!(vary.task, Task::Vary);
        assert_eq!(
            vary.edge_control.as_deref(),
            Some("control_v11p_sd15_canny_fp16.safetensors")
        );
        assert!(vary.depth_control.is_none());

        let model = sources(&request(json!([
            {"node": 3, "text": "", "images": ["view.png"], "depth": "depth.png"},
            {"node": 2, "text": "park", "images": []}
        ])))
        .unwrap();
        assert_eq!(model.image.as_deref(), Some("view.png"));
        let render = plan(&c, None, &model).unwrap();
        assert_eq!(render.task, Task::Render);
        assert!(render.depth_control.is_some() && render.edge_control.is_some());

        let refused = plan(&c, Some("sd_turbo.safetensors"), &model).unwrap_err();
        assert!(refused.contains("No depth or edge ControlNet matches sd_turbo"));
        assert!(plan(&c, Some("flux1-schnell.safetensors"), &text)
            .unwrap_err()
            .contains("not a Stable Diffusion checkpoint"));
        assert!(plan(&c, Some("missing.safetensors"), &text).is_err());
    }

    #[test]
    fn render_graph_controls_depth_and_edges_with_core_nodes_only() {
        let c = catalog();
        let model = sources(&request(json!([
            {"node": 3, "text": "", "images": ["view.png"], "depth": "depth.png"}
        ])))
        .unwrap();
        let render = plan(&c, None, &model).unwrap();
        let up = Uploaded {
            image: Some("slate-view.png".into()),
            depth: Some("slate-depth.png".into()),
            size: Some((683, 384)),
            ..Default::default()
        };
        let graph = workflow(&render, "watercolor", 9, &up).unwrap();
        let kinds = classes(&graph);
        assert_eq!(
            kinds
                .iter()
                .filter(|k| *k == "ControlNetApplyAdvanced")
                .count(),
            2
        );
        assert!(kinds.contains(&"Canny".to_string()));
        assert!(kinds.contains(&"ModelSamplingDiscrete".to_string()));
        assert!(kinds.contains(&"PreviewImage".to_string()));
        assert!(!kinds.iter().any(|k| k.contains("Api") || k == "SaveImage"));
        assert!(
            kinds.contains(&"VAEEncode".to_string()),
            "a render starts from the shaded view"
        );
        let sampler = graph
            .as_object()
            .unwrap()
            .values()
            .find(|n| n["class_type"] == "KSampler")
            .unwrap();
        assert_eq!(sampler["inputs"]["denoise"], RENDER_DENOISE);
        assert_eq!(sampler["inputs"]["sampler_name"], "lcm");
        assert_eq!(sampler["inputs"]["seed"], 9);
        assert!(workflow(&render, "x", 1, &Uploaded::default()).is_err());
        // Depth alone still renders, from an empty latent at the capture size.
        let depth_only = workflow(
            &render,
            "x",
            1,
            &Uploaded {
                depth: Some("slate-depth.png".into()),
                size: Some((683, 384)),
                ..Default::default()
            },
        )
        .unwrap();
        let latent = depth_only
            .as_object()
            .unwrap()
            .values()
            .find(|n| n["class_type"] == "EmptyLatentImage")
            .unwrap();
        assert_eq!(latent["inputs"]["width"], 680);
        assert_eq!(latent["inputs"]["height"], 384);

        let picture = sources(&request(
            json!([{"node": 3, "text": "", "images": ["photo.jpg"]}]),
        ))
        .unwrap();
        let vary = plan(&c, None, &picture).unwrap();
        let graph = workflow(
            &vary,
            "timber",
            1,
            &Uploaded {
                image: Some("slate-image.jpg".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let kinds = classes(&graph);
        assert!(kinds.contains(&"VAEEncode".to_string()));
        assert!(kinds.contains(&"ImageScaleToTotalPixels".to_string()));
        let sampler = graph
            .as_object()
            .unwrap()
            .values()
            .find(|n| n["class_type"] == "KSampler")
            .unwrap();
        assert_eq!(sampler["inputs"]["denoise"], VARY_DENOISE);
    }

    #[test]
    fn a_style_port_picture_guides_the_prompt_and_is_not_the_source() {
        let c = catalog();
        let wired = request(json!([
            {"node": 3, "text": "", "images": ["photo.jpg"], "slot": "media"},
            {"node": 4, "text": "", "images": ["monet.jpg"], "slot": "style"},
            {"node": 2, "text": "park", "images": [], "slot": "prompt"}
        ]));
        let found = sources(&wired).unwrap();
        assert_eq!(found.image.as_deref(), Some("photo.jpg"));
        assert_eq!(found.style, Some((4, "monet.jpg".to_string())));
        let styled = plan(&c, None, &found).unwrap();
        assert_eq!(styled.task, Task::Vary);
        assert_eq!(
            styled.style,
            Some((
                "t2iadapter_style_sd14v1.pth".to_string(),
                "clip_vit_large_patch14.safetensors".to_string()
            ))
        );
        let graph = workflow(
            &styled,
            "park",
            1,
            &Uploaded {
                image: Some("slate-image.jpg".into()),
                style: Some("slate-style.jpg".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let kinds = classes(&graph);
        for class in [
            "CLIPVisionLoader",
            "CLIPVisionEncode",
            "StyleModelLoader",
            "StyleModelApply",
        ] {
            assert!(kinds.contains(&class.to_string()), "{class}");
        }
        // Style alone with a prompt generates; it never becomes the Vary source.
        let only = sources(&request(json!([
            {"node": 4, "text": "", "images": ["monet.jpg"], "slot": "style"}
        ])))
        .unwrap();
        assert!(only.image.is_none());
        assert_eq!(plan(&c, None, &only).unwrap().task, Task::Generate);
        // Weights that cannot steer the chosen family are named.
        let turbo = plan(&c, Some("sd_turbo.safetensors"), &only).unwrap_err();
        assert!(turbo.contains("style adapter that matches sd_turbo"));
        let mut bare = c.clone();
        bare.vision.clear();
        assert!(plan(&bare, None, &only)
            .unwrap_err()
            .contains("CLIP vision"));
        let two = sources(&request(json!([
            {"node": 4, "text": "", "images": ["a.jpg"], "slot": "style"},
            {"node": 5, "text": "", "images": ["b.jpg"], "slot": "style"}
        ])));
        assert!(two.unwrap_err().contains("Style reads one picture"));
    }

    #[test]
    fn a_generated_picture_takes_the_chosen_aspect_at_the_native_area() {
        assert_eq!(generate_size(512, 1.0), (512, 512));
        let (w, h) = generate_size(512, 16.0 / 9.0);
        assert!(w > h && w % 8 == 0 && h % 8 == 0);
        assert!(((w * h) as f32 / (512.0 * 512.0) - 1.0).abs() < 0.05);
        let c = catalog();
        let text = sources(&request(json!([{"node": 2, "text": "park", "images": []}]))).unwrap();
        let generate = plan(&c, None, &text).unwrap();
        let graph = workflow(
            &generate,
            "park",
            1,
            &Uploaded {
                aspect: atlas_agent::Aspect::Portrait,
                ..Default::default()
            },
        )
        .unwrap();
        let latent = graph
            .as_object()
            .unwrap()
            .values()
            .find(|n| n["class_type"] == "EmptyLatentImage")
            .unwrap();
        assert!(latent["inputs"]["height"].as_u64() > latent["inputs"]["width"].as_u64());
    }

    #[test]
    fn a_live_run_replaces_only_its_own_newest_frame() {
        let frame = |id: &str, live: &str| ImageOutput {
            id: id.into(),
            source: format!("{id}.png"),
            request: id.into(),
            prompt: "p".into(),
            model: String::new(),
            task: "render".into(),
            live: live.into(),
        };
        let mut bundle = ImageBundle {
            images: vec![frame("still", ""), frame("kept", "run-1")],
        };
        assert_eq!(settle_live(&mut bundle, None), None);
        assert_eq!(settle_live(&mut bundle, Some("run-2")), None);
        bundle.images.push(frame("a", "run-2"));
        assert_eq!(
            settle_live(&mut bundle, Some("run-2")).as_deref(),
            Some("a.png")
        );
        assert_eq!(bundle.images.len(), 2, "still and the kept frame remain");
    }

    #[test]
    fn extra_sources_are_refused_by_name() {
        let two_images = request(json!([
            {"node": 2, "text": "", "images": ["a.png"]},
            {"node": 3, "text": "", "images": ["b.png"]}
        ]));
        assert!(sources(&two_images)
            .unwrap_err()
            .contains("Disconnect 1 extra image."));
        let two_models = request(json!([
            {"node": 2, "text": "", "images": ["a.png"], "depth": "a-depth.png"},
            {"node": 3, "text": "", "images": ["b.png"], "depth": "b-depth.png"}
        ]));
        assert!(sources(&two_models).unwrap_err().contains("one 3D model"));
        let model_and_picture = request(json!([
            {"node": 2, "text": "", "images": ["a.png"], "depth": "a-depth.png"},
            {"node": 3, "text": "", "images": ["photo.jpg"]},
            {"node": 4, "text": "park", "images": []}
        ]));
        assert!(sources(&model_and_picture)
            .unwrap_err()
            .contains("Disconnect the picture"));
    }

    #[test]
    fn png_header_gives_capture_size() {
        let dir = std::env::temp_dir().join(format!("atlas-comfy-png-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("size.png");
        let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        bytes.extend_from_slice(&683u32.to_be_bytes());
        bytes.extend_from_slice(&384u32.to_be_bytes());
        std::fs::write(&path, bytes).unwrap();
        assert_eq!(png_size(&path), Some((683, 384)));
        std::fs::write(&path, b"not a png at all, no header").unwrap();
        assert_eq!(png_size(&path), None);
        let _ = std::fs::remove_dir_all(dir);
    }
}
