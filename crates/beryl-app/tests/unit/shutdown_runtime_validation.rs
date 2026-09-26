use super::*;
include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/unit/shutdown_support.rs"
));

fn revision(fixture: &Fixture) -> ShutdownWorkRevision {
    fixture
        .service
        .shutdown_work_revision(&fixture.sessions)
        .unwrap()
}

#[test]
fn runtime_validation_preserves_idle_and_pending_sources_and_execution() {
    for pending in [false, true] {
        let fixture = Fixture::with_pending(pending);
        let expected = revision(&fixture);
        let permit = fixture.gate.execution_permit();
        let home = fixture.service.home.as_deref().unwrap();
        let before = home.home_revision().unwrap();
        fixture
            .service
            .try_validate_shutdown_runtime(&fixture.sessions, &expected)
            .unwrap();
        assert_eq!(revision(&fixture), expected);
        assert_eq!(home.home_revision().unwrap(), before);
        permit.commit(|| ()).unwrap();
    }
}

#[test]
fn runtime_validation_refuses_foreign_evidence_and_changed_revisions() {
    let fixture = Fixture::idle();
    let foreign = Fixture::idle();
    let expected = revision(&fixture);
    assert_eq!(
        foreign
            .service
            .try_validate_shutdown_runtime(&foreign.sessions, &expected),
        Err(RuntimeWorkError::Foreign)
    );
    assert!(
        fixture
            .service
            .try_validate_shutdown_runtime(&foreign.sessions, &expected)
            .is_err()
    );
    let permit = fixture.gate.execution_permit();
    for change in 0..7 {
        let mut stale = expected.clone();
        match change {
            0 => stale.flights += 1,
            1 => stale.loaded += 1,
            2 => stale.required.connections.stamp.membership += 1,
            3 => stale.required.connections.stamp.routers += 1,
            4 => stale.required.connections.stamp.responses += 1,
            5 => stale.required.controls.stop.stamp += 1,
            _ => stale.required.controls.compaction.stamp += 1,
        }
        assert_eq!(
            fixture
                .service
                .try_validate_shutdown_runtime(&fixture.sessions, &stale),
            Err(RuntimeWorkError::Stale)
        );
    }
    permit.commit(|| ()).unwrap();
}

#[test]
fn runtime_validation_detects_work_acquired_and_released_after_capture() {
    let fixture = Fixture::idle();
    let expected = revision(&fixture);
    drop(fixture.acquired_projection_flight(fixture.thread));
    assert_eq!(
        fixture
            .service
            .try_validate_shutdown_runtime(&fixture.sessions, &expected),
        Err(RuntimeWorkError::Stale)
    );
    let expected = revision(&fixture);
    drop(
        fixture
            .service
            .stop_coordinator
            .compaction_custody
            .reserve_continuation(fixture.thread, fixture.turn)
            .unwrap(),
    );
    assert_eq!(
        fixture
            .service
            .try_validate_shutdown_runtime(&fixture.sessions, &expected),
        Err(RuntimeWorkError::Stale)
    );
}

#[test]
fn runtime_validation_refuses_busy_registry_without_mutation() {
    let fixture = Fixture::idle();
    let expected = revision(&fixture);
    let permit = fixture.gate.execution_permit();
    let (send, receive) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        let held = fixture.service.connections.lock().unwrap();
        let reader = scope.spawn(|| {
            let result = fixture
                .service
                .try_validate_shutdown_runtime(&fixture.sessions, &expected);
            send.send(result).unwrap();
        });
        let result = receive.recv_timeout(std::time::Duration::from_secs(2));
        drop(held);
        reader.join().unwrap();
        assert_eq!(result.unwrap(), Err(RuntimeWorkError::Busy));
    });
    assert_eq!(revision(&fixture), expected);
    permit.commit(|| ()).unwrap();
}
