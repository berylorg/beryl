use super::*;
use crate::cas_projection::ShutdownWorkObservation;
use crate::process_admission::ProcessAdmissionError;

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
