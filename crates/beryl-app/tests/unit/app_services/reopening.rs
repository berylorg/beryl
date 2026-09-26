use super::*;

pub(super) fn candidate_at(
    directory: &tempfile::TempDir,
) -> (HomeOpenPublication, BerylState, SyndicStorage) {
    let mut candidate = HomeOpenCandidate::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let state = BerylState::register(&mut candidate).unwrap();
    let syndic = SyndicStorage::register(&mut candidate).unwrap();
    let candidate = candidate
        .prepare_publication(
            BerylState::required_domains()
                .unwrap()
                .merge(SyndicStorage::required_domains().unwrap())
                .unwrap(),
        )
        .unwrap();
    (candidate, state, syndic)
}

fn reopen(
    owner: &mut ProcessServiceOwner,
    directory: &tempfile::TempDir,
) -> Result<(), AppServiceOpenFailure> {
    let (candidate, state, syndic) = candidate_at(directory);
    owner.open_initial(
        candidate,
        state,
        syndic,
        configuration(),
        SyndicTimestamp::from_unix_millis(2),
        CommandCancellation::new(),
    )
}

fn assert_fresh_graph_works(owner: &mut ProcessServiceOwner) {
    owner.process.execution_permit().commit(|| ()).unwrap();
    let graph = owner.graph_mut().unwrap();
    let revision = graph.state().settings().revision(graph.home()).unwrap();
    graph.load_theme(revision, None).unwrap();
    assert!(graph.theme().unwrap().current().is_some());
    assert!(
        graph
            .restored_window_attempt()
            .unwrap()
            .validate_lifetime()
            .is_ok()
    );
}

#[test]
fn retired_initial_failures_admit_fresh_services_without_reviving_old_permits() {
    for failure_kind in 0..5 {
        let (directory, candidate, state, syndic, faults) = fixture();
        let old_reference = candidate.service_reference();
        let mut owner = owner(&candidate);
        let process = owner.process.clone();
        let old_permit = process.execution_permit();
        let cancellation = CommandCancellation::new();
        match failure_kind {
            0 => cancellation.cancel(),
            1 => faults.fail_next(FaultPoint::BeforeThemeWatchSpawn),
            2 => {
                let cancellation = cancellation.clone();
                owner.test_before_initial_publication(move |_, _, _| cancellation.cancel());
            }
            3 => owner.test_cancel_initial_worker_release(),
            4 => owner.test_before_initial_publication(move |candidate, _, syndic| {
                faults.fail_next(FaultPoint::BeforeReadConfirmation);
                let access = candidate.recovery_access().unwrap();
                assert!(syndic.revision_candidate(&access).is_err());
            }),
            _ => unreachable!(),
        }
        let failure = owner
            .open_initial(
                candidate,
                state,
                syndic,
                configuration(),
                SyndicTimestamp::from_unix_millis(1),
                cancellation,
            )
            .unwrap_err();
        assert!(failure.rejected_candidate.is_none());
        assert!(owner.retained_close().is_none());
        assert!(old_permit.commit(|| ()).is_err());
        reopen(&mut owner, &directory).unwrap();
        assert_fresh_graph_works(&mut owner);
        process.execution_permit().commit(|| ()).unwrap();
        assert!(old_permit.commit(|| ()).is_err());
        assert!(old_reference.home_revision().is_err());
        close(&mut owner);
    }
}

#[test]
fn complete_shutdown_reopens_same_process_but_never_old_restore_sources() {
    let (directory, candidate, state, syndic, _) = fixture();
    let old_reference = candidate.service_reference();
    let mut owner = owner(&candidate);
    let process = owner.process.clone();
    let old_permit = process.execution_permit();
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
    let restore = owner.graph().unwrap().restored_window_attempt().unwrap();
    let old_marker = owner.graph().unwrap().marker();
    assert!(matches!(
        owner.finish_shutdown(),
        Err(AppServiceCloseError::NotReady)
    ));
    assert!(owner.graph().is_some());
    close(&mut owner);
    reopen(&mut owner, &directory).unwrap();
    assert_fresh_graph_works(&mut owner);
    assert!(old_permit.commit(|| ()).is_err());
    assert!(old_reference.home_revision().is_err());
    assert!(restore.validate_lifetime().is_err());
    assert!(old_marker.dispose(owner.graph().unwrap().home()).is_err());
    process.execution_permit().commit(|| ()).unwrap();
    close(&mut owner);
    reopen(&mut owner, &directory).unwrap();
    assert_fresh_graph_works(&mut owner);
    close(&mut owner);
}

#[test]
fn outstanding_admission_rejects_retry_and_returns_the_original_candidate() {
    let (directory, candidate, state, syndic, _) = fixture();
    let mut owner = owner(&candidate);
    let reservation = owner.process.execution_permit().reserve().unwrap();
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    owner
        .open_initial(
            candidate,
            state,
            syndic,
            configuration(),
            SyndicTimestamp::from_unix_millis(1),
            cancellation,
        )
        .unwrap_err();
    let rejected = reopen(&mut owner, &directory).unwrap_err();
    assert!(matches!(
        rejected.error,
        AppServiceOpenError::Reopening(crate::process_admission::ProcessAdmissionError::Unsettled)
    ));
    assert!(owner.graph().is_none());
    let candidate = rejected
        .rejected_candidate
        .expect("original candidate returned");
    assert_eq!(candidate.home_id(), owner.home_id);
    candidate.close().unwrap();
    drop(reservation);
    reopen(&mut owner, &directory).unwrap();
    assert_fresh_graph_works(&mut owner);
    close(&mut owner);
}

#[test]
fn stale_retirement_fence_is_never_replaced_by_a_newer_fence() {
    let (directory, candidate, state, syndic, _) = fixture();
    let mut owner = owner(&candidate);
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    owner
        .open_initial(
            candidate,
            state,
            syndic,
            configuration(),
            SyndicTimestamp::from_unix_millis(1),
            cancellation,
        )
        .unwrap_err();
    owner.process.fence().unwrap().reopen_if(true).unwrap();
    let newer = owner.process.fence().unwrap();
    for _ in 0..2 {
        let rejected = reopen(&mut owner, &directory).unwrap_err();
        assert!(matches!(
            rejected.error,
            AppServiceOpenError::Reopening(crate::process_admission::ProcessAdmissionError::Stale)
        ));
        rejected.rejected_candidate.unwrap().close().unwrap();
        assert!(owner.graph().is_none());
    }
    newer.reopen_if(true).unwrap();
}

#[test]
fn consumed_shutdown_failure_never_authorizes_a_fresh_attempt() {
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
    owner.begin_shutdown().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match owner
            .poll_shutdown(&ProjectionCancellationToken::new())
            .unwrap()
        {
            AppServiceShutdownProgress::Ready => break,
            AppServiceShutdownProgress::Waiting => {
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(10));
            }
            other => panic!("unexpected shutdown: {other:?}"),
        }
    }
    owner.test_fail_shutdown_completion();
    assert!(matches!(
        owner.finish_shutdown(),
        Err(AppServiceCloseError::PersistentFailure)
    ));
    assert!(owner.graph().is_none());
    assert!(owner.retained_close().is_none());
    let rejected = reopen(&mut owner, &directory).unwrap_err();
    assert!(matches!(
        rejected.error,
        AppServiceOpenError::AlreadyInstalled
    ));
    rejected.rejected_candidate.unwrap().close().unwrap();
}
