use super::*;
use serde_json::{json, Value};

fn panel() -> Value {
    json!({"kind":"mesh","id":"panel","vertices":[[0,0,0],[2,0,0],[0,0,3]],"triangles":[[0,1,2]]})
}
fn finish() -> Value {
    json!({"kind":"appearance","id":"glass","target":"panel","diffuse":[60,100,140],"gloss":0.7,"transparency":0.25})
}
fn parse(operations: Vec<Value>) -> Result<GeometryPlan, String> {
    crate::headless_rhino::parse_parameters(&json!({
        "plan":serde_json::to_string(&json!({"units":"meters","operations":operations})).unwrap()
    }))
}
#[test]
fn open_panel_appearance_and_transform_roundtrip() {
    let plan = parse(vec![
        panel(),
        finish(),
        json!({
            "kind":"move","id":"placed","target":"glass","vector":[1,2,3]
        }),
    ])
    .unwrap();
    let value = serde_json::to_value(&plan).unwrap();
    assert_eq!(value["operations"][1]["diffuse"], json!([60, 100, 140]));
    assert!(value["operations"][0].get("closed").is_none());
    let mut explicit_open = panel();
    explicit_open["closed"] = json!(false);
    assert!(parse(vec![explicit_open]).is_ok());
    // The native SDK owns the stronger closed oriented manifold assertion.
    let mut closed = panel();
    closed["closed"] = json!(true);
    assert!(parse(vec![closed]).is_ok());
}
#[test]
fn rejects_malformed_and_degenerate_mesh_faces() {
    for triangles in [
        json!([]),
        json!([[0, 1, 3]]),
        json!([[0, 0, 2]]),
        json!([[0, -1, 2]]),
        json!([[0, 1.5, 2]]),
        json!([[0, 1]]),
        json!([[0, 1, 2, 0]]),
    ] {
        let mut op = panel();
        op["triangles"] = triangles;
        assert!(parse(vec![op]).is_err());
    }
    for vertices in [
        json!([[0, 0, 0], [1, 1, 1], [2, 2, 2]]),
        json!([[0, 0, 0], [2, 0, 0]]),
        json!([[0, 0, 0], [1000001, 0, 0], [0, 0, 3]]),
        json!([[0, 0, 0], [2, 0], [0, 0, 3]]),
    ] {
        let mut op = panel();
        op["vertices"] = vertices;
        assert!(parse(vec![op]).is_err());
    }
    for closed in [json!("true"), json!(1)] {
        let mut op = panel();
        op["closed"] = closed;
        assert!(parse(vec![op]).is_err());
    }
}
#[test]
fn rejects_mesh_numeric_and_count_bounds_before_launch() {
    let vertices = [[0., 0., 0.], [2., 0., 0.], [0., 0., 3.]];
    for value in [f64::NAN, f64::INFINITY, -1_000_001.] {
        let mut bad = vertices;
        bad[0][0] = value;
        assert!(!mesh_valid(&bad, &[[0, 1, 2]]));
    }
    assert!(!mesh_valid(&vec![[0.; 3]; 4097], &[[0, 1, 2]]));
    assert!(!mesh_valid(&vertices, &vec![[0, 1, 2]; 8193]));
    assert!(!mesh_valid(&vertices, &[[0, 1, u32::MAX]]));
    // Bounded coordinate deltas keep cross products and squared area finite.
    assert!(mesh_valid(
        &[[-1e6, -1e6, -1e6], [1e6, -1e6, 1e6], [0., 1e6, 0.]],
        &[[0, 1, 2]]
    ));
}
#[test]
fn rejects_invalid_materials_and_reference_order() {
    for (field, bad) in [
        ("diffuse", json!([256, 0, 0])),
        ("diffuse", json!([-1, 0, 0])),
        ("diffuse", json!([1.5, 0, 0])),
        ("diffuse", json!([1, 2])),
        ("gloss", json!(-0.1)),
        ("gloss", json!(1.1)),
        ("transparency", json!(-0.1)),
        ("transparency", json!(1.1)),
    ] {
        let mut op = finish();
        op[field] = bad;
        assert!(parse(vec![panel(), op]).is_err());
    }
    for value in [f64::NAN, f64::INFINITY] {
        let op = GeometryOperation::Appearance {
            id: "finish".into(),
            target: "panel".into(),
            diffuse: [0; 3],
            gloss: value,
            transparency: 0.,
        };
        assert!(GeometryPlan {
            units: Units::Meters,
            operations: vec![serde_json::from_value(panel()).unwrap(), op]
        }
        .validate()
        .is_err());
    }
    assert!(parse(vec![finish(), panel()]).is_err());
    let mut duplicate = finish();
    duplicate["id"] = json!("panel");
    assert!(parse(vec![panel(), duplicate]).is_err());
    let mut arbitrary = finish();
    arbitrary["code"] = json!("arbitrary script");
    assert!(parse(vec![panel(), arbitrary]).is_err());
    let mut malformed = panel();
    malformed["path"] = json!("model.obj");
    assert!(parse(vec![malformed]).is_err());
}
#[test]
fn old_brep_reference_transform_and_boolean_shapes_stay_accepted() {
    assert!(parse(vec![
        json!({"kind":"reference","id":"source","object_id":"00000000-0000-0000-0000-000000000001"}),
        json!({"kind":"move","id":"moved","target":"source","vector":[1,2,0]}),
        json!({"kind":"rotate","id":"rotated","target":"moved","origin":[0,0,0],"axis":[0,0,1],"angle":0.5}),
        json!({"kind":"scale","id":"scaled","target":"rotated","origin":[0,0,0],"factor":2}),
        json!({"kind":"box","id":"base","origin":[0,0,0],"size":[2,3,4]}),
        json!({"kind":"sphere","id":"ball","origin":[0,0,0],"radius":1}),
        json!({"kind":"difference","id":"cut","left":"base","right":"ball"}),
        json!({"kind":"union","id":"joined","left":"cut","right":"scaled"})
    ]).is_ok());
}

#[test]
fn advertised_json_plan_admits_a_bounded_mesh_larger_than_plain_scalars() {
    let mut vertices = Vec::new();
    let mut triangles = Vec::new();
    for i in 0..150u32 {
        vertices.extend([[i as f64, 0., 0.], [i as f64, 1., 0.], [i as f64, 0., 1.]]);
        triangles.push([3 * i, 3 * i + 1, 3 * i + 2]);
    }
    let plan = json!({"units":"meters","operations":[{
        "kind":"mesh","id":"panels","vertices":vertices,"triangles":triangles
    }]});
    let encoded = serde_json::to_string(&plan).unwrap();
    assert!(encoded.len() > 4096 && encoded.len() < 24576);
    let parsed = crate::headless_rhino::parse_parameters(&json!({"plan":encoded})).unwrap();
    assert_eq!(parsed.operations.len(), 1);
    assert!(crate::headless_rhino::parse_parameters(&json!({"plan":" ".repeat(24577)})).is_err());
}
