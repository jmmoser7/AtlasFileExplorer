//! Text panels read the last solved values from connected Grasshopper outputs.
use super::*;
use std::fmt::{self, Write};
use std::sync::{Arc, OnceLock};

pub(super) fn input_port() -> &'static GrasshopperPort {
    static PORT: OnceLock<GrasshopperPort> = OnceLock::new();
    PORT.get_or_init(|| GrasshopperPort {
        id: "f2ca3d77-27e4-42b2-8771-814396130005"
            .parse()
            .expect("text panel port UUID"),
        name: "Panel".into(),
        direction: GrasshopperPortDirection::Input,
        kind: GrasshopperPortType::Data,
        access: GrasshopperPortAccess::Tree,
    })
}

#[derive(Default)]
pub(in crate::app::grasshopper) struct Catalog {
    revision: Option<(u64, u64)>,
    has_definitions: bool,
    text_files: HashSet<slate_doc::ItemId>,
}

impl Catalog {
    fn load(tab: &SlateTab) -> Self {
        let mut catalog = Self {
            revision: Some((tab.id, tab.doc.item_paths_revision())),
            ..Self::default()
        };
        for item in &tab.doc.items {
            if item
                .path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("txt"))
            {
                catalog.text_files.insert(item.id);
            } else if slate_doc::media::is_grasshopper(&item.path) {
                catalog.has_definitions = true;
            }
        }
        catalog
    }
}

pub(super) fn eligible(tab: &SlateTab, id: NodeId) -> bool {
    let Some(node) = tab.doc.scene.node(id) else {
        return false;
    };
    if slate_doc::agent_chat::agent(node).is_some() {
        return false;
    }
    match &node.kind {
        NodeKind::Text(_) => true,
        NodeKind::Image(image) => tab.doc.item(image.item).is_some_and(|it| {
            it.path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("txt"))
        }),
        _ => false,
    }
}

#[derive(Default)]
pub(super) struct Panel {
    pub(super) wires: Vec<GrasshopperWire>,
    text: Option<Arc<str>>,
    admitted: bool,
    versions: Vec<Option<OutputRevision>>,
}

/// A new solve invalidates its old result before publishing the next packet.
struct OutputRevision {
    instance: u64,
    generation: u64,
    ready: bool,
    status: Option<String>,
}

impl OutputRevision {
    fn status(view: &View) -> Option<&str> {
        view.status.as_deref().filter(|_| view.outputs.is_none())
    }

    fn capture(view: &View) -> Self {
        Self {
            instance: view.instance,
            generation: view.generation,
            ready: view.outputs.is_some(),
            status: Self::status(view).map(str::to_owned),
        }
    }

    fn matches(&self, view: &View) -> bool {
        self.instance == view.instance
            && self.generation == view.generation
            && self.ready == view.outputs.is_some()
            && self.status.as_deref() == Self::status(view)
    }
}

impl Panel {
    pub(super) fn carry_display(&mut self, previous: &mut Self) {
        if self.wires == previous.wires {
            self.text = previous.text.take();
            self.versions = std::mem::take(&mut previous.versions);
        }
    }
}

impl SlateApp {
    pub(crate) fn is_grasshopper_text_panel(&self, id: NodeId) -> bool {
        let tab = self.tab();
        let catalog = &self.grasshopper.panel_catalog;
        if catalog.revision != Some((tab.id, tab.doc.item_paths_revision())) {
            return eligible(tab, id);
        }
        tab.doc.scene.node(id).is_some_and(|node| match &node.kind {
            NodeKind::Text(text) => text.agent.is_none(),
            NodeKind::Image(image) => {
                image.agent.is_none() && catalog.text_files.contains(&image.item)
            }
            _ => false,
        })
    }

    pub(crate) fn can_connect_grasshopper_output_to_panel(
        &self,
        source: NodeId,
        port: &GrasshopperPort,
        target: NodeId,
    ) -> bool {
        self.grasshopper_viewer(source)
            && port.direction == GrasshopperPortDirection::Output
            && self
                .grasshopper
                .views
                .get(&(self.tab().id, source))
                .is_some_and(|view| {
                    view.ports
                        .iter()
                        .any(|p| p.id == port.id && p.direction == GrasshopperPortDirection::Output)
                })
            && self.is_grasshopper_text_panel(target)
    }

    pub(crate) fn request_visible_grasshopper_panel(&mut self, id: NodeId, ctx: &egui::Context) {
        let tab = self.tab();
        if self.grasshopper.panel_catalog.revision != Some((tab.id, tab.doc.item_paths_revision()))
        {
            self.grasshopper.panel_catalog = Catalog::load(tab);
        }
        if !self.grasshopper.panel_catalog.has_definitions || !self.is_grasshopper_text_panel(id) {
            return;
        }
        self.sync_grasshopper_graph();
        let Some(panel) = self.grasshopper.graph.panels.get_mut(&id) else {
            return;
        };
        if panel.admitted {
            return;
        }
        panel.admitted = true;
        let sources: Vec<_> = panel.wires.iter().map(|wire| wire.source).collect();
        for source in sources {
            self.request_grasshopper(source, ctx);
        }
    }

    pub(crate) fn grasshopper_panel_text(&self, id: NodeId) -> Option<Arc<str>> {
        if self.grasshopper.graph.revision != Some(self.grasshopper_graph_revision()) {
            return None;
        }
        let panel = self.grasshopper.graph.panels.get(&id)?;
        static WAITING: OnceLock<Arc<str>> = OnceLock::new();
        Some(
            panel
                .text
                .as_ref()
                .unwrap_or_else(|| {
                    WAITING.get_or_init(|| Arc::from("Waiting for Grasshopper output…"))
                })
                .clone(),
        )
    }

    pub(super) fn refresh_grasshopper_panels(&mut self, tab: u64) {
        let views = &self.grasshopper.views;
        for panel in self.grasshopper.graph.panels.values_mut() {
            let source = |wire: &GrasshopperWire| views.get(&(tab, wire.source));
            let unchanged = panel.versions.len() == panel.wires.len()
                && panel
                    .wires
                    .iter()
                    .zip(&panel.versions)
                    .all(|(wire, prior)| match (source(wire), prior) {
                        (Some(view), Some(stamp)) => stamp.matches(view),
                        (None, None) => true,
                        _ => false,
                    });
            if panel.text.is_some() && unchanged {
                continue;
            }
            panel.versions.clear();
            panel.versions.extend(
                panel
                    .wires
                    .iter()
                    .map(|wire| source(wire).map(OutputRevision::capture)),
            );
            let mut text = PanelText::default();
            for (i, wire) in panel.wires.iter().enumerate() {
                let view = views.get(&(tab, wire.source));
                let port = view.and_then(|v| v.ports.iter().find(|p| p.id == wire.source_port));
                if panel.wires.len() > 1 {
                    if i > 0 && text.write_str("\n\n").is_err() {
                        break;
                    }
                    if writeln!(
                        text,
                        "{}",
                        port.map_or(wire.source_port.as_str(), |p| p.name.as_str())
                    )
                    .is_err()
                    {
                        break;
                    }
                }
                let packet = view
                    .and_then(|v| v.outputs.as_ref())
                    .and_then(|out| out.get(&wire.source_port));
                let result = match packet {
                    Some(packet) => format_packet(&mut text, packet),
                    None if view.is_some_and(|v| v.outputs.is_some()) => {
                        text.write_str("Output unavailable")
                    }
                    None => text.write_str(
                        view.and_then(|v| v.status.as_deref())
                            .unwrap_or("Waiting for Grasshopper output…"),
                    ),
                };
                if result.is_err() {
                    break;
                }
            }
            panel.text = Some(Arc::from(text.0));
        }
    }
}

// Cap the projection before allocating a galley or clipboard payload for a very large tree.
const MAX_PANEL_BYTES: usize = 256 * 1024;
#[derive(Default)]
struct PanelText(String);
impl Write for PanelText {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        const NOTE: &str = "\n… (panel display limit)";
        let space = (MAX_PANEL_BYTES - NOTE.len()).saturating_sub(self.0.len());
        if value.len() <= space {
            self.0.push_str(value);
            return Ok(());
        }
        let mut cut = space;
        while !value.is_char_boundary(cut) {
            cut -= 1;
        }
        self.0.push_str(&value[..cut]);
        self.0.push_str(NOTE);
        Err(fmt::Error)
    }
}

fn format_packet(out: &mut PanelText, packet: &serde_json::Value) -> fmt::Result {
    let Some(branches) = packet["branches"].as_array().filter(|v| !v.is_empty()) else {
        return out.write_str("<empty tree>");
    };
    for (b, branch) in branches.iter().enumerate() {
        if b != 0 {
            out.write_char('\n')?;
        }
        let path = branch["path"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default();
        let multiple_paths = branches.len() > 1 || path.len() != 1 || path[0].as_u64() != Some(0);
        if multiple_paths {
            out.write_char('{')?;
            for (i, index) in path.iter().enumerate() {
                if i > 0 {
                    out.write_char(';')?;
                }
                write!(out, "{index}")?;
            }
            out.write_str("}\n")?;
        }
        let items = branch["items"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default();
        if items.is_empty() {
            out.write_str("<empty branch>")?;
            continue;
        }
        for (i, item) in items.iter().enumerate() {
            if i != 0 {
                out.write_char('\n')?;
            }
            if items.len() > 1 || multiple_paths {
                write!(out, "{i}  ")?;
            }
            format_item(out, item)?;
        }
    }
    Ok(())
}

fn write_number(out: &mut PanelText, v: &serde_json::Value) -> fmt::Result {
    let Some(n) = v.as_f64() else {
        return out.write_str("<invalid number>");
    };
    // Grasshopper's panel uses six decimal places on ordinary values and
    // a short mantissa outside that range. The linked packet keeps its double.
    let magnitude = n.abs();
    if magnitude < 1.0e-12 {
        return out.write_char('0');
    }
    if !(1.0e-5..1.0e6).contains(&magnitude) {
        let value = format!("{n:.4e}");
        let (mantissa, exponent) = value.split_once('e').ok_or(fmt::Error)?;
        let trimmed = mantissa.trim_end_matches('0');
        out.write_str(trimmed)?;
        if trimmed.ends_with('.') {
            out.write_char('0')?;
        }
        out.write_char('e')?;
        if !exponent.starts_with('-') {
            out.write_char('+')?;
        }
        return out.write_str(exponent);
    }
    let digits = format!("{n:.6}");
    out.write_str(digits.trim_end_matches('0').trim_end_matches('.'))
}

fn format_item(out: &mut PanelText, item: &serde_json::Value) -> fmt::Result {
    let v = &item["value"];
    match item["kind"].as_str().unwrap_or("") {
        "number" => write_number(out, v),
        "integer" => match v.as_i64() {
            Some(value) => write!(out, "{value}"),
            None => out.write_str("<invalid integer>"),
        },
        "boolean" => out.write_str(match v.as_bool() {
            Some(true) => "True",
            Some(false) => "False",
            None => "<invalid boolean>",
        }),
        "text" => out.write_str(v.as_str().unwrap_or("")),
        "null" => out.write_str("<null>"),
        k @ ("point" | "color") => {
            out.write_str(if k == "point" { "(" } else { "RGBA(" })?;
            for (i, n) in v.as_array().into_iter().flatten().enumerate() {
                if i > 0 {
                    out.write_str(", ")?;
                }
                write_number(out, n)?;
            }
            out.write_char(')')
        }
        "geometry" => out.write_str(item["display"].as_str().unwrap_or("Geometry")),
        "display" => out.write_str(v.as_str().unwrap_or("Object")),
        _ => out.write_str("<unsupported value>"),
    }
}

#[cfg(test)]
mod tests;
