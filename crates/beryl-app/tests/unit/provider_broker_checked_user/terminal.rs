#[test]
fn normal_terminal_statuses_publish_exact_durable_outcomes() {
    for (seed, provider_status, expected_outcome) in [
        (
            175,
            NormalTurnTerminalStatus::Completed,
            TurnTerminalOutcome::Complete,
        ),
        (
            176,
            NormalTurnTerminalStatus::Failed,
            TurnTerminalOutcome::Failed,
        ),
        (
            177,
            NormalTurnTerminalStatus::Interrupted,
            TurnTerminalOutcome::Interrupted,
        ),
    ] {
        let mut fixture = CheckedUserFixture::new(seed);
        let cas_item_id = CasItemId::new(format!("checked-user-item-{seed}")).unwrap();
        fixture.submit_checked(UserMessageEchoLifecycle::Started, cas_item_id.clone());
        fixture.submit_checked(UserMessageEchoLifecycle::Completed, cas_item_id);
        fixture.submit_terminal(provider_status);

        let durable = fixture
            .storage
            .turn_state(&fixture.home, fixture.turn_id, point_limit())
            .unwrap()
            .unwrap();
        assert_eq!(durable.lifecycle(), expected_outcome.lifecycle());
        assert_eq!(durable.end_status().unwrap().outcome(), expected_outcome);
        assert_eq!(durable.incomplete_reason(), None);
        assert_eq!(durable.source_event_count(), 4);
        let terminal = fixture.source_event(4);
        assert_eq!(
            terminal.source(),
            Some(&CasTurnSource::new(
                fixture.cas_thread_id.clone(),
                fixture.cas_turn_id.clone(),
            ))
        );
        assert!(matches!(
            terminal.payload(),
            SourceEventPayload::TurnEnded(status)
                if status.outcome() == expected_outcome && status.incomplete_reason().is_none()
        ));
        let proven = fixture.registration.proven_terminal().unwrap();
        assert_eq!(proven.status(), durable.end_status().unwrap());
        assert_eq!(proven.observed_at(), durable.updated_at());
        assert!(matches!(
            fixture
                .storage
                .current_binding(&fixture.home, fixture.thread_id, point_limit())
                .unwrap()
                .unwrap()
                .binding()
                .state(),
            BindingState::Valid(_)
        ));

        fixture.close();
    }
}

#[test]
fn normal_completed_terminal_preserves_outcome_when_item_audit_is_incomplete() {
    let mut fixture = CheckedUserFixture::new(180);
    fixture.submit_checked(
        UserMessageEchoLifecycle::Started,
        CasItemId::new("checked-user-item-180").unwrap(),
    );
    fixture.submit_terminal(NormalTurnTerminalStatus::Completed);

    let durable = fixture
        .storage
        .turn_state(&fixture.home, fixture.turn_id, point_limit())
        .unwrap()
        .unwrap();
    let status = durable.end_status().unwrap();
    assert_eq!(status.outcome(), TurnTerminalOutcome::Complete);
    assert_eq!(
        status.incomplete_reason(),
        Some(TurnIncompleteReason::ItemAuditFailed)
    );
    assert_eq!(durable.lifecycle(), TurnLifecycle::Complete);
    assert_eq!(durable.source_event_count(), 3);
    let terminal = fixture.source_event(3);
    assert!(matches!(
        terminal.payload(),
        SourceEventPayload::TurnEnded(event_status) if *event_status == status
    ));
    assert!(
        fixture
            .storage
            .source_event(
                &fixture.home,
                fixture.turn_id,
                SourceEventSequence::new(4).unwrap(),
                point_limit(),
            )
            .unwrap()
            .is_none()
    );
    assert_eq!(
        fixture.registration.proven_terminal().unwrap().status(),
        status
    );
    assert_eq!(
        fixture
            .storage
            .input_gate(&fixture.home, fixture.thread_id, point_limit())
            .unwrap()
            .unwrap()
            .state(),
        &InputGateState::FinalizingHistory(fixture.turn_id)
    );

    fixture.close();
}

#[test]
fn before_commit_terminal_failure_enters_verification_and_closes_without_terminal_proof() {
    let faults = FaultController::new();
    let mut fixture = CheckedUserFixture::with_faults(181, faults.clone());
    let cas_item_id = CasItemId::new("checked-user-item-181").unwrap();
    fixture.submit_checked(UserMessageEchoLifecycle::Started, cas_item_id.clone());
    fixture.submit_checked(UserMessageEchoLifecycle::Completed, cas_item_id);
    faults.fail_next_in_scope(
        FaultPoint::BeforeCommit,
        syndic_storage::test_faults::live_source_event_fault_scope(),
    );
    fixture.submit_terminal(NormalTurnTerminalStatus::Completed);

    assert_eq!(
        fixture.home.health().state(),
        beryl_home_store::HomeHealthState::Verifying
    );
    assert!(!fixture.commands.failure_observed());
    assert!(fixture.commands.is_open());
    assert_eq!(fixture.commands.active_command_count_for_test(), 0);
    assert_eq!(fixture.router_target_count(), 1);
    assert_eq!(
        fixture.registration.terminal_reason(),
        Some(
            crate::cas_projection::connection::router::LiveEventTargetCloseReason::SourcePublicationFailed
        )
    );
    assert!(fixture.registration.proven_terminal().is_none());

    fixture.close();
}

#[test]
fn persistent_activation_publication_panic_cuts_cleanup_and_drains_before_acknowledgement() {
    let faults = FaultController::new();
    let mut fixture = CheckedUserFixture::with_faults(191, faults.clone());

    faults.panic_next(FaultPoint::BeforeCommit);
    fixture.submit_checked(
        UserMessageEchoLifecycle::Started,
        CasItemId::new("checked-user-item-191").unwrap(),
    );

    assert_eq!(
        fixture.home.health().state(),
        beryl_home_store::HomeHealthState::Failed
    );
    assert!(
        fixture.commands.failure_observed(),
        "command gate state: open={}, active={}",
        fixture.commands.is_open(),
        fixture.commands.active_command_count_for_test()
    );
    assert!(!fixture.commands.is_open());
    assert_eq!(fixture.commands.active_command_count_for_test(), 0);
    assert_eq!(fixture.router_target_count(), 1);
    assert!(fixture.registration.terminal_reason().is_none());
    assert!(fixture.registration.proven_terminal().is_none());

    fixture.close();
}

#[test]
fn persistent_terminal_publication_panic_cuts_cleanup_and_drains_before_acknowledgement() {
    let faults = FaultController::new();
    let mut fixture = CheckedUserFixture::with_faults(190, faults.clone());
    let cas_item_id = CasItemId::new("checked-user-item-190").unwrap();
    fixture.submit_checked(UserMessageEchoLifecycle::Started, cas_item_id.clone());
    fixture.submit_checked(UserMessageEchoLifecycle::Completed, cas_item_id);

    faults.panic_next(FaultPoint::BeforeCommit);
    fixture.submit_terminal(NormalTurnTerminalStatus::Completed);

    assert_eq!(
        fixture.home.health().state(),
        beryl_home_store::HomeHealthState::Failed
    );
    assert!(!fixture.commands.is_open());
    assert_eq!(fixture.commands.active_command_count_for_test(), 0);
    assert_eq!(fixture.router_target_count(), 1);
    assert!(fixture.registration.terminal_reason().is_none());
    assert!(fixture.registration.proven_terminal().is_none());

    fixture.close();
}

#[test]
fn after_persist_terminal_enters_verification_and_closes_without_terminal_proof() {
    let faults = FaultController::new();
    let mut fixture = CheckedUserFixture::with_faults(182, faults.clone());
    let cas_item_id = CasItemId::new("checked-user-item-182").unwrap();
    fixture.submit_checked(UserMessageEchoLifecycle::Started, cas_item_id.clone());
    fixture.submit_checked(UserMessageEchoLifecycle::Completed, cas_item_id);

    faults.fail_next_in_scope(
        FaultPoint::AfterPersist,
        syndic_storage::test_faults::live_source_event_fault_scope(),
    );
    fixture.submit_terminal(NormalTurnTerminalStatus::Completed);

    assert_eq!(
        fixture.home.health().state(),
        beryl_home_store::HomeHealthState::Verifying
    );
    assert!(!fixture.commands.failure_observed());
    assert!(fixture.commands.is_open());
    assert_eq!(fixture.commands.active_command_count_for_test(), 0);
    assert_eq!(fixture.router_target_count(), 1);
    assert_eq!(
        fixture.registration.terminal_reason(),
        Some(
            crate::cas_projection::connection::router::LiveEventTargetCloseReason::SourcePublicationFailed
        )
    );
    assert!(fixture.registration.proven_terminal().is_none());

    fixture.close();
}

#[test]
fn target_close_waits_for_the_paused_terminal_permit_result() {
    let faults = FaultController::new();
    let mut fixture = CheckedUserFixture::with_faults(183, faults.clone());
    let cas_item_id = CasItemId::new("checked-user-item-183").unwrap();
    fixture.submit_checked(UserMessageEchoLifecycle::Started, cas_item_id.clone());
    fixture.submit_checked(UserMessageEchoLifecycle::Completed, cas_item_id);
    let binding_before = fixture
        .storage
        .current_binding(&fixture.home, fixture.thread_id, point_limit())
        .unwrap()
        .unwrap()
        .binding()
        .revision();
    let block = faults.block_next_in_scope(
        FaultPoint::BeforeCommit,
        syndic_storage::test_faults::live_source_event_fault_scope(),
    );

    let (reached, (close_returned_terminal, retained_target_count, prepublication_state)) = fixture
        .submit_terminal_while_publication_paused(
            NormalTurnTerminalStatus::Completed,
            &block,
            |fixture| {
                let close_returned_terminal = fixture.request_target_close(
                    crate::cas_projection::connection::router::LiveEventTargetCloseReason::ReceiverAbandoned,
                );
                let retained_target_count = fixture.router_target_count();
                let state = fixture
                    .storage
                    .turn_state(&fixture.home, fixture.turn_id, point_limit())
                    .unwrap()
                    .unwrap();
                (close_returned_terminal, retained_target_count, state)
            },
        );

    assert!(reached);
    assert!(!close_returned_terminal);
    assert_eq!(retained_target_count, 1);
    assert_eq!(prepublication_state.lifecycle(), TurnLifecycle::Active);
    assert_eq!(prepublication_state.source_event_count(), 3);
    assert!(prepublication_state.end_status().is_none());
    assert_eq!(fixture.router_target_count(), 0);
    assert_eq!(
        fixture.registration.terminal_reason(),
        Some(
            crate::cas_projection::connection::router::LiveEventTargetCloseReason::ReceiverAbandoned
        )
    );
    assert!(fixture.registration.proven_terminal().is_none());
    let durable = fixture
        .storage
        .turn_state(&fixture.home, fixture.turn_id, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(durable.source_event_count(), 4);
    assert_eq!(durable.end_status(), Some(TurnEndStatus::complete()));
    assert!(matches!(
        fixture.source_event(4).payload(),
        SourceEventPayload::TurnEnded(status) if *status == TurnEndStatus::complete()
    ));
    assert!(
        fixture
            .storage
            .source_event(
                &fixture.home,
                fixture.turn_id,
                SourceEventSequence::new(5).unwrap(),
                point_limit(),
            )
            .unwrap()
            .is_none()
    );
    let binding = fixture
        .storage
        .current_binding(&fixture.home, fixture.thread_id, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        binding.binding().revision(),
        binding_before.checked_next().unwrap()
    );
    assert!(matches!(binding.binding().state(), BindingState::Valid(_)));
    let broker = fixture.broker_snapshot();
    assert_eq!(broker.in_flight().current(), 0);
    assert_eq!(broker.submitted(), 3);
    assert_eq!(broker.acked(), 3);

    fixture.close();
}

#[test]
fn unmatched_terminal_identity_rejects_all_later_broker_work() {
    let mut fixture = CheckedUserFixture::new(184);
    let rejected = fixture
        .try_submit_terminal_for_route(
            NormalTurnTerminalStatus::Completed,
            beryl_model::CasThreadId::new("wrong-terminal-thread-184").unwrap(),
            fixture.cas_turn_id.clone(),
        )
        .unwrap_err();
    assert_eq!(
        rejected.cause(),
        OrderedTurnStreamSubmitCause::Rejected(OrderedTurnStreamRejection::InvalidControl)
    );
    assert!(
        fixture
            .try_submit_terminal_for_route(
                NormalTurnTerminalStatus::Completed,
                fixture.cas_thread_id.clone(),
                fixture.cas_turn_id.clone(),
            )
            .is_err()
    );
    assert_eq!(
        fixture.registration.terminal_reason(),
        Some(crate::cas_projection::connection::router::LiveEventTargetCloseReason::StreamFailure)
    );
    let state = fixture
        .storage
        .turn_state(&fixture.home, fixture.turn_id, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(state.lifecycle(), TurnLifecycle::Pending);
    assert_eq!(state.source_event_count(), 0);
    assert!(state.end_status().is_none());

    fixture.close();
}
