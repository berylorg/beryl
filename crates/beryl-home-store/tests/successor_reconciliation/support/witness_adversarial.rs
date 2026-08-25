use super::*;

fn witness_case(
    witness_value: Option<u64>,
    reads: usize,
    passive_collision: bool,
) -> ReconciliationResolution {
    reset_hooks();
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = HomeStore::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let source = store.register_domain::<SourceDomain>().unwrap();
    let witness = store.register_domain::<WitnessDomain>().unwrap();
    let passive = store.register_domain::<PassiveDomain>().unwrap();
    committed(
        store.execute_current(source.current_command(Put::<SourceDomain, SourceRecord>::new(1, 1))),
    );
    committed(
        store.execute_current(
            witness.current_command(Put::<WitnessDomain, WitnessRecord>::new(7, 1)),
        ),
    );
    if let Some(value) = witness_value {
        committed(store.execute_current(
            witness.current_command(Put::<WitnessDomain, WitnessRecord>::new(42, value)),
        ));
    }
    committed(
        store.execute_current(
            passive.current_command(Put::<PassiveDomain, PassiveRecord>::new(9, 1)),
        ),
    );

    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(source.contribution(
            store.domain_revision(source).unwrap(),
            SourcePut {
                key: 1,
                value: 2,
                source: SourceHook,
            },
        ))
        .unwrap();
    command
        .add(witness.contribution(
            store.domain_revision(witness).unwrap(),
            WitnessPut {
                key: 7,
                value: 2,
                witness: WitnessHook { reads },
            },
        ))
        .unwrap();
    command
        .add(passive.contribution(
            store.domain_revision(passive).unwrap(),
            Put::<PassiveDomain, PassiveRecord>::new(9, 2),
        ))
        .unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let handle = match store.execute(command) {
        CommandOutcome::Indeterminate { reconciliation, .. } => reconciliation.install_and_handle(),
        other => panic!("expected indeterminate outcome, got {other:?}"),
    };
    committed(
        store
            .execute_current(source.current_command(Put::<SourceDomain, SourceRecord>::new(1, 42))),
    );
    if passive_collision {
        committed(store.execute_current(
            passive.current_command(Put::<PassiveDomain, PassiveRecord>::new(9, 3)),
        ));
        FAIL_SOURCE.store(true, Ordering::SeqCst);
    }
    let resolution = store.reconcile(&handle).unwrap();
    if passive_collision {
        assert_eq!(SOURCE_CALLS.load(Ordering::SeqCst), 0);
    }
    store.close().unwrap();
    resolution
}

#[test]
fn source_and_witness_success_requires_passive_exact_new() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    assert!(matches!(
        witness_case(Some(42), 1, false),
        ReconciliationResolution::ExactSuccessor { .. }
    ));
    assert_eq!(
        witness_case(Some(42), 1, true),
        ReconciliationResolution::Collision
    );
}

#[test]
fn mismatch_missing_and_quota_exhaustion_are_collision() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    assert_eq!(
        witness_case(Some(43), 1, false),
        ReconciliationResolution::Collision
    );
    assert_eq!(
        witness_case(None, 1, false),
        ReconciliationResolution::Collision
    );
    assert_eq!(
        witness_case(Some(42), 2, false),
        ReconciliationResolution::Collision
    );
}

#[test]
fn oversized_expected_decoded_value_is_rejected_before_current_read() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    reset_hooks();
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = HomeStore::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let source = store.register_domain::<SourceDomain>().unwrap();
    let witness = store.register_domain::<WitnessDomain>().unwrap();
    committed(
        store.execute_current(source.current_command(Put::<SourceDomain, SourceRecord>::new(1, 1))),
    );
    committed(
        store.execute_current(
            witness.current_command(Put::<WitnessDomain, WitnessRecord>::new(7, 1)),
        ),
    );
    committed(store.execute_current(
        witness.current_command(Put::<WitnessDomain, WitnessRecord>::new(42, 42)),
    ));

    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(source.contribution(
            store.domain_revision(source).unwrap(),
            SourcePut {
                key: 1,
                value: 2,
                source: SourceHook,
            },
        ))
        .unwrap();
    command
        .add(witness.contribution(
            store.domain_revision(witness).unwrap(),
            WitnessPut {
                key: 7,
                value: 2,
                witness: OversizedExpectedWitness,
            },
        ))
        .unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let handle = match store.execute(command) {
        CommandOutcome::Indeterminate { reconciliation, .. } => reconciliation.install_and_handle(),
        other => panic!("expected indeterminate outcome, got {other:?}"),
    };
    committed(
        store
            .execute_current(source.current_command(Put::<SourceDomain, SourceRecord>::new(1, 42))),
    );
    assert_eq!(
        store.reconcile(&handle).unwrap(),
        ReconciliationResolution::Collision
    );
    assert_eq!(OVERSIZED_EXPECTED_REJECTIONS.load(Ordering::SeqCst), 1);
    assert_eq!(DERIVED_CURRENT_DECODE_CALLS.load(Ordering::SeqCst), 0);
    store.close().unwrap();
}

#[test]
fn invalid_and_oversized_derived_material_are_collision_before_current_decode() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    assert_eq!(
        reconcile_with_witness(RejectionWitness::<InvalidDerivedKeyRead>(PhantomData)),
        ReconciliationResolution::Collision
    );
    assert_eq!(DERIVED_CURRENT_DECODE_CALLS.load(Ordering::SeqCst), 0);
    assert_eq!(
        reconcile_with_witness(RejectionWitness::<OversizedDerivedKeyRead>(PhantomData)),
        ReconciliationResolution::Collision
    );
    assert_eq!(DERIVED_CURRENT_DECODE_CALLS.load(Ordering::SeqCst), 0);
    assert_eq!(
        reconcile_with_witness(RejectionWitness::<InvalidExpectedRead>(PhantomData)),
        ReconciliationResolution::Collision
    );
    assert_eq!(DERIVED_CURRENT_DECODE_CALLS.load(Ordering::SeqCst), 0);
    assert_eq!(
        reconcile_with_witness(RejectionWitness::<OversizedExpectedEncodingRead>(
            PhantomData
        )),
        ReconciliationResolution::Collision
    );
    assert_eq!(DERIVED_CURRENT_DECODE_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn witness_must_reserve_and_consume_a_derived_read() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    reset_hooks();
    let directory = tempdir().unwrap();
    let mut store = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let witness = store.register_domain::<WitnessDomain>().unwrap();
    let outcome = store.execute_current(witness.current_command(WitnessPut {
        key: 7,
        value: 2,
        witness: NoReadReservationWitness,
    }));
    let CommandOutcome::NotCommitted {
        evidence: CommandError::ContributorReservation { source, .. },
    } = outcome
    else {
        panic!("expected witness reservation rejection, got {outcome:?}");
    };
    let error = source.downcast_ref::<TestError>().unwrap();
    let TestError::Build(error) = error else {
        panic!("expected typed mutation build failure");
    };
    assert!(matches!(
        error.downcast_ref::<MutationBuildError>(),
        Some(MutationBuildError::MissingSuccessorReadReservation { .. })
    ));
    assert_eq!(store.home_revision().unwrap().get(), 1);
    store.close().unwrap();

    assert_eq!(
        reconcile_with_witness(NoConsumptionWitness),
        ReconciliationResolution::Collision
    );
    assert_eq!(DERIVED_CURRENT_DECODE_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn typed_unequal_correlations_collide_even_when_encodings_match() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    reset_hooks();
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = HomeStore::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let source = store.register_domain::<SourceDomain>().unwrap();
    let witness = store.register_domain::<WitnessDomain>().unwrap();
    committed(
        store.execute_current(source.current_command(Put::<SourceDomain, SourceRecord>::new(1, 1))),
    );
    committed(
        store.execute_current(
            witness.current_command(Put::<WitnessDomain, WitnessRecord>::new(7, 1)),
        ),
    );

    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(source.contribution(store.domain_revision(source).unwrap(), AliasedSourcePut))
        .unwrap();
    command
        .add(witness.contribution(store.domain_revision(witness).unwrap(), AliasedWitnessPut))
        .unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let handle = match store.execute(command) {
        CommandOutcome::Indeterminate { reconciliation, .. } => reconciliation.install_and_handle(),
        other => panic!("expected indeterminate outcome, got {other:?}"),
    };
    committed(
        store.execute_current(source.current_command(Put::<SourceDomain, SourceRecord>::new(1, 3))),
    );
    assert_eq!(
        store.reconcile(&handle).unwrap(),
        ReconciliationResolution::Collision
    );
    store.close().unwrap();
}
