#[test]
fn zero_worker_failure_cut_seals_targets_without_retaining_dispatch_guards() {
    let directory = tempfile::tempdir().unwrap();
    let home = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
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
    let home = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
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
