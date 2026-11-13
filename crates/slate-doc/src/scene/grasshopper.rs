//! Authored overrides for the named inputs of a Grasshopper viewer.

use super::{ConnectorEnd, ConnectorNode, NodeId};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt, str::FromStr};

/// A definition object's InstanceGuid in canonical hyphenated form.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct GrasshopperControlId(String);

/// The stable InstanceGuid of the marker exposing the port in the definition.
pub type GrasshopperPortId = GrasshopperControlId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrasshopperPortDirection {
    Input,
    Output,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrasshopperPortType {
    Number,
    Integer,
    Boolean,
    Text,
    Color,
    Point,
    Geometry,
    Data,
    #[serde(other)]
    Unsupported,
}

impl GrasshopperPortType {
    pub fn accepts(self, output: Self) -> bool {
        self != Self::Unsupported
            && output != Self::Unsupported
            && (self == output
                || self == Self::Data
                || output == Self::Data
                || matches!(
                    (self, output),
                    (Self::Number, Self::Integer) | (Self::Geometry, Self::Point)
                ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrasshopperPortAccess {
    Item,
    List,
    Tree,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrasshopperPort {
    pub id: GrasshopperPortId,
    pub name: String,
    pub direction: GrasshopperPortDirection,
    pub kind: GrasshopperPortType,
    pub access: GrasshopperPortAccess,
}

/// A and B supply the node identities so copying/remapping a connector moves
/// both endpoints without duplicating workbook-local IDs in this metadata.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrasshopperWirePorts {
    pub source_port: GrasshopperPortId,
    pub target_port: GrasshopperPortId,
    pub kind: GrasshopperPortType,
    #[serde(default, skip_serializing_if = "super::is_false")]
    pub bundled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GrasshopperWire {
    pub source: NodeId,
    pub target: NodeId,
    pub source_port: GrasshopperPortId,
    pub target_port: GrasshopperPortId,
    pub kind: GrasshopperPortType,
    pub bundled: bool,
}

pub fn grasshopper_wire(conn: &ConnectorNode) -> Option<GrasshopperWire> {
    let wire = conn.grasshopper.as_deref()?;
    let (ConnectorEnd::Anchored { node: source, .. }, ConnectorEnd::Anchored { node: target, .. }) =
        (&conn.a, &conn.b)
    else {
        return None;
    };
    Some(GrasshopperWire {
        source: *source,
        target: *target,
        source_port: wire.source_port.clone(),
        target_port: wire.target_port.clone(),
        kind: wire.kind,
        bundled: wire.bundled,
    })
}

impl GrasshopperControlId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for GrasshopperControlId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for GrasshopperControlId {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.len() != 36
            || !value.bytes().enumerate().all(|(i, byte)| match i {
                8 | 13 | 18 | 23 => byte == b'-',
                _ => byte.is_ascii_hexdigit(),
            })
        {
            return Err("expected a hyphenated Grasshopper InstanceGuid");
        }
        Ok(Self(value.to_ascii_lowercase()))
    }
}

impl TryFrom<String> for GrasshopperControlId {
    type Error = &'static str;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

/// A viewer's authored value. The definition owns ranges, defaults, and choices.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum GrasshopperValue {
    Number(f64),
    Bool(bool),
    Text(String),
    Color([u8; 4]),
    /// Opaque IDs of options supplied by the loaded definition.
    Choice(Vec<String>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GrasshopperControlKind {
    Slider,
    Toggle,
    List,
    Text,
    Color,
    #[serde(other)]
    Readonly,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrasshopperChoice {
    pub id: String,
    pub label: String,
}

/// Definition-owned metadata transported from the Grasshopper worker.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GrasshopperControl {
    pub id: GrasshopperControlId,
    pub name: String,
    pub kind: GrasshopperControlKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<GrasshopperValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub precision: Option<usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<GrasshopperChoice>,
    #[serde(default)]
    pub multiple: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// Overrides belong to one viewer. Missing keys use the definition's values.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GrasshopperOverrides(BTreeMap<GrasshopperControlId, GrasshopperValue>);

impl GrasshopperOverrides {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn get(&self, id: &GrasshopperControlId) -> Option<&GrasshopperValue> {
        self.0.get(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&GrasshopperControlId, &GrasshopperValue)> {
        self.0.iter()
    }

    pub fn set(
        &mut self,
        id: GrasshopperControlId,
        value: GrasshopperValue,
    ) -> Result<(), &'static str> {
        if matches!(&value, GrasshopperValue::Number(number) if !number.is_finite()) {
            return Err("Grasshopper input must be a finite number");
        }
        if matches!(&value, GrasshopperValue::Choice(ids) if ids.iter().any(String::is_empty)) {
            return Err("Grasshopper option IDs must be nonempty");
        }
        self.0.insert(id, value);
        Ok(())
    }

    pub fn remove(&mut self, id: &GrasshopperControlId) {
        self.0.remove(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u8) -> GrasshopperControlId {
        format!("a4a3e1bc-c137-497e-8a0d-1e7f8ce605{n:02x}")
            .parse()
            .unwrap()
    }

    #[test]
    fn round_trips_typed_values_as_a_map_of_instance_ids() {
        let mut values = GrasshopperOverrides::default();
        let authored = [
            GrasshopperValue::Number(1.25),
            GrasshopperValue::Bool(false),
            GrasshopperValue::Text(String::new()),
            GrasshopperValue::Color([5, 140, 149, 255]),
            GrasshopperValue::Choice(vec!["one".into(), "three".into()]),
            GrasshopperValue::Choice(Vec::new()),
        ];
        for (n, value) in authored.into_iter().enumerate() {
            values.set(id(n as u8), value).unwrap();
        }

        let encoded = serde_json::to_value(&values).unwrap();
        assert_eq!(
            encoded[id(0).as_str()],
            serde_json::json!({ "type": "number", "value": 1.25 })
        );
        assert_eq!(
            encoded[id(4).as_str()],
            serde_json::json!({ "type": "choice", "value": ["one", "three"] })
        );
        assert_eq!(
            serde_json::from_value::<GrasshopperOverrides>(encoded).unwrap(),
            values
        );
    }

    #[test]
    fn canonicalizes_instance_ids_and_rejects_non_guids() {
        assert_eq!(id(0).as_str().to_ascii_uppercase().parse(), Ok(id(0)));
        assert!(serde_json::from_str::<GrasshopperOverrides>(
            r#"{"slider-name":{"type":"number","value":1}}"#
        )
        .is_err());
    }

    #[test]
    fn a_rejected_edit_keeps_the_previous_override() {
        let control = id(0);
        let mut values = GrasshopperOverrides::default();
        values
            .set(control.clone(), GrasshopperValue::Number(4.0))
            .unwrap();
        assert!(values
            .set(control.clone(), GrasshopperValue::Number(f64::INFINITY))
            .is_err());
        assert!(values
            .set(
                control.clone(),
                GrasshopperValue::Choice(vec![String::new()])
            )
            .is_err());
        assert_eq!(values.get(&control), Some(&GrasshopperValue::Number(4.0)));
        values.remove(&control);
        assert!(values.is_empty());
    }
}
