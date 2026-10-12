use super::{ImageNode, NodeId, NodeKind, PaintLayer, PaintLayerId};
use serde::{Deserialize, Serialize};

/// Stable identity for one image in an album, scoped to its owning image node.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "id")]
pub enum ImageResultId {
    Item(crate::ItemId),
    Output(String),
}

/// Authored layer stack for one result in an image album.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageResultLayers {
    pub result: ImageResultId,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layers: Vec<PaintLayer>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub metadata: Vec<PaintLayerMeta>,
}

/// Stable layer identity with its authored palette name and order key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaintLayerMeta {
    pub layer: PaintLayerId,
    #[serde(default)]
    pub name: String,
    /// Lower keys paint above higher keys.
    pub order: u64,
}

impl ImageResultLayers {
    pub fn new(result: ImageResultId) -> Self {
        Self {
            result,
            layers: Vec::new(),
            metadata: Vec::new(),
        }
    }

    pub fn layer_meta(&self, layer: PaintLayerId) -> Option<&PaintLayerMeta> {
        self.metadata.iter().find(|meta| meta.layer == layer)
    }

    pub fn layer_meta_mut(&mut self, layer: PaintLayerId) -> Option<&mut PaintLayerMeta> {
        self.metadata.iter_mut().find(|meta| meta.layer == layer)
    }

    /// Complete metadata for transitional callers and remove stale entries.
    pub fn normalize_metadata(&mut self) {
        for (index, layer) in self.layers.iter().enumerate() {
            if !self.metadata.iter().any(|meta| meta.layer == layer.id) {
                self.metadata.push(PaintLayerMeta {
                    layer: layer.id,
                    name: format!("Layer {}", index + 1),
                    order: index as u64,
                });
            }
        }
        self.metadata
            .retain(|meta| self.layers.iter().any(|layer| layer.id == meta.layer));
        self.metadata.sort_by_key(|meta| (meta.order, meta.layer));
    }

    pub fn add_layer(&mut self, layer: PaintLayer) -> usize {
        let index = 0;
        self.metadata.push(PaintLayerMeta {
            layer: layer.id,
            name: format!("Layer {}", self.layers.len() + 1),
            order: 0,
        });
        self.layers.insert(index, layer);
        self.reorder_layers();
        index
    }

    pub fn remove_layer(&mut self, index: usize) -> Option<PaintLayer> {
        let layer = self.layers.get(index)?;
        let id = layer.id;
        let removed = self.layers.remove(index);
        self.metadata.retain(|meta| meta.layer != id);
        self.reorder_layers();
        Some(removed)
    }

    /// Reorder the vector and rewrite stable order keys, preserving layer IDs.
    pub fn move_layer(&mut self, from: usize, to: usize) -> bool {
        if from >= self.layers.len() || to >= self.layers.len() || from == to {
            return false;
        }
        let layer = self.layers.remove(from);
        self.layers.insert(to, layer);
        self.reorder_layers();
        true
    }

    fn reorder_layers(&mut self) {
        for (index, layer) in self.layers.iter().enumerate() {
            if let Some(meta) = self.metadata.iter_mut().find(|meta| meta.layer == layer.id) {
                meta.order = index as u64;
            }
        }
    }
}

/// Remap every layer, child and prompt reference across all result stacks.
pub fn fresh_layer_node_ids(img: &mut ImageNode, mut next_id: impl FnMut() -> NodeId) {
    fn next_layer_id(img: &ImageNode) -> u64 {
        img.paint_layers
            .iter()
            .chain(img.result_layers.iter().flat_map(|stack| &stack.layers))
            .map(|layer| layer.id.0)
            .max()
            .unwrap_or(0)
            .saturating_add(1)
    }

    fn remap_stack(
        layers: &mut [PaintLayer],
        next_layer: &mut u64,
        next_id: &mut dyn FnMut() -> NodeId,
    ) -> std::collections::BTreeMap<PaintLayerId, PaintLayerId> {
        let mut layer_ids = std::collections::BTreeMap::new();
        for layer in layers {
            let old_layer = layer.id;
            layer.id = PaintLayerId(*next_layer);
            *next_layer = next_layer.saturating_add(1);
            layer_ids.insert(old_layer, layer.id);
            let mut node_ids = std::collections::BTreeMap::new();
            for child in &mut layer.nodes {
                let old = child.id;
                child.id = next_id();
                node_ids.insert(old, child.id);
                if let NodeKind::Image(nested) = &mut child.kind {
                    fresh_layer_node_ids(nested, &mut *next_id);
                }
            }
            for prompt in &mut layer.prompts {
                if let Some(id) = node_ids.get(&prompt.node) {
                    prompt.node = *id;
                }
            }
        }
        layer_ids
    }

    let mut next_layer = next_layer_id(img);
    let _ = remap_stack(&mut img.paint_layers, &mut next_layer, &mut next_id);
    for stack in &mut img.result_layers {
        let layer_ids = remap_stack(&mut stack.layers, &mut next_layer, &mut next_id);
        for meta in &mut stack.metadata {
            if let Some(id) = layer_ids.get(&meta.layer) {
                meta.layer = *id;
            }
        }
    }
}

/// The ordinary image legacy field resolves only to its matching item.
pub fn layers_for_result<'a>(img: &'a ImageNode, result: &ImageResultId) -> &'a [PaintLayer] {
    img.result_layers
        .iter()
        .find(|stack| &stack.result == result)
        .map(|stack| stack.layers.as_slice())
        .or_else(|| match result {
            ImageResultId::Item(item) if *item == img.item && !item.is_none() => {
                Some(img.paint_layers.as_slice())
            }
            _ => None,
        })
        .unwrap_or(&[])
}

pub fn result_layers<'a>(
    img: &'a ImageNode,
    result: &ImageResultId,
) -> Option<&'a ImageResultLayers> {
    img.result_layers
        .iter()
        .find(|stack| &stack.result == result)
}

/// Resolve the stored vector addressed by a scene locator. `None` is the
/// transitional card-wide stack; `Some` resolves exactly that result.
pub fn layers_for_location<'a>(
    img: &'a ImageNode,
    result: Option<&ImageResultId>,
) -> Option<&'a [PaintLayer]> {
    match result {
        Some(result) => result_layers(img, result).map(|stack| stack.layers.as_slice()),
        None => Some(img.paint_layers.as_slice()),
    }
}

pub fn ensure_result_layers_mut(
    img: &mut ImageNode,
    result: ImageResultId,
) -> &mut ImageResultLayers {
    if let Some(index) = img
        .result_layers
        .iter()
        .position(|stack| stack.result == result)
    {
        return &mut img.result_layers[index];
    }
    img.result_layers.push(ImageResultLayers::new(result));
    img.result_layers.last_mut().expect("just inserted")
}

/// Move an ambiguous pre-album stack only to the result the caller has
/// resolved as selected. It must never seed later/future outputs by fallback.
/// Moving keeps IDs unique; a `SceneCmd::Patch` makes first-edit migration
/// undoable.
pub fn seed_result_from_legacy(
    img: &mut ImageNode,
    result: ImageResultId,
) -> &mut ImageResultLayers {
    let index = img
        .result_layers
        .iter()
        .position(|stack| stack.result == result);
    if let Some(index) = index {
        if !img.result_layers[index].layers.is_empty() {
            return &mut img.result_layers[index];
        }
    }
    // Legacy renderers paint ascending order bottom-to-top; result stacks
    // store palette order top-to-bottom.
    let mut legacy_layers = std::mem::take(&mut img.paint_layers);
    legacy_layers.reverse();
    let index = match index {
        Some(index) => index,
        None => {
            img.result_layers.push(ImageResultLayers::new(result));
            img.result_layers.len() - 1
        }
    };
    if img.result_layers[index].layers.is_empty() && !legacy_layers.is_empty() {
        let stack = &mut img.result_layers[index];
        stack.layers = legacy_layers;
        stack.metadata = stack
            .layers
            .iter()
            .enumerate()
            .map(|(index, layer)| PaintLayerMeta {
                layer: layer.id,
                name: format!("Layer {}", index + 1),
                order: index as u64,
            })
            .collect();
    } else {
        img.paint_layers = legacy_layers;
    }
    &mut img.result_layers[index]
}

/// Copy one selected result to an ordinary image with independent nested IDs.
pub fn clone_result_layers_to_item(
    img: &ImageNode,
    source: &ImageResultId,
    item: crate::ItemId,
    mut next_id: impl FnMut() -> NodeId,
) -> Option<ImageResultLayers> {
    let mut copy = result_layers(img, source)?.clone();
    copy.result = ImageResultId::Item(item);
    let mut temporary = ImageNode {
        result_layers: vec![copy],
        ..ImageNode::new(item)
    };
    fresh_layer_node_ids(&mut temporary, &mut next_id);
    temporary.result_layers.into_iter().next()
}
