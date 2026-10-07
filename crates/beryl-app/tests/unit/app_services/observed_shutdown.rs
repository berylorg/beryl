use super::*;
use crate::cas_projection::ShutdownWorkObservation;
use crate::process_admission::ProcessAdmissionError;

#[test]
fn window_shutdown_preserves_original_lease_across_work_refresh_and_admits_exact_mode() {
    use crate::cas_projection::{ShutdownCoordinatorError, ShutdownWorkError};
    use crate::window_acquisition::{
        RuntimeBackedWindowProcessRegistry, WindowCloseAdmissionError,
    };
    use beryl_model::WindowId;

    for count in [1, 2] {
        let (directory, mut owner) = running();
        let ids: Vec<_> = (1..=count)
            .map(|value| WindowId::from_bytes([value; 16]))
            .collect();
        let residents: Vec<_> = ids
            .iter()
            .map(|id| owner.windows.reserve_main_window(*id).unwrap())
            .collect();
        let lease = owner
            .windows
            .admit_close(owner.windows.snapshot_for_close(&ids).unwrap())
            .unwrap();
        let permit = owner.process.execution_permit();
        let restore = owner.graph().unwrap().restored_window_attempt().unwrap();
        let observation = observe(&owner);
        let foreign = RuntimeBackedWindowProcessRegistry::new(owner.process.clone());
        let foreign_resident = foreign.reserve_main_window(ids[0]).unwrap();
        let foreign_lease = foreign
            .admit_close(foreign.snapshot_for_close(&[ids[0]]).unwrap())
            .unwrap();
        assert!(matches!(
            owner.try_begin_observed_window_shutdown(&observation, &foreign_lease, ids[0], false),
            Err(AppServiceCloseError::Work(ShutdownWorkError::Window(
                WindowCloseAdmissionError::WindowSetChanged
            )))
        ));
        drop(foreign_lease);
        drop(foreign_resident);
        if count == 2 {
            assert!(matches!(
                owner.try_begin_observed_window_shutdown(&observation, &lease, ids[0], true),
                Err(AppServiceCloseError::Coordinator(
                    ShutdownCoordinatorError::Work(ShutdownWorkError::Window(
                        WindowCloseAdmissionError::WindowSetChanged
                    ))
                ))
            ));
        }
        let unsettled = owner.process.execution_permit().reserve().unwrap();
        assert!(
            owner
                .try_begin_observed_window_shutdown(&observation, &lease, ids[0], count == 1)
                .is_err()
        );
        assert!(owner.graph().unwrap().shutdown.is_none());
        permit.commit(|| ()).unwrap();
        restore.validate_lifetime().unwrap();
        assert_eq!(lease.is_final(ids[0]), Ok(count == 1));
        drop(unsettled);
        let deadline = Instant::now() + Duration::from_secs(5);
        let admitted = loop {
            let fresh = observe(&owner);
            match owner.try_begin_observed_window_shutdown(&fresh, &lease, ids[0], count == 1) {
                Ok(()) => break fresh,
                Err(error) => {
                    assert!(owner.graph().unwrap().shutdown.is_none());
                    permit.commit(|| ()).unwrap();
                    restore.validate_lifetime().unwrap();
                    assert!(
                        Instant::now() < deadline,
                        "window admission did not settle: {error}"
                    );
                    std::thread::yield_now();
                }
            }
        };
        assert_eq!(permit.commit(|| ()), Err(ProcessAdmissionError::Fenced));
        assert!(restore.validate_lifetime().is_err());
        let attempt = owner.graph().unwrap().shutdown;
        assert!(attempt.is_some());
        assert!(matches!(
            owner.try_begin_observed_window_shutdown(&admitted, &lease, ids[0], count == 1),
            Err(AppServiceCloseError::AlreadyShuttingDown)
        ));
        assert_eq!(owner.graph().unwrap().shutdown, attempt);
        ready(&mut owner);
        drop(residents);
        drop(lease);
        owner.finish_shutdown().unwrap();
        assert_reopens(&directory);
    }
}

#[test]
fn released_window_custody_refuses_graph_shutdown_without_retiring_restore_authority() {
    use beryl_model::WindowId;
    let (_directory, mut owner) = running();
    let id = WindowId::from_bytes([1; 16]);
    let resident = owner.windows.reserve_main_window(id).unwrap();
    let lease = owner
        .windows
        .admit_close(owner.windows.snapshot_for_close(&[id]).unwrap())
        .unwrap();
    let observation = observe(&owner);
    let restore = owner.graph().unwrap().restored_window_attempt().unwrap();
    let permit = owner.process.execution_permit();
    drop(resident);
    assert!(matches!(
        owner.try_begin_observed_window_shutdown(&observation, &lease, id, true),
        Err(AppServiceCloseError::Coordinator(
            crate::cas_projection::ShutdownCoordinatorError::Work(
                crate::cas_projection::ShutdownWorkError::Window(
                    crate::window_acquisition::WindowCloseAdmissionError::WindowSetChanged
                )
            )
        ))
    ));
    assert!(owner.graph().unwrap().shutdown.is_none());
    restore.validate_lifetime().unwrap();
    permit.commit(|| ()).unwrap();
    drop(lease);
    close(&mut owner);
}

fn running() -> (tempfile::TempDir, ProcessServiceOwner) {
    let (directory, candidate, state, syndic, _) = fixture();
    let mut owner = owner(&candidate);
    owner
        .open_initial(
            candidate,
            state,
            syndic,
            configuration(),
            SyndicTimestamp::from_unix_millis(1),
            CommandCancellation::new(),
        )
        .unwrap();
    (directory, owner)
}

fn observe(owner: &ProcessServiceOwner) -> ShutdownWorkObservation {
    let job = owner.prepare_shutdown_observation().unwrap();
    std::thread::spawn(move || job.collect(&ProjectionCancellationToken::new()))
        .join()
        .unwrap()
        .unwrap()
}

fn admit(owner: &mut ProcessServiceOwner) -> ShutdownWorkObservation {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let observation = observe(owner);
        match owner.try_begin_observed_shutdown(&observation) {
            Ok(()) => return observation,
            Err(error) => {
                assert!(owner.graph().unwrap().shutdown.is_none());
                owner.process.execution_permit().commit(|| ()).unwrap();
                assert!(
                    Instant::now() < deadline,
                    "admission did not settle: {error}"
                );
                std::thread::yield_now();
            }
        }
    }
}

fn ready(owner: &mut ProcessServiceOwner) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match owner
            .poll_shutdown(&ProjectionCancellationToken::new())
            .unwrap()
        {
            AppServiceShutdownProgress::Ready => return,
            AppServiceShutdownProgress::Waiting => {
                assert!(Instant::now() < deadline, "shutdown did not settle");
                std::thread::yield_now();
            }
            other => panic!("unexpected shutdown progress: {other:?}"),
        }
    }
}

#[test]
fn consuming_shutdown_retains_original_runtime_cleanup_and_home_until_retry() {
    let (directory, mut owner) = running();
    let home = owner.graph().unwrap().home().service_reference();
    let (probe, interest) = super::runtime_cleanup_support::failed_runtime(&owner);
    owner.begin_shutdown().unwrap();
    ready(&mut owner);
    assert!(matches!(
        owner.finish_shutdown(),
        Err(AppServiceFinalizationError::Consumed(
            AppServiceCloseError::CasClosePending
        ))
    ));
    assert!(owner.graph().is_none());
    assert!(owner.closing_graph.is_some());
    assert!(owner.failed_cas_close.is_some());
    assert!(!owner.initial_attempt_is_settled());
    assert!(owner.process.execution_permit().commit(|| ()).is_err());
    assert!(home.home_revision().is_ok());
    assert!(
        HomeOpenCandidate::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT
        ))
        .is_err()
    );
    let first_retirements = probe.counts().1;
    assert!(first_retirements >= 2);
    assert!(matches!(
        owner.finish_shutdown(),
        Err(AppServiceFinalizationError::Consumed(
            AppServiceCloseError::CasClosePending
        ))
    ));
    assert_eq!(probe.counts().0, 1);
    assert_eq!(probe.counts().1, first_retirements + 1);
    probe.complete_retirement();
    owner.finish_shutdown().unwrap();
    assert!(owner.closing_graph.is_none());
    assert!(owner.failed_cas_close.is_none());
    assert!(owner.initial_attempt_is_settled());
    assert_eq!(probe.counts().0, 1);
    assert_eq!(probe.counts().1, first_retirements + 2);
    assert!(home.home_revision().is_err());
    drop(interest);
    let (candidate, state, syndic) = super::reopening::candidate_at(&directory);
    owner
        .open_initial(
            candidate,
            state,
            syndic,
            configuration(),
            SyndicTimestamp::from_unix_millis(2),
            CommandCancellation::new(),
        )
        .unwrap();
    assert!(owner.graph().is_some());
    owner.process.execution_permit().commit(|| ()).unwrap();
    assert_eq!(probe.counts().0, 1);
    close(&mut owner);
    assert_reopens(&directory);
}

#[test]
fn observed_shutdown_retires_restore_authority_and_duplicate_requests_preserve_readiness() {
    let (directory, mut owner) = running();
    let restore = owner.graph().unwrap().restored_window_attempt().unwrap();
    let permit = owner.process.execution_permit();
    let observation = admit(&mut owner);
    assert!(!observation.has_work());
    assert!(restore.validate_lifetime().is_err());
    assert_eq!(permit.commit(|| ()), Err(ProcessAdmissionError::Fenced));
    assert!(owner.graph().unwrap().restored_window_attempt().is_err());
    let attempt = owner.graph().unwrap().shutdown;
    ready(&mut owner);
    assert!(matches!(
        owner.try_begin_observed_shutdown(&observation),
        Err(AppServiceCloseError::AlreadyShuttingDown)
    ));
    assert!(matches!(
        owner.observe_shutdown_work(&ProjectionCancellationToken::new()),
        Err(AppServiceCloseError::AlreadyShuttingDown)
    ));
    assert_eq!(owner.graph().unwrap().shutdown, attempt);
    assert!(owner.graph().unwrap().shutdown_ready);
    owner.finish_shutdown().unwrap();
    assert!(owner.graph().is_none());
    assert!(matches!(
        owner.try_begin_observed_shutdown(&observation),
        Err(AppServiceCloseError::Unavailable)
    ));
    assert!(matches!(
        owner.observe_shutdown_work(&ProjectionCancellationToken::new()),
        Err(AppServiceCloseError::Unavailable)
    ));
    assert_reopens(&directory);
}

#[test]
fn rejected_observed_admission_preserves_resident_graph_and_original_restore_lifetime() {
    let (_directory, mut owner) = running();
    let (_other_directory, mut other) = running();
    let restore = owner.graph().unwrap().restored_window_attempt().unwrap();
    let permit = owner.process.execution_permit();
    let foreign = observe(&other);
    assert!(matches!(
        owner.try_begin_observed_shutdown(&foreign),
        Err(AppServiceCloseError::Coordinator(
            crate::cas_projection::ShutdownCoordinatorError::Work(_)
        ))
    ));
    restore.validate_lifetime().unwrap();
    permit.commit(|| ()).unwrap();
    assert!(owner.graph().unwrap().shutdown.is_none());
    let observation = observe(&owner);
    let reservation = owner.process.execution_permit().reserve().unwrap();
    assert!(owner.try_begin_observed_shutdown(&observation).is_err());
    restore.validate_lifetime().unwrap();
    permit.commit(|| ()).unwrap();
    assert!(owner.graph().unwrap().shutdown.is_none());
    drop(reservation);
    close(&mut other);
    admit(&mut owner);
    ready(&mut owner);
    owner.finish_shutdown().unwrap();
}

#[test]
fn cancelled_observed_shutdown_grants_a_fresh_restore_lifetime_after_coherent_reopening() {
    let (_directory, mut owner) = running();
    let restore = owner.graph().unwrap().restored_window_attempt().unwrap();
    let permit = owner.process.execution_permit();
    let previous_job = owner.prepare_shutdown_observation().unwrap();
    admit(&mut owner);
    let cancellation = ProjectionCancellationToken::new();
    cancellation.cancel();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match owner.poll_shutdown(&cancellation).unwrap() {
            AppServiceShutdownProgress::Failed { reason, reopened } => {
                assert_eq!(reason, crate::cas_projection::ShutdownFailure::Cancelled);
                if reopened {
                    break;
                }
                assert!(owner.graph().unwrap().restored_window_attempt().is_err());
                assert!(Instant::now() < deadline, "reopening did not settle");
                std::thread::yield_now();
            }
            other => panic!("unexpected cancellation: {other:?}"),
        }
    }
    assert!(owner.graph().unwrap().shutdown.is_none());
    assert!(restore.validate_lifetime().is_err());
    assert!(matches!(
        previous_job.collect(&ProjectionCancellationToken::new()),
        Err(AppServiceCloseError::Unavailable)
    ));
    owner
        .graph()
        .unwrap()
        .restored_window_attempt()
        .unwrap()
        .validate_lifetime()
        .unwrap();
    assert_eq!(permit.commit(|| ()), Err(ProcessAdmissionError::Stale));
    owner.process.execution_permit().commit(|| ()).unwrap();
    admit(&mut owner);
    ready(&mut owner);
    owner.finish_shutdown().unwrap();
}

#[test]
fn prepared_observation_does_not_retain_a_retired_graph_or_home() {
    let (directory, mut owner) = running();
    let job = owner.prepare_shutdown_observation().unwrap();
    close(&mut owner);
    assert_reopens(&directory);
    assert!(matches!(
        job.collect(&ProjectionCancellationToken::new()),
        Err(AppServiceCloseError::Unavailable)
    ));
}

#[test]
fn graph_retirement_during_worker_collection_rejects_the_completed_observation() {
    let (directory, mut owner) = running();
    let job = owner.prepare_shutdown_observation().unwrap();
    let (collected, collection) = std::sync::mpsc::channel();
    let (resume, resumed) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        job.test_collect_then(&ProjectionCancellationToken::new(), || {
            collected.send(()).unwrap();
            resumed.recv_timeout(Duration::from_secs(20)).unwrap();
        })
    });
    collection.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(owner.graph().is_some());
    owner.process.execution_permit().commit(|| ()).unwrap();
    close(&mut owner);
    assert_reopens(&directory);
    resume.send(()).unwrap();
    assert!(matches!(
        worker.join().unwrap(),
        Err(AppServiceCloseError::Unavailable)
    ));
}

#[test]
fn cancelled_worker_observation_preserves_graph_and_execution_authority() {
    let (_directory, mut owner) = running();
    let job = owner.prepare_shutdown_observation().unwrap();
    let cancellation = ProjectionCancellationToken::new();
    cancellation.cancel();
    let worker = std::thread::spawn(move || job.collect(&cancellation));
    assert!(matches!(
        worker.join().unwrap(),
        Err(AppServiceCloseError::Work(_))
    ));
    owner
        .graph()
        .unwrap()
        .restored_window_attempt()
        .unwrap()
        .validate_lifetime()
        .unwrap();
    owner.process.execution_permit().commit(|| ()).unwrap();
    close(&mut owner);
}

#[test]
fn cancellation_after_worker_collection_does_not_publish_idle_evidence() {
    let (_directory, mut owner) = running();
    let job = owner.prepare_shutdown_observation().unwrap();
    let worker = std::thread::spawn(move || {
        let cancellation = ProjectionCancellationToken::new();
        job.test_collect_then(&cancellation, || cancellation.cancel())
    });
    assert!(matches!(
        worker.join().unwrap(),
        Err(AppServiceCloseError::Work(_))
    ));
    owner.process.execution_permit().commit(|| ()).unwrap();
    close(&mut owner);
}
