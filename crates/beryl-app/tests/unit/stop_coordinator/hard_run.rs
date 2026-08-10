#[test]
fn provider_activity_publication_does_not_wait_for_stop_coordination_state() {
    let fixture = StopFixture::new(80);
    let effect = published_activity(
        &fixture,
        1,
        PublishedHardStopActivityKind::Command,
        PublishedHardStopActivityLifecycle::Active,
    );
    let state = fixture.coordinator.state.lock().unwrap();
    let coordinator = Arc::clone(&fixture.coordinator);
    let (published, receiver) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        coordinator.record_published_activity(effect);
        published.send(()).unwrap();
    });

    receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("provider activity must not acquire the stop-coordination mutex");
    drop(state);
    worker.join().unwrap();

    let owner = match fixture.coordinator.coordinate(
        &fixture.router,
        fixture.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        _ => panic!("first hard stop must own primary dispatch"),
    };
    let (attachment, late_run) = attach_hard(&fixture, owner.operation_id());
    assert!(late_run.is_none());
    let run = match owner.settle_before_dispatch().unwrap() {
        StopDispatchSettlement::HardStop(run) => run,
        _ => panic!("attached hard stop must reserve its run"),
    };
    assert_eq!(run.target(), Some(HardStopTargetKind::CoarseThreadCleanup));
    run.finish_unavailable_without_dispatch().unwrap();
    assert_eq!(
        attachment.wait().unwrap().limitations()[1].omitted_active(),
        1
    );
}

#[test]
fn dropping_hard_stop_run_owner_preserves_durable_stop_without_home_io() {
    let fixture = StopFixture::new(81);
    fixture
        .coordinator
        .record_published_activity(published_activity(
            &fixture,
            1,
            PublishedHardStopActivityKind::Command,
            PublishedHardStopActivityLifecycle::Active,
        ));
    let owner = match fixture.coordinator.coordinate(
        &fixture.router,
        fixture.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        _ => panic!("first hard stop must own primary dispatch"),
    };
    let operation_id = owner.operation_id();
    let (attachment, late_run) = attach_hard(&fixture, operation_id);
    assert!(late_run.is_none());
    let run = match owner.settle_before_dispatch().unwrap() {
        StopDispatchSettlement::HardStop(run) => run,
        _ => panic!("attached hard stop must reserve its run"),
    };
    let revision = fixture.home.home_revision().unwrap();

    drop(run);

    let _ = attachment.wait().unwrap();
    assert_eq!(fixture.home.home_revision().unwrap(), revision);
    let state = fixture.coordinator.state.lock().unwrap();
    let local = state.stops.get(&fixture.thread).unwrap();
    assert_eq!(local.operation_id, operation_id);
    assert_eq!(local.dispatch, LocalDispatchState::PossiblyDispatched);
    drop(state);
    assert!(matches!(
        fixture
            .storage
            .stop_admission_read(&fixture.home, fixture.thread, point_limit())
            .unwrap(),
        StopAdmissionRead::Stopping(live) if live.operation_id() == operation_id
    ));
}

#[test]
fn direct_hard_cleanup_retains_original_election_until_authorization_boundary() {
    let fixture = StopFixture::new(96);
    fixture
        .coordinator
        .record_published_activity(published_activity(
            &fixture,
            1,
            PublishedHardStopActivityKind::Command,
            PublishedHardStopActivityLifecycle::Active,
        ));
    let owner = match fixture.coordinator.coordinate(
        &fixture.router,
        fixture.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        _ => panic!("first hard stop must own primary dispatch"),
    };
    let (attachment, late_run) = attach_hard(&fixture, owner.operation_id());
    assert!(late_run.is_none());
    let mut run = match owner.settle_before_dispatch().unwrap() {
        StopDispatchSettlement::HardStop(run) => run,
        _ => panic!("attached direct hard stop must reserve its run"),
    };
    assert!(!run.requires_fresh_election());

    let (wait_sender, wait_receiver) = mpsc::sync_channel(0);
    fixture
        .router
        .observe_next_terminal_publication_wait_for_test(wait_sender);
    let (terminal_sender, terminal_receiver) = mpsc::sync_channel(0);
    let router = Arc::clone(&fixture.router);
    let cas_thread_id = fixture.target.cas_thread_id().clone();
    let cas_turn_id = fixture.target.cas_turn_id().clone();
    let terminal = std::thread::spawn(move || {
        let published = router
            .acquire_terminal_source_publication(&cas_thread_id, &cas_turn_id)
            .is_ok_and(|permit| {
                permit
                    .finish_terminal(
                        crate::cas_projection::connection::ProvenTerminalOutcome::new(
                            TurnEndStatus::complete(),
                            timestamp(96),
                        ),
                    )
                    .is_ok()
            });
        terminal_sender.send(published).unwrap();
    });

    wait_receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("terminal publication must reach the inherited stop-election wait");
    assert!(matches!(
        terminal_receiver.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    run.release_inherited_election_after_authorization()
        .unwrap();
    assert!(
        terminal_receiver
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
    );
    terminal.join().unwrap();

    fixture
        .coordinator
        .terminal_consumed(fixture.thread, fixture.turn);
    assert!(matches!(
        run.finish_unavailable_without_dispatch().unwrap(),
        StopDispatchSettlement::Stopping(_)
    ));
    assert_eq!(attachment.wait().unwrap().targets().len(), 1);
}

#[test]
fn terminal_consumption_does_not_wait_behind_coordinate_blocked_on_router_publication() {
    let fixture = StopFixture::new(97);
    let terminal_permit = fixture
        .router
        .acquire_terminal_source_publication(
            fixture.target.cas_thread_id(),
            fixture.target.cas_turn_id(),
        )
        .unwrap();
    let (wait_sender, wait_receiver) = mpsc::sync_channel(0);
    fixture
        .router
        .observe_next_stop_election_wait_for_test(wait_sender);

    let (coordinate_sender, coordinate_receiver) = mpsc::sync_channel(0);
    let coordinator = Arc::clone(&fixture.coordinator);
    let router = Arc::clone(&fixture.router);
    let proof = fixture.proof.clone();
    let coordinate = std::thread::spawn(move || {
        coordinate_sender
            .send(coordinator.coordinate(&router, proof, StopCause::SelectedOperationControl))
            .unwrap();
    });
    wait_receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("coordinate must reach the router wait behind terminal source publication");
    assert!(matches!(
        coordinate_receiver.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));

    let (consumed_sender, consumed_receiver) = mpsc::sync_channel(0);
    let coordinator = Arc::clone(&fixture.coordinator);
    let thread_id = fixture.thread;
    let turn_id = fixture.turn;
    let consumed = std::thread::spawn(move || {
        coordinator.terminal_consumed(thread_id, turn_id);
        consumed_sender.send(()).unwrap();
    });
    consumed_receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("terminal consumption must not need state retained by router election waiting");

    terminal_permit
        .finish_terminal(
            crate::cas_projection::connection::ProvenTerminalOutcome::new(
                TurnEndStatus::complete(),
                timestamp(97),
            ),
        )
        .unwrap();
    assert!(
        coordinate_receiver
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
            .is_err()
    );
    consumed.join().unwrap();
    coordinate.join().unwrap();
}

#[test]
fn exact_terminal_and_authority_loss_clear_activity_without_a_local_stop() {
    for (seed, terminal) in [(84, true), (85, false)] {
        let fixture = StopFixture::new(seed);
        fixture
            .coordinator
            .record_published_activity(published_activity(
                &fixture,
                1,
                PublishedHardStopActivityKind::Command,
                PublishedHardStopActivityLifecycle::Active,
            ));
        if terminal {
            fixture
                .coordinator
                .terminal_consumed(fixture.thread, fixture.turn);
        } else {
            assert!(
                !fixture
                    .coordinator
                    .abandon_for_authority_loss(fixture.thread, fixture.turn)
                    .unwrap()
            );
        }

        let owner = match fixture.coordinator.coordinate(
            &fixture.router,
            fixture.proof.clone(),
            StopCause::SelectedOperationControl,
        ) {
            Ok(StopOwnership::Primary(owner)) => owner,
            _ => panic!("first hard stop must own primary dispatch"),
        };
        let (attachment, late_run) = attach_hard(&fixture, owner.operation_id());
        assert!(late_run.is_none());
        let run = match owner.settle_before_dispatch().unwrap() {
            StopDispatchSettlement::HardStop(run) => run,
            _ => panic!("attached hard stop must reserve its run"),
        };
        assert_eq!(run.target(), None);
        run.finish(None).unwrap();
        assert_eq!(
            attachment.wait().unwrap().limitations()[1].omitted_active(),
            0
        );
    }
}

#[test]
fn accepted_primary_admits_one_late_hard_run_and_duplicates_join_its_frozen_result() {
    let fixture = StopFixture::new(90);
    let mut primary = match fixture.coordinator.coordinate(
        &fixture.router,
        fixture.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        _ => panic!("first stop must own primary dispatch"),
    };
    let operation_id = primary.operation_id();
    primary.begin_dispatch().unwrap();
    fixture
        .coordinator
        .mark_primary_accepted(fixture.thread, operation_id, primary.attempt)
        .unwrap();
    primary.settled = true;
    primary.permit.take().unwrap().finish();
    drop(primary);

    let (first, first_run) = attach_hard(&fixture, operation_id);
    let run = first_run.expect("first late attachment must own the hard continuation");
    let (second, second_run) = attach_hard(&fixture, operation_id);
    assert!(second_run.is_none(), "a duplicate must not own another run");
    assert_eq!(run.target(), None);
    assert!(matches!(
        run.finish(None).unwrap(),
        StopDispatchSettlement::Stopping(stopping) if stopping == operation_id
    ));

    let (third, third_run) = attach_hard(&fixture, operation_id);
    assert!(
        third_run.is_none(),
        "a duplicate of a frozen result must remain attachment-only"
    );
    let first_result = first.wait().unwrap();
    assert_eq!(first_result, second.wait().unwrap());
    assert_eq!(first_result, third.wait().unwrap());
}

#[test]
fn completion_unknown_primary_cannot_admit_a_late_hard_run() {
    let fixture = StopFixture::new(91);
    let mut primary = match fixture.coordinator.coordinate(
        &fixture.router,
        fixture.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        _ => panic!("first stop must own primary dispatch"),
    };
    let operation_id = primary.operation_id();
    primary.begin_dispatch().unwrap();
    fixture
        .coordinator
        .mark_possibly_dispatched(fixture.thread, operation_id, primary.attempt)
        .unwrap();
    primary.settled = true;
    primary.permit.take().unwrap().finish();
    drop(primary);

    assert!(matches!(
        fixture.coordinator.attach_hard_stop(operation_id),
        Err(StopCoordinationError::TargetUnavailable)
    ));
    assert_eq!(
        fixture
            .coordinator
            .state
            .lock()
            .unwrap()
            .stops
            .get(&fixture.thread)
            .unwrap()
            .dispatch,
        LocalDispatchState::PossiblyDispatched
    );
}
