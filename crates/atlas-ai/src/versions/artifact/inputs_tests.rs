use super::*;
use atlas_agent::{ContextItem, InputSnapshot};
use std::collections::{BTreeMap, HashMap};

#[test]
fn explicit_file_data_and_depth_are_frozen_once_and_ambient_context_is_removed() {
    let f = Fixture::new("inputs");
    let source = f.0.join("pie.png");
    let depth = f.0.join("depth.png");
    let table = f.0.join("data.csv");
    std::fs::write(&source, b"exact reference image").unwrap();
    std::fs::write(&depth, b"exact depth image").unwrap();
    std::fs::write(&table, b"name,value\na,10\nb,20\n").unwrap();
    let run = f.run("r1", None);
    let input = ContextItem {
        node: 12,
        text: "visible text".into(),
        images: vec![source.to_string_lossy().into_owned(); 2],
        depth: Some(depth.to_string_lossy().into_owned()),
        outputs: BTreeMap::from([("old".into(), "original-path".into())]),
        active: Some("old".into()),
        slot: None,
    };
    let snapshot = InputSnapshot {
        revision: "old".into(),
        wired: vec![input.clone()],
        context: vec![ContextItem {
            text: "ambient".into(),
            ..input
        }],
    };
    let (frozen, provenance) =
        freeze_artifact_inputs(&run, &snapshot, &HashMap::from([(12, table.clone())])).unwrap();
    assert_eq!(frozen.context, frozen.wired);
    let item = &frozen.wired[0];
    assert!(item.outputs.is_empty() && item.active.is_none());
    assert_eq!(item.images[0], item.images[1]);
    assert_eq!(
        std::fs::read(&item.images[0]).unwrap(),
        b"exact reference image"
    );
    assert_eq!(
        std::fs::read(item.depth.as_ref().unwrap()).unwrap(),
        b"exact depth image"
    );
    assert!(item.text.contains("inputs") && !item.text.contains(table.to_str().unwrap()));
    assert_eq!(
        std::fs::read_dir(run.staging_output.parent().unwrap().join("inputs"))
            .unwrap()
            .count(),
        3
    );
    assert_eq!(
        provenance.len(),
        4,
        "three unique source byte hashes and one canonical text snapshot"
    );
    assert_eq!(
        provenance
            .iter()
            .find(|p| p.source == source.to_string_lossy())
            .unwrap()
            .source_hash,
        hash_artifact_input(b"exact reference image")
    );
    std::fs::write(source, b"later image").unwrap();
    std::fs::write(table, b"later data").unwrap();
    assert_eq!(
        std::fs::read(&item.images[0]).unwrap(),
        b"exact reference image"
    );
}

#[test]
fn input_file_count_and_total_byte_bounds_are_enforced() {
    let f = Fixture::new("input_limits");
    let run = f.run("r1", None);
    let source = f.0.join("large.png");
    std::fs::File::create(&source)
        .unwrap()
        .set_len(MAX_ARTIFACT_INPUT_BYTES + 1)
        .unwrap();
    let item = ContextItem {
        node: 1,
        text: String::new(),
        images: vec![source.to_string_lossy().into_owned()],
        outputs: BTreeMap::new(),
        active: None,
        depth: None,
        slot: None,
    };
    let snapshot = InputSnapshot {
        wired: vec![item.clone()],
        ..Default::default()
    };
    assert!(freeze_artifact_inputs(&run, &snapshot, &HashMap::new()).is_err());
    let run = f.run("r2", None);
    let mut item = item;
    item.images.clear();
    for i in 0..17 {
        let source = f.0.join(format!("small-{i}.png"));
        std::fs::write(&source, b"small").unwrap();
        item.images.push(source.to_string_lossy().into_owned());
    }
    assert!(freeze_artifact_inputs(
        &run,
        &InputSnapshot {
            wired: vec![item],
            ..Default::default()
        },
        &HashMap::new()
    )
    .is_err());
}
