//! Enforced v0 parameter dialect: closed object, <=64 scalar properties;
//! scalar type, enum, numeric bounds and string lengths only. No references,
//! nesting, combinators, regex, coercion or executable expressions.

use serde_json::{Map, Value};

use crate::{ErrorCode, ProtocolError};

pub const MAX_PARAMETER_BYTES: usize = 64 * 1024;
pub const MAX_PARAMETER_STRING: u64 = 4096;

fn invalid(message: impl Into<String>) -> ProtocolError {
    ProtocolError::new(ErrorCode::InvalidDescriptor, message)
}

pub fn validate_parameter_schema(schema: &Value) -> Result<(), ProtocolError> {
    let object = schema
        .as_object()
        .ok_or_else(|| invalid("parameter schema must be an object"))?;
    allowed_keys(
        object,
        &[
            "$schema",
            "title",
            "description",
            "type",
            "properties",
            "required",
            "additionalProperties",
        ],
    )?;
    if object.get("type").and_then(Value::as_str) != Some("object")
        || object.get("additionalProperties") != Some(&Value::Bool(false))
    {
        return Err(invalid("v0 parameters require a closed object schema"));
    }
    let properties = object
        .get("properties")
        .and_then(Value::as_object)
        .ok_or_else(|| invalid("properties must be an object"))?;
    if properties.len() > 64 {
        return Err(invalid("too many parameter properties"));
    }
    for (name, shape) in properties {
        if !crate::validation::valid_id(name) {
            return Err(invalid("invalid parameter name"));
        }
        let shape = shape
            .as_object()
            .ok_or_else(|| invalid("scalar shape must be an object"))?;
        allowed_keys(
            shape,
            &[
                "title",
                "description",
                "type",
                "enum",
                "minimum",
                "maximum",
                "minLength",
                "maxLength",
            ],
        )?;
        let kind = shape
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("missing scalar type"))?;
        if !["string", "integer", "number", "boolean"].contains(&kind) {
            return Err(invalid("unsupported scalar parameter type"));
        }
        for key in ["minimum", "maximum"] {
            if let Some(bound) = shape.get(key) {
                if !["integer", "number"].contains(&kind) || bound.as_f64().is_none() {
                    return Err(invalid(
                        "numeric bounds require a numeric property and number",
                    ));
                }
            }
        }
        for key in ["minLength", "maxLength"] {
            if let Some(bound) = shape.get(key) {
                if kind != "string" || bound.as_u64().is_none_or(|n| n > MAX_PARAMETER_STRING) {
                    return Err(invalid("string bounds must be <=4096"));
                }
            }
        }
        if let Some(choices) = shape.get("enum") {
            let choices = choices
                .as_array()
                .ok_or_else(|| invalid("enum must be an array"))?;
            if choices.is_empty()
                || choices.len() > 64
                || choices.iter().any(|v| !matches_type(v, kind))
            {
                return Err(invalid(
                    "enum must contain 1..64 values of the declared scalar type",
                ));
            }
        }
        if shape
            .get("minimum")
            .zip(shape.get("maximum"))
            .is_some_and(|(a, b)| numeric_order(a, b) == Some(std::cmp::Ordering::Greater))
            || shape.get("minLength").and_then(Value::as_u64).unwrap_or(0)
                > shape
                    .get("maxLength")
                    .and_then(Value::as_u64)
                    .unwrap_or(MAX_PARAMETER_STRING)
        {
            return Err(invalid("inverted parameter bounds"));
        }
    }
    if let Some(required) = object.get("required") {
        let required = required
            .as_array()
            .ok_or_else(|| invalid("required must be an array"))?;
        let unique: std::collections::BTreeSet<_> =
            required.iter().filter_map(Value::as_str).collect();
        if required.len() > properties.len()
            || unique.len() != required.len()
            || required
                .iter()
                .any(|v| v.as_str().is_none_or(|name| !properties.contains_key(name)))
        {
            return Err(invalid("required names must exist in properties"));
        }
    }
    if serde_json::to_vec(schema).map_or(true, |bytes| bytes.len() > MAX_PARAMETER_BYTES) {
        return Err(invalid("parameter schema exceeds size limit"));
    }
    Ok(())
}

fn allowed_keys(object: &Map<String, Value>, allowed: &[&str]) -> Result<(), ProtocolError> {
    if let Some(key) = object.keys().find(|key| !allowed.contains(&key.as_str())) {
        return Err(invalid(format!("unsupported schema keyword: {key}")));
    }
    if ["title", "description", "$schema"]
        .iter()
        .any(|key| object.get(*key).is_some_and(|value| !value.is_string()))
    {
        return Err(invalid("schema annotations must be strings"));
    }
    if object
        .get("$schema")
        .and_then(Value::as_str)
        .is_some_and(|s| s != "https://json-schema.org/draft/2020-12/schema")
    {
        return Err(invalid(
            "v0 parameter schemas use the JSON Schema 2020-12 dialect",
        ));
    }
    Ok(())
}

fn matches_type(value: &Value, kind: &str) -> bool {
    match kind {
        "string" => value
            .as_str()
            .is_some_and(|s| s.chars().count() as u64 <= MAX_PARAMETER_STRING),
        "boolean" => value.is_boolean(),
        "integer" => {
            value.is_i64() || value.is_u64() || value.as_f64().is_some_and(|n| n.fract() == 0.0)
        }
        "number" => value.is_number(),
        _ => false,
    }
}

fn integral_value(value: &Value) -> Option<i128> {
    value
        .as_i64()
        .map(i128::from)
        .or_else(|| value.as_u64().map(i128::from))
        .or_else(|| {
            let number = value.as_f64()?;
            (number.fract() == 0.0 && number >= i128::MIN as f64 && number < i128::MAX as f64)
                .then_some(number as i128)
        })
}

fn numeric_order(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    match (integral_value(a), integral_value(b)) {
        (Some(a), Some(b)) => Some(a.cmp(&b)),
        _ => a.as_f64()?.partial_cmp(&b.as_f64()?),
    }
}

fn scalar_equal(a: &Value, b: &Value) -> bool {
    if a.is_number() && b.is_number() {
        numeric_order(a, b) == Some(std::cmp::Ordering::Equal)
    } else {
        a == b
    }
}

pub fn validate_parameters(schema: &Value, parameters: &Value) -> Result<(), ProtocolError> {
    validate_parameter_schema(schema)?;
    let bad = |message| ProtocolError::new(ErrorCode::InvalidRequest, message);
    let params = parameters
        .as_object()
        .ok_or_else(|| bad("parameters must be an object"))?;
    if serde_json::to_vec(parameters).map_or(true, |b| b.len() > MAX_PARAMETER_BYTES) {
        return Err(bad("parameters exceed size limit"));
    }
    let properties = schema["properties"].as_object().expect("validated object");
    if let Some(required) = schema.get("required").and_then(Value::as_array) {
        if required
            .iter()
            .any(|name| !params.contains_key(name.as_str().expect("validated name")))
        {
            return Err(bad("missing required parameter"));
        }
    }
    for (name, value) in params {
        let shape = properties
            .get(name)
            .ok_or_else(|| bad("unknown parameter"))?;
        let kind = shape["type"].as_str().expect("validated type");
        if !matches_type(value, kind) {
            return Err(bad(
                "parameter has wrong scalar type or exceeds length limit",
            ));
        }
        if shape
            .get("enum")
            .and_then(Value::as_array)
            .is_some_and(|choices| !choices.iter().any(|choice| scalar_equal(choice, value)))
        {
            return Err(bad("parameter is outside enum"));
        }
        if value.is_number()
            && (shape
                .get("minimum")
                .is_some_and(|bound| numeric_order(value, bound) == Some(std::cmp::Ordering::Less))
                || shape.get("maximum").is_some_and(|bound| {
                    numeric_order(value, bound) == Some(std::cmp::Ordering::Greater)
                }))
        {
            return Err(bad("parameter is outside numeric bounds"));
        }
        if let Some(string) = value.as_str() {
            let length = string.chars().count() as u64;
            if length < shape.get("minLength").and_then(Value::as_u64).unwrap_or(0)
                || length
                    > shape
                        .get("maxLength")
                        .and_then(Value::as_u64)
                        .unwrap_or(MAX_PARAMETER_STRING)
            {
                return Err(bad("parameter is outside string length bounds"));
            }
        }
    }
    Ok(())
}
