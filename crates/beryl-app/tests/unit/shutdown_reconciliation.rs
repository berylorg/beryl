include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/unit/shutdown_binding.rs"
));

#[test]
fn indeterminate_cancellation_blocks_settlement_until_reconciliation_completes() {
    let fixture = Fixture::new();
    let cancellation = activate(&fixture);
    let fence = fixture.gate.fence().unwrap();
    assert!(fixture.read(&fence).unwrap().is_none());
    let home = fixture.service.home.as_deref().unwrap();
    let storage = &fixture.service.storage;
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command
        .add(storage.cancel_binding_activation(storage.revision(home).unwrap(), cancellation))
        .unwrap();
    fixture
        .faults
        .fail_next(FaultPoint::AfterCommitBeforePersist);
    let CommandOutcome::Indeterminate { reconciliation, .. } = home.execute(command) else {
        panic!("cancellation must retain reconciliation");
    };
    let handle = reconciliation.install_and_handle();
    assert!(
        storage
            .pending_dispatch_evidence(home, fixture.thread, point_limit())
            .unwrap()
            .is_some()
    );
    assert!(fixture.read(&fence).unwrap().is_none());
    assert!(matches!(
        home.reconcile(&handle).unwrap(),
        ReconciliationResolution::ExactNew { .. }
    ));
    assert!(home.pending_reconciliations().is_empty());
    let settled = fixture.read(&fence).unwrap().unwrap();
    settled
        .revalidate(
            &fixture.service,
            &fixture.sessions,
            &ProjectionCancellationToken::new(),
        )
        .unwrap();
}
