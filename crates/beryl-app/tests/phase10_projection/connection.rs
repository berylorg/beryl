use super::*;

#[test]
fn explicit_connection_invalidation_bulk_revokes_every_held_projection() {
    let mut fixture = Fixture::new(9);
    fixture.submit_text("first pending");
    let second_thread = fixture.create_ordinary_pending(10, "second pending");
    let server = FakeAppServer::spawn(vec![
        ProjectionStep::Fresh {
            target: "phase10-first-loaded-target",
        },
        ProjectionStep::Fresh {
            target: "phase10-second-loaded-target",
        },
    ]);
    let mut session = server.admit(execution_binding().runtime_id(), process(10));
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();

    let first = coordinator
        .obtain_projection(
            &fixture.store,
            fixture.storage,
            &mut session,
            &request(&fixture, fixture.thread),
            &ProjectionCancellationToken::new(),
        )
        .unwrap();
    let second = coordinator
        .obtain_projection(
            &fixture.store,
            fixture.storage,
            &mut session,
            &request(&fixture, second_thread),
            &ProjectionCancellationToken::new(),
        )
        .unwrap();
    assert!(first.is_live().unwrap());
    assert!(second.is_live().unwrap());

    session.invalidate_connection();

    assert!(!first.is_live().unwrap());
    assert!(!second.is_live().unwrap());
    server.join();
}

#[test]
fn explicit_release_preserves_every_unsubscribe_status_as_non_authorizing() {
    let cases = [
        (
            11,
            "phase10-release-unsubscribed",
            "unsubscribed",
            ThreadUnsubscribeStatus::Unsubscribed,
        ),
        (
            12,
            "phase10-release-not-loaded",
            "notLoaded",
            ThreadUnsubscribeStatus::NotLoaded,
        ),
        (
            13,
            "phase10-release-not-subscribed",
            "notSubscribed",
            ThreadUnsubscribeStatus::NotSubscribed,
        ),
    ];

    for (seed, target, wire_status, expected) in cases {
        let mut fixture = Fixture::new(seed);
        fixture.submit_text("release pending");
        let server = FakeAppServer::spawn(vec![
            ProjectionStep::Fresh { target },
            ProjectionStep::Unsubscribe {
                target,
                reply: UnsubscribeReply::Status(wire_status),
            },
        ]);
        let mut session = server.admit(execution_binding().runtime_id(), process(seed.into()));
        let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
        let projection = coordinator
            .obtain_projection(
                &fixture.store,
                fixture.storage,
                &mut session,
                &request(&fixture, fixture.thread),
                &ProjectionCancellationToken::new(),
            )
            .unwrap();

        assert_eq!(
            projection.release().unwrap(),
            LoadedProjectionReleaseOutcome::Unsubscribe(expected)
        );
        server.join();
    }
}

#[test]
fn unsubscribe_rpc_failure_does_not_restore_revoked_local_authority() {
    let mut fixture = Fixture::new(15);
    fixture.submit_text("unsubscribe failure pending");
    let projection_request = request(&fixture, fixture.thread);
    let target = "phase10-unsubscribe-rejected";
    let server = FakeAppServer::spawn(vec![
        ProjectionStep::Fresh { target },
        ProjectionStep::Unsubscribe {
            target,
            reply: UnsubscribeReply::Reject,
        },
        ProjectionStep::Resume {
            source: target.to_string(),
        },
    ]);
    let mut session = server.admit(execution_binding().runtime_id(), process(15));
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let projection = coordinator
        .obtain_projection(
            &fixture.store,
            fixture.storage,
            &mut session,
            &projection_request,
            &ProjectionCancellationToken::new(),
        )
        .unwrap();
    let released_generation = projection.loaded_session_generation();

    let error = projection.release().unwrap_err();
    assert!(matches!(
        error,
        beryl_app::cas_projection::LoadedProjectionReleaseError::Backend(_)
    ));

    let reloaded = coordinator
        .obtain_projection(
            &fixture.store,
            fixture.storage,
            &mut session,
            &projection_request,
            &ProjectionCancellationToken::new(),
        )
        .unwrap();
    assert_eq!(reloaded.cas_thread_id().as_str(), target);
    assert_ne!(reloaded.loaded_session_generation(), released_generation);
    assert!(reloaded.is_live().unwrap());
    server.join();
}

#[test]
fn unsubscribe_transport_loss_retires_connection_and_other_leases() {
    let mut fixture = Fixture::new(16);
    fixture.submit_text("disconnect first pending");
    let second_thread = fixture.create_ordinary_pending(17, "disconnect second pending");
    let first_target = "phase10-unsubscribe-disconnect-first";
    let second_target = "phase10-unsubscribe-disconnect-second";
    let server = FakeAppServer::spawn(vec![
        ProjectionStep::Fresh {
            target: first_target,
        },
        ProjectionStep::Fresh {
            target: second_target,
        },
        ProjectionStep::Unsubscribe {
            target: first_target,
            reply: UnsubscribeReply::Disconnect,
        },
    ]);
    let mut session = server.admit(execution_binding().runtime_id(), process(16));
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let first = coordinator
        .obtain_projection(
            &fixture.store,
            fixture.storage,
            &mut session,
            &request(&fixture, fixture.thread),
            &ProjectionCancellationToken::new(),
        )
        .unwrap();
    let second = coordinator
        .obtain_projection(
            &fixture.store,
            fixture.storage,
            &mut session,
            &request(&fixture, second_thread),
            &ProjectionCancellationToken::new(),
        )
        .unwrap();

    let error = first.release().unwrap_err();
    assert!(matches!(
        error,
        beryl_app::cas_projection::LoadedProjectionReleaseError::Backend(_)
    ));
    assert!(!second.is_live().unwrap());
    server.join();
}

#[test]
fn recovered_current_projection_rejects_another_exact_connection() {
    let mut fixture = Fixture::new(14);
    let first = fixture.submit_text("history user");
    fixture.complete_with_assistant(first, "history assistant");
    fixture.submit_text("connection-owned pending");
    fixture.retire_current_binding(fixture.thread);
    let projection_request = request(&fixture, fixture.thread);
    let first_server = FakeAppServer::spawn(vec![ProjectionStep::Recover {
        target: "phase10-connection-owned-target",
        injection: InjectionReply::Success,
    }]);
    let mut first_session = first_server.admit(execution_binding().runtime_id(), process(14));
    let first_coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let first_projection = first_coordinator
        .obtain_projection(
            &fixture.store,
            fixture.storage,
            &mut first_session,
            &projection_request,
            &ProjectionCancellationToken::new(),
        )
        .unwrap();
    let second_server = FakeAppServer::spawn(Vec::new());
    let mut second_session = second_server.admit(execution_binding().runtime_id(), process(14));
    let second_coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let error = second_coordinator
        .obtain_projection(
            &fixture.store,
            fixture.storage,
            &mut second_session,
            &projection_request,
            &ProjectionCancellationToken::new(),
        )
        .unwrap_err();
    assert!(matches!(
        error,
        ProjectionExecutionError::LoadedProjectionConnectionMismatch { ref thread_id }
            if thread_id.as_str() == "phase10-connection-owned-target"
    ));
    assert!(first_projection.is_live().unwrap());

    first_session.invalidate_connection();
    assert!(!first_projection.is_live().unwrap());
    first_server.join();
    second_server.join();
}

#[test]
fn recovered_source_and_fork_child_leases_coexist_on_one_connection() {
    let mut fixture = Fixture::new(18);
    let first = fixture.submit_text("history user");
    fixture.complete_with_assistant(first, "history assistant");
    let parent_pending = fixture.submit_text("parent pending");
    fixture.retire_current_binding(fixture.thread);
    let parent_target = "phase10-recovered-fork-source";
    let child_target = "phase10-recovered-fork-child";
    let server = FakeAppServer::spawn(vec![
        ProjectionStep::Recover {
            target: parent_target,
            injection: InjectionReply::Success,
        },
        ProjectionStep::Fork {
            source: parent_target.to_string(),
            through_turn: Some(format!("phase10-turn-{}", parent_pending.turn)),
            target: child_target,
        },
    ]);
    let mut session = server.admit(execution_binding().runtime_id(), process(18));
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let parent = coordinator
        .obtain_projection(
            &fixture.store,
            fixture.storage,
            &mut session,
            &request(&fixture, fixture.thread),
            &ProjectionCancellationToken::new(),
        )
        .unwrap();
    let CasLineageProof::RecoveredInjection(parent_proof) = parent.lineage_proof() else {
        panic!("parent projection must retain recovered-injection provenance")
    };
    fixture.advance_clock_to(
        parent_proof
            .completed_at()
            .unix_millis()
            .checked_add(1)
            .unwrap(),
    );
    fixture.complete_with_assistant(parent_pending, "parent answer");
    let child_thread = fixture.create_child_pending("child pending");
    let child = coordinator
        .obtain_projection(
            &fixture.store,
            fixture.storage,
            &mut session,
            &request(&fixture, child_thread),
            &ProjectionCancellationToken::new(),
        )
        .unwrap();

    assert_eq!(parent.cas_thread_id().as_str(), parent_target);
    assert_eq!(child.cas_thread_id().as_str(), child_target);
    assert!(parent.is_live().unwrap());
    assert!(child.is_live().unwrap());
    session.invalidate_connection();
    assert!(!parent.is_live().unwrap());
    assert!(!child.is_live().unwrap());
    server.join();
}

#[test]
fn recovered_source_fork_rejects_another_exact_connection() {
    let mut fixture = Fixture::new(19);
    let first = fixture.submit_text("history user");
    fixture.complete_with_assistant(first, "history assistant");
    let parent_pending = fixture.submit_text("parent pending");
    fixture.retire_current_binding(fixture.thread);
    let parent_target = "phase10-wrong-connection-fork-source";
    let first_server = FakeAppServer::spawn(vec![ProjectionStep::Recover {
        target: parent_target,
        injection: InjectionReply::Success,
    }]);
    let mut first_session = first_server.admit(execution_binding().runtime_id(), process(19));
    let first_coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let parent = first_coordinator
        .obtain_projection(
            &fixture.store,
            fixture.storage,
            &mut first_session,
            &request(&fixture, fixture.thread),
            &ProjectionCancellationToken::new(),
        )
        .unwrap();
    let CasLineageProof::RecoveredInjection(parent_proof) = parent.lineage_proof() else {
        panic!("parent projection must retain recovered-injection provenance")
    };
    fixture.advance_clock_to(
        parent_proof
            .completed_at()
            .unix_millis()
            .checked_add(1)
            .unwrap(),
    );
    fixture.complete_with_assistant(parent_pending, "parent answer");
    let child_thread = fixture.create_child_pending("child pending");

    let second_server = FakeAppServer::spawn(Vec::new());
    let mut second_session = second_server.admit(execution_binding().runtime_id(), process(19));
    let second_coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let error = second_coordinator
        .obtain_projection(
            &fixture.store,
            fixture.storage,
            &mut second_session,
            &request(&fixture, child_thread),
            &ProjectionCancellationToken::new(),
        )
        .unwrap_err();
    assert!(matches!(
        error,
        ProjectionExecutionError::LoadedProjectionConnectionMismatch { ref thread_id }
            if thread_id.as_str() == parent_target
    ));
    assert!(parent.is_live().unwrap());

    first_session.invalidate_connection();
    assert!(!parent.is_live().unwrap());
    first_server.join();
    second_server.join();
}
