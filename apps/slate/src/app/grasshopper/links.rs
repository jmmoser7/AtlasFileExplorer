//! Typed inputs share Slate's wire journal and the viewer's solve scheduler.
use super::*;
use slate_doc::scene::{
    grasshopper_wire, ConnectorEnd, GrasshopperPort, GrasshopperPortAccess,
    GrasshopperPortDirection, GrasshopperPortId, GrasshopperPortType, GrasshopperWire,
    GrasshopperWirePorts, Side,
};
use std::collections::{HashSet, VecDeque};

#[derive(Default)]
pub(super) struct Graph {
    revision: Option<(u64, u64, u64)>,
    incoming: HashMap<NodeId, Vec<GrasshopperWire>>,
    downstream: HashMap<NodeId, Vec<NodeId>>,
}

impl Graph {
    fn read(tab: &SlateTab, revision: (u64, u64, u64)) -> Self {
        let mut graph = Self {
            revision: Some(revision),
            ..Self::default()
        };
        for node in &tab.doc.scene.nodes {
            if let NodeKind::Connector(conn) = &node.kind {
                if let Some(wire) = grasshopper_wire(conn) {
                    graph
                        .downstream
                        .entry(wire.source)
                        .or_default()
                        .push(wire.target);
                    graph.incoming.entry(wire.target).or_default().push(wire);
                }
            }
        }
        graph
    }

    fn reaches(&self, start: NodeId, target: NodeId) -> bool {
        let mut stack = vec![start];
        let mut seen = HashSet::new();
        while let Some(node) = stack.pop() {
            if node == target {
                return true;
            }
            if seen.insert(node) {
                stack.extend(self.downstream.get(&node).into_iter().flatten().copied());
            }
        }
        false
    }

    fn order(&self, ids: impl Iterator<Item = NodeId>) -> (Vec<NodeId>, Vec<NodeId>) {
        let mut degrees: HashMap<_, _> = ids
            .map(|id| (id, self.incoming.get(&id).map_or(0, Vec::len)))
            .collect();
        let mut pending: VecDeque<_> = degrees
            .iter()
            .filter_map(|(&id, &degree)| (degree == 0).then_some(id))
            .collect();
        let mut order = Vec::with_capacity(degrees.len());
        while let Some(id) = pending.pop_front() {
            degrees.remove(&id);
            order.push(id);
            for target in self.downstream.get(&id).into_iter().flatten() {
                if let Some(degree) = degrees.get_mut(target) {
                    *degree -= 1;
                    if *degree == 0 {
                        pending.push_back(*target);
                    }
                }
            }
        }
        (order, degrees.into_keys().collect())
    }
}

fn literal_kind(value: &GrasshopperValue) -> Option<GrasshopperPortType> {
    Some(match value {
        GrasshopperValue::Number(n)
            if n.is_finite()
                && n.fract() == 0.0
                && *n >= f64::from(i32::MIN)
                && *n <= f64::from(i32::MAX) =>
        {
            GrasshopperPortType::Integer
        }
        GrasshopperValue::Number(n) if n.is_finite() => GrasshopperPortType::Number,
        GrasshopperValue::Bool(_) => GrasshopperPortType::Boolean,
        GrasshopperValue::Text(_) => GrasshopperPortType::Text,
        GrasshopperValue::Color(_) => GrasshopperPortType::Color,
        _ => return None,
    })
}

fn literal_packet(
    value: &GrasshopperValue,
    target: GrasshopperPortType,
) -> Option<serde_json::Value> {
    let kind = literal_kind(value)?;
    if !target.accepts(kind) {
        return None;
    }
    let value = match value {
        GrasshopperValue::Number(n) if kind == GrasshopperPortType::Integer => {
            serde_json::json!(*n as i32)
        }
        GrasshopperValue::Number(n) => serde_json::json!(n),
        GrasshopperValue::Bool(b) => serde_json::json!(b),
        GrasshopperValue::Text(s) => serde_json::json!(s),
        GrasshopperValue::Color(rgba) => serde_json::json!(rgba),
        GrasshopperValue::Choice(_) => return None,
    };
    Some(serde_json::json!({"branches":[{"path":[0],"items":[{"kind":kind,"value":value}]}]}))
}

impl SlateApp {
    fn grasshopper_graph_revision(&self) -> (u64, u64, u64) {
        (self.tab().id, self.scene_gen, self.doc().scene.scene_gen())
    }

    fn sync_grasshopper_graph(&mut self) {
        let revision = self.grasshopper_graph_revision();
        if self.grasshopper.graph.revision != Some(revision) {
            self.grasshopper.graph = Graph::read(self.tab(), revision);
            self.grasshopper.links_dirty = true;
        }
    }

    pub(crate) fn grasshopper_ports(&self, id: NodeId) -> &[GrasshopperPort] {
        self.grasshopper
            .views
            .get(&(self.tab().id, id))
            .map_or(&[], |view| &view.ports)
    }

    pub(crate) fn grasshopper_output_access(
        &self,
        id: NodeId,
        port: &GrasshopperPortId,
    ) -> Option<GrasshopperPortAccess> {
        let branches = self
            .grasshopper
            .views
            .get(&(self.tab().id, id))?
            .outputs
            .as_ref()?
            .get(port)?
            .get("branches")?
            .as_array()?;
        if branches.len() > 1 {
            return Some(GrasshopperPortAccess::Tree);
        }
        let branch = branches.first()?;
        Some(if branch.get("items")?.as_array()?.len() == 1 {
            GrasshopperPortAccess::Item
        } else {
            GrasshopperPortAccess::List
        })
    }

    pub(crate) fn connect_grasshopper_ports(
        &mut self,
        source: NodeId,
        source_port: &GrasshopperPortId,
        target: NodeId,
        target_port: &GrasshopperPortId,
        source_t: f32,
        target_t: f32,
    ) -> Result<NodeId, String> {
        if self.tab().read_only
            || !self.grasshopper_viewer(source)
            || !self.grasshopper_viewer(target)
            || [source, target]
                .iter()
                .any(|id| self.doc().scene.node(*id).is_some_and(|node| node.locked))
        {
            return Err("Both Grasshopper viewers must be editable.".into());
        }
        let output = self
            .grasshopper_ports(source)
            .iter()
            .find(|p| &p.id == source_port && p.direction == GrasshopperPortDirection::Output)
            .ok_or("That Grasshopper output is unavailable.")?;
        let input = self
            .grasshopper_ports(target)
            .iter()
            .find(|p| &p.id == target_port && p.direction == GrasshopperPortDirection::Input)
            .ok_or("That Grasshopper input is unavailable.")?;
        if !input.kind.accepts(output.kind) {
            return Err("The Grasshopper ports have incompatible types.".into());
        }
        let kind = output.kind;
        self.sync_grasshopper_graph();
        if self.grasshopper.graph.reaches(target, source) {
            return Err("That connection would create a Grasshopper loop.".into());
        }
        if self
            .grasshopper
            .graph
            .incoming
            .get(&target)
            .into_iter()
            .flatten()
            .any(|wire| &wire.target_port == target_port)
        {
            return Err("Disconnect the current wire before connecting this input.".into());
        }
        if !source_t.is_finite() || !target_t.is_finite() {
            return Err("Invalid Grasshopper grip position.".into());
        }
        let endpoint = |node, side, t| ConnectorEnd::Anchored { node, side, t };
        let mut node = self.build_connector(
            endpoint(source, Side::Right, source_t),
            endpoint(target, Side::Left, target_t),
        );
        let NodeKind::Connector(conn) = &mut node.kind else {
            unreachable!()
        };
        conn.binding = None;
        conn.grasshopper = Some(Box::new(GrasshopperWirePorts {
            source_port: source_port.clone(),
            target_port: target_port.clone(),
            kind,
            bundled: false,
        }));
        self.add_nodes(vec![node])
            .into_iter()
            .next()
            .ok_or_else(|| "Could not add the Grasshopper wire.".into())
    }

    pub(crate) fn grasshopper_literal(
        &self,
        id: NodeId,
        port: &GrasshopperPortId,
    ) -> Option<&GrasshopperValue> {
        grasshopper_item(self.tab(), id)?
            .0
            .grasshopper_literals
            .as_deref()?
            .get(port)
    }

    pub(crate) fn set_grasshopper_literal(
        &mut self,
        id: NodeId,
        port: &GrasshopperPortId,
        value: Option<GrasshopperValue>,
    ) -> Result<(), String> {
        if self.tab().read_only
            || !self.grasshopper_viewer(id)
            || self.doc().scene.node(id).is_some_and(|node| node.locked)
        {
            return Err("Unlock this Grasshopper viewer to change its input.".into());
        }
        if let Some(value) = &value {
            let input = self
                .grasshopper_ports(id)
                .iter()
                .find(|p| &p.id == port && p.direction == GrasshopperPortDirection::Input)
                .ok_or("That Grasshopper input is unavailable.")?;
            if !literal_kind(value).is_some_and(|kind| input.kind.accepts(kind)) {
                return Err("This value does not match the Grasshopper input type.".into());
            }
        }
        self.last_board_edit = None;
        self.patch_nodes(&[id], |node| {
            if let NodeKind::Image(image) = &mut node.kind {
                let values = image.grasshopper_literals.get_or_insert_with(Box::default);
                if let Some(value) = &value {
                    let _ = values.set(port.clone(), value.clone());
                } else {
                    values.remove(port);
                }
                if values.is_empty() {
                    image.grasshopper_literals = None;
                }
            }
        });
        self.last_board_edit = None;
        Ok(())
    }

    pub(super) fn request_grasshopper_dependencies(&mut self, ctx: &egui::Context) {
        self.sync_grasshopper_graph();
        if !self.grasshopper.links_dirty {
            return;
        }
        let tab = self.tab().id;
        let mut pending: Vec<_> = self
            .grasshopper
            .views
            .keys()
            .filter_map(|(doc, id)| (*doc == tab).then_some(*id))
            .collect();
        let mut seen = HashSet::new();
        while let Some(id) = pending.pop() {
            if !seen.insert(id) {
                continue;
            }
            if let Some(wires) = self.grasshopper.graph.incoming.get(&id) {
                pending.extend(wires.iter().map(|wire| wire.source));
            }
            self.request_grasshopper(id, ctx);
        }
    }

    pub(super) fn update_grasshopper_links(&mut self) {
        let tab = self.tab().id;
        let graph = &self.grasshopper.graph;
        let ids = self
            .grasshopper
            .views
            .keys()
            .filter_map(|(doc, id)| (*doc == tab).then_some(*id));
        let (order, blocked) = graph.order(ids);
        for id in order {
            let inputs = self.grasshopper_inputs(tab, id);
            self.grasshopper
                .views
                .get_mut(&(tab, id))
                .unwrap()
                .update_linked(inputs);
        }
        for id in blocked {
            // Authored commands prevent loops, but malformed/legacy scene
            // imports must still keep dependent solves out of the worker.
            let missing = graph
                .incoming
                .get(&id)
                .into_iter()
                .flatten()
                .find(|wire| !self.grasshopper.views.contains_key(&(tab, wire.source)));
            let note = if let Some(wire) = missing {
                let label = self
                    .grasshopper_ports(id)
                    .iter()
                    .find(|port| port.id == wire.target_port)
                    .map_or(wire.target_port.as_str(), |port| port.name.as_str());
                format!("Grasshopper input {label} has a missing upstream definition.")
            } else {
                "Grasshopper connections form a loop or depend on an unavailable definition.".into()
            };
            self.grasshopper
                .views
                .get_mut(&(tab, id))
                .unwrap()
                .update_linked(Err(note));
        }
    }

    fn grasshopper_inputs(&self, tab: u64, id: NodeId) -> Result<PortPackets, String> {
        let Some((image, _)) = grasshopper_item(self.tab(), id) else {
            return Ok(PortPackets::new());
        };
        let mut inputs = PortPackets::new();
        let ports = self.grasshopper_ports(id);
        let target_view = &self.grasshopper.views[&(tab, id)];
        let schema_known = target_view.port_revision.is_some()
            && target_view.port_revision == target_view.source_revision;
        if let Some(values) = image.grasshopper_literals.as_deref() {
            for (port, value) in values.iter() {
                let target = ports
                    .iter()
                    .find(|p| &p.id == port && p.direction == GrasshopperPortDirection::Input);
                if schema_known && target.is_none() {
                    continue;
                }
                if let Some(packet) =
                    literal_packet(value, target.map_or(GrasshopperPortType::Data, |p| p.kind))
                {
                    inputs.insert(port.clone(), packet);
                }
            }
        }
        let mut wired = HashSet::new();
        for wire in self
            .grasshopper
            .graph
            .incoming
            .get(&id)
            .into_iter()
            .flatten()
        {
            let target = ports.iter().find(|p| {
                p.id == wire.target_port && p.direction == GrasshopperPortDirection::Input
            });
            let port_label = target.map_or(wire.target_port.as_str(), |port| port.name.as_str());
            if !wired.insert(&wire.target_port) {
                return Err(format!("Multiple Grasshopper wires target {port_label}."));
            }
            if schema_known && target.is_none() {
                return Err(format!("Grasshopper input {port_label} is unavailable."));
            }
            let source = &self.grasshopper.views[&(tab, wire.source)];
            let outputs = source.outputs.as_ref().ok_or_else(|| {
                if source.due.is_none()
                    && source.wait.is_none()
                    && source.status.is_some()
                    && self
                        .grasshopper
                        .background
                        .as_ref()
                        .is_none_or(|bg| bg.busy != Some((tab, wire.source)))
                {
                    format!("Grasshopper input {port_label} is blocked by an upstream solve error.")
                } else {
                    "Waiting for connected Grasshopper inputs…".to_owned()
                }
            })?;
            let packet = outputs.get(&wire.source_port).ok_or_else(|| {
                format!("Grasshopper input {port_label} has an unavailable output.")
            })?;
            if let Some(target) = target.filter(|_| schema_known) {
                let output = source.ports.iter().find(|p| {
                    p.id == wire.source_port && p.direction == GrasshopperPortDirection::Output
                });
                if output.is_none_or(|output| !target.kind.accepts(output.kind)) {
                    return Err(format!(
                        "Grasshopper input {port_label} has an incompatible output type."
                    ));
                }
            }
            // The tree travels unchanged. Grasshopper itself handles input
            // data matching. A connected value takes precedence over a literal.
            inputs.insert(wire.target_port.clone(), packet.clone());
        }
        Ok(inputs)
    }
}

impl View {
    fn update_linked(&mut self, inputs: Result<PortPackets, String>) {
        match inputs {
            Ok(inputs) => {
                if self.linked.as_ref() != Some(&inputs) {
                    self.linked = Some(inputs);
                    self.invalidate(Duration::ZERO);
                }
                self.wait = None;
            }
            Err(reason) => {
                if self.linked.take().is_some() {
                    self.invalidate(Duration::ZERO);
                }
                self.wait = Some(reason.clone());
                self.status = Some(reason);
            }
        }
    }
}

#[cfg(test)]
mod tests;
