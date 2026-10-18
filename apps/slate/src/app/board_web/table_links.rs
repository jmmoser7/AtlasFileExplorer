//! Dashboard table links: bounded background reads and cached script serialization.

use super::*;
use crossbeam_channel::{bounded, Receiver, Sender, TrySendError};

const REFRESH: Duration = Duration::from_secs(1);
const MAX_PENDING: usize = 8;
const MAX_CACHED: usize = 64;

#[derive(Clone, Copy, PartialEq, Eq)]
struct Fingerprint {
    modified: Option<SystemTime>,
    len: u64,
}

struct Entry {
    checked: Instant,
    fingerprint: Option<Fingerprint>,
    value: serde_json::Value,
    pending: bool,
}

struct Request {
    path: PathBuf,
    previous: Option<Fingerprint>,
    ctx: egui::Context,
}

struct Reply {
    path: PathBuf,
    fingerprint: Option<Fingerprint>,
    /// None means the file has not changed, rather than missing content.
    changed: Option<serde_json::Value>,
}

type Wire = (Vec<u64>, String, PathBuf);

#[derive(Default)]
pub(super) struct TableLinks {
    entries: HashMap<PathBuf, Entry>,
    worker: Option<(Sender<Request>, Receiver<Reply>)>,
    revision: u64,
    scripts: HashMap<NodeId, (Vec<Wire>, u64, String)>,
}

fn health(state: &str) -> serde_json::Value {
    serde_json::json!({ "health": state, "columns": [], "rows": [] })
}

impl TableLinks {
    fn drain(&mut self) {
        let Some((_, rx)) = &self.worker else {
            return;
        };
        for reply in rx.try_iter().take(MAX_PENDING) {
            let Some(entry) = self.entries.get_mut(&reply.path) else {
                continue;
            };
            entry.checked = Instant::now();
            entry.pending = false;
            entry.fingerprint = reply.fingerprint;
            if let Some(value) = reply.changed {
                if entry.value != value {
                    entry.value = value;
                    self.revision = self.revision.wrapping_add(1);
                }
            }
        }
    }

    fn refresh(&mut self, path: &Path, ctx: &egui::Context) {
        let entry = self.entries.get(path);
        if entry.is_some_and(|e| e.pending || e.checked.elapsed() < REFRESH) {
            return;
        }
        if self.entries.values().filter(|e| e.pending).count() >= MAX_PENDING {
            return;
        }
        if entry.is_none() && self.entries.len() >= MAX_CACHED {
            let oldest = self
                .entries
                .iter()
                .filter(|(_, e)| !e.pending)
                .min_by_key(|(_, e)| e.checked)
                .map(|(p, _)| p.clone());
            if let Some(oldest) = oldest {
                self.entries.remove(&oldest);
            }
        }
        let previous = self.entries.get(path).and_then(|e| e.fingerprint);
        let (tx, _) = self.worker.get_or_insert_with(start_worker);
        match tx.try_send(Request {
            path: path.to_owned(),
            previous,
            ctx: ctx.clone(),
        }) {
            Ok(()) => {
                self.entries
                    .entry(path.to_owned())
                    .or_insert_with(|| Entry {
                        checked: Instant::now(),
                        fingerprint: None,
                        value: health("unknown"),
                        pending: false,
                    })
                    .pending = true;
            }
            Err(TrySendError::Full(_)) => {}
            Err(TrySendError::Disconnected(_)) => self.worker = None,
        }
    }

    fn script(&mut self, id: NodeId, wires: Vec<Wire>, ctx: &egui::Context) -> String {
        self.drain();
        for (_, _, path) in &wires {
            self.refresh(path, ctx);
        }
        if let Some((old_wires, revision, script)) = self.scripts.get(&id) {
            if old_wires == &wires && *revision == self.revision {
                return script.clone();
            }
        }
        let mut inputs = serde_json::Map::new();
        let mut used = HashSet::new();
        for (_, slot, path) in &wires {
            let mut slot = slot.clone();
            if used.contains(&slot) {
                slot = format!("{slot}-{}", used.len() + 1);
            }
            used.insert(slot.clone());
            let value = self
                .entries
                .get(path)
                .map(|e| e.value.clone())
                .unwrap_or_else(|| health("unknown"));
            inputs.insert(slot, value);
        }
        let script = atlas_agent::slate_link_script(&serde_json::Value::Object(inputs));
        if self.scripts.len() >= MAX_CACHED && !self.scripts.contains_key(&id) {
            self.scripts.clear();
        }
        self.scripts
            .insert(id, (wires, self.revision, script.clone()));
        script
    }
}

fn start_worker() -> (Sender<Request>, Receiver<Reply>) {
    let (tx, rx) = bounded::<Request>(MAX_PENDING);
    let (reply_tx, reply_rx) = bounded(MAX_PENDING);
    std::thread::spawn(move || {
        for request in rx {
            let reply = read_table(&request.path, request.previous);
            if reply_tx.send(reply).is_err() {
                break;
            }
            request.ctx.request_repaint();
        }
    });
    (tx, reply_rx)
}

/// All directory-entry checks and byte reads stay on the worker. In particular,
/// cloud placeholders are refused before read_linked_table can open the file.
fn read_table(path: &Path, previous: Option<Fingerprint>) -> Reply {
    let fingerprint = if atlas_core::cloud::is_dehydrated(path) {
        None
    } else {
        std::fs::metadata(path).ok().map(|m| Fingerprint {
            modified: m.modified().ok(),
            len: m.len(),
        })
    };
    table_reply(
        path,
        fingerprint,
        previous,
        atlas_core::table::read_linked_table,
    )
}

fn table_reply(
    path: &Path,
    mut fingerprint: Option<Fingerprint>,
    previous: Option<Fingerprint>,
    read: impl FnOnce(&Path) -> Option<atlas_core::table::LinkedTable>,
) -> Reply {
    let changed = if fingerprint.is_some() && fingerprint == previous {
        None
    } else if let Some(table) = fingerprint.and_then(|_| read(path)) {
        Some(serde_json::json!({
            "health": "ok", "columns": table.columns, "rows": table.rows,
        }))
    } else {
        // A share/Excel lock may permit metadata but refuse the bytes. Only a
        // successful parse can suppress the next read at unchanged metadata.
        fingerprint = None;
        Some(health("missing"))
    };
    Reply {
        path: path.to_owned(),
        fingerprint,
        changed,
    }
}

impl SlateApp {
    /// Table binding discovery stays in the board adapter; table I/O and
    /// serialization share one cache, rather than repeating for every page paint.
    pub(super) fn dashboard_link_script(&mut self, id: NodeId, ctx: &egui::Context) -> String {
        if self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::scene::artifact)
            .is_some()
        {
            return String::new();
        }
        use slate_doc::agent_inputs::{endpoint_node, InputKind};
        let mut wires = Vec::new();
        for node in &self.doc().scene.nodes {
            let NodeKind::Connector(conn) = &node.kind else {
                continue;
            };
            let Some(binding) = &conn.binding else {
                continue;
            };
            if binding.kind != InputKind::Table
                || endpoint_node(binding.target_end(conn)) != Some(id)
            {
                continue;
            }
            let Some(source_id) = endpoint_node(binding.source_end(conn)) else {
                continue;
            };
            let Some(source) = self.doc().scene.node(source_id) else {
                continue;
            };
            let NodeKind::Image(image) = &source.kind else {
                continue;
            };
            let Some(item) = self.doc().item(image.item) else {
                continue;
            };
            let path = super::super::board_portal::resolve_source(
                self.tab().path.as_deref(),
                &item.path.to_string_lossy(),
            );
            let slot = binding
                .slot
                .clone()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "data".into());
            let order = if binding.order.is_empty() {
                vec![node.id.0]
            } else {
                binding.order.clone()
            };
            wires.push((order, slot, path));
        }
        wires.sort_by(|a, b| a.0.cmp(&b.0));
        self.web.link_cache.script(id, wires, ctx)
    }
}

#[cfg(test)]
mod tests;
