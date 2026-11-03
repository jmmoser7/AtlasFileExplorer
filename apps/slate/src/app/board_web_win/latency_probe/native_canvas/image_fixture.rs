//! Synthetic analogue of the current board's image/link/agent proportions.
//! The real workbook was inspected structurally only; no user bytes live here.
use crate::app::tests::Harness;
use slate_doc::scene::{ConnectorEnd, ImageNode, NodeKind, Side, WorldRect};
use slate_doc::ItemId;

pub(super) fn outputs_per_source() -> usize {
    let count = std::env::var("SLATE_LATENCY_OUTPUTS_PER_SOURCE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10usize);
    assert!((1..=500).contains(&count));
    count
}

pub(super) fn seed(h: &mut Harness, count: usize, images: &[ItemId]) -> WorldRect {
    let output_count = outputs_per_source();
    let agent_count = (count * 35 / 335).max(1);
    let image_count = (count * 262 / 335).max(agent_count + 1);
    let columns = (image_count as f32).sqrt().ceil() as usize;
    let mut ids = Vec::new();
    for i in 0..image_count {
        let mut image = ImageNode::new(images[i % images.len()]);
        if i < agent_count {
            let dir = h
                .base
                .join("workspace")
                .join(format!("synthetic-source-{i}"));
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join("session.json");
            image.agent = Some(serde_json::from_value(serde_json::json!({
                "provider": "comfy", "session": format!("synthetic-{i}"), "view": "images",
                "bundle": { "locator": path }, "instruction": "Synthetic local image reference; never submitted."
            })).unwrap());
            let outputs: Vec<_> = (0..output_count)
                .map(|j| atlas_agent::ImageOutput {
                    id: format!("output-{i}-{j}"),
                    source: h
                        .app
                        .item_path(images[(i + j) % images.len()])
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    prompt: "Synthetic offline benchmark output".into(),
                    ..Default::default()
                })
                .collect();
            let session: atlas_agent::AgentSession = serde_json::from_value(serde_json::json!({
                "provider": "comfy", "turns": [
                    { "role": "user", "text": "Synthetic image instruction", "at": 0 },
                    { "role": "assistant", "text": "Synthetic local output metadata. ".repeat(32), "at": 1 }
                ], "bundle": { "images": outputs }
            })).unwrap();
            atlas_ai::agent::atomic_write_json(&path, &session).unwrap();
        }
        let node = h.app.doc_mut().scene.build_node(
            WorldRect::new(
                (i % columns) as f32 * 200.0,
                (i / columns) as f32 * 150.0,
                156.0,
                112.0,
            ),
            NodeKind::Image(image),
        );
        ids.push(h.app.add_nodes(vec![node])[0]);
    }
    // Canonical connector construction retains actual native geometry and ink.
    for i in 0..count.saturating_sub(image_count) {
        let source = ids[agent_count + i % (image_count - agent_count)];
        let target = ids[i % agent_count];
        let wire = h.app.build_connector(
            ConnectorEnd::Anchored {
                node: source,
                side: Side::Right,
                t: 0.5,
            },
            ConnectorEnd::Anchored {
                node: target,
                side: Side::Left,
                t: 0.5,
            },
        );
        h.app.add_nodes(vec![wire]);
    }
    WorldRect::new(
        0.0,
        0.0,
        columns as f32 * 200.0,
        image_count.div_ceil(columns) as f32 * 150.0,
    )
}
