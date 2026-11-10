//! Advertised scalar-string contract, serialized from the closed geometry DTOs.
use super::{GeometryOperation as Op, GeometryPlan, Units};

const EXAMPLE: &str = "\nExample plan JSON: ";

fn example() -> GeometryPlan {
    GeometryPlan {
        units: Units::Meters,
        operations: vec![Op::Box {
            id: "base".into(),
            origin: [0.; 3],
            size: [18., 10., 4.],
        }],
    }
}

// These are shape examples, not a request to reference an invented source object.
fn shapes() -> Vec<Op> {
    vec![
        Op::Box {
            id: "base".into(),
            origin: [0.; 3],
            size: [18., 10., 4.],
        },
        Op::Sphere {
            id: "ball".into(),
            origin: [0.; 3],
            radius: 1.,
        },
        Op::Cylinder {
            id: "column".into(),
            origin: [0.; 3],
            radius: 1.,
            height: 4.,
        },
        Op::Extrude {
            id: "prism".into(),
            points: vec![[0., 0.], [2., 0.], [0., 2.]],
            height: 3.,
        },
        Op::Mesh {
            id: "panel".into(),
            vertices: vec![[0., 0., 0.], [2., 0., 0.], [0., 0., 3.]],
            triangles: vec![[0, 1, 2]],
            closed: None,
        },
        Op::Appearance {
            id: "finished".into(),
            target: "panel".into(),
            diffuse: [60, 100, 140],
            gloss: 0.7,
            transparency: 0.25,
        },
        Op::Move {
            id: "moved".into(),
            target: "base".into(),
            vector: [1., 0., 0.],
        },
        Op::Rotate {
            id: "rotated".into(),
            target: "base".into(),
            origin: [0.; 3],
            axis: [0., 0., 1.],
            angle: 0.5,
        },
        Op::Scale {
            id: "scaled".into(),
            target: "base".into(),
            origin: [0.; 3],
            factor: 2.,
        },
        Op::Union {
            id: "joined".into(),
            left: "base".into(),
            right: "ball".into(),
        },
        Op::Difference {
            id: "cut".into(),
            left: "base".into(),
            right: "ball".into(),
        },
        Op::Reference {
            id: "existing".into(),
            object_id: "00000000-0000-0000-0000-000000000001".into(),
        },
    ]
}

pub(super) fn description() -> String {
    let units = serde_json::to_string(&[Units::Source, Units::Meters, Units::Millimeters])
        .expect("closed unit names serialize");
    let shapes =
        serde_json::to_string(&shapes()).expect("finite typed operation examples serialize");
    let example = serde_json::to_string(&example()).expect("finite typed plan example serializes");
    format!(
        "Required: a JSON-encoded GeometryPlan string, NOT natural language or code. \
         Its JSON object has exactly units and operations. units is one of {units}; \
         operations is an ordered array of at most 64 closed operation objects. \
         Supported operation shapes (replace sample values): {shapes}. \
         Every operation requires a unique 1..64 ASCII letter/digit/underscore/hyphen id. \
         target/left/right must name earlier operation ids; left and right must differ. \
         reference.object_id must be an actual existing source object UUID obtained by inspect, \
         never the illustrative UUID above. Coordinates/numbers must be finite with magnitude \
         at most 1000000; sizes/radius/height/factor must be at least 0.000001. \
         Extrude points are 3..128 XY pairs. Rotation axis must be nonzero. \
         Mesh has 3..4096 vertices, 1..8192 indexed triangles, no degenerate faces; \
         optional closed:true asserts a native closed oriented manifold; omitted/false permits open panels. \
         Appearance consumes target like a transform, setting actual standard Rhino material: \
         diffuse is three RGB bytes (0..255), gloss/transparency are 0..1; gloss maps to Material.Shine. \
         Transforms preserve assigned appearance; union/difference accept Brep/extrusion only, never mesh operands. \
         Native preview shows diffuse and gloss; transparency is saved but currently renders opaque. \
         Unknown fields, scripts, paths and operation kinds are rejected. \
         Keep the entire plan string within 24576 UTF-8 bytes. \
         Pass the following example as the value of parameters.plan, encoded as a JSON string.{EXAMPLE}{example}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advertised_geometry_example_roundtrips_through_strict_parser() {
        let descriptor = crate::headless_rhino::command_descriptor();
        atlas_connector::validate_parameter_schema(&descriptor.parameters).unwrap();
        let advertised = descriptor.parameters["properties"]["plan"]["description"]
            .as_str()
            .unwrap();
        let encoded = advertised.split_once(EXAMPLE).unwrap().1;
        let plan =
            crate::headless_rhino::parse_parameters(&serde_json::json!({"plan":encoded})).unwrap();
        assert_eq!(
            serde_json::to_value(plan).unwrap(),
            serde_json::to_value(example()).unwrap()
        );
        assert!(advertised.contains("NOT natural language"));
        // Every advertised shape is produced by the DTO and accepted by its validator.
        let samples = GeometryPlan {
            units: Units::Meters,
            operations: shapes(),
        };
        samples.validate().unwrap();
        let serialized = serde_json::to_string(&samples).unwrap();
        crate::headless_rhino::parse_parameters(&serde_json::json!({"plan":serialized})).unwrap();
    }

    #[test]
    fn geometry_contract_keeps_invalid_plans_rejected() {
        for plan in [
            "Create a two-story rectangular building.",
            r#"{"units":"meters","operations":[{"kind":"script","id":"bad","code":"x"}]}"#,
            r#"{"units":"meters","operations":[],"output":"source.3dm"}"#,
            r#"{"units":"meters","operations":[{"kind":"box","id":"bad","origin":[0,0,0],"size":[0,1,1]}]}"#,
        ] {
            assert!(
                crate::headless_rhino::parse_parameters(&serde_json::json!({"plan":plan})).is_err()
            );
        }
        assert!(crate::headless_rhino::parse_parameters(&serde_json::json!({
            "plan":serde_json::to_string(&example()).unwrap(), "script":"x"
        }))
        .is_err());
    }
}
