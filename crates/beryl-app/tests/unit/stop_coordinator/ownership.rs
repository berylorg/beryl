#[test]
fn dropping_claimed_stop_owner_preserves_durable_claim_without_home_io() {
    let fixture = StopFixture::new(29);
    let owner = match fixture.coordinator.coordinate(
        &fixture.router,
        fixture.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        Ok(StopOwnership::Joined { .. }) => panic!("first stop must own dispatch"),
        Err(error) => panic!("stop coordination failed: {error}"),
    };
    let operation_id = owner.operation_id();
    let revision = fixture.home.home_revision().unwrap();

    drop(owner);

    assert_eq!(fixture.home.home_revision().unwrap(), revision);
    let state = fixture.coordinator.state.lock().unwrap();
    let local = state.stops.get(&fixture.thread).unwrap();
    assert_eq!(local.operation_id, operation_id);
    assert_eq!(local.dispatch, LocalDispatchState::ClaimUnresolved);
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
fn dropping_dispatching_stop_owner_widens_ambiguity_without_home_io() {
    let fixture = StopFixture::new(30);
    let owner = match fixture.coordinator.coordinate(
        &fixture.router,
        fixture.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        Ok(StopOwnership::Joined { .. }) => panic!("first stop must own dispatch"),
        Err(error) => panic!("stop coordination failed: {error}"),
    };
    owner.begin_dispatch().unwrap();
    let operation_id = owner.operation_id();
    let revision = fixture.home.home_revision().unwrap();

    drop(owner);

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
fn router_valid_proof_for_the_wrong_storage_target_is_rejected() {
    let fixture = StopFixture::new(31);
    let wrong = fixture.wrong_storage_target_proof(31);

    assert!(matches!(
        fixture
            .coordinator
            .coordinate(&fixture.router, wrong, StopCause::SelectedOperationControl,),
        Err(StopCoordinationError::TargetUnavailable)
    ));
    assert!(matches!(
        fixture
            .storage
            .stop_admission_read(&fixture.home, fixture.thread, point_limit())
            .unwrap(),
        StopAdmissionRead::Admissible(_)
    ));
}

#[test]
fn matching_causes_join_one_primary_and_each_new_operation_gets_a_new_attempt() {
    let fixture = StopFixture::new(41);
    let first_owner = match fixture.coordinator.coordinate(
        &fixture.router,
        fixture.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        Ok(StopOwnership::Joined { .. }) => panic!("first request must own dispatch"),
        Err(error) => panic!("first stop coordination failed: {error}"),
    };
    let first_operation = first_owner.operation_id;
    let first_attempt = first_owner.attempt;

    let joined_operation = match fixture.coordinator.coordinate(
        &fixture.router,
        fixture.proof.clone(),
        StopCause::DiagnosticControl,
    ) {
        Ok(StopOwnership::Joined { operation_id, .. }) => operation_id,
        Ok(StopOwnership::Primary(_)) => panic!("matching cause must not own a second dispatch"),
        Err(error) => panic!("cause join failed: {error}"),
    };
    assert_eq!(joined_operation, first_operation);
    let joined = fixture.live_stop();
    assert!(
        joined
            .record()
            .causes()
            .contains(StopCause::SelectedOperationControl)
    );
    assert!(
        joined
            .record()
            .causes()
            .contains(StopCause::DiagnosticControl)
    );
    assert!(matches!(
        first_owner.settle_before_dispatch().unwrap(),
        StopDispatchSettlement::SafelyReopened(operation_id)
            if operation_id == first_operation
    ));

    let second_owner = match fixture.coordinator.coordinate(
        &fixture.router,
        fixture.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        Ok(StopOwnership::Joined { .. }) => panic!("reopened operation must admit a new primary"),
        Err(error) => panic!("second stop coordination failed: {error}"),
    };
    assert_ne!(second_owner.operation_id, first_operation);
    assert_ne!(second_owner.attempt, first_attempt);
    assert_ne!(
        second_owner.operation_id.nonce().as_bytes(),
        second_owner.attempt.as_bytes()
    );
    assert!(matches!(
        second_owner.settle_before_dispatch().unwrap(),
        StopDispatchSettlement::SafelyReopened(_)
    ));
}

#[test]
fn proven_nondispatch_reopens_without_approval_but_approval_ownership_abandons() {
    let safe = StopFixture::new(51);
    let safe_owner = match safe.coordinator.coordinate(
        &safe.router,
        safe.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        Ok(StopOwnership::Joined { .. }) => panic!("first safe stop must own dispatch"),
        Err(error) => panic!("safe stop coordination failed: {error}"),
    };
    assert!(matches!(
        safe_owner.settle_before_dispatch().unwrap(),
        StopDispatchSettlement::SafelyReopened(_)
    ));
    assert!(matches!(
        safe.storage
            .stop_admission_read(&safe.home, safe.thread, point_limit())
            .unwrap(),
        StopAdmissionRead::Admissible(_)
    ));

    let approval = StopFixture::new(52);
    let primary = match approval.coordinator.coordinate(
        &approval.router,
        approval.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        Ok(StopOwnership::Joined { .. }) => panic!("first approval fixture stop must own dispatch"),
        Err(error) => panic!("approval fixture coordination failed: {error}"),
    };
    let operation_id = primary.operation_id;
    assert!(matches!(
        approval.coordinator.coordinate(
            &approval.router,
            approval.proof.clone(),
            StopCause::InterruptingApproval,
        ),
        Ok(StopOwnership::Joined {
            operation_id: joined,
            ..
        }) if joined == operation_id
    ));
    assert!(matches!(
        primary.settle_before_dispatch().unwrap(),
        StopDispatchSettlement::Abandoned(abandoned) if abandoned == operation_id
    ));
    assert_eq!(
        approval
            .coordinator
            .state
            .lock()
            .unwrap()
            .stops
            .get(&approval.thread)
            .unwrap()
            .dispatch,
        LocalDispatchState::DurablyAbandoned
    );
    assert!(
        approval
            .coordinator
            .abandon_for_authority_loss(approval.thread, approval.turn)
            .unwrap()
    );
    assert!(
        !approval
            .coordinator
            .state
            .lock()
            .unwrap()
            .stops
            .contains_key(&approval.thread)
    );
}

#[test]
fn safe_reopen_requires_the_exact_local_and_durable_dispatch_authority() {
    let fixture = StopFixture::new(53);
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
    let attempt = owner.attempt;

    {
        let mut state = fixture.coordinator.state.lock().unwrap();
        let local = state.stops.get_mut(&fixture.thread).unwrap();
        local.attempt = None;
        local.dispatch = LocalDispatchState::AdmittedNotClaimed;
    }
    assert!(matches!(
        fixture.coordinator.settle_unclaimed(operation_id),
        Err(StopCoordinationError::LocalAuthorityMismatch)
    ));
    let claimed = fixture.live_stop();
    assert_eq!(claimed.state(), StopOperationState::DispatchClaimed);
    assert_eq!(claimed.attempt(), Some(attempt));

    let foreign_attempt = StopAttemptNonce::from_bytes([0xa5; 16]);
    {
        let mut state = fixture.coordinator.state.lock().unwrap();
        let local = state.stops.get_mut(&fixture.thread).unwrap();
        local.attempt = Some(foreign_attempt);
        local.dispatch = LocalDispatchState::ClaimedNotDispatched;
    }
    assert!(matches!(
        fixture
            .coordinator
            .settle_proven_nondispatch(operation_id, Some(foreign_attempt)),
        Err(StopCoordinationError::LocalAuthorityMismatch)
    ));
    let claimed = fixture.live_stop();
    assert_eq!(claimed.state(), StopOperationState::DispatchClaimed);
    assert_eq!(claimed.attempt(), Some(attempt));

    {
        let mut state = fixture.coordinator.state.lock().unwrap();
        let local = state.stops.get_mut(&fixture.thread).unwrap();
        local.attempt = Some(attempt);
        local.dispatch = LocalDispatchState::ClaimUnresolved;
    }
    assert!(matches!(
        fixture
            .coordinator
            .settle_proven_nondispatch(operation_id, Some(attempt)),
        Err(StopCoordinationError::LocalAuthorityMismatch)
    ));
    let claimed = fixture.live_stop();
    assert_eq!(claimed.state(), StopOperationState::DispatchClaimed);
    assert_eq!(claimed.attempt(), Some(attempt));

    fixture
        .coordinator
        .state
        .lock()
        .unwrap()
        .stops
        .get_mut(&fixture.thread)
        .unwrap()
        .dispatch = LocalDispatchState::ClaimedNotDispatched;
    assert!(matches!(
        owner.settle_before_dispatch().unwrap(),
        StopDispatchSettlement::SafelyReopened(reopened) if reopened == operation_id
    ));
}
