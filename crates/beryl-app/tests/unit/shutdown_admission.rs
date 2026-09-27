use super::*;
include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/unit/shutdown_support.rs"
));

fn observe(fixture: &Fixture) -> ShutdownWorkObservation {
    fixture
        .service
        .observe_shutdown_work(&fixture.sessions, &ProjectionCancellationToken::new())
        .unwrap()
}

#[test]
fn window_admission_refreshes_only_work_evidence_after_stale_observation() {
    use crate::window_acquisition::RuntimeBackedWindowProcessRegistry;
    let fixture = Fixture::idle();
    let id = beryl_model::WindowId::from_bytes([1; 16]);
    let registry = RuntimeBackedWindowProcessRegistry::new(fixture.gate.clone());
    let _resident = registry.reserve_main_window(id).unwrap();
    let lease = registry
        .admit_close(registry.snapshot_for_close(&[id]).unwrap())
        .unwrap();
    let window = registry
        .prepare_shutdown_admission(&lease, id, true)
        .unwrap();
    let permit = fixture.gate.execution_permit();
    let stale = observe(&fixture);
    drop(fixture.service.connection_work_boundary().begin_change());
    assert!(matches!(
        fixture.service.try_admit_observed_shutdown_with_window(
            &fixture.sessions,
            &stale,
            Some(window)
        ),
        Err(ShutdownWorkError::Runtime(RuntimeWorkError::Stale))
    ));
    permit.commit(|| ()).unwrap();
    assert_eq!(lease.is_final(id), Ok(true));
    let fresh = observe(&fixture);
    let fence = fixture
        .service
        .try_admit_observed_shutdown_with_window(&fixture.sessions, &fresh, Some(window))
        .unwrap();
    assert_eq!(
        permit.commit(|| ()),
        Err(crate::process_admission::ProcessAdmissionError::Fenced)
    );
    fence.reopen_if(true).unwrap();
    assert_eq!(lease.is_final(id), Ok(true));
}

#[test]
fn observed_shutdown_admits_idle_and_pending_work_and_invalidates_old_permits() {
    for pending in [false, true] {
        let fixture = Fixture::with_pending(pending);
        let mut observation = observe(&fixture);
        assert_eq!(observation.has_work(), pending);
        let permit = fixture.gate.execution_permit();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let fence = loop {
            match fixture
                .service
                .try_admit_observed_shutdown(&fixture.sessions, &observation)
            {
                Ok(fence) => break fence,
                Err(ShutdownWorkError::Runtime(RuntimeWorkError::Busy)) => {
                    permit.commit(|| ()).unwrap();
                    assert!(std::time::Instant::now() < deadline, "runtime stayed busy");
                    std::thread::yield_now();
                    observation = observe(&fixture);
                    assert_eq!(observation.has_work(), pending);
                }
                other => panic!("unexpected admission result: {other:?}"),
            }
        };
        assert_eq!(
            permit.commit(|| ()),
            Err(crate::process_admission::ProcessAdmissionError::Fenced)
        );
        fence.reopen_if(true).unwrap();
        assert_eq!(
            permit.commit(|| ()),
            Err(crate::process_admission::ProcessAdmissionError::Stale)
        );
        assert_eq!(observe(&fixture).has_work(), pending);
    }
}

#[test]
fn observed_shutdown_refuses_foreign_sources_and_interval_proofs() {
    let fixture = Fixture::idle();
    let foreign = Fixture::idle();
    let observation = observe(&fixture);
    let other = observe(&foreign);
    let permit = fixture.gate.execution_permit();
    assert!(matches!(
        fixture
            .service
            .try_admit_observed_shutdown(&fixture.sessions, &other),
        Err(ShutdownWorkError::Runtime(RuntimeWorkError::Foreign))
    ));
    assert!(matches!(
        fixture
            .service
            .try_admit_observed_shutdown(&foreign.sessions, &observation),
        Err(ShutdownWorkError::Runtime(RuntimeWorkError::Foreign))
    ));
    let mut mixed = observation.clone();
    mixed.connection_interval = other.connection_interval;
    assert!(matches!(
        fixture
            .service
            .try_admit_observed_shutdown(&fixture.sessions, &mixed),
        Err(ShutdownWorkError::Runtime(RuntimeWorkError::Foreign))
    ));
    let mut mixed = observation;
    mixed.home_interval = other.home_interval;
    assert!(matches!(
        fixture
            .service
            .try_admit_observed_shutdown(&fixture.sessions, &mixed),
        Err(ShutdownWorkError::Home(
            beryl_home_store::HomeObservedCoherenceError::ForeignObservation
        ))
    ));
    permit.commit(|| ()).unwrap();
}

#[test]
fn observed_shutdown_refuses_runtime_changes_and_released_connection_activity() {
    let fixture = Fixture::idle();
    let permit = fixture.gate.execution_permit();
    let observation = observe(&fixture);
    drop(fixture.acquired_projection_flight(fixture.thread));
    assert!(matches!(
        fixture
            .service
            .try_admit_observed_shutdown(&fixture.sessions, &observation),
        Err(ShutdownWorkError::Runtime(RuntimeWorkError::Stale))
    ));
    let observation = observe(&fixture);
    drop(fixture.service.connection_work_boundary().begin_change());
    assert!(matches!(
        fixture
            .service
            .try_admit_observed_shutdown(&fixture.sessions, &observation),
        Err(ShutdownWorkError::Runtime(RuntimeWorkError::Stale))
    ));
    permit.commit(|| ()).unwrap();
    let fresh = observe(&fixture);
    fixture
        .service
        .try_admit_observed_shutdown(&fixture.sessions, &fresh)
        .unwrap()
        .reopen_if(true)
        .unwrap();
}

#[test]
fn observed_shutdown_refuses_durable_changes_without_mutating_execution_or_pending_work() {
    let fixture = Fixture::idle();
    let observation = observe(&fixture);
    let permit = fixture.gate.execution_permit();
    submission_fixture::submit_atoms(
        fixture.service.home.as_deref().unwrap(),
        fixture.service.storage.clone(),
        fixture.assets.clone(),
        fixture.thread,
        SyndicDraftId::from_bytes([81; 16]),
        SyndicItemId::from_bytes([82; 16]),
        &[submission_fixture::Atom::Text(
            "work admitted before shutdown",
        )],
        83,
        SyndicTimestamp::from_unix_millis(4),
    );
    assert!(matches!(
        fixture
            .service
            .try_admit_observed_shutdown(&fixture.sessions, &observation),
        Err(ShutdownWorkError::Home(
            beryl_home_store::HomeObservedCoherenceError::Observation(
                beryl_home_store::HomeMutationObservationError::Stale
            )
        ))
    ));
    permit.commit(|| ()).unwrap();
    assert!(observe(&fixture).has_work());
}

#[test]
fn observed_shutdown_busy_sources_release_every_earlier_guard() {
    let fixture = Fixture::idle();
    let observation = observe(&fixture);
    let permit = fixture.gate.execution_permit();
    let check = || {
        assert!(matches!(
            fixture
                .service
                .try_admit_observed_shutdown(&fixture.sessions, &observation),
            Err(ShutdownWorkError::Runtime(RuntimeWorkError::Busy))
        ));
        permit.commit(|| ()).unwrap();
    };
    let sessions = fixture
        .sessions
        .try_hold_work_revision(&observation.revision.required.sessions)
        .unwrap();
    check();
    drop(sessions);
    let compaction = fixture.service.context_compaction.as_ref().unwrap();
    let controls = compaction
        .try_hold_control_revisions(
            &fixture.service.stop_coordinator,
            observation.revision.required.controls.stop.stamp,
            observation.revision.required.controls.compaction.stamp,
        )
        .unwrap();
    check();
    drop(controls);
    let flights = FlightRegistry::try_hold_work_revision(observation.revision.flights).unwrap();
    check();
    drop(flights);
    let loaded = registry::try_hold_work_revision(observation.revision.loaded).unwrap();
    check();
    drop(loaded);
    let commands = fixture
        .service
        .command_authorizer
        .try_hold_work_open()
        .unwrap();
    check();
    drop(commands);
    let change = fixture.service.connection_work_boundary().begin_change();
    check();
    drop(change);
    let fresh = observe(&fixture);
    fixture
        .service
        .try_admit_observed_shutdown(&fixture.sessions, &fresh)
        .unwrap()
        .reopen_if(true)
        .unwrap();
}

#[test]
fn observed_shutdown_refuses_unsettled_and_poisoned_authority() {
    let fixture = Fixture::idle();
    let observation = observe(&fixture);
    let permit = fixture.gate.execution_permit();
    let reservation = permit.reserve().unwrap();
    assert!(matches!(
        fixture
            .service
            .try_admit_observed_shutdown(&fixture.sessions, &observation),
        Err(ShutdownWorkError::Admission(
            crate::process_admission::ProcessAdmissionError::Unsettled
        ))
    ));
    drop(reservation);
    fixture.service.command_gate.poison_for_test();
    assert!(matches!(
        fixture
            .service
            .try_admit_observed_shutdown(&fixture.sessions, &observation),
        Err(ShutdownWorkError::Runtime(RuntimeWorkError::Unavailable))
    ));
    permit.commit(|| ()).unwrap();
}

#[test]
fn observed_shutdown_home_contention_refuses_without_waiting_or_fencing() {
    let fixture = Fixture::idle();
    let observation = observe(&fixture);
    let permit = fixture.gate.execution_permit();
    let (held_tx, held_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        let fixture = &fixture;
        let observation = &observation;
        let holder = scope.spawn(move || {
            fixture
                .service
                .home
                .as_deref()
                .unwrap()
                .try_elect_observed_coherent(
                    &observation.home_interval,
                    fixture.service.home_generation,
                    || {
                        held_tx.send(()).unwrap();
                        release_rx
                            .recv_timeout(std::time::Duration::from_secs(5))
                            .unwrap();
                    },
                )
                .unwrap();
        });
        held_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        let result = fixture
            .service
            .try_admit_observed_shutdown(&fixture.sessions, &observation);
        release_tx.send(()).unwrap();
        holder.join().unwrap();
        assert!(matches!(
            result,
            Err(ShutdownWorkError::Home(
                beryl_home_store::HomeObservedCoherenceError::Observation(
                    beryl_home_store::HomeMutationObservationError::Busy
                )
            ))
        ));
    });
    permit.commit(|| ()).unwrap();
    fixture
        .service
        .try_admit_observed_shutdown(&fixture.sessions, &observation)
        .unwrap()
        .reopen_if(true)
        .unwrap();
}

#[test]
fn observed_shutdown_and_connection_change_have_one_exact_publication_order() {
    let fixture = Fixture::idle();
    for _ in 0..32 {
        let observation = observe(&fixture);
        let permit = fixture.gate.execution_permit();
        let start = std::sync::Barrier::new(2);
        let result = std::thread::scope(|scope| {
            let change = scope.spawn(|| {
                start.wait();
                drop(fixture.service.connection_work_boundary().begin_change());
            });
            start.wait();
            let result = fixture
                .service
                .try_admit_observed_shutdown(&fixture.sessions, &observation);
            change.join().unwrap();
            result
        });
        match result {
            Ok(fence) => {
                assert_eq!(
                    permit.commit(|| ()),
                    Err(crate::process_admission::ProcessAdmissionError::Fenced)
                );
                fence.reopen_if(true).unwrap();
            }
            Err(ShutdownWorkError::Runtime(RuntimeWorkError::Busy | RuntimeWorkError::Stale)) => {
                permit.commit(|| ()).unwrap();
            }
            other => panic!("unexpected connection/admission race result: {other:?}"),
        }
    }
}

#[test]
fn observed_shutdown_performs_no_storage_read_and_refuses_failed_home_coherence() {
    let mut fixture = Fixture::idle();
    let scheduler = fixture.service.scheduler.take().unwrap();
    scheduler.request_shutdown();
    assert!(matches!(
        scheduler.join().unwrap(),
        crate::cas_projection::accepted_input_scheduler::AcceptedInputSchedulerExit::Clean
    ));
    let observation = observe(&fixture);
    fixture
        .faults
        .fail_next(beryl_home_store::test_faults::FaultPoint::BeforeReadConfirmation);
    fixture
        .service
        .try_admit_observed_shutdown(&fixture.sessions, &observation)
        .unwrap()
        .reopen_if(true)
        .unwrap();
    let permit = fixture.gate.execution_permit();
    assert!(
        fixture
            .service
            .home
            .as_deref()
            .unwrap()
            .home_revision()
            .is_err()
    );
    assert!(matches!(
        fixture
            .service
            .try_admit_observed_shutdown(&fixture.sessions, &observation),
        Err(ShutdownWorkError::Home(
            beryl_home_store::HomeObservedCoherenceError::Coherence(
                beryl_home_store::HomeCoherenceError::Unhealthy(
                    beryl_home_store::HomeHealthState::Failed
                )
            )
        ))
    ));
    permit.commit(|| ()).unwrap();
}
