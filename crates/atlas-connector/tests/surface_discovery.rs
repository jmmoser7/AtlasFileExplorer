mod common;
use atlas_connector::*;
use common::*;

fn native_surface() -> SurfaceDescriptor {
    SurfaceDescriptor {
        surface_id: "native-main".into(),
        mode: SurfaceMode::NativeFullUi,
        process_ownership: ProcessOwnership::AttachedExisting,
        native_handle: Some("12345".into()),
        can_detach: true,
    }
}

fn native_session() -> Session {
    let mut hello = MockConnector::new(identity("alpha")).hello();
    hello.surface_modes = vec![SurfaceMode::NativeFullUi];
    let (mut session, _) =
        Session::negotiate(&HostPolicy::default(), &hello, "test-session".into(), 1).unwrap();
    bind(&mut session);
    session
}

#[test]
fn discovery_is_source_scoped_and_independent_of_command_access() {
    let mut hello = MockConnector::new(identity("alpha")).hello();
    hello.capabilities.offered = vec!["lifecycle".into(), "sources".into(), "surfaces".into()];
    hello.surface_modes = vec![SurfaceMode::NativeFullUi];
    let (mut session, _) =
        Session::negotiate(&HostPolicy::default(), &hello, "test-session".into(), 1).unwrap();
    expect_error(
        begin(
            &mut session,
            request(
                "before-bind",
                Operation::DescribeSurfaces,
                Some(source().scope()),
            ),
        ),
        ErrorCode::SourceNotBound,
    );
    bind(&mut session);
    expect_error(
        begin(
            &mut session,
            request("no-command-access", Operation::DescribeCommands, None),
        ),
        ErrorCode::UnsupportedOperation,
    );
    let discover = request(
        "discover",
        Operation::DescribeSurfaces,
        Some(source().scope()),
    );
    begin(&mut session, discover).unwrap();
    let descriptor = native_surface();
    let reply = session
        .complete(
            "discover",
            ReplyBody::Surfaces {
                surfaces: vec![descriptor.clone()],
            },
        )
        .unwrap();
    let ReplyBody::Surfaces { surfaces } = reply.outcome else {
        panic!("surface list")
    };
    assert_eq!(surfaces[0].native_handle.as_deref(), Some("12345"));
    begin(
        &mut session,
        request(
            "attach",
            Operation::Surface {
                action: SurfaceAction::Attach {
                    surface: descriptor,
                },
            },
            Some(source().scope()),
        ),
    )
    .unwrap();
    session.complete("attach", ReplyBody::Ack).unwrap();
    assert_eq!(session.surface().unwrap().surface_id, "native-main");
}

#[test]
fn discovery_refuses_unnegotiated_group_wrong_source_and_stale_revision() {
    let hello = MockConnector::new(identity("alpha")).hello();
    let policy = HostPolicy {
        capabilities: vec![CapabilityGroup::Lifecycle, CapabilityGroup::Sources],
        ..HostPolicy::default()
    };
    let (mut session, _) = Session::negotiate(&policy, &hello, "test-session".into(), 1).unwrap();
    expect_error(
        begin(
            &mut session,
            request("unsupported", Operation::DescribeSurfaces, None),
        ),
        ErrorCode::UnsupportedOperation,
    );
    let mut session = native_session();
    let wrong = SourceScope {
        source_id: "other".into(),
        revision: "rev-a".into(),
    };
    expect_error(
        begin(
            &mut session,
            request("wrong-source", Operation::DescribeSurfaces, Some(wrong)),
        ),
        ErrorCode::SourceMismatch,
    );
    let stale = SourceScope {
        source_id: "source-1".into(),
        revision: "rev-b".into(),
    };
    expect_error(
        begin(
            &mut session,
            request("stale", Operation::DescribeSurfaces, Some(stale)),
        ),
        ErrorCode::StaleRevision,
    );
}

#[test]
fn discovered_surfaces_have_bounded_unique_ids_and_honest_modes_handles_and_detach() {
    let mut session = native_session();
    begin(
        &mut session,
        request(
            "discover",
            Operation::DescribeSurfaces,
            Some(source().scope()),
        ),
    )
    .unwrap();
    let descriptor = native_surface();
    let mut bad_mode = descriptor.clone();
    bad_mode.mode = SurfaceMode::NativeViewport;
    let mut no_handle = descriptor.clone();
    no_handle.native_handle = None;
    let mut zero_handle = descriptor.clone();
    zero_handle.native_handle = Some("0".into());
    let mut unbounded_handle = descriptor.clone();
    unbounded_handle.native_handle = Some("1".repeat(21));
    let mut no_recovery = descriptor.clone();
    no_recovery.can_detach = false;
    for surfaces in [
        vec![bad_mode],
        vec![no_handle],
        vec![zero_handle],
        vec![unbounded_handle],
        vec![no_recovery],
        vec![descriptor.clone(), descriptor.clone()],
        (0..=MAX_SESSION_SURFACES)
            .map(|i| {
                let mut s = descriptor.clone();
                s.surface_id = format!("surface-{i}");
                s
            })
            .collect(),
    ] {
        expect_error(
            session.complete("discover", ReplyBody::Surfaces { surfaces }),
            ErrorCode::InvalidResult,
        );
    }
    session
        .complete(
            "discover",
            ReplyBody::Surfaces {
                surfaces: vec![descriptor],
            },
        )
        .unwrap();
}

#[test]
fn headless_mock_honestly_returns_no_native_surfaces() {
    for vendor in ["alpha", "beta"] {
        let mut mock = MockConnector::new(identity(vendor));
        mock.negotiate(&HostPolicy::default(), "test-session".into(), 1)
            .unwrap();
        let bind_request = request(
            "bind",
            Operation::BindSource {
                source: source(),
                mode: SourceMode::Linked,
                selection_id: "selection".into(),
            },
            None,
        );
        let authority = bind_request.caller.clone();
        mock.handle(bind_request, &authority);
        let discover = request(
            "discover",
            Operation::DescribeSurfaces,
            Some(source().scope()),
        );
        let authority = discover.caller.clone();
        assert_eq!(
            mock.handle(discover, &authority).outcome,
            ReplyBody::Surfaces {
                surfaces: Vec::new()
            }
        );
    }
}
