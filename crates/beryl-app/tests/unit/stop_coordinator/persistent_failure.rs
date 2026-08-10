#[test]
fn persistent_failure_freezes_claimed_owner_without_durable_settlement() {
    let fixture = StopFixture::new(54);
    let owner = match fixture.coordinator.coordinate(
        &fixture.router,
        fixture.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        Ok(StopOwnership::Joined { .. }) => panic!("first stop must own dispatch"),
        Err(error) => panic!("stop coordination failed: {error}"),
    };
    let operation_id = owner.operation_id;
    let revision = fixture.home.home_revision().unwrap();
    let identity = failure_identity(&fixture);

    assert!(
        fixture
            .command_gate
            .elect_persistent_failure_for_test(identity.failure_generation)
            .unwrap()
    );
    fixture
        .coordinator
        .freeze_for_persistent_failure(identity)
        .unwrap();
    assert!(matches!(
        owner.begin_dispatch(),
        Err(StopCoordinationError::HomeAuthorityLost)
    ));
    assert_eq!(
        fixture
            .coordinator
            .persistent_failure_evidence(identity, fixture.thread)
            .unwrap(),
        PersistentFailureStopEvidence::ClaimedNotDispatched
    );
    assert!(matches!(
        owner.settle_before_dispatch().unwrap(),
        StopDispatchSettlement::Stopping(stopping) if stopping == operation_id
    ));

    assert_eq!(fixture.home.home_revision().unwrap(), revision);
    let durable = fixture.live_stop();
    assert_eq!(durable.state(), StopOperationState::DispatchClaimed);
    assert!(
        fixture
            .coordinator
            .state
            .lock()
            .unwrap()
            .stops
            .contains_key(&fixture.thread)
    );
}
#[test]
fn persistent_failure_cut_and_stop_claim_have_deterministic_two_order_linearization() {
    let claim_first = StopFixture::new(192);
    let claim_pause = claim_first
        .coordinator
        .install_race_pause(StopRaceStage::ClaimFenceHeld);
    let coordinator = Arc::clone(&claim_first.coordinator);
    let router = Arc::clone(&claim_first.router);
    let proof = claim_first.proof.clone();
    let claim = std::thread::spawn(move || {
        coordinator.coordinate(&router, proof, StopCause::SelectedOperationControl)
    });
    assert!(
        claim_pause.wait_until_reached(Duration::from_secs(10)),
        "claim-first coordinate did not reach the held stop fence"
    );
    let identity = failure_identity(&claim_first);
    assert!(
        claim_first
            .command_gate
            .elect_persistent_failure_for_test(identity.failure_generation)
            .unwrap()
    );
    let freeze_coordinator = Arc::clone(&claim_first.coordinator);
    let (freeze_started_tx, freeze_started_rx) = mpsc::sync_channel(1);
    let freeze = std::thread::spawn(move || {
        freeze_started_tx.send(()).unwrap();
        freeze_coordinator.freeze_for_persistent_failure(identity)
    });
    freeze_started_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("claim-first failure freeze started while the claim fence was held");
    claim_pause.release();
    let owner = match claim.join().unwrap() {
        Ok(StopOwnership::Primary(owner)) => owner,
        Ok(StopOwnership::Joined { .. }) => panic!("claim-first stop unexpectedly joined"),
        Err(error) => panic!("claim-first stop failed after winning its mutex fence: {error}"),
    };
    freeze.join().unwrap().unwrap();
    assert_eq!(
        claim_first
            .coordinator
            .persistent_failure_evidence(identity, claim_first.thread)
            .unwrap(),
        PersistentFailureStopEvidence::ClaimedNotDispatched
    );
    assert!(matches!(
        owner.settle_before_dispatch().unwrap(),
        StopDispatchSettlement::Stopping(_)
    ));
    assert_eq!(
        claim_first.live_stop().state(),
        StopOperationState::DispatchClaimed
    );

    let cut_first = StopFixture::new(193);
    let claim_pause = cut_first
        .coordinator
        .install_race_pause(StopRaceStage::BeforeClaimFence);
    let coordinator = Arc::clone(&cut_first.coordinator);
    let router = Arc::clone(&cut_first.router);
    let proof = cut_first.proof.clone();
    let claim = std::thread::spawn(move || {
        coordinator.coordinate(&router, proof, StopCause::SelectedOperationControl)
    });
    assert!(
        claim_pause.wait_until_reached(Duration::from_secs(10)),
        "cut-first coordinate did not reach the pre-claim fence"
    );
    let identity = failure_identity(&cut_first);
    assert!(
        cut_first
            .command_gate
            .elect_persistent_failure_for_test(identity.failure_generation)
            .unwrap()
    );
    cut_first
        .coordinator
        .freeze_for_persistent_failure(identity)
        .unwrap();
    assert_eq!(
        cut_first
            .coordinator
            .persistent_failure_evidence(identity, cut_first.thread)
            .unwrap(),
        PersistentFailureStopEvidence::NoLocalStop
    );
    claim_pause.release();
    assert!(matches!(
        claim.join().unwrap(),
        Err(StopCoordinationError::HomeAuthorityLost)
    ));
    assert!(
        cut_first
            .coordinator
            .state
            .lock()
            .unwrap()
            .stops
            .is_empty()
    );
    assert!(matches!(
        cut_first
            .storage
            .stop_admission_read(&cut_first.home, cut_first.thread, point_limit())
            .unwrap(),
        StopAdmissionRead::Admissible(_)
    ));
}

#[test]
fn dispatch_winning_before_cut_is_retained_as_ambiguous() {
    let fixture = StopFixture::new(55);
    let owner = match fixture.coordinator.coordinate(
        &fixture.router,
        fixture.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        Ok(StopOwnership::Joined { .. }) => panic!("first stop must own dispatch"),
        Err(error) => panic!("stop coordination failed: {error}"),
    };
    let operation_id = owner.operation_id;
    owner.begin_dispatch().unwrap();
    let revision = fixture.home.home_revision().unwrap();
    let identity = failure_identity(&fixture);

    fixture
        .command_gate
        .elect_persistent_failure_for_test(identity.failure_generation)
        .unwrap();
    fixture
        .coordinator
        .freeze_for_persistent_failure(identity)
        .unwrap();
    let evidence = fixture
        .coordinator
        .persistent_failure_evidence(identity, fixture.thread)
        .unwrap();
    assert_eq!(evidence, PersistentFailureStopEvidence::Dispatching);
    assert!(!evidence.permits_volatile_interrupt());
    assert!(matches!(
        owner.settle_before_dispatch().unwrap(),
        StopDispatchSettlement::Stopping(stopping) if stopping == operation_id
    ));
    assert_eq!(fixture.home.home_revision().unwrap(), revision);
    assert_eq!(
        fixture.live_stop().state(),
        StopOperationState::DispatchClaimed
    );
}

#[test]
fn persistent_failure_cut_and_begin_dispatch_have_deterministic_two_order_linearization() {
    let dispatch_first = StopFixture::new(194);
    let owner = match dispatch_first.coordinator.coordinate(
        &dispatch_first.router,
        dispatch_first.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        Ok(StopOwnership::Joined { .. }) => panic!("dispatch-first stop unexpectedly joined"),
        Err(error) => panic!("dispatch-first stop coordination failed: {error}"),
    };
    let dispatch_pause = dispatch_first
        .coordinator
        .install_race_pause(StopRaceStage::BeginDispatchFenceHeld);
    let dispatch = std::thread::spawn(move || {
        let result = owner.begin_dispatch();
        (owner, result)
    });
    assert!(
        dispatch_pause.wait_until_reached(Duration::from_secs(10)),
        "dispatch-first owner did not reach the held dispatch fence"
    );
    let identity = failure_identity(&dispatch_first);
    assert!(
        dispatch_first
            .command_gate
            .elect_persistent_failure_for_test(identity.failure_generation)
            .unwrap()
    );
    let freeze_coordinator = Arc::clone(&dispatch_first.coordinator);
    let (freeze_started_tx, freeze_started_rx) = mpsc::sync_channel(1);
    let freeze = std::thread::spawn(move || {
        freeze_started_tx.send(()).unwrap();
        freeze_coordinator.freeze_for_persistent_failure(identity)
    });
    freeze_started_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("dispatch-first failure freeze started while the dispatch fence was held");
    dispatch_pause.release();
    let (owner, dispatch_result) = dispatch.join().unwrap();
    dispatch_result.unwrap();
    freeze.join().unwrap().unwrap();
    assert_eq!(
        dispatch_first
            .coordinator
            .persistent_failure_evidence(identity, dispatch_first.thread)
            .unwrap(),
        PersistentFailureStopEvidence::Dispatching
    );
    assert!(matches!(
        owner.settle_before_dispatch().unwrap(),
        StopDispatchSettlement::Stopping(_)
    ));

    let cut_first = StopFixture::new(195);
    let owner = match cut_first.coordinator.coordinate(
        &cut_first.router,
        cut_first.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        Ok(StopOwnership::Joined { .. }) => panic!("cut-first stop unexpectedly joined"),
        Err(error) => panic!("cut-first stop coordination failed: {error}"),
    };
    let dispatch_pause = cut_first
        .coordinator
        .install_race_pause(StopRaceStage::BeforeBeginDispatchFence);
    let dispatch = std::thread::spawn(move || {
        let result = owner.begin_dispatch();
        (owner, result)
    });
    assert!(
        dispatch_pause.wait_until_reached(Duration::from_secs(10)),
        "cut-first owner did not reach the pre-dispatch fence"
    );
    let identity = failure_identity(&cut_first);
    assert!(
        cut_first
            .command_gate
            .elect_persistent_failure_for_test(identity.failure_generation)
            .unwrap()
    );
    cut_first
        .coordinator
        .freeze_for_persistent_failure(identity)
        .unwrap();
    dispatch_pause.release();
    let (owner, dispatch_result) = dispatch.join().unwrap();
    assert!(matches!(
        dispatch_result,
        Err(StopCoordinationError::HomeAuthorityLost)
    ));
    assert_eq!(
        cut_first
            .coordinator
            .persistent_failure_evidence(identity, cut_first.thread)
            .unwrap(),
        PersistentFailureStopEvidence::ClaimedNotDispatched
    );
    assert!(matches!(
        owner.settle_before_dispatch().unwrap(),
        StopDispatchSettlement::Stopping(_)
    ));
}

#[test]
fn persistent_failure_classifies_every_retained_stop_dispatch_state() {
    let fixture = StopFixture::new(56);
    let owner = match fixture.coordinator.coordinate(
        &fixture.router,
        fixture.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        Ok(StopOwnership::Joined { .. }) => panic!("first stop must own dispatch"),
        Err(error) => panic!("stop coordination failed: {error}"),
    };
    let identity = failure_identity(&fixture);
    fixture
        .command_gate
        .elect_persistent_failure_for_test(identity.failure_generation)
        .unwrap();
    fixture
        .coordinator
        .freeze_for_persistent_failure(identity)
        .unwrap();
    let attempt = owner.attempt;

    let cases = [
        (
            LocalDispatchState::AdmittedNotClaimed,
            None,
            PersistentFailureStopEvidence::AdmittedNotClaimed,
            true,
        ),
        (
            LocalDispatchState::ClaimUnresolved,
            Some(attempt),
            PersistentFailureStopEvidence::ClaimUnresolved,
            false,
        ),
        (
            LocalDispatchState::ClaimedNotDispatched,
            Some(attempt),
            PersistentFailureStopEvidence::ClaimedNotDispatched,
            true,
        ),
        (
            LocalDispatchState::Dispatching,
            Some(attempt),
            PersistentFailureStopEvidence::Dispatching,
            false,
        ),
        (
            LocalDispatchState::HardStopRunningProvenNondispatch,
            Some(attempt),
            PersistentFailureStopEvidence::HardStopRunning,
            false,
        ),
        (
            LocalDispatchState::ProvenNondispatchSettling,
            Some(attempt),
            PersistentFailureStopEvidence::ProvenNondispatchSettling,
            false,
        ),
        (
            LocalDispatchState::PrimaryAccepted,
            Some(attempt),
            PersistentFailureStopEvidence::PrimaryAccepted,
            false,
        ),
        (
            LocalDispatchState::PossiblyDispatched,
            Some(attempt),
            PersistentFailureStopEvidence::PossiblyDispatched,
            false,
        ),
        (
            LocalDispatchState::DurablyAbandoned,
            Some(attempt),
            PersistentFailureStopEvidence::DurablyAbandoned,
            false,
        ),
        (
            LocalDispatchState::FailureFrozenNondispatch,
            Some(attempt),
            PersistentFailureStopEvidence::ClaimedNotDispatched,
            true,
        ),
    ];
    for (dispatch, retained_attempt, expected, permits_volatile) in cases {
        {
            let mut state = fixture.coordinator.state.lock().unwrap();
            let local = state.stops.get_mut(&fixture.thread).unwrap();
            local.dispatch = dispatch;
            local.attempt = retained_attempt;
        }
        let evidence = fixture
            .coordinator
            .persistent_failure_evidence(identity, fixture.thread)
            .unwrap();
        assert_eq!(evidence, expected);
        assert_eq!(evidence.permits_volatile_interrupt(), permits_volatile);
    }

    assert!(matches!(
        owner.settle_before_dispatch().unwrap(),
        StopDispatchSettlement::Stopping(_)
    ));
}
