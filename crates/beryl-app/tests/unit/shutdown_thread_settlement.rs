use super::*;
include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/unit/shutdown_support.rs"
));

mod reconciliation {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/shutdown_reconciliation.rs"
    ));
}

#[test]
fn pending_settlement_waits_for_admission_flight_and_continuation_custody() {
    let fixture = Fixture::new();
    let reservation = fixture.gate.execution_permit().reserve().unwrap();
    let fence = fixture.gate.fence().unwrap();
    assert!(fixture.read(&fence).unwrap().is_none());
    drop(reservation);
    let coordinator =
        CasProjectionCoordinator::for_healthy_home(fixture.service.home.as_deref().unwrap())
            .unwrap();
    let flight = coordinator.begin_projection(fixture.thread).unwrap();
    assert!(fixture.read(&fence).unwrap().is_none());
    drop(flight);
    let continuation = fixture
        .service
        .stop_coordinator
        .compaction_custody
        .reserve_continuation(fixture.thread, fixture.turn)
        .unwrap();
    assert!(fixture.read(&fence).unwrap().is_none());
    drop(continuation);
    let settled = fixture.read(&fence).unwrap().unwrap();
    let ShutdownThreadDisposition::Pending(pending) = settled.disposition() else {
        panic!("pending work changed disposition");
    };
    assert_eq!(pending.turn_id(), fixture.turn);
    assert_eq!(pending.thread_id(), fixture.thread);
    settled
        .revalidate(
            &fixture.service,
            &fixture.sessions,
            &ProjectionCancellationToken::new(),
        )
        .unwrap();
    assert!(matches!(
        coordinator.begin_projection(fixture.thread),
        Err(ProjectionCoordinatorError::ProjectionInFlight { .. })
    ));
    drop(settled);
    drop(coordinator.begin_projection(fixture.thread).unwrap());
}

#[test]
fn foreign_and_reopened_fences_cannot_authorize_settlement() {
    let fixture = Fixture::new();
    let foreign = ProcessAdmissionGate::new().fence().unwrap();
    assert!(matches!(
        fixture.read(&foreign),
        Err(ShutdownThreadSettlementError::Process(
            ProcessAdmissionError::Stale
        ))
    ));
    let fence = fixture.gate.fence().unwrap();
    let proof = fixture.read(&fence).unwrap().unwrap();
    fence.reopen_if(true).unwrap();
    assert!(matches!(
        proof.revalidate(
            &fixture.service,
            &fixture.sessions,
            &ProjectionCancellationToken::new()
        ),
        Err(ShutdownThreadSettlementError::Process(
            ProcessAdmissionError::Stale
        ))
    ));
    drop(proof);
    let fresh = fixture.gate.fence().unwrap();
    assert!(matches!(
        fixture.read(&fence),
        Err(ShutdownThreadSettlementError::Process(
            ProcessAdmissionError::Stale
        ))
    ));
    assert!(fixture.read(&fresh).unwrap().is_some());
}

#[test]
fn durable_corruption_never_revalidates_a_pending_settlement() {
    let fixture = Fixture::new();
    let fence = fixture.gate.fence().unwrap();
    let proof = fixture.read(&fence).unwrap().unwrap();
    let ShutdownThreadDisposition::Pending(pending) = proof.disposition() else {
        unreachable!()
    };
    let home = fixture.service.home.as_deref().unwrap();
    let mut changes = syndic_storage::test_faults::FixtureBatch::new();
    changes
        .delete(syndic_storage::test_faults::FixtureDelete::ContentManifest(
            pending.input().id(),
        ))
        .unwrap();
    execute(
        home,
        fixture
            .service
            .storage
            .fixture_contribution(fixture.service.storage.revision(home).unwrap(), changes),
    );
    assert!(
        proof
            .revalidate(
                &fixture.service,
                &fixture.sessions,
                &ProjectionCancellationToken::new()
            )
            .is_err()
    );
    drop(proof);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        match fixture.read(&fence) {
            Err(_) => break,
            Ok(Some(_)) => panic!("corrupt pending input authorized settlement"),
            Ok(None) => {
                assert!(
                    std::time::Instant::now() < deadline,
                    "corruption did not converge to failure"
                );
                std::thread::yield_now();
            }
        }
    }
}
