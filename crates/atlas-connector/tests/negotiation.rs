mod common;
use atlas_connector::*;
use common::*;

#[test]
fn exact_versions_and_required_groups_are_enforced() {
    let mut hello = MockConnector::new(identity("alpha")).hello();
    hello.versions = vec![ProtocolVersion { major: 0, minor: 2 }];
    expect_error(
        Session::negotiate(&HostPolicy::default(), &hello, "s".into(), 1),
        ErrorCode::IncompatibleProtocol,
    );
    hello.versions.push(V0_1);
    hello.capabilities.offered.push("future_optional".into());
    let (_, welcome) = Session::negotiate(&HostPolicy::default(), &hello, "s".into(), 1).unwrap();
    assert!(!welcome.capabilities.contains(&CapabilityGroup::Capture));
    hello.capabilities.required.push("future_optional".into());
    expect_error(
        Session::negotiate(&HostPolicy::default(), &hello, "s".into(), 1),
        ErrorCode::RequiredCapabilityMissing,
    );
}

#[test]
fn optional_unknown_fields_survive_without_becoming_required_capabilities() {
    let mut value = serde_json::to_value(MockConnector::new(identity("alpha")).hello()).unwrap();
    value["future_metadata"] = serde_json::json!({"anything": "data"});
    let hello: Hello = serde_json::from_value(value).unwrap();
    assert!(Session::negotiate(&HostPolicy::default(), &hello, "s".into(), 1).is_ok());
    let invalid = serde_json::json!({"method":"evaluate_script", "script":"anything"});
    assert!(serde_json::from_value::<Operation>(invalid).is_err());
}

#[test]
fn surface_only_and_command_only_capabilities_do_not_imply_other_access() {
    let mut hello = MockConnector::new(identity("alpha")).hello();
    hello.capabilities.offered = vec!["lifecycle".into(), "sources".into(), "surfaces".into()];
    let (mut session, welcome) =
        Session::negotiate(&HostPolicy::default(), &hello, "test-session".into(), 1).unwrap();
    assert_eq!(
        welcome.capabilities,
        vec![
            CapabilityGroup::Lifecycle,
            CapabilityGroup::Sources,
            CapabilityGroup::Surfaces
        ]
    );
    expect_error(
        begin(
            &mut session,
            request("catalog", Operation::DescribeCommands, None),
        ),
        ErrorCode::UnsupportedOperation,
    );
    hello.capabilities.offered = vec!["lifecycle".into(), "sources".into(), "commands".into()];
    hello.surface_modes.clear();
    assert!(Session::negotiate(&HostPolicy::default(), &hello, "s".into(), 1).is_ok());
}

#[test]
fn two_application_identities_use_the_same_public_adapter() {
    for vendor in ["alpha", "beta"] {
        let mut mock = MockConnector::new(identity(vendor));
        let welcome = mock
            .negotiate(&HostPolicy::default(), "test-session".into(), 1)
            .unwrap();
        let binding = request(
            "bind",
            Operation::BindSource {
                source: source(),
                mode: SourceMode::Linked,
                selection_id: "selection".into(),
            },
            None,
        );
        let reply = mock.handle(binding.clone(), &binding.caller);
        assert!(matches!(reply.outcome, ReplyBody::SourceBound { .. }));
        let req = inspect("inspect");
        let reply = mock.handle(req.clone(), &req.caller);
        let ReplyBody::CommandResult { value, .. } = reply.outcome else {
            panic!("expected inspection")
        };
        assert_eq!(value["application_id"], welcome.identity.application_id);
    }
}

#[test]
fn installation_descriptor_is_offer_only_and_registration_cannot_launch_code() {
    let mut descriptor = ConnectorDescriptor {
        protocol: PROTOCOL_FAMILY.into(),
        versions: vec![V0_1],
        registration: "org.example.editor".into(),
        facets: vec!["3d".into()],
        extensions: vec![".dat".into()],
        capabilities: vec![CapabilityGroup::Surfaces],
    };
    assert!(descriptor.validate().is_ok());
    assert!(descriptor.supports_version(V0_1));
    descriptor.registration = "C:\\tools\\editor.exe --eval".into();
    expect_error(descriptor.validate(), ErrorCode::InvalidDescriptor);
    assert!(!descriptor.supports_version(V0_1));
}

#[test]
fn negotiated_session_rejects_unbounded_or_inconsistent_descriptors() {
    let mut hello = MockConnector::new(identity("alpha")).hello();
    hello.commands.push(hello.commands[0].clone());
    expect_error(
        Session::negotiate(&HostPolicy::default(), &hello, "s".into(), 1),
        ErrorCode::InvalidDescriptor,
    );
    hello.commands.pop();
    hello.surface_modes.clear();
    expect_error(
        Session::negotiate(&HostPolicy::default(), &hello, "s".into(), 1),
        ErrorCode::InvalidDescriptor,
    );
    hello.capabilities.offered.retain(|s| s != "surfaces");
    hello.view_schemas = vec![ViewSchema {
        namespace: "not_namespaced".into(),
        version: 1,
    }];
    expect_error(
        Session::negotiate(&HostPolicy::default(), &hello, "s".into(), 1),
        ErrorCode::UnsupportedSchema,
    );
}

#[test]
fn source_scoped_groups_explicitly_require_source_binding_capability() {
    for group in ["surfaces", "commands", "views", "capture", "publication"] {
        let mut hello = MockConnector::new(identity("alpha")).hello();
        hello.capabilities.offered = vec!["lifecycle".into(), group.into()];
        expect_error(
            Session::negotiate(&HostPolicy::default(), &hello, "s".into(), 1),
            ErrorCode::RequiredCapabilityMissing,
        );
    }
}

#[test]
fn schemas_and_fixtures_come_from_serializable_public_types() {
    let schemas = schema_documents();
    assert_eq!(schemas.len(), 10);
    assert!(schemas["wire-frame.schema.json"]["$defs"].is_object());
    for (_, frame) in fixture_frames() {
        let mut bytes = Vec::new();
        write_frame(&mut bytes, &frame).unwrap();
        assert_eq!(
            read_frame::<WireFrame>(&mut bytes.as_slice()).unwrap(),
            Some(frame)
        );
    }
}
