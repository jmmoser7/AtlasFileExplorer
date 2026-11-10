//! Only documented public event metadata. Never model thoughts, bodies or hashes.
use serde_json::{json, Value};
use std::collections::BTreeMap;
#[derive(Default)]
pub(super) struct Status {
    counts: BTreeMap<&'static str, u64>,
    first: u64,
    last: u64,
    latest: Value,
    last_important: Value,
}
impl Status {
    pub(super) fn observe(&mut self, message: &Value) -> Option<(Value, Option<&'static str>)> {
        let kind = match message["type"].as_str() {
            Some("system") => match message["subtype"].as_str() {
                Some("init") => "init",
                Some("status") => "status",
                Some("api_retry") => "api_retry",
                Some("control_request_progress") => "control_request_progress",
                Some("thinking_tokens") => "thinking_tokens",
                Some("task_started") => "task_started",
                Some("task_progress") => "task_progress",
                Some("task_updated") => "task_updated",
                Some("task_notification") => "task_notification",
                Some("session_state_changed") => "session_state_changed",
                Some("model_refusal_fallback") => "model_refusal_fallback",
                Some("model_refusal_no_fallback") => "model_refusal_no_fallback",
                _ => "other_system",
            },
            Some("rate_limit_event") => "rate_limit_event",
            Some("tool_progress") => "tool_progress",
            _ => return None,
        };
        let at = super::now();
        if self.first == 0 {
            self.first = at;
        }
        self.last = at;
        *self.counts.entry(kind).or_default() += 1;
        let mut fields =
            json!({"type":if message["type"]=="system"{"system"}else{kind},"subtype":kind});
        let status = message["status"]
            .as_str()
            .or_else(|| message["rate_limit_info"]["status"].as_str());
        if let Some(status) = status.filter(|s| {
            matches!(
                *s,
                "started"
                    | "api_retry"
                    | "compacting"
                    | "completed"
                    | "failed"
                    | "stopped"
                    | "killed"
                    | "running"
                    | "allowed"
                    | "allowed_warning"
                    | "rejected"
            )
        }) {
            fields["status"] = json!(status);
        }
        for key in [
            "attempt",
            "retry_count",
            "max_retries",
            "retry_delay_ms",
            "error_status",
            "status_code",
        ] {
            if let Some(value) = message[key].as_u64().filter(|v| *v <= 3_600_000) {
                fields[key] = json!(value);
            }
        }
        if message["rate_limit_info"]["errorCode"] == "credits_required" {
            fields["error_code"] = json!("credits_required");
        }
        if let Some(code) = message["error_code"].as_str().filter(|s| {
            matches!(
                *s,
                "rate_limit_error"
                    | "overloaded_error"
                    | "api_error"
                    | "authentication_error"
                    | "invalid_request_error"
            )
        }) {
            fields["error_code"] = json!(code);
        }
        if let Some(id) = message["request_id"]
            .as_str()
            .filter(|id| atlas_agent::collaborator::valid_id(id))
        {
            fields["control_request_id"] = json!(id);
        }
        self.latest = fields.clone();
        let important = matches!(
            kind,
            "api_retry" | "model_refusal_fallback" | "model_refusal_no_fallback"
        ) || fields["status"] == "api_retry"
            || (kind == "rate_limit_event" && fields["status"] != "allowed");
        if !important || self.last_important == fields {
            return None;
        }
        self.last_important = fields.clone();
        let label = if fields["error_code"] == "credits_required" {
            Some("Subscription usage is currently unavailable")
        } else if kind == "api_retry" || fields["status"] == "api_retry" {
            Some("The provider is retrying a request")
        } else if kind.starts_with("model_refusal") {
            Some("The provider reported a model refusal")
        } else {
            None
        };
        Some((fields, label))
    }
    pub(super) fn summary(&self, request: &str) -> Value {
        json!({"request":request,"first_at_unix_ms":self.first,"last_at_unix_ms":self.last,"public_category_counts":self.counts,"latest_whitelisted_status":self.latest,"raw_body_recorded":false,"private_thinking_text_or_hash_recorded":false})
    }
}
