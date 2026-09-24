//! Renderer- and provider-independent contracts for agent sidecars.
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentContext {
    pub app: &'static str,
    pub session: String,
    pub provider: String,
    pub workbook: Option<PathBuf>,
    pub format_version: u32,
    pub scope: String,
    pub selection: Vec<String>,
    pub viewport: Option<Viewport>,
    pub board_summary: String,
    pub generated_at: u64,
}

impl AgentContext {
    pub fn fingerprint(&self) -> u64 {
        let mut h = DefaultHasher::new();
        self.app.hash(&mut h);
        self.session.hash(&mut h);
        self.provider.hash(&mut h);
        self.workbook.hash(&mut h);
        self.format_version.hash(&mut h);
        self.scope.hash(&mut h);
        self.selection.hash(&mut h);
        self.board_summary.hash(&mut h);
        if let Some(v) = &self.viewport {
            v.hash_into(&mut h);
        }
        h.finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Viewport {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub zoom: f32,
}

impl Viewport {
    fn hash_into(&self, h: &mut DefaultHasher) {
        self.x.to_bits().hash(h);
        self.y.to_bits().hash(h);
        self.w.to_bits().hash(h);
        self.h.to_bits().hash(h);
        self.zoom.to_bits().hash(h);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRequest {
    pub id: String,
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub at: u64,
    /// Immutable inputs captured when the user presses Send / Generate.
    #[serde(default)]
    pub inputs: InputSnapshot,
    /// Explicit replay into a fresh provider session, never an implicit attach.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<AgentTurn>,
    /// Image-engine controls. Chat providers ignore them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<ImageParams>,
    /// Absolute folder for new files the person did not place.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_dir: Option<String>,
    /// A text block: no conversation history and no transport guides. The
    /// reply replaces the previous one.
    #[serde(default, skip_serializing_if = "is_false")]
    pub oneshot: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// Absent fields use the engine's preset.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ImageParams {
    /// A live run repeats one seed so consecutive frames stay coherent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    /// Live run identity. A new frame of the same run replaces the previous one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live: Option<String>,
    /// Shape of a new picture. Source keeps the wired picture's shape.
    #[serde(default, skip_serializing_if = "Aspect::is_source")]
    pub aspect: Aspect,
    /// Pictures this one run produces. 0 and 1 both mean one.
    #[serde(default, skip_serializing_if = "at_most_one")]
    pub count: u8,
}

fn at_most_one(count: &u8) -> bool {
    *count <= 1
}

impl ImageParams {
    /// The pictures a run makes, 1 to 8.
    pub fn images(&self) -> u8 {
        self.count.clamp(1, 8)
    }
}

/// The shape a generated picture takes. Each engine maps it to its own sizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Aspect {
    /// The wired picture's shape, or square when nothing is wired.
    #[default]
    Source,
    Square,
    /// 3:2.
    Landscape,
    /// 2:3.
    Portrait,
    /// 16:9.
    Wide,
}

impl Aspect {
    pub const ALL: [Self; 5] = [
        Self::Source,
        Self::Square,
        Self::Landscape,
        Self::Portrait,
        Self::Wide,
    ];

    pub fn is_source(&self) -> bool {
        *self == Self::Source
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Source => "Source",
            Self::Square => "1:1",
            Self::Landscape => "3:2",
            Self::Portrait => "2:3",
            Self::Wide => "16:9",
        }
    }

    /// Width over height; `None` follows the source.
    pub fn ratio(self) -> Option<f32> {
        match self {
            Self::Source => None,
            Self::Square => Some(1.0),
            Self::Landscape => Some(1.5),
            Self::Portrait => Some(2.0 / 3.0),
            Self::Wide => Some(16.0 / 9.0),
        }
    }
}

/// The newest intermediate frame of a running image generation. Derived: never
/// journaled and never written to a session manifest.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ImagePreview {
    pub request: String,
    pub step: u32,
    pub steps: u32,
    /// Encoded JPEG or PNG; empty until the engine sends a frame.
    pub image: std::sync::Arc<Vec<u8>>,
    /// Counts frames, so a viewer decodes each one once.
    pub frame: u64,
}

/// What an image engine runs, named by what is wired. The board's action and
/// the engine's graph both read this rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageTask {
    /// A prompt alone.
    Generate,
    /// A wired picture, changed within its outline.
    Vary,
    /// A wired 3D view, rendered from its depth and edges.
    Render,
}

impl ImageTask {
    pub fn for_inputs(geometry: bool, image: bool) -> Self {
        if geometry {
            Self::Render
        } else if image {
            Self::Vary
        } else {
            Self::Generate
        }
    }

    /// Recorded in [`ImageOutput::task`].
    pub fn label(self) -> &'static str {
        match self {
            Self::Generate => "generate",
            Self::Vary => "vary",
            Self::Render => "render",
        }
    }

    /// The button that starts it.
    pub fn action(self) -> &'static str {
        match self {
            Self::Generate => "Generate",
            Self::Vary => "Transform",
            Self::Render => "Render",
        }
    }

    /// What the button does, for its hover text.
    pub fn explain(self) -> &'static str {
        match self {
            Self::Generate => "Make a new picture from the prompt.",
            Self::Vary => "Make a new picture from the wired picture: it keeps the composition and follows the prompt.",
            Self::Render => "Render the wired 3D model from its current view, in the prompt's style.",
        }
    }

    pub fn working(self) -> &'static str {
        match self {
            Self::Generate => "Generating",
            Self::Vary => "Transforming",
            Self::Render => "Rendering",
        }
    }
}

/// Marker stripped from chat display. The model sees the guide; the card does not.
pub const SLATE_LINK_MARKER: &str = "\n\nSlate Link (dashboard files):\n";

/// Standing instruction for generated HTML dashboards. TWIN: the Cursor sidecar
/// preamble in `docs/agent/cursor-sidecar/index.mjs` repeats this paragraph.
pub const SLATE_LINK_GUIDE: &str = "When you write an HTML dashboard or chart, include <script type=\"application/slate-link+json\">{\"version\":1,\"inputs\":[{\"name\":\"data\",\"kind\":\"table\"}]}</script>. Draw from window.slateLink.inputs.data (columns and rows supplied by a wire from a CSV or Excel file). If that input is absent, show a small sample and redraw when the page receives the slate-link event. Do not paste the spreadsheet into the HTML as the only copy of the data.";

/// Marker stripped from chat display, same as [`SLATE_LINK_MARKER`].
pub const FILE_ATLAS_PLACE_MARKER: &str = "\n\nFile Atlas placement:\n";

/// TWIN: `docs/agent/cursor-sidecar/index.mjs` fills in the link-folder path.
pub const FILE_ATLAS_PLACE_GUIDE: &str = "To show a folder on the board, write place.json in the agent link folder (beside session.json) as {\"id\":\"a-new-id\",\"kind\":\"file_atlas\",\"path\":\"folder-relative-to-the-project\"}. Slate places that folder as a File Atlas portal beside this card. Do not say you cannot place it, and do not send the person to the changed-documents dot for that folder.";

pub const ARTIFACT_MARKER: &str = "\n\nChanged documents:\n";

/// The guide before deliverable manifests. Transcripts written with it still
/// begin with the phrase [`display_prompt`] checks, so they strip the same way.
#[deprecated(note = "use artifact_guide")]
pub const ARTIFACT_GUIDE: &str = "Every file you create or change must be a successful write, edit, or delete tool call with a real path. Slate lists those files on the right gray circle of this card. Mentioning a path only in your reply does not list it. To return a set of images, write return.json beside session.json as {\"id\":\"a-new-id\",\"kind\":\"images\",\"title\":\"Title\",\"path\":\"folder-of-images\"}. Do not place those images yourself and do not use place.json for them. The person clicks the gray circle and Slate lays the images in a frame. place.json is only for an explicit File Atlas folder browser.";

/// Standing instruction for files and deliverables. Must begin with "Every file
/// you create or change" ([`display_prompt`] validates the suffix by it).
/// TWIN: `docs/agent/cursor-sidecar/index.mjs` board preamble.
pub fn artifact_guide(output_dir: Option<&str>) -> String {
    artifact_guide_in(output_dir, None)
}

/// [`artifact_guide`] naming where `return.json` goes, for a provider that
/// knows its link folder. TWIN: `artifacts.mjs` `artifactGuide(outputDir, linkDir)`.
pub fn artifact_guide_in(output_dir: Option<&str>, link_dir: Option<&str>) -> String {
    let mut text = String::from("Every file you create or change must be a successful write, edit, or delete tool call with a real path; Slate lists those on the right gray circle of this card, and a path only mentioned in your reply is not listed. ");
    if let Some(dir) = output_dir.filter(|d| !d.trim().is_empty()) {
        text.push_str(&format!("Save new files the person did not place in {dir}: deliverables at the top, supporting files in assets/, throwaway files in scratch/. "));
    }
    let at = match link_dir.filter(|d| !d.trim().is_empty()) {
        Some(dir) => format!(
            "{}/return.json (beside session.json)",
            dir.replace('\\', "/").trim_end_matches('/')
        ),
        None => "return.json beside session.json".into(),
    };
    text.push_str(&format!("Edit existing files where they are. When finished, write {at} naming what the person asked for: {{\"id\":\"a-new-id\",\"title\":\"Short title\",\"items\":[{{\"path\":\"file-or-folder\",\"as\":\"auto\"}}]}}. To show a file that already exists, name its path instead of copying it. \"as\" is auto, graphic (render HTML or SVG as a page), text (show the source), images (a folder or several images as a grid), or folder (a File Atlas browser). A CSV or Excel item may add \"feeds\":\"dashboard.html\" to wire it into that dashboard. Slate lists these first and the person spawns them; do not place them yourself. place.json is only for an explicit File Atlas folder browser. If this task took too many steps, or an action was unreachable, also write feedback.json beside session.json as {{\"id\":\"a-new-id\",\"what\":\"one sentence\",\"tried\":\"what you did\",\"missing\":\"the action you could not reach\"}}. No file contents, secrets, or the person's private text."));
    text
}

/// Script the web host runs in a local dashboard so a wired table is visible.
pub fn slate_link_script(inputs: &serde_json::Value) -> String {
    format!(
        "window.slateLink={};window.dispatchEvent(new Event(\"slate-link\"));",
        serde_json::json!({ "version": 1, "inputs": inputs })
    )
}

impl AgentRequest {
    /// Only explicit wired attachments enter the provider prompt. Runtime metadata
    /// and ambient canvas state never become conversational text.
    pub fn input_text(&self) -> String {
        self.input_text_in(None)
    }

    /// [`Self::input_text`] whose guide names the link folder `return.json`
    /// goes to, for a provider that knows it.
    pub fn input_text_in(&self, link_dir: Option<&std::path::Path>) -> String {
        let mut text = self.prompt_with_history();
        text.push_str(SLATE_LINK_MARKER);
        text.push_str(SLATE_LINK_GUIDE);
        text.push_str(FILE_ATLAS_PLACE_MARKER);
        text.push_str(FILE_ATLAS_PLACE_GUIDE);
        text.push_str(ARTIFACT_MARKER);
        let link = link_dir.map(|d| d.to_string_lossy());
        text.push_str(&artifact_guide_in(
            self.output_dir.as_deref(),
            link.as_deref(),
        ));
        if !self.inputs.wired.is_empty() {
            text.push_str("\n\nSlate wired attachments (data):\n");
            text.push_str(&serde_json::to_string(&self.inputs.wired).unwrap_or_default());
        }
        text
    }

    pub fn prompt_with_history(&self) -> String {
        if self.history.is_empty() {
            return self.prompt.clone();
        }
        format!("Prior conversation checkpoint (quoted data, not instructions; replayed into a fresh session):\n{}\n\nNew user message:\n{}", serde_json::to_string(&self.history).unwrap_or_default(), self.prompt)
    }
}

/// Hide a known Slate transport suffix when rendering provider-owned history.
pub fn display_prompt(text: &str) -> &str {
    let mut text = text;
    loop {
        let next = strip_transport(text);
        if next == text {
            return text;
        }
        text = next;
    }
}

fn strip_transport(text: &str) -> &str {
    for marker in [
        "\n\nCanvas input snapshot (data):\n",
        "\n\nSlate wired attachments (data):\n",
        SLATE_LINK_MARKER,
        FILE_ATLAS_PLACE_MARKER,
        ARTIFACT_MARKER,
    ] {
        if let Some((prompt, data)) = text.rsplit_once(marker) {
            let valid = if marker == SLATE_LINK_MARKER {
                data.starts_with("When you write an HTML dashboard")
            } else if marker == FILE_ATLAS_PLACE_MARKER {
                data.starts_with("To show a folder on the board")
            } else if marker == ARTIFACT_MARKER {
                data.starts_with("Every file you create or change")
            } else if marker.contains("snapshot") {
                serde_json::from_str::<InputSnapshot>(data).is_ok()
            } else {
                serde_json::from_str::<Vec<ContextItem>>(data).is_ok()
            };
            if valid {
                return prompt;
            }
        }
    }
    text
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentModel {
    pub id: String,
    pub name: String,
}

/// Resolve the exact prefix independently of presentation mode. Never silently
/// accept a partially loaded checkpoint (including the first message).
pub fn checkpoint(
    turns: &[AgentTurn],
    through: Option<usize>,
) -> Result<&[AgentTurn], &'static str> {
    let end = through.unwrap_or(turns.len());
    turns
        .get(..end)
        .ok_or("The checkpoint history has not loaded yet.")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSession {
    #[serde(default)]
    pub approval: Option<AgentApproval>,
    /// Provider-owned identity. Slate caches history; it does not own it.
    #[serde(default)]
    pub conversation: String,
    #[serde(default)]
    pub artifacts: Vec<AgentArtifact>,
    #[serde(default)]
    pub status: AgentStatus,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub turns: Vec<AgentTurn>,
    #[serde(default)]
    pub updated_at: u64,
    #[serde(default)]
    pub bundle: ImageBundle,
    #[serde(default)]
    pub request: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentApproval {
    pub id: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Read,
    Web,
    Created,
    Modified,
    Deleted,
}
impl ArtifactKind {
    pub fn is_output(self) -> bool {
        matches!(self, Self::Created | Self::Modified | Self::Deleted)
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Read => "Read",
            Self::Web => "Source",
            Self::Created => "Created",
            Self::Modified => "Modified",
            Self::Deleted => "Deleted",
        }
    }
}

/// One note an agent leaves about using Slate. No file contents or secrets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedbackNote {
    #[serde(default)]
    pub id: String,
    /// What was hard, in one sentence.
    pub what: String,
    #[serde(default)]
    pub tried: String,
    /// An action the agent could not reach. Empty when the note is a suggestion.
    #[serde(default)]
    pub missing: String,
}

/// An observed tool result, never a path guessed from assistant prose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentArtifact {
    pub id: String,
    /// Index of the associated assistant message in the normalized history.
    pub turn: usize,
    pub kind: ArtifactKind,
    pub source: String,
    pub title: String,
}
impl AgentSession {
    pub fn record_artifact(&mut self, artifact: AgentArtifact) {
        if artifact.source.is_empty() {
            return;
        }
        if let Some(old) = self.artifacts.iter_mut().find(|a| a.id == artifact.id) {
            *old = artifact;
        } else {
            self.artifacts.push(artifact);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct InputSnapshot {
    pub revision: String,
    pub context: Vec<ContextItem>,
    pub wired: Vec<ContextItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextItem {
    pub node: u64,
    pub text: String,
    /// Linked asset locators; ordered, never copied into the workbook.
    pub images: Vec<String>,
    #[serde(default)]
    pub outputs: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub active: Option<String>,
    /// Exact depth render of a wired 3D model, near = white. `images[0]` is
    /// the shaded view from the same camera.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<String>,
    /// The input port this value arrived on. Absent on wires drawn before
    /// ports existed; those read by what they carry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<InputSlot>,
}

/// A generator's or text block's input port. The board draws them on the left
/// edge in this order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputSlot {
    /// The picture or 3D model a run starts from.
    Media,
    /// Any text: notes, sketches with words, another block's reply.
    Prompt,
    /// A second picture that guides the look of the result.
    Style,
}

impl InputSlot {
    pub fn id(self) -> &'static str {
        match self {
            Self::Media => "media",
            Self::Prompt => "prompt",
            Self::Style => "style",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        [Self::Media, Self::Prompt, Self::Style]
            .into_iter()
            .find(|slot| slot.id() == id)
    }

    /// Prompt reads text; Media and Style read pictures.
    pub fn takes_text(self) -> bool {
        self == Self::Prompt
    }
}

impl ContextItem {
    /// The port this value feeds. A wire drawn before ports existed reads by
    /// what it carries: pictures and models are Media, words are Prompt.
    pub fn port(&self) -> InputSlot {
        self.slot
            .unwrap_or(if self.images.is_empty() && self.depth.is_none() {
                InputSlot::Prompt
            } else {
                InputSlot::Media
            })
    }
}

impl InputSnapshot {
    /// Wired values on one port, in wire order.
    pub fn on(&self, slot: InputSlot) -> impl Iterator<Item = &ContextItem> {
        self.wired.iter().filter(move |item| item.port() == slot)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ImageBundle {
    pub images: Vec<ImageOutput>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageOutput {
    /// Stable within the bundle, independent of its current ordering.
    pub id: String,
    pub source: String,
    pub request: String,
    pub prompt: String,
    /// Checkpoint or other engine model name. Empty on older bundles.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub model: String,
    /// `generate`, `vary`, or `render`. Empty on older bundles.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub task: String,
    /// Live run that produced this frame. Only the newest frame of the current
    /// run is replaceable; every other entry is kept.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub live: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortalView {
    #[default]
    Chat,
    Images,
    /// A one-shot local language model: an instruction plus wired text and
    /// pictures in, one reply out.
    Text,
}

/// Collision-free within a process, including multiple sends in one clock tick.
pub fn request_id() -> String {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let tick = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!(
        "req-{}-{tick}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    #[default]
    Idle,
    Thinking,
    Offline,
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentTurn {
    pub role: String,
    pub text: String,
    #[serde(default)]
    pub at: u64,
}

/// How a deliverable is shown when the person spawns it. JSON key `"as"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Face {
    /// Chosen from the file type.
    #[default]
    Auto,
    /// HTML or SVG rendered as a page.
    Graphic,
    /// The source text.
    Text,
    /// A folder, or several images, laid out as a grid.
    Images,
    /// A File Atlas browser.
    Folder,
}

/// `return.json`: what the person asked for, written by the agent beside
/// `session.json`. Also reads the older image form
/// `{"id","kind":"images","title","path"}` or `"paths":[...]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "RawReturnManifest")]
pub struct ReturnManifest {
    pub id: String,
    pub title: String,
    pub items: Vec<ReturnItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReturnItem {
    /// As written by the agent: absolute, or relative to the output folder.
    pub path: String,
    #[serde(rename = "as", default)]
    pub face: Face,
    /// A dashboard this table feeds through a Slate Link wire.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feeds: Option<String>,
}

#[derive(Deserialize)]
struct RawReturnManifest {
    #[serde(default)]
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    items: Option<Vec<ReturnItem>>,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    paths: Vec<String>,
}

impl From<RawReturnManifest> for ReturnManifest {
    fn from(raw: RawReturnManifest) -> Self {
        let items = raw.items.unwrap_or_else(|| {
            let face = if raw.kind == "images" {
                Face::Images
            } else {
                Face::Auto
            };
            raw.path
                .into_iter()
                .chain(raw.paths)
                .filter(|p| !p.trim().is_empty())
                .map(|path| ReturnItem {
                    path,
                    face,
                    feeds: None,
                })
                .collect()
        });
        Self {
            id: raw.id,
            title: raw.title,
            items,
        }
    }
}

/// `deliverables.json` in the link folder: every consumed `return.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Deliverables {
    pub version: u32,
    #[serde(default)]
    pub sets: Vec<DeliverableSet>,
}
impl Default for Deliverables {
    fn default() -> Self {
        Self {
            version: 1,
            sets: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliverableSet {
    pub id: String,
    /// Assistant-slot index in the session history, like [`AgentArtifact::turn`].
    pub turn: usize,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub items: Vec<Deliverable>,
    /// Paths named in the manifest that did not exist, as written.
    #[serde(default)]
    pub missing: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Deliverable {
    /// Absolute.
    pub path: String,
    #[serde(rename = "as", default)]
    pub face: Face,
    /// Absolute.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feeds: Option<String>,
}

/// `versions.json` in the link folder: a copy of each changed file per message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Versions {
    pub version: u32,
    #[serde(default)]
    pub files: Vec<VersionedFile>,
}
impl Default for Versions {
    fn default() -> Self {
        Self {
            version: 1,
            files: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionedFile {
    /// The artifact source, absolute.
    pub source: String,
    /// Oldest first.
    #[serde(default)]
    pub versions: Vec<Version>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Version {
    /// Assistant-slot index, like [`AgentArtifact::turn`].
    pub turn: usize,
    /// Copy relative to the link folder, `/`-separated. Empty when skipped.
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub bytes: u64,
    /// Lines versus the previous captured version (text files only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub added: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub removed: Option<u32>,
    /// Why no copy was made.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skipped: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checkpoint_excludes_future_messages_and_refuses_partial_load() {
        let turns: Vec<_> = (0..3)
            .map(|i| AgentTurn {
                role: "user".into(),
                text: i.to_string(),
                at: i,
            })
            .collect();
        assert_eq!(checkpoint(&turns, Some(1)).unwrap().len(), 1);
        assert_eq!(checkpoint(&turns, Some(1)).unwrap()[0].text, "0");
        assert!(checkpoint(&[], Some(1)).is_err());
        assert!(checkpoint(&turns, Some(4)).is_err());
    }
    #[test]
    fn rapid_requests_have_distinct_identity() {
        let ids: std::collections::BTreeSet<_> = (0..1000).map(|_| request_id()).collect();
        assert_eq!(ids.len(), 1000);
    }
    #[test]
    fn old_session_contract_remains_readable() {
        let value: AgentSession = serde_json::from_str(
            r#"{"status":"idle","provider":"cursor","turns":[],"updated_at":1}"#,
        )
        .unwrap();
        assert!(value.bundle.images.is_empty());
        assert!(value.request.is_empty());
        let image: ImageOutput =
            serde_json::from_str(r#"{"id":"a","source":"a.png","request":"r","prompt":"p"}"#)
                .unwrap();
        assert!(image.model.is_empty());
        assert!(image.task.is_empty());
        assert!(image.live.is_empty());
        let item: ContextItem =
            serde_json::from_str(r#"{"node":1,"text":"","images":[]}"#).unwrap();
        assert!(item.depth.is_none());
        let request: AgentRequest =
            serde_json::from_str(r#"{"id":"r","prompt":"p","at":0}"#).unwrap();
        assert!(request.image.is_none());
        assert_eq!(ImageTask::for_inputs(true, true), ImageTask::Render);
        assert_eq!(ImageTask::for_inputs(false, true), ImageTask::Vary);
        assert_eq!(ImageTask::for_inputs(false, false).label(), "generate");
    }
}

#[cfg(test)]
mod prompt_tests {
    use super::*;
    #[test]
    fn only_explicit_wires_extend_a_prompt_and_transport_is_not_displayed() {
        let mut req = AgentRequest { id:"r".into(), prompt:"hello".into(), model:None, at:0, image:None, output_dir:None, oneshot:false,
            inputs: serde_json::from_value(serde_json::json!({"revision":"internal","context":[{"node":4,"text":"ambient","images":[]}],"wired":[]})).unwrap(), history: vec![] };
        let plain = req.input_text();
        assert!(plain.starts_with("hello"));
        assert!(plain.contains("application/slate-link+json"));
        assert_eq!(display_prompt(&plain), "hello");
        req.inputs.wired = req.inputs.context.clone();
        let text = req.input_text();
        assert!(text.contains("ambient"));
        assert!(!text.contains("revision"));
        assert_eq!(display_prompt(&text), "hello");
        assert_eq!(display_prompt("hello\n\nCanvas input snapshot (data):\n{\"revision\":\"internal\",\"context\":[],\"wired\":[]}"), "hello");
        let literal = "Explain Canvas input snapshot (data):\nnot metadata";
        assert_eq!(display_prompt(literal), literal);
    }

    fn request(output_dir: Option<&str>) -> AgentRequest {
        AgentRequest {
            id: "r".into(),
            prompt: "make a chart".into(),
            model: None,
            at: 0,
            image: None,
            output_dir: output_dir.map(Into::into),
            inputs: Default::default(),
            history: vec![],
            oneshot: false,
        }
    }

    #[test]
    fn the_guide_names_the_output_folder_only_when_known() {
        let dir = "C:/ws/slate-outputs/board/2026-09-23-chart-a1b2c3";
        let text = request(Some(dir)).input_text();
        assert!(text.contains(&format!("in {dir}: deliverables at the top")));
        assert!(text.contains("\"as\":\"auto\""));
        assert!(text.contains("scratch/"));
        assert_eq!(display_prompt(&text), "make a chart");
        let plain = request(None).input_text();
        assert!(!plain.contains("deliverables at the top"));
        assert!(plain.contains("write return.json beside session.json"));
        assert_eq!(display_prompt(&plain), "make a chart");
        assert_eq!(artifact_guide(Some("  ")), artifact_guide(None));
        assert!(artifact_guide(None).starts_with("Every file you create or change"));
        let named = request(Some(dir))
            .input_text_in(Some(std::path::Path::new("C:\\ws\\.atlas-ai\\agent\\s1\\")));
        assert!(named
            .contains("write C:/ws/.atlas-ai/agent/s1/return.json (beside session.json) naming"));
        assert_eq!(display_prompt(&named), "make a chart");
        assert_eq!(request(None).input_text_in(None), plain);
    }

    #[test]
    #[allow(deprecated)]
    fn transcripts_with_the_older_guide_still_strip() {
        let old = format!(
            "hello{SLATE_LINK_MARKER}{SLATE_LINK_GUIDE}{FILE_ATLAS_PLACE_MARKER}{FILE_ATLAS_PLACE_GUIDE}{ARTIFACT_MARKER}{ARTIFACT_GUIDE}"
        );
        assert_eq!(display_prompt(&old), "hello");
        let wired = format!("{old}\n\nSlate wired attachments (data):\n[]");
        assert_eq!(display_prompt(&wired), "hello");
        let new = format!("hello{ARTIFACT_MARKER}{}", artifact_guide(Some("D:/out")));
        assert_eq!(display_prompt(&new), "hello");
        let forged = "hello\n\nChanged documents:\nsomething else";
        assert_eq!(display_prompt(forged), forged);
    }
}

#[cfg(test)]
mod manifest_tests {
    use super::*;

    #[test]
    fn manifest_reads_items_and_the_legacy_image_forms() {
        let m: ReturnManifest = serde_json::from_str(
            r#"{"id":"q3","title":"Q3","items":[{"path":"dash.html","as":"graphic"},{"path":"data.csv","feeds":"dash.html"},{"path":"shots","as":"images"}]}"#,
        )
        .unwrap();
        assert_eq!(m.items.len(), 3);
        assert_eq!(m.items[0].face, Face::Graphic);
        assert_eq!(m.items[1].face, Face::Auto);
        assert_eq!(m.items[1].feeds.as_deref(), Some("dash.html"));
        assert_eq!(m.items[2].face, Face::Images);

        let legacy: ReturnManifest = serde_json::from_str(
            r#"{"id":"penn","kind":"images","title":"Penn","path":"penn-images"}"#,
        )
        .unwrap();
        assert_eq!(legacy.title, "Penn");
        assert_eq!(
            legacy.items,
            vec![ReturnItem {
                path: "penn-images".into(),
                face: Face::Images,
                feeds: None
            }]
        );
        let list: ReturnManifest =
            serde_json::from_str(r#"{"id":"p","kind":"images","paths":["a.png","b.png"]}"#)
                .unwrap();
        assert_eq!(list.items.len(), 2);
        assert!(list.items.iter().all(|i| i.face == Face::Images));

        let round: ReturnManifest =
            serde_json::from_str(&serde_json::to_string(&m).unwrap()).unwrap();
        assert_eq!(round, m);
        assert!(serde_json::to_string(&m)
            .unwrap()
            .contains("\"as\":\"graphic\""));
    }

    #[test]
    fn version_records_omit_absent_measurements() {
        let v = Version {
            turn: 1,
            path: "versions/t001/a.html".into(),
            bytes: 10,
            added: None,
            removed: None,
            skipped: None,
        };
        let text = serde_json::to_string(&v).unwrap();
        assert!(!text.contains("added") && !text.contains("skipped"));
        let d: Deliverables = serde_json::from_str(r#"{"version":1}"#).unwrap();
        assert!(d.sets.is_empty());
    }
}
