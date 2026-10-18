use atlas_connector::*;
use serde_json::json;

#[test]
fn binary_handshake_uses_public_strict_types_and_generated_schemas() {
    let fixtures = artifact_fixture_documents();
    let transfer: ArtifactTransfer =
        serde_json::from_value(fixtures["artifact-transfer.json"].clone()).unwrap();
    transfer.validate().unwrap();
    assert_eq!(transfer.byte_length, 5);
    assert_eq!(
        transfer.sha256,
        "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
    );
    let completion: ArtifactCompletion =
        serde_json::from_value(fixtures["artifact-completion.json"].clone()).unwrap();
    let receipt: ArtifactReceipt =
        serde_json::from_value(fixtures["artifact-receipt.json"].clone()).unwrap();
    assert_eq!(completion, ArtifactCompletion::new(&transfer));
    assert_eq!(receipt, ArtifactReceipt::new(&transfer));
    let schemas = schema_documents();
    assert_eq!(
        schemas["artifact-completion.schema.json"]["additionalProperties"],
        false
    );
    assert_eq!(
        schemas["artifact-receipt.schema.json"]["additionalProperties"],
        false
    );
    assert!(serde_json::from_value::<ArtifactCompletion>(
        json!({"transfer_id":"fixture-hello", "complete":true, "extra":true})
    )
    .is_err());
    assert!(serde_json::from_value::<ArtifactReceipt>(json!({"transfer_id":"fixture-hello", "sha256":transfer.sha256, "verified":true, "extra":true})).is_err());
}

#[test]
fn transfer_id_hash_and_boolean_tampering_cannot_match_expected_receipts() {
    let transfer: ArtifactTransfer =
        serde_json::from_value(artifact_fixture_documents()["artifact-transfer.json"].clone())
            .unwrap();
    let expected_completion = ArtifactCompletion::new(&transfer);
    let expected_receipt = ArtifactReceipt::new(&transfer);
    let mut completion = expected_completion.clone();
    completion.complete = false;
    assert_ne!(completion, expected_completion);
    let mut completion = expected_completion.clone();
    completion.transfer_id = "other-transfer".into();
    assert_ne!(completion, expected_completion);
    let mut receipt = expected_receipt.clone();
    receipt.verified = false;
    assert_ne!(receipt, expected_receipt);
    let mut receipt = expected_receipt.clone();
    receipt.sha256 = "f".repeat(64);
    assert_ne!(receipt, expected_receipt);
    let mut receipt = expected_receipt.clone();
    receipt.transfer_id = "other-transfer".into();
    assert_ne!(receipt, expected_receipt);
}
