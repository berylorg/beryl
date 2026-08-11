#[test]
fn stop_cancels_only_the_exact_automatic_phase_continuation() {
    let fixture = StopFixture::new(61);
    let other_turn = SyndicTurnId::from_bytes([0xee; 16]);
    assert!(
        fixture
            .coordinator
            .record_lifecycle_yield(
                fixture.thread,
                fixture.turn,
                LifecycleYieldOutcome::PhaseContinue,
            )
            .unwrap()
    );
    assert!(
        fixture
            .coordinator
            .record_lifecycle_yield(
                fixture.thread,
                other_turn,
                LifecycleYieldOutcome::PhaseContinue,
            )
            .unwrap()
    );

    let owner = match fixture.coordinator.coordinate(
        &fixture.router,
        fixture.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        Ok(StopOwnership::Joined { .. }) => panic!("first stop must own dispatch"),
        Err(error) => panic!("stop coordination failed: {error}"),
    };
    assert_eq!(
        fixture
            .coordinator
            .take_terminal_lifecycle_yield(fixture.thread, fixture.turn)
            .unwrap(),
        None
    );
    assert_eq!(
        fixture
            .coordinator
            .take_terminal_lifecycle_yield(fixture.thread, other_turn)
            .unwrap(),
        Some(LifecycleYieldOutcome::PhaseContinue)
    );
    assert!(matches!(
        owner.settle_before_dispatch().unwrap(),
        StopDispatchSettlement::SafelyReopened(_)
    ));
}
#[test]
fn window_close_barrier_retains_exact_convergence_classification() {
    let fixture = StopFixture::new(71);
    let distinct_turn = SyndicTurnId::from_bytes([0xed; 16]);
    let owner = match fixture.coordinator.coordinate(
        &fixture.router,
        fixture.proof.clone(),
        StopCause::HealthyHomeWindowClose,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        Ok(StopOwnership::Joined { .. }) => panic!("first close stop must own dispatch"),
        Err(error) => panic!("window-close stop coordination failed: {error}"),
    };
    let mut barrier = WindowCloseStopBarrier::new(
        Arc::clone(&fixture.coordinator),
        owner.operation_id,
        fixture.turn,
        true,
    );
    assert_eq!(barrier.operation_id(), owner.operation_id);
    assert!(barrier.primary_owner());
    assert_eq!(
        barrier.poll().unwrap(),
        WindowCloseStopBarrierStatus::Waiting
    );
    assert!(matches!(
        owner.settle_before_dispatch().unwrap(),
        StopDispatchSettlement::SafelyReopened(_)
    ));

    let gate = InputGateRevision::new(1).unwrap();
    let matching_pending = StopAdmissionIneligibility::PendingTurn {
        turn_id: fixture.turn,
        current_gate_revision: gate,
    };
    let matching_terminal = StopAdmissionIneligibility::AwaitingTerminal {
        turn_id: fixture.turn,
        current_gate_revision: gate,
    };
    let matching_finalization = StopAdmissionIneligibility::FinalizingHistory {
        turn_id: fixture.turn,
        current_gate_revision: gate,
    };
    for reason in [matching_pending, matching_terminal, matching_finalization] {
        assert_eq!(
            window_close_ineligible_status(reason, fixture.turn).unwrap(),
            WindowCloseStopBarrierStatus::Waiting
        );
    }
    assert_eq!(
        window_close_ineligible_status(
            StopAdmissionIneligibility::Idle {
                current_gate_revision: gate,
            },
            fixture.turn,
        )
        .unwrap(),
        WindowCloseStopBarrierStatus::Converged
    );
    assert_eq!(
        window_close_ineligible_status(
            StopAdmissionIneligibility::Compacting {
                turn_id: distinct_turn,
                current_gate_revision: gate,
            },
            fixture.turn,
        )
        .unwrap(),
        WindowCloseStopBarrierStatus::Converged
    );
    assert!(matches!(
        window_close_ineligible_status(
            StopAdmissionIneligibility::AwaitingSteering {
                turn_id: fixture.turn,
                current_gate_revision: gate,
            },
            fixture.turn,
        ),
        Err(StopCoordinationError::LocalAuthorityMismatch)
    ));
}
