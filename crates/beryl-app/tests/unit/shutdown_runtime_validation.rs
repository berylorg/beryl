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

#[test]
fn retained_runtime_guards_exclude_readers_and_release_without_changing_authority() {
    let fixture = Fixture::idle();
    let expected = revision(&fixture);
    let permit = fixture.gate.execution_permit();
    let compaction = fixture.service.context_compaction.as_ref().unwrap();
    let sessions = fixture
        .sessions
        .try_hold_work_revision(&expected.required.sessions)
        .unwrap();
    let controls = compaction
        .try_hold_control_revisions(
            &fixture.service.stop_coordinator,
            expected.required.controls.stop.stamp,
            expected.required.controls.compaction.stamp,
        )
        .unwrap();
    let flights = FlightRegistry::try_hold_work_revision(expected.flights).unwrap();
    let loaded = registry::try_hold_work_revision(expected.loaded).unwrap();
    let commands = fixture
        .service
        .command_authorizer
        .try_hold_work_open()
        .unwrap();
    std::thread::scope(|scope| {
        scope
            .spawn(|| {
                assert_eq!(
                    fixture.sessions.try_work_revision(),
                    Err(RuntimeWorkError::Busy)
                );
                assert_eq!(
                    fixture.service.stop_coordinator.try_work_revision(),
                    Err(RuntimeWorkError::Busy)
                );
                assert_eq!(compaction.try_work_revision(), Err(RuntimeWorkError::Busy));
                assert_eq!(
                    FlightRegistry::try_work_revision(),
                    Err(RuntimeWorkError::Busy)
                );
                assert_eq!(registry::try_work_revision(), Err(RuntimeWorkError::Busy));
                assert_eq!(
                    fixture.service.command_authorizer.try_check_work_open(),
                    Err(RuntimeWorkError::Busy)
                );
            })
            .join()
            .unwrap();
    });
    drop((commands, loaded, flights, controls, sessions));
    assert_eq!(revision(&fixture), expected);
    permit.commit(|| ()).unwrap();
}

#[test]
fn retained_runtime_guards_refuse_stale_foreign_and_partial_acquisition_without_changes() {
    let fixture = Fixture::idle();
    let foreign = Fixture::idle();
    let expected = revision(&fixture);
    let permit = fixture.gate.execution_permit();
    let compaction = fixture.service.context_compaction.as_ref().unwrap();
    assert_eq!(
        foreign
            .sessions
            .try_hold_work_revision(&expected.required.sessions)
            .err(),
        Some(RuntimeWorkError::Foreign)
    );
    assert_eq!(
        compaction
            .try_hold_control_revisions(
                &foreign.service.stop_coordinator,
                expected.required.controls.stop.stamp,
                expected.required.controls.compaction.stamp,
            )
            .err(),
        Some(RuntimeWorkError::Foreign)
    );
    assert_eq!(
        fixture
            .service
            .stop_coordinator
            .try_hold_work_revision(expected.required.controls.stop.stamp + 1)
            .err(),
        Some(RuntimeWorkError::Stale)
    );
    assert_eq!(
        FlightRegistry::try_hold_work_revision(expected.flights + 1).err(),
        Some(RuntimeWorkError::Stale)
    );
    assert_eq!(
        registry::try_hold_work_revision(expected.loaded + 1).err(),
        Some(RuntimeWorkError::Stale)
    );
    // A later source refusal must release the already acquired stop and operations guards.
    assert_eq!(
        compaction
            .try_hold_control_revisions(
                &fixture.service.stop_coordinator,
                expected.required.controls.stop.stamp,
                expected.required.controls.compaction.stamp + 1,
            )
            .err(),
        Some(RuntimeWorkError::Stale)
    );
    let controls = compaction
        .try_hold_control_revisions(
            &fixture.service.stop_coordinator,
            expected.required.controls.stop.stamp,
            expected.required.controls.compaction.stamp,
        )
        .unwrap();
    drop(controls);
    let held = fixture
        .service
        .command_authorizer
        .try_hold_work_open()
        .unwrap();
    assert_eq!(
        fixture
            .sessions
            .try_hold_work_revision(&expected.required.sessions)
            .err(),
        Some(RuntimeWorkError::Busy)
    );
    drop(held);
    drop(
        fixture
            .sessions
            .try_hold_work_revision(&expected.required.sessions)
            .unwrap(),
    );
    assert_eq!(revision(&fixture), expected);
    permit.commit(|| ()).unwrap();
}

fn poison_retained_guard(guard: impl Sized) {
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _held = guard;
            panic!("inject retained runtime guard unwind");
        }))
        .is_err()
    );
}

#[test]
fn retained_control_guards_cover_operations_and_release_partial_acquisitions() {
    let fixture = Fixture::idle();
    let permit = fixture.gate.execution_permit();
    fixture
        .service
        .context_compaction
        .as_ref()
        .unwrap()
        .verify_control_guard_exclusion_for_test();
    permit.commit(|| ()).unwrap();
}

#[test]
fn retained_runtime_guards_refuse_poison_without_fencing_process() {
    let fixture = Fixture::idle();
    let expected = revision(&fixture);
    let permit = fixture.gate.execution_permit();
    poison_retained_guard(
        fixture
            .sessions
            .try_hold_work_revision(&expected.required.sessions)
            .unwrap(),
    );
    assert_eq!(
        fixture
            .sessions
            .try_hold_work_revision(&expected.required.sessions)
            .err(),
        Some(RuntimeWorkError::Unavailable)
    );
    let compaction = fixture.service.context_compaction.as_ref().unwrap();
    poison_retained_guard(
        compaction
            .try_hold_control_revisions(
                &fixture.service.stop_coordinator,
                expected.required.controls.stop.stamp,
                expected.required.controls.compaction.stamp,
            )
            .unwrap(),
    );
    assert_eq!(
        compaction
            .try_hold_control_revisions(
                &fixture.service.stop_coordinator,
                expected.required.controls.stop.stamp,
                expected.required.controls.compaction.stamp,
            )
            .err(),
        Some(RuntimeWorkError::Unavailable)
    );
    poison_retained_guard(FlightRegistry::try_hold_work_revision(expected.flights).unwrap());
    assert_eq!(
        FlightRegistry::try_hold_work_revision(expected.flights).err(),
        Some(RuntimeWorkError::Unavailable)
    );
    poison_retained_guard(registry::try_hold_work_revision(expected.loaded).unwrap());
    assert_eq!(
        registry::try_hold_work_revision(expected.loaded).err(),
        Some(RuntimeWorkError::Unavailable)
    );
    poison_retained_guard(
        fixture
            .service
            .command_authorizer
            .try_hold_work_open()
            .unwrap(),
    );
    assert_eq!(
        fixture
            .service
            .command_authorizer
            .try_hold_work_open()
            .err(),
        Some(RuntimeWorkError::Unavailable)
    );
    permit.commit(|| ()).unwrap();
}
