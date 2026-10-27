//! A completed native command can truthfully report a partial domain effect.
use crossbeam_channel::Sender;
use serde_json::{json, Value};

pub(super) fn require_complete_clear(
    receipt: &Value,
    index: usize,
    author: &str,
    events: &Sender<Value>,
) -> Result<(), String> {
    if receipt["command"] != "org.slate.rhino.draft_clear" {
        return Ok(());
    }
    let value = &receipt["result"]["value"];
    if receipt["result"]["status"] == "command_result"
        && value["counts_verified"] == true
        && value["cleared"] == true
        && value["remaining"].as_u64() == Some(0)
    {
        return Ok(());
    }
    // Preserve the real receipt and post-effect scope before the controller
    // stops. This is a known partial mutation, never a no-effect rejection.
    let _ = events.try_send(json!({
        "status":"native_partial", "index":index, "author":author,
        "command":"board.model.connector.invoke", "native_receipt":receipt,
        "at_unix_ms":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis()
    }));
    Err("Rhino draft clear did not verify an empty draft; its actual receipt was retained and subsequent geometry was stopped".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_clear_preserves_actual_effect_scope_before_stopping() {
        let (events, received) = crossbeam_channel::bounded(8);
        for value in [
            json!({"cleared":false,"remaining":2,"counts_verified":true,"removed":453}),
            json!({"cleared":false,"remaining":null,"counts_verified":false}),
            json!({"cleared":true,"remaining":0,"counts_verified":false}),
            json!({"cleared":true,"remaining":1,"counts_verified":true}),
        ] {
            let receipt = json!({"command":"org.slate.rhino.draft_clear","scope":{"source_id":"draft","revision":"actual-after-clear"},"result":{"status":"command_result","value":value}});
            assert!(require_complete_clear(&receipt, 3, "Modeler", &events).is_err());
            let event = received.try_recv().unwrap();
            assert_eq!(event["status"], "native_partial");
            assert_eq!(event["index"], 3);
            assert_eq!(event["native_receipt"], receipt);
        }
        let complete = json!({"command":"org.slate.rhino.draft_clear","result":{"status":"command_result","value":{"cleared":true,"remaining":0,"counts_verified":true}}});
        assert!(require_complete_clear(&complete, 3, "Modeler", &events).is_ok());
        assert!(require_complete_clear(
            &json!({"command":"org.slate.rhino.draft_box"}),
            4,
            "Modeler",
            &events
        )
        .is_ok());
        assert!(received.try_recv().is_err());
    }
}
