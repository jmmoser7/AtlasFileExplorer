//! Retain the validated native result without retaining an unbounded payload.
use atlas_connector::{ReplyBody, SourceScope};
use serde_json::{json, Value};

pub(super) fn invocation_receipt(
    command: &str,
    scope: Option<SourceScope>,
    body: ReplyBody,
) -> Value {
    let result = bounded_body(body, 64 * 1024);
    json!({"command":command,"scope":scope,"result":result})
}

pub(super) fn bounded_body(body: ReplyBody, limit: usize) -> Value {
    let bytes = serde_json::to_vec(&body).unwrap_or_default();
    if bytes.len() <= limit {
        serde_json::to_value(body).unwrap_or(Value::Null)
    } else {
        json!({"status":"validated_result_omitted","bytes":bytes.len(),"sha256":model_preview::view_meta::hash_bytes(&bytes)})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_receipts_preserve_small_results_and_bound_large_successes() {
        let scope = SourceScope {
            source_id: "model".into(),
            revision: "after-save".into(),
        };
        let small = invocation_receipt(
            "inspect",
            Some(scope),
            ReplyBody::CommandResult {
                value: json!({"objects":455,"path":"draft.3dm"}),
                revision: Some("after-save".into()),
            },
        );
        assert_eq!(small["scope"]["revision"], "after-save");
        assert_eq!(small["result"]["value"]["objects"], 455);
        let large = invocation_receipt(
            "inspect",
            None,
            ReplyBody::CommandResult {
                value: json!("x".repeat(70 * 1024)),
                revision: None,
            },
        );
        assert_eq!(large["result"]["status"], "validated_result_omitted");
        assert!(serde_json::to_vec(&large).unwrap().len() < 1024);
    }
}
