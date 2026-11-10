//! Shared native receipt projection for collaborators and explicit generator tasks.
use super::*;
use serde_json::json;
use serde_json::Value;
pub(crate) fn outcome_value(
    outcome: &NativeOutcome,
    binding: Option<&atlas_connector::SourceBinding>,
) -> Value {
    let mut value = match outcome {
        NativeOutcome::NotDispatched { reason, error } => {
            json!({"status":"not_dispatched","error":reason,"native_error":error})
        }
        NativeOutcome::RevokedBeforeAdmission => {
            json!({"status":"revoked_before_admission","error":"Native authority was revoked before dispatch"})
        }
        NativeOutcome::Rejected { error } => {
            json!({"status":"native_rejected","error":error.message,"native_error_code":error.code})
        }
        NativeOutcome::OutcomeUnknown { receipt, reason } => {
            json!({"status":"outcome_unknown","error":reason,"receipt":receipt.as_ref().map(|r| json!({"session_id":r.session_id,"generation":r.generation,"request_id":r.request_id,"source":r.source})),"automatic_replay":false})
        }
        NativeOutcome::Completed {
            result: NativeResult::Model { path, report, .. },
        } => json!({"status":"completed","model_source":path,"native_result":report}),
        NativeOutcome::Completed {
            result: NativeResult::Value(result),
        } => json!({"status":"completed","native_result":result}),
        NativeOutcome::Completed {
            result: NativeResult::Capture { path, size },
        } => json!({"status":"completed","image_source":path,"width":size[0],"height":size[1]}),
        NativeOutcome::Completed {
            result: NativeResult::CaptureUnavailable { reason },
        } => {
            json!({"status":"capture_artifact_unavailable","error":reason,"native_capture_completed":true})
        }
    };
    if let Some(binding) = binding {
        value["source"] = json!(binding.source.scope());
        value["document_id"] = json!(binding.document_id);
        value["mode"] = json!(binding.mode);
    }
    value
}
