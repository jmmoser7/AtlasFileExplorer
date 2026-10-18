//! Image tool requests and collection, owned by the Codex adapter.
use super::*;

/// The instruction an image run sends. Codex's built-in image tool needs no
/// API key: it runs under the person's ChatGPT sign-in.
pub fn image_turn(request: &AgentRequest) -> String {
    image_turn_with_inputs(request, &image_inputs(request))
}

/// One ordered manifest drives both localImage attachments and their roles.
pub(super) fn image_inputs(request: &AgentRequest) -> Vec<(atlas_agent::InputSlot, &str)> {
    [atlas_agent::InputSlot::Media, atlas_agent::InputSlot::Style]
        .into_iter()
        .flat_map(|slot| {
            request
                .inputs
                .on(slot)
                .flat_map(move |item| item.images.iter().map(move |path| (slot, path.as_str())))
        })
        .collect()
}

pub(super) fn image_turn_with_inputs(
    request: &AgentRequest,
    inputs: &[(atlas_agent::InputSlot, &str)],
) -> String {
    use atlas_agent::{Aspect, InputSlot};
    let params = request.image.clone().unwrap_or_default();
    let count = params.images();
    let source = inputs.iter().any(|(slot, _)| *slot == InputSlot::Media);
    let shape = match params.aspect {
        Aspect::Source if source => "the same shape as the source image".to_string(),
        Aspect::Source => "square".to_string(),
        aspect => format!("{} (width:height)", aspect.label()),
    };
    let mut roles = String::new();
    for (index, (slot, path)) in inputs.iter().enumerate() {
        let role = if *slot == InputSlot::Media {
            "source to transform: keep its composition"
        } else {
            "style reference only: follow its look, not its subject"
        };
        roles.push_str(&format!(
            "Image {} is a {role}. Exact local path: {}.\n",
            index + 1,
            serde_json::to_string(path).unwrap_or_default()
        ));
    }
    if !inputs.is_empty() {
        let paths: Vec<_> = inputs.iter().map(|(_, path)| *path).collect();
        roles.push_str(&format!("Use these exact paths as the image_gen referenced_image_paths for every requested variation: {}. These are the images captured when the person submitted this request; do not substitute earlier originals, previous tool results, or imagined filenames. You may inspect these attached references with the image viewer.\n", serde_json::to_string(&paths).unwrap_or_default()));
    }
    let plural = if count == 1 { "" } else { "s" };
    let variants = if count > 1 {
        ", each a distinct variation"
    } else {
        ""
    };
    format!(
        "Create {count} image{plural} with your built-in image_gen tool, one image_gen call per image{variants}. Shape: {shape}. {roles}Do not run shell commands or manually write files, and do not ask questions: Slate collects the images from Codex's generated images folder. Use only image viewing and generation tools for this task. When done, reply with one short line.\n\nImage prompt:\n{}",
        request.prompt.trim()
    )
}

/// The instruction a text block sends through the person's ChatGPT sign-in.
pub fn oneshot_turn(request: &AgentRequest) -> String {
    format!(
        "Answer directly in this reply. Do not run commands, read or write files, or ask questions. Reply with only the requested text: no preamble, no headings, and no closing remarks. Any attached image is the picture the instruction refers to.\n\n{}",
        request.prompt.trim()
    )
}

/// The pictures one image run collects from `generated_images/<thread>`.
pub(super) struct Pictures {
    pub(super) watch: Option<std::path::PathBuf>,
    pub(super) dest: std::path::PathBuf,
    /// Files present before the run, and files already collected.
    pub(super) seen: std::collections::BTreeSet<std::path::PathBuf>,
    /// A file is taken once its size is the same on two scans.
    pub(super) sizes: BTreeMap<std::path::PathBuf, u64>,
    pub(super) found: usize,
    pub(super) last_scan: Option<Instant>,
    pub(super) settled: bool,
}

impl Pictures {
    pub(super) fn new(thread: &str, dest: std::path::PathBuf) -> Self {
        let watch = codex_home().map(|home| home.join("generated_images").join(thread));
        let seen = watch.as_deref().map(picture_files).unwrap_or_default();
        Self {
            watch,
            dest,
            seen: seen.into_iter().collect(),
            sizes: BTreeMap::new(),
            found: 0,
            last_scan: None,
            settled: false,
        }
    }

    /// The turn ended: take every remaining file without waiting a scan.
    pub(super) fn settle(&mut self) {
        self.settled = true;
    }

    /// Copy newly finished pictures into the album. True when one arrived.
    pub(super) fn collect(
        &mut self,
        request: &AgentRequest,
        session: &mut AgentSession,
    ) -> Result<bool, String> {
        if !self.settled
            && self
                .last_scan
                .is_some_and(|at| at.elapsed() < Duration::from_millis(250))
        {
            return Ok(false);
        }
        self.last_scan = Some(Instant::now());
        let Some(watch) = self.watch.as_deref() else {
            return Ok(false);
        };
        let mut arrived = false;
        for path in picture_files(watch) {
            if self.seen.contains(&path) {
                continue;
            }
            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            if size == 0 || (!self.settled && self.sizes.insert(path.clone(), size) != Some(size)) {
                continue;
            }
            std::fs::create_dir_all(&self.dest).map_err(|e| e.to_string())?;
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("png");
            let id = format!("{}-{}", request.id, self.found);
            let copy = self.dest.join(format!("{id}.{ext}"));
            std::fs::copy(&path, &copy).map_err(|e| e.to_string())?;
            self.seen.insert(path);
            self.found += 1;
            let vary = request
                .inputs
                .on(atlas_agent::InputSlot::Media)
                .any(|i| !i.images.is_empty());
            session.bundle.images.push(atlas_agent::ImageOutput {
                id,
                source: copy.to_string_lossy().into_owned(),
                request: request.id.clone(),
                prompt: request.prompt.clone(),
                model: request.model.clone().unwrap_or_else(|| "ChatGPT".into()),
                task: if vary { "vary" } else { "generate" }.into(),
                live: String::new(),
            });
            arrived = true;
        }
        Ok(arrived)
    }
}

/// Picture files in `dir`, oldest first.
pub(super) fn picture_files(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| {
                    ["png", "webp", "jpg", "jpeg"].contains(&x.to_ascii_lowercase().as_str())
                })
        })
        .map(|e| {
            let at = e.metadata().and_then(|m| m.modified()).ok();
            (at, e.path())
        })
        .collect();
    files.sort();
    files.into_iter().map(|(_, p)| p).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachment_order_and_role_manifest_handle_multiple_sources_and_styles() {
        let request: AgentRequest = serde_json::from_value(json!({
            "id":"run", "at":0, "prompt":"warmer light", "image":{"count":2},
            "inputs":{"revision":"captured","context":[],"wired":[
                {"node":3,"text":"","images":["C:/style a.png","C:/style b.png"],"slot":"style"},
                {"node":1,"text":"","images":["C:/shown result.png","C:/second source.png"],"slot":"media"}
            ]}
        })).unwrap();
        let manifest = image_inputs(&request);
        assert_eq!(
            manifest.iter().map(|(_, path)| *path).collect::<Vec<_>>(),
            [
                "C:/shown result.png",
                "C:/second source.png",
                "C:/style a.png",
                "C:/style b.png"
            ]
        );
        let prompt = image_turn_with_inputs(&request, &manifest);
        assert!(prompt.contains("Image 2 is a source to transform"));
        assert!(prompt.contains("Image 3 is a style reference only"));
        assert!(prompt.contains("referenced_image_paths for every requested variation"));
        assert!(prompt.contains(
            r#"["C:/shown result.png","C:/second source.png","C:/style a.png","C:/style b.png"]"#
        ));
        assert!(!prompt.contains("Do not run commands, read or write files"));
    }

    #[test]
    fn style_only_generation_does_not_call_the_style_a_primary_source() {
        let request: AgentRequest = serde_json::from_value(json!({
            "id":"style", "at":0, "prompt":"a pavilion", "image":{"aspect":"source"},
            "inputs":{"revision":"captured","context":[],"wired":[{"node":2,"text":"","images":["monet.jpg"],"slot":"style"}]}
        }))
        .unwrap();
        let prompt = image_turn(&request);
        assert!(prompt.contains("Shape: square"));
        assert!(prompt.contains("Image 1 is a style reference only"));
        assert!(!prompt.contains("source to transform"));
    }
}
