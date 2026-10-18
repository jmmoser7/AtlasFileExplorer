use crate::*;
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
    #[serde(default)]
    pub goal: Option<GoalVerdict>,
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
    #[serde(default)]
    goal: Option<GoalVerdict>,
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
            goal: raw.goal,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal: Option<GoalVerdict>,
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
