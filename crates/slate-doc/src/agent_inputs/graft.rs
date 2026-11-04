//! Immutable longest-list matching over the existing typed input snapshot.
use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraftedInputJob {
    pub branch: usize,
    pub count: u8,
    pub inputs: InputSnapshot,
}

/// Preserve the submitted instruction suffix while replacing only its wired
/// prompt prefix with the selected branch's captured prompt values.
pub fn grafted_prompt(
    prompt: &str,
    before: &InputSnapshot,
    after: &InputSnapshot,
    separator: &str,
) -> String {
    let words = |inputs: &InputSnapshot| {
        inputs
            .on(InputSlot::Prompt)
            .map(|item| item.text.trim())
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join(separator)
    };
    let old = words(before);
    if old.is_empty() || !prompt.starts_with(&old) {
        return prompt.to_owned();
    }
    format!("{}{}", words(after), &prompt[old.len()..])
}

/// Grafting changes only selected channels. Context and every other channel
/// remain shared bundles, copied into each job rather than mutable sessions.
pub fn plan_grafted_inputs(
    snapshot: &InputSnapshot,
    grafted: &BTreeSet<String>,
    count: u8,
) -> Result<Vec<GraftedInputJob>, String> {
    plan_grafted_inputs_with_shared(snapshot, grafted, count, &BTreeSet::new())
}

/// Declared table/control sources stay bundled even when legacy snapshots
/// infer their textual port. Their native wire meaning has priority.
pub fn plan_grafted_inputs_with_shared(
    snapshot: &InputSnapshot,
    grafted: &BTreeSet<String>,
    count: u8,
    shared: &BTreeSet<u64>,
) -> Result<Vec<GraftedInputJob>, String> {
    let mut channels = BTreeMap::new();
    for id in grafted {
        let slot = InputSlot::from_id(id).ok_or_else(|| format!("Unknown grafted input: {id}"))?;
        if slot == InputSlot::View {
            return Err("View inputs cannot be grafted.".into());
        }
        let mut items = Vec::new();
        for item in snapshot.on(slot) {
            if shared.contains(&item.node) {
                continue;
            }
            if slot.takes_text() {
                if !item.text.trim().is_empty() {
                    items.push(item.clone());
                }
            } else {
                // The snapshot owner permits an empty image reference only for
                // an asynchronously captured source (for example a web page).
                // It is still one connected reference, unlike an empty channel.
                if item.images.is_empty() {
                    items.push(item.clone());
                }
                for image in &item.images {
                    let mut value = item.clone();
                    value.images = vec![image.clone()];
                    // Output identity and its source revision remain in the
                    // captured map; active follows this exact selected image.
                    value.active = item
                        .outputs
                        .iter()
                        .find(|(_, path)| *path == image)
                        .map(|(id, _)| id.clone());
                    items.push(value);
                }
            }
        }
        if items.is_empty() {
            return Err(format!("The grafted {id} input is empty."));
        }
        channels.insert(id.as_str(), items);
    }
    let branches = channels.values().map(Vec::len).max().unwrap_or(1);
    Ok((0..branches)
        .map(|branch| {
            let mut inputs = snapshot.clone();
            inputs.wired.retain(|item| {
                shared.contains(&item.node) || !channels.contains_key(item.port().id())
            });
            for items in channels.values() {
                inputs
                    .wired
                    .push(items[branch.min(items.len() - 1)].clone());
            }
            // Retain the captured source revision and append a deterministic branch
            // identity; it never depends on completion order or live scene edits.
            if !channels.is_empty() {
                inputs.revision = format!("{}:graft:{branch}", snapshot.revision);
            }
            GraftedInputJob {
                branch,
                count: count.max(1),
                inputs,
            }
        })
        .collect())
}

pub fn shared_generator_sources(scene: &Scene, target: NodeId) -> BTreeSet<u64> {
    scene
        .nodes
        .iter()
        .filter_map(|node| {
            let NodeKind::Connector(conn) = &node.kind else {
                return None;
            };
            let binding = conn.binding.as_ref()?;
            (endpoint_node(binding.target_end(conn)) == Some(target)
                && (matches!(binding.kind, InputKind::Table | InputKind::Control)
                    || binding.slot.as_deref() == Some(InputSlot::View.id())))
            .then(|| endpoint_node(binding.source_end(conn)).map(|id| id.0))
            .flatten()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn value(slot: InputSlot, names: &[&str]) -> ContextItem {
        ContextItem {
            node: 7,
            text: String::new(),
            images: names.iter().map(|s| s.to_string()).collect(),
            outputs: Default::default(),
            active: None,
            depth: None,
            slot: Some(slot),
        }
    }

    #[test]
    fn branch_prompt_replaces_only_the_submitted_wired_prefix() {
        let before = InputSnapshot {
            wired: vec![
                ContextItem {
                    text: "P".into(),
                    ..value(InputSlot::Prompt, &[])
                },
                ContextItem {
                    text: "Q".into(),
                    ..value(InputSlot::Prompt, &[])
                },
            ],
            ..Default::default()
        };
        let after = InputSnapshot {
            wired: vec![before.wired[1].clone()],
            ..Default::default()
        };
        assert_eq!(
            grafted_prompt("P\n\nQ\n\nSubmitted instruction", &before, &after, "\n\n"),
            "Q\n\nSubmitted instruction"
        );
        assert_eq!(
            grafted_prompt("Independent explicit question", &before, &after, "\n\n"),
            "Independent explicit question"
        );
    }
    #[test]
    fn longest_matching_repeats_last_and_copies_shared_values() {
        let snapshot = InputSnapshot {
            revision: "r1".into(),
            context: vec![],
            wired: vec![
                value(InputSlot::Media, &["A", "B", "C"]),
                value(InputSlot::Style, &["X", "Y"]),
                ContextItem {
                    text: "shared".into(),
                    ..value(InputSlot::Prompt, &[])
                },
            ],
        };
        let mut jobs = plan_grafted_inputs(
            &snapshot,
            &BTreeSet::from(["media".into(), "style".into()]),
            3,
        )
        .unwrap();
        assert_eq!(jobs.len(), 3);
        assert_eq!(jobs[2].count, 3);
        assert_eq!(
            jobs[2].inputs.on(InputSlot::Style).next().unwrap().images,
            ["Y"]
        );
        assert_eq!(
            jobs[2].inputs.on(InputSlot::Prompt).next().unwrap().text,
            "shared"
        );
        jobs[0].inputs.wired.clear();
        assert_eq!(jobs[1].inputs.wired.len(), 3);
        assert_eq!(snapshot.wired[0].images.len(), 3);
    }
    #[test]
    fn empty_graft_rejects_and_ungrafted_bundle_remains_whole() {
        let snapshot = InputSnapshot {
            wired: vec![value(InputSlot::Style, &["X", "Y"])],
            ..Default::default()
        };
        assert!(plan_grafted_inputs(&snapshot, &BTreeSet::from(["prompt".into()]), 1).is_err());
        let jobs = plan_grafted_inputs(&snapshot, &BTreeSet::new(), 2).unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].inputs, snapshot);
    }
}
