//! What a conversation made, as the board lists it on a card's output circle.
//!
//! The link worker ([`crate::agent::AgentLink`]) consumes `return.json`, loads
//! `deliverables.json` and `versions.json`, and lists the pictures of image
//! sets ([`OutputWatch`]). The board receives the result as an immutable
//! [`LinkOutputs`] snapshot and splits one message's files into requested and
//! incidental lists with [`partition`], which does no I/O.

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::path::{Path, PathBuf};

use atlas_agent::{AgentSession, Deliverables, Face, VersionedFile, Versions};

use crate::deliverables::{consume_return, load_deliverables, normalize};
use crate::versions::load_versions;

/// Pictures an image set lays out at most.
pub const MAX_SET_IMAGES: usize = 40;
const RASTER: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "tif", "tiff", "avif",
];

/// One link folder's deliverables and versions, read on the link worker.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LinkOutputs {
    /// The folder `versions[].path` is relative to.
    pub link_dir: PathBuf,
    pub deliverables: Deliverables,
    pub versions: Versions,
    /// The conversation's recorded output folder (`output.json`).
    pub output_dir: Option<PathBuf>,
    /// Pictures of each `as: images` deliverable that is a folder, keyed by
    /// its path: raster files only, sorted, at most [`MAX_SET_IMAGES`], cloud
    /// placeholders left out.
    pub image_sets: HashMap<String, Vec<PathBuf>>,
    /// Deliverable paths that are folders.
    pub folders: HashSet<String>,
    /// The last `return.json` that could not be read.
    pub error: Option<String>,
}

/// Worker state for one link folder.
#[derive(Debug, Default)]
pub struct OutputWatch {
    loaded: bool,
    output_dir: Option<PathBuf>,
    error: Option<String>,
}

impl OutputWatch {
    /// Consume `return.json` when present and reload after a change. `roots`
    /// are the working folder and the AI workspace, tried after the output
    /// folder. Returns a snapshot the first time and after any change.
    /// Blocking I/O: call on the link worker.
    pub fn tick(
        &mut self,
        link_dir: &Path,
        session: &AgentSession,
        roots: &[PathBuf],
        captured: bool,
    ) -> Option<LinkOutputs> {
        let mut changed = captured || !self.loaded;
        if link_dir.join("feedback.json").is_file() {
            let _ = crate::feedback::consume(link_dir);
        }
        if link_dir.join("return.json").is_file() {
            let output = self.output_dir(link_dir);
            let mut bases: Vec<&Path> = output.iter().map(PathBuf::as_path).collect();
            bases.extend(roots.iter().map(PathBuf::as_path));
            match consume_return(link_dir, session, &bases) {
                Ok(Some(_)) => {
                    self.error = None;
                    changed = true;
                }
                Ok(None) => {}
                Err(e) => {
                    self.error = Some(e);
                    changed = true;
                }
            }
        }
        if !changed {
            return None;
        }
        self.loaded = true;
        let output_dir = self.output_dir(link_dir);
        Some(load(link_dir, output_dir, self.error.clone()))
    }

    fn output_dir(&mut self, link_dir: &Path) -> Option<PathBuf> {
        if self.output_dir.is_none() {
            self.output_dir = crate::agent::recorded_output_dir(link_dir);
        }
        self.output_dir.clone()
    }
}

/// What [`consume_return`] reads from a session: turn roles and artifact
/// turns, without message text.
pub fn skeleton(session: Option<&AgentSession>) -> AgentSession {
    AgentSession {
        approval: None,
        conversation: String::new(),
        artifacts: session.map(|s| s.artifacts.clone()).unwrap_or_default(),
        status: session.map(|s| s.status.clone()).unwrap_or_default(),
        provider: String::new(),
        turns: session
            .into_iter()
            .flat_map(|s| &s.turns)
            .map(|t| atlas_agent::AgentTurn {
                role: t.role.clone(),
                text: String::new(),
                at: t.at,
            })
            .collect(),
        updated_at: 0,
        bundle: Default::default(),
        request: String::new(),
    }
}

/// Read a link folder's records. Existence checks and directory listings
/// only; no deliverable's bytes are read. Blocking I/O.
pub fn load(link_dir: &Path, output_dir: Option<PathBuf>, error: Option<String>) -> LinkOutputs {
    let deliverables = load_deliverables(link_dir);
    let mut image_sets = HashMap::new();
    let mut folders = HashSet::new();
    for item in deliverables.sets.iter().flat_map(|s| &s.items) {
        let path = Path::new(&item.path);
        if atlas_core::cloud::is_dehydrated(path) || !path.is_dir() {
            continue;
        }
        folders.insert(item.path.clone());
        if item.face == Face::Images {
            image_sets.insert(item.path.clone(), list_images(path));
        }
    }
    LinkOutputs {
        link_dir: link_dir.to_path_buf(),
        deliverables,
        versions: load_versions(link_dir),
        output_dir,
        image_sets,
        folders,
        error,
    }
}

fn list_images(folder: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.path())
        .filter(|p| is_raster(p))
        .collect();
    files.sort();
    files.retain(|p| !atlas_core::cloud::is_dehydrated(p));
    files.truncate(MAX_SET_IMAGES);
    files
}

/// Raster pictures an image set shows. SVG diagrams are not photos.
pub fn is_raster(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| RASTER.contains(&e.to_ascii_lowercase().as_str()))
}

/// One capsule on a card's output circle.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputItem {
    /// Stable within the conversation: the comparison key of the path, or
    /// `set:<id>:images` for loose pictures collected into one image set.
    pub key: String,
    /// File stem, or the manifest title for an image set.
    pub name: String,
    /// The file or folder. For loose pictures, the first one.
    pub path: PathBuf,
    pub face: Face,
    /// A dashboard this table feeds, absolute.
    pub feeds: Option<PathBuf>,
    /// Pictures of an image set, in order. Empty for everything else.
    pub images: Vec<PathBuf>,
    pub folder: bool,
    /// Deleted in this message: listed, never spawned.
    pub deleted: bool,
    pub requested: bool,
    pub turn: usize,
    /// Title of the deliverable set this item came from.
    pub set_title: String,
    /// Captured version records of this file.
    pub versions: usize,
    /// Which of them this message produced, from 1; 0 when none was captured.
    pub version: usize,
    /// That version's copy, when a later message changed the file since: the
    /// card then shows what its own message made, not the live file.
    pub snapshot: Option<PathBuf>,
}

impl OutputItem {
    /// What spawning this capsule shows.
    pub fn shown_path(&self) -> &Path {
        self.snapshot.as_deref().unwrap_or(&self.path)
    }
}

/// One message's files: requested first, in manifest order, then the rest.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MessageOutputs {
    pub items: Vec<OutputItem>,
    /// How many of `items` are requested.
    pub requested: usize,
    /// Paths a manifest named that did not exist, as written.
    pub missing: Vec<String>,
    /// Title of the newest deliverable set in the message.
    pub title: String,
}

impl MessageOutputs {
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Items that can be placed on the board.
    pub fn spawnable(&self) -> impl Iterator<Item = &OutputItem> {
        self.items.iter().filter(|i| !i.deleted)
    }

    pub fn get(&self, key: &str) -> Option<&OutputItem> {
        self.items.iter().find(|i| i.key == key)
    }
}

/// Split the output files of the assistant turns in `turns` (one card's
/// window) into requested and incidental items.
///
/// Requested: the items named in those turns' deliverable sets; for a turn
/// with no set, the files it created or modified inside the output folder.
/// Everything else the turns created, modified, or deleted is incidental.
/// A deleted file is listed but not spawnable. Pure: no I/O.
pub fn partition(
    session: &AgentSession,
    outputs: &LinkOutputs,
    turns: Range<usize>,
) -> MessageOutputs {
    let mut out = MessageOutputs::default();
    let mut requested: Vec<OutputItem> = Vec::new();
    let mut set_turns = HashSet::new();
    for set in outputs
        .deliverables
        .sets
        .iter()
        .filter(|s| turns.contains(&s.turn))
    {
        set_turns.insert(set.turn);
        if !set.title.is_empty() {
            out.title = set.title.clone();
        }
        out.missing.extend(set.missing.iter().cloned());
        let mut loose: Option<usize> = None;
        for d in &set.items {
            let path = PathBuf::from(&d.path);
            let folder = outputs.folders.contains(&d.path);
            if d.face == Face::Images && !folder {
                if !is_raster(&path) {
                    continue;
                }
                let at = match loose {
                    Some(at) => at,
                    None => {
                        let entry = OutputItem {
                            key: format!("set:{}:images", set.id),
                            name: title_or(&set.title, "Images"),
                            images: Vec::new(),
                            face: Face::Images,
                            ..item(path.clone(), set.turn, &set.title, false)
                        };
                        let at = upsert(&mut requested, entry);
                        loose = Some(at);
                        at
                    }
                };
                if requested[at].images.len() < MAX_SET_IMAGES {
                    requested[at].images.push(path);
                }
                continue;
            }
            let mut entry = item(path, set.turn, &set.title, folder);
            entry.face = d.face;
            entry.feeds = d.feeds.as_ref().map(PathBuf::from);
            if d.face == Face::Images {
                entry.name = title_or(&set.title, &entry.name);
                entry.images = outputs.image_sets.get(&d.path).cloned().unwrap_or_default();
            }
            upsert(&mut requested, entry);
        }
    }
    let inside = outputs
        .output_dir
        .as_deref()
        .map(|d| format!("{}/", path_key(&d.to_string_lossy())));
    let mut incidental: Vec<OutputItem> = Vec::new();
    for a in session
        .artifacts
        .iter()
        .filter(|a| a.kind.is_output() && turns.contains(&a.turn))
    {
        let path = normalize(Path::new(&a.source));
        let key = path_key(&path.to_string_lossy());
        // return.json, place.json and the like are the agent talking to Slate.
        if key.contains("/.atlas-ai/") {
            continue;
        }
        let deleted = a.kind == atlas_agent::ArtifactKind::Deleted;
        if let Some(existing) = requested.iter_mut().find(|i| i.key == key) {
            if deleted && a.turn >= existing.turn {
                existing.deleted = true;
            }
            continue;
        }
        let in_output = !set_turns.contains(&a.turn)
            && !deleted
            && inside.as_deref().is_some_and(|d| key.starts_with(d));
        let mut entry = item(path, a.turn, "", false);
        entry.deleted = deleted;
        if in_output {
            upsert(&mut requested, entry);
        } else if let Some(at) = incidental.iter().position(|i| i.key == key) {
            if incidental[at].turn <= a.turn {
                incidental[at] = entry;
            }
        } else {
            incidental.push(entry);
        }
    }
    // A file later found in the requested list is not listed twice.
    incidental.retain(|i| !requested.iter().any(|r| r.key == i.key));
    for i in &mut requested {
        i.requested = true;
    }
    out.requested = requested.len();
    out.items = requested;
    out.items.extend(incidental);
    for i in &mut out.items {
        if !i.images.is_empty() {
            continue;
        }
        let Some(file) = versions_of(&outputs.versions, &i.path) else {
            continue;
        };
        i.versions = file.versions.len();
        // This message's own capture, else the newest one before it.
        let Some(at) = file.versions.iter().rposition(|v| v.turn <= i.turn) else {
            continue;
        };
        i.version = at + 1;
        let v = &file.versions[at];
        if at + 1 < file.versions.len() && v.skipped.is_none() {
            i.snapshot = Some(outputs.link_dir.join(&v.path));
        }
    }
    out
}

fn item(path: PathBuf, turn: usize, set_title: &str, folder: bool) -> OutputItem {
    let name = if folder {
        path.file_name()
    } else {
        path.file_stem().or_else(|| path.file_name())
    }
    .map(|n| n.to_string_lossy().into_owned())
    .unwrap_or_else(|| path.to_string_lossy().into_owned());
    OutputItem {
        key: path_key(&path.to_string_lossy()),
        name,
        path,
        face: Face::Auto,
        feeds: None,
        images: Vec::new(),
        folder,
        deleted: false,
        requested: false,
        turn,
        set_title: set_title.to_string(),
        versions: 0,
        version: 0,
        snapshot: None,
    }
}

fn title_or(title: &str, fallback: &str) -> String {
    if title.trim().is_empty() {
        fallback.to_string()
    } else {
        title.trim().to_string()
    }
}

/// A newer entry for the same key replaces the older one in place.
fn upsert(list: &mut Vec<OutputItem>, entry: OutputItem) -> usize {
    match list.iter().position(|i| i.key == entry.key) {
        Some(at) => {
            list[at] = entry;
            at
        }
        None => {
            list.push(entry);
            list.len() - 1
        }
    }
}

/// Paths compare with `/` separators, and case-insensitively on Windows.
pub fn path_key(path: &str) -> String {
    let key = path.replace('\\', "/");
    let key = key.trim_end_matches('/');
    if cfg!(windows) {
        key.to_lowercase()
    } else {
        key.to_string()
    }
}

/// Captured versions of one file, oldest first.
pub fn versions_of<'a>(versions: &'a Versions, path: &Path) -> Option<&'a VersionedFile> {
    let key = path_key(&path.to_string_lossy());
    versions.files.iter().find(|f| path_key(&f.source) == key)
}

/// The person's message that led to assistant `turn`, as the card shows it.
pub fn prompt_before(session: &AgentSession, turn: usize) -> Option<&str> {
    session
        .turns
        .get(..turn.min(session.turns.len()))?
        .iter()
        .rev()
        .find(|t| t.role == "user")
        .map(|t| atlas_agent::display_prompt(&t.text).trim())
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_agent::{
        AgentArtifact, AgentStatus, AgentTurn, ArtifactKind, Deliverable, DeliverableSet, Version,
    };

    fn session(artifacts: Vec<(usize, ArtifactKind, &str)>) -> AgentSession {
        AgentSession {
            approval: None,
            conversation: String::new(),
            artifacts: artifacts
                .into_iter()
                .enumerate()
                .map(|(i, (turn, kind, source))| AgentArtifact {
                    id: format!("a{i}"),
                    turn,
                    kind,
                    source: source.into(),
                    title: String::new(),
                })
                .collect(),
            status: AgentStatus::Idle,
            provider: "codex".into(),
            turns: ["user", "assistant", "user", "assistant"]
                .iter()
                .enumerate()
                .map(|(i, r)| AgentTurn {
                    role: (*r).into(),
                    text: format!("message {i}"),
                    at: 0,
                })
                .collect(),
            updated_at: 0,
            bundle: Default::default(),
            request: String::new(),
        }
    }

    fn set(id: &str, turn: usize, items: &[(&str, Face)]) -> DeliverableSet {
        DeliverableSet {
            id: id.into(),
            turn,
            title: "Q3 sales".into(),
            items: items
                .iter()
                .map(|(p, f)| Deliverable {
                    path: (*p).into(),
                    face: *f,
                    feeds: None,
                })
                .collect(),
            missing: vec!["gone.md".into()],
        }
    }

    #[test]
    fn manifest_items_are_requested_and_other_edits_sort_below() {
        let s = session(vec![
            (1, ArtifactKind::Created, "/p/out/dash.html"),
            (1, ArtifactKind::Modified, "/p/src/main.rs"),
            (1, ArtifactKind::Deleted, "/p/old.txt"),
            (1, ArtifactKind::Read, "/p/readme.md"),
            (3, ArtifactKind::Modified, "/p/later.rs"),
        ]);
        let mut outputs = LinkOutputs::default();
        outputs.deliverables.sets = vec![set(
            "q3",
            1,
            &[
                ("/p/out/dash.html", Face::Graphic),
                ("/p/out/a.png", Face::Images),
                ("/p/out/b.png", Face::Images),
                ("/p/out/c.svg", Face::Images),
                ("/p/out/data.csv", Face::Auto),
            ],
        )];
        let m = partition(&s, &outputs, 0..2);
        let names: Vec<_> = m.items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["dash", "Q3 sales", "data", "main", "old"]);
        assert_eq!(m.requested, 3);
        assert_eq!(m.items[1].images.len(), 2, "svg is not part of a photo set");
        assert!(m.items[..3].iter().all(|i| i.requested));
        assert!(!m.items[3].requested && !m.items[3].deleted);
        assert!(m.items[4].deleted);
        assert_eq!(m.spawnable().count(), 4);
        assert_eq!(m.missing, ["gone.md"]);
        assert_eq!(m.title, "Q3 sales");
        assert!(partition(&s, &outputs, 2..usize::MAX).items[0].name == "later");
    }

    #[test]
    fn each_message_names_and_shows_its_own_version() {
        let s = session(vec![
            (1, ArtifactKind::Created, "C:\\p\\dash.html"),
            (3, ArtifactKind::Modified, "C:\\p\\dash.html"),
        ]);
        let link = PathBuf::from("C:/ws/.atlas-ai/agent/s1");
        let mut outputs = LinkOutputs {
            link_dir: link.clone(),
            ..Default::default()
        };
        let version = |turn: usize, path: &str| Version {
            turn,
            path: path.into(),
            bytes: 1,
            added: Some(1),
            removed: Some(0),
            skipped: None,
        };
        outputs.versions.files = vec![VersionedFile {
            source: "C:/p/dash.html".into(),
            versions: vec![
                version(1, "versions/t001/dash.html"),
                version(3, "versions/t003/dash.html"),
            ],
        }];
        let first = &partition(&s, &outputs, 0..2).items[0];
        assert_eq!((first.versions, first.version), (2, 1));
        assert_eq!(first.shown_path(), link.join("versions/t001/dash.html"));
        let second = &partition(&s, &outputs, 2..4).items[0];
        assert_eq!((second.versions, second.version), (2, 2));
        assert_eq!(second.snapshot, None, "the newest version is the live file");
        assert_eq!(second.shown_path(), second.path);
    }

    #[test]
    fn link_folder_writes_are_not_outputs() {
        let (link_file, report) = if cfg!(windows) {
            (
                "C:\\ws\\.atlas-ai\\agent\\s1\\return.json",
                "C:\\p\\report.md",
            )
        } else {
            ("/ws/.atlas-ai/agent/s1/return.json", "/p/report.md")
        };
        let s = session(vec![
            (1, ArtifactKind::Created, link_file),
            (1, ArtifactKind::Created, report),
        ]);
        let m = partition(&s, &LinkOutputs::default(), 0..usize::MAX);
        assert_eq!(
            m.items.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(),
            ["report"]
        );
    }

    #[test]
    fn without_a_manifest_new_files_in_the_output_folder_are_requested() {
        let s = session(vec![
            (1, ArtifactKind::Modified, "/p/src/lib.rs"),
            (
                1,
                ArtifactKind::Created,
                "/p/slate-outputs/b/run/report.html",
            ),
            (
                1,
                ArtifactKind::Deleted,
                "/p/slate-outputs/b/run/scratch/tmp.txt",
            ),
        ]);
        let outputs = LinkOutputs {
            output_dir: Some(PathBuf::from("/p/slate-outputs/b/run")),
            ..Default::default()
        };
        let m = partition(&s, &outputs, 0..usize::MAX);
        assert_eq!(m.requested, 1);
        assert_eq!(m.items[0].name, "report");
        assert_eq!(
            m.items.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(),
            ["report", "lib", "tmp"]
        );
        assert!(
            m.items[2].deleted,
            "deleted files stay incidental and disabled"
        );
    }

    #[test]
    fn versions_and_prompts_are_found_by_path() {
        let mut s = session(vec![(3, ArtifactKind::Modified, "C:\\p\\dash.html")]);
        s.turns[2].text = format!(
            "make it blue{}{}",
            atlas_agent::ARTIFACT_MARKER,
            atlas_agent::artifact_guide(None)
        );
        let mut outputs = LinkOutputs::default();
        outputs.versions.files = vec![VersionedFile {
            source: "C:/p/dash.html".into(),
            versions: vec![
                Version {
                    turn: 1,
                    path: "versions/t001/dash.html".into(),
                    bytes: 1,
                    added: Some(3),
                    removed: Some(0),
                    skipped: None,
                },
                Version {
                    turn: 3,
                    path: "versions/t003/dash.html".into(),
                    bytes: 1,
                    added: Some(1),
                    removed: Some(1),
                    skipped: None,
                },
            ],
        }];
        let m = partition(&s, &outputs, 0..usize::MAX);
        assert_eq!(m.items[0].versions, 2);
        assert_eq!(prompt_before(&s, 3), Some("make it blue"));
        assert_eq!(prompt_before(&s, 1), Some("message 0"));
        assert_eq!(prompt_before(&s, 0), None);
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "atlas_ai_outputs_{name}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn sources_consume_a_return_only_once_the_roots_arrive() {
        let root = temp("sources");
        let link = root.join("link");
        let ws = root.join("ws");
        std::fs::create_dir_all(&link).unwrap();
        std::fs::create_dir_all(&ws).unwrap();
        std::fs::write(ws.join("a.md"), "# a").unwrap();
        std::fs::write(
            link.join("return.json"),
            r#"{"id":"x","items":[{"path":"a.md"}]}"#,
        )
        .unwrap();
        let mut sources = crate::agent::AgentSources::default();
        let _ = sources.poll(&link, None);
        std::thread::sleep(std::time::Duration::from_millis(200));
        let _ = sources.poll(&link, None);
        assert!(sources.outputs(&link).is_none());
        assert!(
            link.join("return.json").exists(),
            "no roots, nothing consumed"
        );
        sources.set_roots(&link, vec![ws.clone()]);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while sources.outputs(&link).is_none() {
            assert!(std::time::Instant::now() < deadline, "no snapshot");
            std::thread::sleep(std::time::Duration::from_millis(20));
            let _ = sources.poll(&link, None);
        }
        let outputs = sources.outputs(&link).unwrap();
        assert_eq!(outputs.deliverables.sets.len(), 1);
        assert_eq!(
            Path::new(&outputs.deliverables.sets[0].items[0].path),
            ws.join("a.md")
        );
        assert!(!link.join("return.json").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn the_watch_consumes_returns_against_its_roots_and_lists_image_folders() {
        let root = temp("watch");
        let link = root.join("link");
        let ws = root.join("ws");
        std::fs::create_dir_all(&link).unwrap();
        std::fs::create_dir_all(ws.join("shots")).unwrap();
        for name in ["b.png", "a.jpg", "skip.svg"] {
            std::fs::write(ws.join("shots").join(name), "x").unwrap();
        }
        let s = session(vec![]);
        let mut watch = OutputWatch::default();
        let first = watch
            .tick(&link, &s, std::slice::from_ref(&ws), false)
            .unwrap();
        assert!(first.deliverables.sets.is_empty());
        assert!(watch
            .tick(&link, &s, std::slice::from_ref(&ws), false)
            .is_none());
        std::fs::write(
            link.join("return.json"),
            r#"{"id":"shots","kind":"images","title":"Shots","path":"shots"}"#,
        )
        .unwrap();
        let next = watch
            .tick(&link, &s, std::slice::from_ref(&ws), false)
            .unwrap();
        let folder = ws.join("shots").to_string_lossy().into_owned();
        assert!(next.folders.contains(&folder));
        let pictures = &next.image_sets[&folder];
        assert_eq!(
            pictures
                .iter()
                .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            ["a.jpg", "b.png"]
        );
        let m = partition(&s, &next, 0..usize::MAX);
        assert_eq!(m.items[0].name, "Shots");
        assert_eq!(m.items[0].images.len(), 2);
        assert!(watch
            .tick(&link, &s, std::slice::from_ref(&ws), true)
            .is_some());
        std::fs::write(link.join("return.json"), "{broken").unwrap();
        assert!(watch.tick(&link, &s, &[ws], false).unwrap().error.is_some());
        let _ = std::fs::remove_dir_all(root);
    }
}
