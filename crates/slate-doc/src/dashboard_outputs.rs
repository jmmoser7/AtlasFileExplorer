//! Bounded data published by an HTML component for Grasshopper connections.
use crate::scene::{
    GrasshopperPort, GrasshopperPortAccess as Access, GrasshopperPortDirection, GrasshopperPortId,
    GrasshopperPortType as Kind,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

pub mod controller;

#[cfg(test)]
mod tests;

pub const MAX_BYTES: usize = 262144;
const MAX_ITEMS: usize = 10_000;

#[derive(Default, Debug, PartialEq)]
pub struct Snapshot {
    pub ports: Vec<GrasshopperPort>,
    pub packets: BTreeMap<GrasshopperPortId, Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Declaration {
    version: u8,
    outputs: Vec<Output>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Output {
    id: GrasshopperPortId,
    name: String,
    #[serde(rename = "type")]
    kind: Kind,
    access: Access,
    value: Value,
}

pub fn parse(text: &str) -> Result<Snapshot, &'static str> {
    if text.len() > MAX_BYTES {
        return Err("Output declaration exceeds 256 KiB.");
    }
    let declaration: Declaration =
        serde_json::from_str(text).map_err(|_| "Invalid dashboard output declaration.")?;
    if declaration.version != 1 || declaration.outputs.len() > 32 {
        return Err("Expected version 1 with up to 32 dashboard outputs.");
    }
    let mut result = Snapshot::default();
    let mut remaining = MAX_ITEMS;
    for Output {
        id,
        name,
        kind,
        access,
        value,
    } in declaration.outputs
    {
        if name.trim().is_empty() || name.len() > 128 || result.packets.contains_key(&id) {
            return Err("Output names must be 1–128 bytes and IDs must be unique.");
        }
        if !matches!(
            kind,
            Kind::Number | Kind::Integer | Kind::Boolean | Kind::Text | Kind::Color | Kind::Point
        ) {
            return Err("Unsupported dashboard output type.");
        }
        let raw_branches = match access {
            Access::Item => vec![json!({"path":[0],"items":[value]})],
            Access::List => vec![json!({"path":[0],"items":value})],
            Access::Tree => match value {
                Value::Array(a) => a,
                _ => return Err("Expected a tree of branches."),
            },
        };
        if raw_branches.len() > 1024 {
            return Err("Too many tree branches.");
        }
        let mut paths = BTreeSet::new();
        let mut branches = Vec::new();
        for branch in raw_branches {
            let path = branch["path"].as_array().ok_or("Invalid branch path.")?;
            if path.is_empty() || path.len() > 64 {
                return Err("Invalid branch path.");
            }
            let path: Vec<_> = path
                .iter()
                .map(|v| {
                    v.as_u64()
                        .and_then(|n| i32::try_from(n).ok())
                        .ok_or("Invalid branch index.")
                })
                .collect::<Result<_, _>>()?;
            if !paths.insert(path.clone()) {
                return Err("Duplicate branch path.");
            }
            let items = branch["items"]
                .as_array()
                .ok_or("Expected a list of items.")?;
            remaining = remaining
                .checked_sub(items.len())
                .ok_or("Too many dashboard items.")?;
            let mut typed = Vec::with_capacity(items.len());
            for value in items {
                let valid = match kind {
                    Kind::Number => value.as_f64().is_some_and(f64::is_finite),
                    Kind::Integer => value.as_i64().is_some_and(|v| i32::try_from(v).is_ok()),
                    Kind::Boolean => value.is_boolean(),
                    Kind::Text => value.is_string(),
                    Kind::Color => value.as_array().is_some_and(|a| {
                        a.len() == 4 && a.iter().all(|n| n.as_u64().is_some_and(|n| n <= 255))
                    }),
                    Kind::Point => value.as_array().is_some_and(|a| {
                        a.len() == 3 && a.iter().all(|n| n.as_f64().is_some_and(f64::is_finite))
                    }),
                    _ => false,
                };
                if !valid {
                    return Err("Dashboard value does not match its declared type.");
                }
                typed.push(json!({"kind":kind,"value":value}));
            }
            branches.push(json!({"path":path,"items":typed}));
        }
        result
            .packets
            .insert(id.clone(), json!({"branches":branches}));
        result.ports.push(GrasshopperPort {
            id,
            name,
            direction: GrasshopperPortDirection::Output,
            kind,
            access,
        });
    }
    Ok(result)
}
