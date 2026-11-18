use super::*;
use serde_json::json;

fn recipe(inputs: Value) -> Vec<u8> {
    serde_json::to_vec(&json!({"version":1,"title":"Sample & <model>","inputs":inputs})).unwrap()
}

#[test]
fn native_archive_contains_independent_typed_controls_wires_and_inert_code() {
    let fields = json!([
        {"name":"Radius","key":"r","type":"number","default":5,"min":1,"max":20},
        {"name":"Steps","key":"n","type":"integer","default":3,"min":1,"max":10},
        {"name":"Enabled","key":"ok","type":"boolean","default":true},
        {"name":"Label","key":"label","type":"text","default":"A & B"}
    ]);
    let body = "model = ok ? new Sphere(Point3d.Origin, r).ToBrep() : null;";
    let first = generate("fixture", &recipe(fields.clone()), body).unwrap();
    let parsed = roxmltree::Document::parse(&first).unwrap();
    let items = |name: &str| {
        parsed
            .descendants()
            .filter(|n| n.has_tag_name("item") && n.attribute("name") == Some(name))
            .filter_map(|n| n.text())
            .map(str::to_string)
            .collect::<Vec<_>>()
    };
    assert_eq!(items("ObjectCount"), ["9"]);
    // These values are required by the native contextual parameter readers.
    assert_eq!(
        items("Minimum"),
        ["-1.7976931348623157E+308", "-2147483648"]
    );
    assert_eq!(items("Maximum"), ["1.7976931348623157E+308", "2147483647"]);
    assert_eq!(items("TrueValue"), ["True"]);
    assert_eq!(items("FalseValue"), ["False"]);
    assert!(items("Date")[0].parse::<u64>().unwrap() > 621_355_968_000_000_000);
    for name in ["Revisions", "Projection", "Views"] {
        assert!(parsed
            .descendants()
            .any(|n| n.has_tag_name("chunk") && n.attribute("name") == Some(name)));
    }
    let classes = items("GUID");
    for guid in [
        "7b36b876-9451-46f5-8220-a200d969cc66",
        "b228887e-0852-4d9f-bd46-2591646e0d7c",
        "51ef601d-f86e-4ee4-bcf2-3d459d3e95e9",
        "fed87bdd-8327-49cd-949c-09d70f3c345c",
    ] {
        assert!(classes.contains(&guid.to_owned()));
    }
    let ids = items("InstanceGuid");
    let unique = ids.iter().collect::<HashSet<_>>();
    assert_eq!(unique.len(), ids.len());
    assert!(items("Source").iter().all(|id| unique.contains(id)));
    assert_eq!(items("UserText"), ["A & B"]);
    assert!(items("Name").contains(&"Sample & <model>".into()));
    let code = String::from_utf8(STANDARD.decode(&items("Text")[0]).unwrap()).unwrap();
    assert!(
        code.contains(body)
            && code.contains("System.Convert.ToInt32(")
            && code.contains("System.Convert.ToBoolean(")
    );
    let mut reversed = fields.as_array().unwrap().clone();
    reversed.reverse();
    let second = generate("fixture", &recipe(reversed.into()), body).unwrap();
    for id in ids {
        assert!(second.contains(&id));
    }
    assert!(!empty_xml(first.as_bytes()));
}

#[test]
fn refuses_invalid_control_defaults_keys_unknown_fields_and_absent_script() {
    let valid = json!({"name":"Depth","key":"d","type":"number","min":0,"max":10,"default":2});
    for field in [
        json!({"min":9}),
        json!({"key":"bad key"}),
        json!({"key":"model"}),
        json!({"type":"integer","default":2.2}),
        json!({"path":"outside"}),
        json!({"type":"boolean","default":true}),
    ] {
        let mut changed = valid.clone();
        for (k, v) in field.as_object().unwrap() {
            changed[k] = v.clone();
        }
        assert!(generate("fixture", &recipe(json!([changed])), "model = null;").is_err());
    }
    assert!(generate("fixture", &recipe(json!([valid, valid])), "model=null;").is_err());
    assert!(generate("fixture", &recipe(json!([])), "  ").is_err());
}
