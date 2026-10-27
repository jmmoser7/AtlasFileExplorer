//! Versioned, declarative control intents. A declaration grants no authority.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

pub const VERSION: u32 = 1;
pub const MAX_PACKET_BYTES: usize = 32 * 1024;
pub const MAX_CONTROLS: usize = 32;
pub const MAX_INTENTS: usize = 8;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub version: u32,
    pub controls: Vec<Control>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Control {
    pub id: String,
    pub input: String,
    pub command: String,
    pub arguments: Value,
    pub value: ValueSpec,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ValueSpec {
    Trigger,
    Integer {
        path: Vec<String>,
        min: i64,
        max: i64,
    },
    Number {
        path: Vec<String>,
        min: f64,
        max: f64,
    },
    Boolean {
        path: Vec<String>,
    },
    Choice {
        path: Vec<String>,
        choices: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Intent {
    pub version: u32,
    pub control_id: String,
    pub sequence: u64,
    pub value: Value,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Packet {
    pub manifest: Manifest,
    pub intents: Vec<Intent>,
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

impl Manifest {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.version != VERSION || self.controls.is_empty() || self.controls.len() > MAX_CONTROLS
        {
            return Err("Unsupported or oversized control manifest");
        }
        let mut ids = HashSet::new();
        for control in &self.controls {
            if !identifier(&control.id)
                || !identifier(&control.input)
                || !identifier(&control.command)
                || !ids.insert(&control.id)
                || !control.arguments.is_object()
            {
                return Err("Invalid or duplicate control declaration");
            }
            control.arguments_for(&match &control.value {
                ValueSpec::Trigger => Value::Null,
                ValueSpec::Integer { min, max, .. } if min <= max => Value::from(*min),
                ValueSpec::Number { min, max, .. }
                    if min.is_finite() && max.is_finite() && min <= max =>
                {
                    Value::from(*min)
                }
                ValueSpec::Boolean { .. } => Value::Bool(false),
                ValueSpec::Choice { choices, .. } if !choices.is_empty() && choices.len() <= 32 => {
                    Value::String(choices[0].clone())
                }
                _ => return Err("Invalid control value bounds"),
            })?;
        }
        Ok(())
    }
}

impl Control {
    pub fn arguments_for(&self, value: &Value) -> Result<Value, &'static str> {
        let path = match &self.value {
            ValueSpec::Trigger if value.is_null() => return Ok(self.arguments.clone()),
            ValueSpec::Integer { path, min, max }
                if value.as_i64().is_some_and(|v| v >= *min && v <= *max) =>
            {
                path
            }
            ValueSpec::Number { path, min, max }
                if value
                    .as_f64()
                    .is_some_and(|v| v.is_finite() && v >= *min && v <= *max) =>
            {
                path
            }
            ValueSpec::Boolean { path } if value.is_boolean() => path,
            ValueSpec::Choice { path, choices }
                if value
                    .as_str()
                    .is_some_and(|v| choices.iter().any(|c| c == v)) =>
            {
                path
            }
            _ => return Err("Control value is outside its declared type or bounds"),
        };
        if path.is_empty() || path.len() > 4 || path.iter().any(|p| !identifier(p)) {
            return Err("Invalid control parameter path");
        }
        let mut arguments = self.arguments.clone();
        let mut target = &mut arguments;
        for key in &path[..path.len() - 1] {
            target = target
                .get_mut(key)
                .ok_or("Control parameter parent does not exist")?;
        }
        target
            .as_object_mut()
            .ok_or("Control parameter parent must be an object")?
            .insert(path.last().unwrap().clone(), value.clone());
        Ok(arguments)
    }
}

pub fn parse_packet(text: &str) -> Result<Packet, &'static str> {
    if text.len() > MAX_PACKET_BYTES {
        return Err("Control packet is too large");
    }
    let packet: Packet = serde_json::from_str(text).map_err(|_| "Invalid control packet")?;
    packet.manifest.validate()?;
    if packet.intents.len() > MAX_INTENTS
        || packet.intents.iter().any(|i| {
            i.version != VERSION
                || i.sequence == 0
                || i.sequence > 9_007_199_254_740_991
                || !identifier(&i.control_id)
        })
    {
        return Err("Invalid or oversized control intent batch");
    }
    Ok(packet)
}

/// Idempotent installation; table-link refreshes do not reset pending intents.
pub const INSTALL_SCRIPT: &str = include_str!("control.js");
/// Fixed host expression, never supplied by a page.
pub const POLL_SCRIPT: &str =
    "(()=>{const value=window.slateControl?.__drain?.();return typeof value==='string'&&value.length<=32768?value:'';})()";

#[cfg(test)]
mod tests {
    use super::*;
    fn control() -> Control {
        Control {
            id: "B01.floors".into(),
            input: "model".into(),
            command: "board.model.connector.invoke".into(),
            arguments: serde_json::json!({"command_id":"org.slate.rhino.draft_building_floors","parameters":{"building_id":"B01"}}),
            value: ValueSpec::Integer {
                path: vec!["parameters".into(), "floors".into()],
                min: 1,
                max: 80,
            },
        }
    }
    #[test]
    fn scalar_updates_only_declared_leaf() {
        let c = control();
        let a = c.arguments_for(&Value::from(12)).unwrap();
        assert_eq!(a["parameters"]["floors"], 12);
        assert_eq!(a["parameters"]["building_id"], "B01");
        for v in [
            Value::from(0),
            Value::from(81),
            Value::from(1.5),
            Value::String("12".into()),
        ] {
            assert!(c.arguments_for(&v).is_err());
        }
    }
    #[test]
    fn duplicate_and_unknown_fields_fail_closed() {
        let c = control();
        assert!(Manifest {
            version: 1,
            controls: vec![c.clone(), c.clone()]
        }
        .validate()
        .is_err());
        let packet = serde_json::json!({"manifest":{"version":1,"controls":[c]},"intents":[{"version":1,"control_id":"B01.floors","sequence":1,"value":12,"target":45}]});
        assert!(parse_packet(&packet.to_string()).is_err());
    }
    #[test]
    fn unsupported_version_and_oversized_packet_fail() {
        assert!(Manifest {
            version: 2,
            controls: vec![control()]
        }
        .validate()
        .is_err());
        assert!(parse_packet(&" ".repeat(MAX_PACKET_BYTES + 1)).is_err());
    }
}
