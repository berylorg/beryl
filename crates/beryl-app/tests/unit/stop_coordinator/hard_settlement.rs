#[test]
fn terminal_or_consumed_hard_slot_cannot_be_recreated() {
    let terminal = StopFixture::new(92);
    let mut terminal_primary = match terminal.coordinator.coordinate(
        &terminal.router,
        terminal.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        _ => panic!("first terminal fixture stop must own primary dispatch"),
    };
    let terminal_operation = terminal_primary.operation_id();
    terminal
        .coordinator
        .terminal_consumed(terminal.thread, terminal.turn);
    terminal_primary.settled = true;
    terminal_primary.permit.take().unwrap().finish();
    drop(terminal_primary);
    assert!(matches!(
        terminal.coordinator.attach_hard_stop(terminal_operation),
        Err(StopCoordinationError::TargetUnavailable)
    ));

    let consumed = StopFixture::new(93);
    let consumed_primary = match consumed.coordinator.coordinate(
        &consumed.router,
        consumed.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        _ => panic!("first consumed fixture stop must own primary dispatch"),
    };
    let consumed_operation = consumed_primary.operation_id();
    let (attachment, initial_run) = attach_hard(&consumed, consumed_operation);
    assert!(initial_run.is_none());
    consumed.coordinator.consume_hard_slot(consumed_operation);
    assert!(matches!(
        consumed.coordinator.attach_hard_stop(consumed_operation),
        Err(StopCoordinationError::TargetUnavailable)
    ));
    let run = match consumed_primary.settle_before_dispatch().unwrap() {
        StopDispatchSettlement::HardStop(run) => run,
        _ => panic!("the admitted attachment must retain its sole run"),
    };
    assert!(matches!(
        run.finish(None).unwrap(),
        StopDispatchSettlement::SafelyReopened(reopened) if reopened == consumed_operation
    ));
    assert_eq!(
        attachment.wait().unwrap().operation_id(),
        consumed_operation
    );
    assert!(matches!(
        consumed.coordinator.attach_hard_stop(consumed_operation),
        Err(StopCoordinationError::TargetUnavailable)
    ));
}

#[test]
fn proven_nondispatch_and_hard_attachment_have_closed_serialized_orders() {
    let attach_first = StopFixture::new(94);
    let primary = match attach_first.coordinator.coordinate(
        &attach_first.router,
        attach_first.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        _ => panic!("first attach-first stop must own primary dispatch"),
    };
    let operation_id = primary.operation_id();
    let (attachment, premature_run) = attach_hard(&attach_first, operation_id);
    assert!(premature_run.is_none());
    let run = match primary.settle_before_dispatch().unwrap() {
        StopDispatchSettlement::HardStop(run) => run,
        _ => panic!("attachment must linearize before nondispatch settlement"),
    };
    assert!(matches!(
        run.finish(None).unwrap(),
        StopDispatchSettlement::SafelyReopened(reopened) if reopened == operation_id
    ));
    assert_eq!(attachment.wait().unwrap().operation_id(), operation_id);
    assert!(matches!(
        attach_first.coordinator.attach_hard_stop(operation_id),
        Err(StopCoordinationError::TargetUnavailable)
    ));

    let settlement_first = StopFixture::new(95);
    let primary = match settlement_first.coordinator.coordinate(
        &settlement_first.router,
        settlement_first.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        _ => panic!("first settlement-first stop must own primary dispatch"),
    };
    let operation_id = primary.operation_id();
    assert!(matches!(
        primary.settle_before_dispatch().unwrap(),
        StopDispatchSettlement::SafelyReopened(reopened) if reopened == operation_id
    ));
    assert!(matches!(
        settlement_first.coordinator.attach_hard_stop(operation_id),
        Err(StopCoordinationError::TargetUnavailable)
    ));
    assert!(
        !settlement_first
            .coordinator
            .state
            .lock()
            .unwrap()
            .hard
            .slots
            .contains_key(&operation_id),
        "settlement-first must not manufacture an unowned waiter slot"
    );
}

#[test]
fn duplicate_hard_callers_join_one_frozen_result_and_safe_reopen_waits_for_finish() {
    let fixture = StopFixture::new(81);
    fixture
        .coordinator
        .record_published_activity(published_activity(
            &fixture,
            1,
            PublishedHardStopActivityKind::ChildOrSubagent,
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
    let operation_id = owner.operation_id;
    let (first, first_late_run) = attach_hard(&fixture, operation_id);
    let (second, second_late_run) = attach_hard(&fixture, operation_id);
    assert!(first_late_run.is_none());
    assert!(second_late_run.is_none());

    fixture
        .coordinator
        .record_published_activity(published_activity(
            &fixture,
            2,
            PublishedHardStopActivityKind::ChildOrSubagent,
            PublishedHardStopActivityLifecycle::Active,
        ));
    let run = match owner.settle_before_dispatch().unwrap() {
        StopDispatchSettlement::HardStop(run) => run,
        _ => panic!("attached hard stop must reserve its run"),
    };
    assert_eq!(run.target(), None);
    assert!(matches!(
        fixture
            .storage
            .stop_admission_read(&fixture.home, fixture.thread, point_limit())
            .unwrap(),
        StopAdmissionRead::Stopping(_)
    ));
    assert!(matches!(
        run.finish(None).unwrap(),
        StopDispatchSettlement::SafelyReopened(reopened) if reopened == operation_id
    ));

    let first_result = first.wait().unwrap();
    let second_result = second.wait().unwrap();
    assert_eq!(first_result, second_result);
    assert!(first_result.targets().is_empty());
    assert_eq!(
        first_result.limitations()[0].limitation(),
        beryl_backend::ExactHardStopLimitation::ChildOrSubagentInterruptionUnsupported
    );
    assert_eq!(first_result.limitations()[0].omitted_active(), 1);
    assert!(matches!(
        fixture.coordinator.attach_hard_stop(operation_id),
        Err(StopCoordinationError::TargetUnavailable)
    ));
}

#[test]
fn pinned_command_snapshot_admits_only_one_coarse_cleanup_and_drop_settles_waiters() {
    let fixture = StopFixture::new(82);
    for seed in 0..=64 {
        fixture
            .coordinator
            .record_published_activity(published_activity(
                &fixture,
                seed,
                PublishedHardStopActivityKind::Command,
                PublishedHardStopActivityLifecycle::Active,
            ));
    }
    let owner = match fixture.coordinator.coordinate(
        &fixture.router,
        fixture.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        _ => panic!("first hard stop must own primary dispatch"),
    };
    let operation_id = owner.operation_id;
    let (attachment, late_run) = attach_hard(&fixture, operation_id);
    assert!(late_run.is_none());
    let run = match owner.settle_before_dispatch().unwrap() {
        StopDispatchSettlement::HardStop(run) => run,
        _ => panic!("attached hard stop must reserve its run"),
    };
    assert_eq!(run.target(), Some(HardStopTargetKind::CoarseThreadCleanup));
    assert!(matches!(
        run.finish_unavailable_without_dispatch().unwrap(),
        StopDispatchSettlement::SafelyReopened(reopened) if reopened == operation_id
    ));

    let result = attachment.wait().unwrap();
    assert_eq!(result.targets().len(), 1);
    assert_eq!(
        result.targets()[0].disposition(),
        HardStopTargetDisposition::UnavailableWithoutDispatch
    );
    assert_eq!(result.limitations()[1].omitted_active(), 65);
    assert!(!result.limitations()[1].count_overflowed());
    assert!(matches!(
        fixture
            .storage
            .stop_admission_read(&fixture.home, fixture.thread, point_limit())
            .unwrap(),
        StopAdmissionRead::Admissible(_)
    ));
}

#[test]
fn unavailable_cleanup_does_not_abandon_an_accepted_primary_stop() {
    let fixture = StopFixture::new(89);
    fixture
        .coordinator
        .record_published_activity(published_activity(
            &fixture,
            1,
            PublishedHardStopActivityKind::Command,
            PublishedHardStopActivityLifecycle::Active,
        ));
    let mut owner = match fixture.coordinator.coordinate(
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
    owner.begin_dispatch().unwrap();
    let run = fixture
        .coordinator
        .begin_hard_run(
            operation_id,
            &owner.target,
            owner.attempt,
            true,
            owner.timeout,
        )
        .unwrap()
        .unwrap();
    owner.settled = true;
    owner.permit.take().unwrap().finish();
    drop(owner);

    assert!(matches!(
        run.finish_unavailable_without_dispatch().unwrap(),
        StopDispatchSettlement::Stopping(stopping) if stopping == operation_id
    ));
    let result = attachment.wait().unwrap();
    assert_eq!(
        result.targets()[0].disposition(),
        HardStopTargetDisposition::UnavailableWithoutDispatch
    );
    assert!(matches!(
        fixture
            .storage
            .stop_admission_read(&fixture.home, fixture.thread, point_limit())
            .unwrap(),
        StopAdmissionRead::Stopping(live) if live.operation_id() == operation_id
    ));
}

#[test]
fn terminal_consumption_holds_only_finalization_release_until_hard_run_finishes() {
    let fixture = StopFixture::new(83);
    let owner = match fixture.coordinator.coordinate(
        &fixture.router,
        fixture.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        _ => panic!("first hard stop must own primary dispatch"),
    };
    let operation_id = owner.operation_id;
    let (attachment, late_run) = attach_hard(&fixture, operation_id);
    assert!(late_run.is_none());
    let run = match owner.settle_before_dispatch().unwrap() {
        StopDispatchSettlement::HardStop(run) => run,
        _ => panic!("attached hard stop must reserve its run"),
    };
    fixture
        .coordinator
        .terminal_consumed(fixture.thread, fixture.turn);
    assert!(matches!(
        fixture.coordinator.attach_hard_stop(operation_id),
        Err(StopCoordinationError::TargetUnavailable)
    ));

    let coordinator = Arc::clone(&fixture.coordinator);
    let thread_id = fixture.thread;
    let turn_id = fixture.turn;
    let (released, receiver) = mpsc::channel();
    let waiter = std::thread::spawn(move || {
        coordinator
            .wait_for_finalization_release(thread_id, turn_id)
            .unwrap();
        released.send(()).unwrap();
    });
    assert!(receiver.recv_timeout(Duration::from_millis(25)).is_err());
    assert!(matches!(
        run.finish(None).unwrap(),
        StopDispatchSettlement::Stopping(stopping) if stopping == operation_id
    ));
    receiver.recv_timeout(Duration::from_secs(1)).unwrap();
    waiter.join().unwrap();
    assert_eq!(attachment.wait().unwrap().operation_id(), operation_id);
}

#[test]
fn consumed_finished_slot_without_waiters_is_removed_immediately() {
    let fixture = StopFixture::new(86);
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
    drop(attachment);
    let run = match owner.settle_before_dispatch().unwrap() {
        StopDispatchSettlement::HardStop(run) => run,
        _ => panic!("attached hard stop must reserve its run"),
    };
    fixture
        .coordinator
        .terminal_consumed(fixture.thread, fixture.turn);

    assert!(matches!(
        run.finish(None).unwrap(),
        StopDispatchSettlement::Stopping(stopping) if stopping == operation_id
    ));
    assert!(
        !fixture
            .coordinator
            .state
            .lock()
            .unwrap()
            .hard
            .slots
            .contains_key(&operation_id)
    );

    let fixture = StopFixture::new(88);
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
    drop(attachment);
    fixture.coordinator.consume_hard_slot(operation_id);
    fixture
        .coordinator
        .finish_hard_without_run(operation_id)
        .unwrap();
    assert!(
        !fixture
            .coordinator
            .state
            .lock()
            .unwrap()
            .hard
            .slots
            .contains_key(&operation_id)
    );
    drop(owner);
}

#[test]
fn durable_settlement_error_still_finishes_result_and_releases_hold() {
    let fixture = StopFixture::new(87);
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
    fixture.coordinator.remove_local(operation_id);
    fixture.coordinator.consume_hard_slot(operation_id);

    let coordinator = Arc::clone(&fixture.coordinator);
    let thread_id = fixture.thread;
    let turn_id = fixture.turn;
    let (released, receiver) = mpsc::channel();
    let waiter = std::thread::spawn(move || {
        coordinator
            .wait_for_finalization_release(thread_id, turn_id)
            .unwrap();
        released.send(()).unwrap();
    });
    assert!(receiver.recv_timeout(Duration::from_millis(25)).is_err());

    assert!(matches!(
        run.finish(None),
        Err(StopCoordinationError::LocalAuthorityMismatch)
    ));
    receiver.recv_timeout(Duration::from_secs(1)).unwrap();
    waiter.join().unwrap();
    assert_eq!(attachment.wait().unwrap().operation_id(), operation_id);
    assert!(
        !fixture
            .coordinator
            .state
            .lock()
            .unwrap()
            .hard
            .slots
            .contains_key(&operation_id)
    );
}
