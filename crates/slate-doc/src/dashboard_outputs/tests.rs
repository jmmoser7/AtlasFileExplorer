use super::*;

const ID: &str = "1b993386-89d8-4b25-b102-452ea7120eb4";
fn output(kind: &str, access: &str, value: Value) -> Value {
    json!({"id":ID,"name":"Weights","type":kind,"access":access,"value":value})
}
fn packet(outputs: Vec<Value>) -> Result<Snapshot, &'static str> {
    parse(&json!({"version":1,"outputs":outputs}).to_string())
}
#[test]
fn dashboard_outputs_preserve_tree_paths_and_grasshopper_types() {
    let tree = json!([{"path":[2,0],"items":[0.2,0.8]},{"path":[3],"items":[]}]);
    let mut text = output("text", "item", json!("0.2,0.8"));
    text["id"] = json!("1b993386-89d8-4b25-b102-452ea7120eb5");
    let result = packet(vec![output("number", "tree", tree), text]).unwrap();
    assert_eq!(result.ports.len(), 2);
    let branches = &result.packets[&result.ports[0].id]["branches"];
    assert_eq!(branches[0]["path"], json!([2, 0]));
    assert_eq!(
        branches[0]["items"][1],
        json!({"kind":"number","value":0.8})
    );
    assert_eq!(branches[1]["items"], json!([]));
    assert_eq!(
        result.packets[&result.ports[1].id]["branches"][0]["items"][0]["value"],
        json!("0.2,0.8")
    );
    for (kind, value) in [
        ("boolean", json!(false)),
        ("integer", json!(-2)),
        ("point", json!([1, 2, 3])),
        ("color", json!([0, 35, 255, 255])),
    ] {
        assert!(packet(vec![output(kind, "item", value)]).is_ok());
    }
}
#[test]
fn dashboard_outputs_reject_bad_types_duplicate_ids_and_unbounded_data() {
    let raw = output("integer", "list", json!([1, 2]));
    assert!(packet(vec![raw.clone(), raw]).is_err());
    for value in [
        output("integer", "item", json!(0.5)),
        output("number", "item", json!("2")),
        output("color", "item", json!([0, 0, 256, 255])),
        output("geometry", "list", json!([])),
        output("number", "tree", json!([{"path":[-1],"items":[]}])),
    ] {
        assert!(packet(vec![value]).is_err());
    }
    assert!(packet(vec![output("number", "list", json!(vec![0; 10001]))]).is_err());
    assert!(parse(&" ".repeat(MAX_BYTES + 1)).is_err());
}

#[test]
fn controller_edits_are_optional_bounded_json_and_have_distinct_revisions() {
    let raw = r#"{"version":1,"outputs":[]}"#;
    assert!(controller::observation(raw).unwrap().1.is_none());
    let mut edit = json!({"base":null,"state":{"schema":ID,"revision":ID,"value":r#"{"u":0.25}"#}});
    let packet = |edit: &Value| format!("{edit}\u{1e}{raw}");
    assert!(controller::observation(&packet(&edit)).unwrap().1.is_some());
    edit["base"] = json!(ID);
    assert!(controller::observation(&packet(&edit)).is_err());
    edit["base"] = Value::Null;
    for invalid in [
        "oops".into(),
        format!("\"{}\"", "a".repeat(controller::MAX_STATE_BYTES)),
    ] {
        edit["state"]["value"] = json!(invalid);
        assert!(controller::observation(&packet(&edit)).is_err());
    }
}
