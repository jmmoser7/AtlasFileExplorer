use atlas_connector::{ArtifactTransfer, MAX_ARTIFACT_BYTES};
use atlas_connector_runtime::{
    artifact::{
        receive_artifact, transfer_address, ArtifactError, ArtifactSender, ArtifactStore,
        MAX_STORE_TRANSFERS,
    },
    transport::{Cancellation, PipeAddress},
};
use std::{sync::Arc, time::Duration};

fn control() -> PipeAddress {
    PipeAddress::from_token("a".repeat(64)).unwrap()
}

fn descriptor() -> ArtifactTransfer {
    ArtifactStore::default()
        .insert("capture-1", "image/png", b"abc".to_vec())
        .unwrap()
}

#[test]
fn retained_bytes_have_real_sha256_and_immutable_ids() {
    let mut store = ArtifactStore::new(8).unwrap();
    let first = store
        .insert("capture-1", "image/png", b"abc".to_vec())
        .unwrap();
    assert_eq!(
        first.sha256,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(first.byte_length, 3);
    assert_eq!(&*store.get("capture-1").unwrap(), b"abc");
    assert_eq!(store.descriptor("capture-1"), Some(&first));
    assert!(Arc::ptr_eq(
        &store.get("capture-1").unwrap(),
        &store.get("capture-1").unwrap()
    ));
    assert!(matches!(
        store.insert("capture-1", "image/jpeg", b"changed".to_vec()),
        Err(ArtifactError::IdConflict)
    ));
    assert_eq!(&*store.get("capture-1").unwrap(), b"abc");
    assert_eq!(store.used_bytes(), 3);
    assert_eq!(store.len(), 1);
}

#[test]
fn failed_inserts_do_not_consume_budget() {
    let mut store = ArtifactStore::new(3).unwrap();
    assert!(store.is_empty());
    assert!(matches!(
        store.insert("empty", "image/png", vec![]),
        Err(ArtifactError::InvalidDescriptor(_))
    ));
    assert!(matches!(
        store.insert("bad-mime", "image/png\r\nx: y", vec![1]),
        Err(ArtifactError::InvalidDescriptor(_))
    ));
    assert!(matches!(
        store.insert("../path", "image/png", vec![1]),
        Err(ArtifactError::InvalidDescriptor(_))
    ));
    assert!(matches!(
        store.insert("too-large", "image/png", vec![1; 4]),
        Err(ArtifactError::BudgetExceeded)
    ));
    assert_eq!(store.used_bytes(), 0);
    store.insert("fits", "image/png", vec![1; 3]).unwrap();
    assert!(matches!(
        store.insert("full", "image/png", vec![1]),
        Err(ArtifactError::BudgetExceeded)
    ));
    assert_eq!(store.used_bytes(), store.byte_budget());
}

#[test]
fn tiny_artifacts_cannot_exhaust_memory_through_unbounded_descriptors() {
    let mut store = ArtifactStore::default();
    for id in 0..MAX_STORE_TRANSFERS {
        store
            .insert(
                format!("tiny-{id}"),
                "application/octet-stream",
                vec![id as u8],
            )
            .unwrap();
    }
    assert!(matches!(
        store.insert("one-more", "image/png", vec![1]),
        Err(ArtifactError::TooManyTransfers)
    ));
    assert_eq!(store.used_bytes(), MAX_STORE_TRANSFERS);
}

#[test]
fn transfer_name_is_derived_without_accepting_paths_or_urls() {
    let descriptor = descriptor();
    let address = transfer_address(&control(), &descriptor).unwrap();
    assert_eq!(
        address.token(),
        "395d6749d2ead254b559ac20402526e35026ae1b5c0b10434069a1cae08651bf"
    );
    assert_ne!(address, control());
    let mut another = descriptor.clone();
    another.transfer_id = "capture-2".into();
    assert_ne!(address, transfer_address(&control(), &another).unwrap());
    let other_session = PipeAddress::from_token("b".repeat(64)).unwrap();
    assert_ne!(
        address,
        transfer_address(&other_session, &descriptor).unwrap()
    );
    another.transfer_id = "https://outside/file".into();
    assert!(matches!(
        transfer_address(&control(), &another),
        Err(ArtifactError::InvalidDescriptor(_))
    ));
}

#[test]
fn malformed_and_oversized_descriptors_fail_before_connect_or_allocation() {
    let cancel = Cancellation::default();
    let receive = |d: &ArtifactTransfer, budget| {
        receive_artifact(&control(), d, 1, budget, Duration::ZERO, &cancel)
    };
    let mut d = descriptor();
    d.byte_length = u64::MAX;
    assert!(matches!(
        receive(&d, 3),
        Err(ArtifactError::InvalidDescriptor(_))
    ));
    d.byte_length = 0;
    assert!(matches!(
        receive(&d, 3),
        Err(ArtifactError::InvalidDescriptor(_))
    ));
    d = descriptor();
    assert!(matches!(receive(&d, 2), Err(ArtifactError::BudgetExceeded)));
    d.sha256 = "A".repeat(64);
    assert!(matches!(
        receive(&d, 3),
        Err(ArtifactError::InvalidDescriptor(_))
    ));
    assert!(matches!(
        ArtifactStore::new(0),
        Err(ArtifactError::InvalidBudget)
    ));
    assert!(matches!(
        ArtifactStore::new(MAX_ARTIFACT_BYTES as usize + 1),
        Err(ArtifactError::InvalidBudget)
    ));
}

#[test]
fn sender_refuses_wrong_bytes_before_creating_a_pipe() {
    let d = descriptor();
    assert!(matches!(
        ArtifactSender::bind(&control(), d.clone(), Arc::from(&b"xyz"[..]), 1),
        Err(ArtifactError::HashMismatch)
    ));
    assert!(matches!(
        ArtifactSender::bind(&control(), d, Arc::from(&b"short"[..]), 1),
        Err(ArtifactError::DescriptorMismatch)
    ));
}
