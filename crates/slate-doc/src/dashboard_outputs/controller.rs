//! Portable, data-only parameters authored on a single local dashboard instance.
use super::{parse, GrasshopperPortId, Snapshot, MAX_BYTES};
use serde::{de::Error, Deserialize, Deserializer, Serialize};
use serde_json::Value;

pub const MAX_STATE_BYTES: usize = 8192;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ControllerState {
    pub schema: GrasshopperPortId,
    pub revision: GrasshopperPortId,
    #[serde(deserialize_with = "bounded_json")]
    pub value: String,
}

fn bounded_json<'de, D: Deserializer<'de>>(input: D) -> Result<String, D::Error> {
    let text = String::deserialize(input)?;
    if text.len() > MAX_STATE_BYTES {
        return Err(D::Error::custom("Dashboard parameters exceed 8 KiB"));
    }
    serde_json::from_str::<Value>(&text).map_err(D::Error::custom)?;
    Ok(text)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControllerEdit {
    pub base: Option<GrasshopperPortId>,
    pub state: ControllerState,
}

pub fn observation(raw: &str) -> Result<(Snapshot, Option<ControllerEdit>), &'static str> {
    // A literal record separator cannot occur inside valid JSON.
    let Some((parameters, outputs)) = raw.split_once('\u{1e}') else {
        return Ok((parse(raw)?, None));
    };
    if parameters.len() > MAX_STATE_BYTES * 6 + 512 || outputs.len() > MAX_BYTES {
        return Err("Dashboard observation exceeds its size limit.");
    }
    let edit: ControllerEdit =
        serde_json::from_str(parameters).map_err(|_| "Invalid dashboard parameters.")?;
    if edit.base.as_ref() == Some(&edit.state.revision) {
        return Err("Dashboard parameter edit needs a new revision.");
    }
    Ok((parse(outputs)?, Some(edit)))
}
