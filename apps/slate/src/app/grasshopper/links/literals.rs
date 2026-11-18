use super::*;

pub(super) fn literal_kind(value: &GrasshopperValue) -> Option<GrasshopperPortType> {
    Some(match value {
        GrasshopperValue::Number(n)
            if n.is_finite()
                && n.fract() == 0.0
                && *n >= f64::from(i32::MIN)
                && *n <= f64::from(i32::MAX) =>
        {
            GrasshopperPortType::Integer
        }
        GrasshopperValue::Number(n) if n.is_finite() => GrasshopperPortType::Number,
        GrasshopperValue::Bool(_) => GrasshopperPortType::Boolean,
        GrasshopperValue::Text(_) => GrasshopperPortType::Text,
        GrasshopperValue::Color(_) => GrasshopperPortType::Color,
        _ => return None,
    })
}

pub(super) fn literal_packet(
    value: &GrasshopperValue,
    target: GrasshopperPortType,
) -> Option<serde_json::Value> {
    let kind = literal_kind(value)?;
    if !target.accepts(kind) {
        return None;
    }
    let value = match value {
        GrasshopperValue::Number(n) if kind == GrasshopperPortType::Integer => {
            serde_json::json!(*n as i32)
        }
        GrasshopperValue::Number(n) => serde_json::json!(n),
        GrasshopperValue::Bool(b) => serde_json::json!(b),
        GrasshopperValue::Text(s) => serde_json::json!(s),
        GrasshopperValue::Color(rgba) => serde_json::json!(rgba),
        GrasshopperValue::Choice(_) => return None,
    };
    Some(serde_json::json!({"branches":[{"path":[0],"items":[{"kind":kind,"value":value}]}]}))
}
