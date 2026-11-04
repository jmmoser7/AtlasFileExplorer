use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Units {
    Source,
    Meters,
    Millimeters,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeometryPlan {
    pub units: Units,
    pub operations: Vec<GeometryOperation>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GeometryOperation {
    Reference {
        id: String,
        object_id: String,
    },
    Box {
        id: String,
        origin: [f64; 3],
        size: [f64; 3],
    },
    Sphere {
        id: String,
        origin: [f64; 3],
        radius: f64,
    },
    Cylinder {
        id: String,
        origin: [f64; 3],
        radius: f64,
        height: f64,
    },
    Extrude {
        id: String,
        points: Vec<[f64; 2]>,
        height: f64,
    },
    Move {
        id: String,
        target: String,
        vector: [f64; 3],
    },
    Rotate {
        id: String,
        target: String,
        origin: [f64; 3],
        axis: [f64; 3],
        angle: f64,
    },
    Scale {
        id: String,
        target: String,
        origin: [f64; 3],
        factor: f64,
    },
    Union {
        id: String,
        left: String,
        right: String,
    },
    Difference {
        id: String,
        left: String,
        right: String,
    },
}

impl GeometryOperation {
    pub fn id(&self) -> &str {
        match self {
            Self::Reference { id, .. }
            | Self::Box { id, .. }
            | Self::Sphere { id, .. }
            | Self::Cylinder { id, .. }
            | Self::Extrude { id, .. }
            | Self::Move { id, .. }
            | Self::Rotate { id, .. }
            | Self::Scale { id, .. }
            | Self::Union { id, .. }
            | Self::Difference { id, .. } => id,
        }
    }
}

fn number(v: f64) -> bool {
    v.is_finite() && v.abs() <= 1_000_000.0
}
fn positive(v: f64) -> bool {
    number(v) && v >= 0.000001
}
fn point(v: &[f64]) -> bool {
    v.iter().all(|v| number(*v))
}

impl GeometryPlan {
    pub fn validate(&self) -> Result<(), String> {
        if self.operations.len() > 64 {
            return Err("Expected at most 64 geometry operations".into());
        }
        let mut ids = HashSet::new();
        for op in &self.operations {
            let id = op.id();
            if id.is_empty()
                || id.len() > 64
                || !id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
                || ids.contains(id)
            {
                return Err(
                    "Operation IDs must be unique, 1..64 ASCII letters/digits/underscore/hyphen"
                        .into(),
                );
            }
            let valid = match op {
                GeometryOperation::Reference { object_id, .. } => {
                    object_id.len() == 36
                        && object_id.bytes().enumerate().all(|(i, b)| {
                            if [8, 13, 18, 23].contains(&i) {
                                b == b'-'
                            } else {
                                b.is_ascii_hexdigit()
                            }
                        })
                }
                GeometryOperation::Box { origin, size, .. } => {
                    point(origin) && size.iter().all(|v| positive(*v))
                }
                GeometryOperation::Sphere { origin, radius, .. } => {
                    point(origin) && positive(*radius)
                }
                GeometryOperation::Cylinder {
                    origin,
                    radius,
                    height,
                    ..
                } => point(origin) && positive(*radius) && positive(*height),
                GeometryOperation::Extrude { points, height, .. } => {
                    (3..=128).contains(&points.len())
                        && points.iter().all(|p| point(p))
                        && positive(*height)
                }
                GeometryOperation::Move { target, vector, .. } => {
                    ids.contains(target.as_str()) && point(vector)
                }
                GeometryOperation::Rotate {
                    target,
                    origin,
                    axis,
                    angle,
                    ..
                } => {
                    ids.contains(target.as_str())
                        && point(origin)
                        && point(axis)
                        && axis.iter().map(|v| v * v).sum::<f64>() >= 1e-12
                        && number(*angle)
                }
                GeometryOperation::Scale {
                    target,
                    origin,
                    factor,
                    ..
                } => ids.contains(target.as_str()) && point(origin) && positive(*factor),
                GeometryOperation::Union { left, right, .. }
                | GeometryOperation::Difference { left, right, .. } => {
                    left != right && ids.contains(left.as_str()) && ids.contains(right.as_str())
                }
            };
            if !valid {
                return Err(format!(
                    "Invalid geometry or forward/cyclic reference at {id}"
                ));
            }
            ids.insert(id);
        }
        if serde_json::to_vec(self).map_err(|e| e.to_string())?.len() > 262144 {
            return Err("Plan exceeds 256 KiB".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sphere(radius: f64) -> GeometryOperation {
        GeometryOperation::Sphere {
            id: "a".into(),
            origin: [0.; 3],
            radius,
        }
    }
    #[test]
    fn refuses_nonfinite_degenerate_and_unbounded() {
        for radius in [f64::NAN, f64::INFINITY, 0., -1., 1_000_001.] {
            assert!(GeometryPlan {
                units: Units::Meters,
                operations: vec![sphere(radius)]
            }
            .validate()
            .is_err());
        }
    }
    #[test]
    fn refuses_forward_reference_and_duplicate_ids() {
        let plan = GeometryPlan {
            units: Units::Meters,
            operations: vec![
                GeometryOperation::Move {
                    id: "b".into(),
                    target: "a".into(),
                    vector: [1.; 3],
                },
                sphere(1.),
            ],
        };
        assert!(plan.validate().is_err());
        assert!(GeometryPlan {
            units: Units::Meters,
            operations: vec![sphere(1.), sphere(2.)]
        }
        .validate()
        .is_err());
    }
    #[test]
    fn refuses_unknown_operations_and_paths() {
        assert!(serde_json::from_str::<GeometryPlan>(
            r#"{"units":"meters","operations":[{"kind":"script","id":"a","code":"x"}]}"#
        )
        .is_err());
        assert!(serde_json::from_str::<GeometryPlan>(
            r#"{"units":"meters","operations":[],"output":"source.3dm"}"#
        )
        .is_err());
    }
    #[test]
    fn accepts_closed_transform_plan() {
        assert!(GeometryPlan {
            units: Units::Meters,
            operations: vec![
                sphere(1.),
                GeometryOperation::Scale {
                    id: "b".into(),
                    target: "a".into(),
                    origin: [0.; 3],
                    factor: 2.
                }
            ]
        }
        .validate()
        .is_ok());
    }
}
