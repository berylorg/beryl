fn reset_hooks() {
    SOURCE_CALLS.store(0, Ordering::SeqCst);
    BLOCK_SOURCE.store(false, Ordering::SeqCst);
    RELEASE_SOURCE.store(false, Ordering::SeqCst);
    FAIL_SOURCE.store(false, Ordering::SeqCst);
    OVERSIZED_EXPECTED_REJECTIONS.store(0, Ordering::SeqCst);
    DERIVED_CURRENT_DECODE_CALLS.store(0, Ordering::SeqCst);
}

fn committed(outcome: CommandOutcome) {
    assert!(matches!(outcome, CommandOutcome::Committed { .. }));
}

fn reconcile_with_witness<W>(witness_hook: W) -> ReconciliationResolution
where
    W: SuccessorWitness<WitnessDomain, Protocol>,
{
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
            store.domain_revision(&source).unwrap(),
            SourcePut {
                key: 1,
                value: 2,
                source: SourceHook,
            },
        ))
        .unwrap();
    command
        .add(witness.contribution(
            store.domain_revision(&witness).unwrap(),
            WitnessPut {
                key: 7,
                value: 2,
                witness: witness_hook,
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
    let resolution = store.reconcile(&handle).unwrap();
    store.close().unwrap();
    resolution
}
