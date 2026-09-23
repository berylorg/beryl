#[test]
fn failure_witness_preserves_each_registration_turn_identity_without_interrupt_authority() {
    let directory = tempfile::tempdir().unwrap();
    let home = HomeOpenCandidate::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap()
    .prepare_publication(HomeDomainRequirements::new())
    .unwrap()
    .publish()
    .unwrap();
    let owner = SyndicThreadId::from_bytes([201; 16]);
    let active_turn = beryl_model::SyndicTurnId::from_bytes([202; 16]);
    let pending = super::pending_activation(201);
    let operation =
        CompactionOperationId::new(owner, CompactionOperationNonce::from_bytes([203; 16]));
    let variants = [
        (
            TargetTurnRegistration::Active {
                syndic_turn_id: active_turn,
                cas_turn_id: CasTurnId::new("already-active").unwrap(),
            },
            active_turn,
        ),
        (
            TargetTurnRegistration::Pending(pending.clone()),
            pending.turn_id(),
        ),
        (
            TargetTurnRegistration::ContextCompaction(ContextCompactionTargetAuthority::new(
                operation,
                operation.provider_turn_id(),
            )),
            operation.provider_turn_id(),
        ),
    ];
    for (turn, expected) in variants {
        let capture_expected = matches!(turn, TargetTurnRegistration::Active { .. });
        let (router, gate) = router_with_gate_for(201, 89_201);
        let identity = PersistentFailureCutIdentity::new(
            home.home_id(),
            home.health().generation().unwrap(),
            router.commands.service_generation(),
            PersistentFailureGeneration::FIRST,
        );
        let command = live_command(&router);
        let registration = router
            .register(
                &command,
                LoadedThreadKey {
                    runtime_id: router.runtime_id,
                    process_generation: router.process_generation,
                    cas_thread_id: CasThreadId::new("identity-capture").unwrap(),
                },
                owner,
                CasLoadedSessionGeneration::new(
                    router.process_generation,
                    CasLoadedThreadGeneration::new(1).unwrap(),
                ),
                identity.home_generation.get(),
                Duration::from_secs(1),
                turn,
            )
            .unwrap();
        drop(command);
        assert!(
            gate.elect_persistent_failure_for_test(identity.failure_generation)
                .unwrap()
        );
        let batch = router
            .freeze_persistent_failure_targets(identity, false)
            .unwrap();
        let witness = batch.witnesses().next().unwrap();
        assert_eq!(witness.outage_target().unwrap().is_some(), capture_expected);
        let mut candidates = batch.into_candidates();
        let candidate = candidates.pop().unwrap();
        assert!(candidates.is_empty());
        assert_eq!(candidate.syndic_thread_id(), owner);
        assert_eq!(candidate.syndic_turn_id(), Some(expected));
        assert_eq!(
            candidate.into_proof().err(),
            Some(PersistentFailureTargetIneligibility::PriorPrimaryAmbiguous)
        );
        assert_eq!(router.failure_dispatch_guard_count_for_test(), 0);
        drop(registration);
    }
}

#[test]
fn zero_worker_failure_cut_seals_targets_without_retaining_dispatch_guards() {
    let directory = tempfile::tempdir().unwrap();
    let home = beryl_home_store::HomeOpenCandidate::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap()
    .prepare_publication(beryl_home_store::HomeDomainRequirements::new())
    .unwrap()
    .publish()
    .unwrap();
    let identity = PersistentFailureCutIdentity::new(
        home.home_id(),
        home.health().generation().unwrap(),
        ProjectionServiceGeneration::allocate().unwrap(),
        PersistentFailureGeneration::FIRST,
    );
    let (router, registration, identity) = eligibility_target(EligibilityCase::Eligible, identity);
    let mut candidates = router
        .freeze_persistent_failure_targets(identity, false)
        .unwrap()
        .into_candidates();
    assert_eq!(candidates.len(), 1);
    let proof = candidates.pop().unwrap().into_proof().unwrap();
    assert_eq!(
        router.authorize_persistent_failure_dispatch(proof).err(),
        Some(PersistentFailureTargetIneligibility::IdentityMismatch)
    );
    let state = router.state.lock().unwrap();
    assert_eq!(state.targets.len(), 1);
    drop(state);
    assert_eq!(router.failure_dispatch_guard_count_for_test(), 0);
    assert!(
        router
            .freeze_persistent_failure_targets(identity, true)
            .is_err()
    );
    drop(registration);
    assert_eq!(router.state.lock().unwrap().targets.len(), 1);
}

#[test]
fn retired_failure_cut_preserves_targets_as_nondispatch_without_guards() {
    let directory = tempfile::tempdir().unwrap();
    let home = beryl_home_store::HomeOpenCandidate::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap()
    .prepare_publication(beryl_home_store::HomeDomainRequirements::new())
    .unwrap()
    .publish()
    .unwrap();
    let identity = PersistentFailureCutIdentity::new(
        home.home_id(),
        home.health().generation().unwrap(),
        ProjectionServiceGeneration::allocate().unwrap(),
        PersistentFailureGeneration::FIRST,
    );
    let (router, _registration, identity) = eligibility_target(EligibilityCase::Eligible, identity);
    router.state.lock().unwrap().retired = Some(LiveEventTargetCloseReason::StreamFailure);
    let mut candidates = router
        .freeze_persistent_failure_targets(identity, true)
        .unwrap()
        .into_candidates();
    assert_eq!(candidates.len(), 1);
    assert_eq!(
        candidates.pop().unwrap().into_proof().err(),
        Some(PersistentFailureTargetIneligibility::RouterUnavailable)
    );
    let state = router.state.lock().unwrap();
    assert_eq!(state.targets.len(), 1);
    drop(state);
    assert_eq!(router.failure_dispatch_guard_count_for_test(), 0);
}
