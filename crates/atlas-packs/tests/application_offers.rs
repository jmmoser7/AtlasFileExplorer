use atlas_connector::{ConnectorDescriptor, ProtocolVersion, PROTOCOL_FAMILY, V0_1};
use atlas_packs::{Catalog, ContractId, PackHealth, PackId, PackManifest, ProbeSpec};

fn program(id: &str) -> PackManifest {
    PackManifest {
        id: PackId::new(id),
        title: id.into(),
        contract: ContractId::Program,
        contract_version: 1,
        install_note: None,
        probe: ProbeSpec::Custom {
            id: "test-install".into(),
        },
        retired: false,
        credential: None,
        application: Some(ConnectorDescriptor {
            protocol: PROTOCOL_FAMILY.into(),
            versions: vec![V0_1],
            registration: id.into(),
            facets: vec!["Spatial3D".into()],
            extensions: vec![".3dm".into()],
            capabilities: vec![],
        }),
    }
}

#[test]
fn old_manifests_need_no_connector_and_round_trip_without_new_fields() {
    let fixture: PackManifest = serde_json::from_str(include_str!(
        "../../../docs/application-connectors/v0.1/mock-pack.json"
    ))
    .unwrap();
    fixture.application.as_ref().unwrap().validate().unwrap();
    for manifest in atlas_packs::shipped_manifests()
        .into_iter()
        .filter(|m| m.id.as_str() != "org.slate.rhino-inside")
    {
        assert!(manifest.application.is_none());
        let value = serde_json::to_value(&manifest).unwrap();
        assert!(value.get("application").is_none());
        assert_eq!(
            serde_json::from_value::<PackManifest>(value).unwrap(),
            manifest
        );
    }
}

#[test]
fn installation_and_protocol_compatibility_are_distinct_gates() {
    let good = program("org.example.editor");
    let mut old = program("org.example.old-editor");
    old.application.as_mut().unwrap().versions = vec![ProtocolVersion { major: 9, minor: 0 }];
    let missing = program("org.example.missing-editor");
    let mut retired = program("org.example.retired-editor");
    retired.retired = true;
    let mut chat = program("org.example.chat");
    chat.contract = ContractId::Chat;
    let mut catalog = Catalog::from_manifests(vec![good, old, missing, chat, retired]);
    for id in [
        "org.example.editor",
        "org.example.old-editor",
        "org.example.chat",
        "org.example.retired-editor",
    ] {
        catalog.set_health(&PackId::new(id), PackHealth::Ok);
    }
    let offers = catalog.application_offers();
    assert_eq!(offers.len(), 1);
    assert_eq!(offers[0].id.as_str(), "org.example.editor");
    assert!(offers[0].supports_extension("3DM"));
    assert!(!offers[0].supports_extension("xlsx"));
    assert_eq!(
        catalog.health(&PackId::new("org.example.old-editor")),
        PackHealth::Ok
    );
}

#[test]
fn malformed_registration_is_not_an_execution_offer() {
    let mut malformed = program("org.example.bad");
    malformed.application.as_mut().unwrap().registration = String::new();
    let mut catalog = Catalog::from_manifests(vec![malformed]);
    catalog.set_health(&PackId::new("org.example.bad"), PackHealth::Ok);
    assert!(catalog.application_offers().is_empty());
}

#[test]
fn a_program_without_a_connector_remains_a_legacy_program() {
    let mut legacy = program("legacy");
    legacy.application = None;
    let mut catalog = Catalog::from_manifests(vec![legacy]);
    catalog.set_health(&PackId::new("legacy"), PackHealth::Ok);
    assert_eq!(
        catalog.ready(ContractId::Program),
        vec![PackId::new("legacy")]
    );
    assert!(catalog.application_offers().is_empty());
}

#[test]
fn rhino_pack_is_optional_and_advertises_the_generic_protocol() {
    let manifest = atlas_packs::shipped_manifests()
        .into_iter()
        .find(|m| m.id.as_str() == "org.slate.rhino-inside")
        .unwrap();
    let descriptor = manifest.application.as_ref().unwrap();
    descriptor.validate().unwrap();
    assert!(descriptor.supports_version(atlas_connector::V0_2));
    let value = serde_json::to_value(&manifest).unwrap();
    assert_eq!(
        serde_json::from_value::<PackManifest>(value).unwrap(),
        manifest
    );
    let catalog = Catalog::from_manifests(vec![manifest]);
    assert!(
        catalog.application_offers().is_empty(),
        "unprobed installations do not become launch offers"
    );
}
