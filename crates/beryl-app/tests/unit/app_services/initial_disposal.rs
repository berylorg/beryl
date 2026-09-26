use super::*;
use beryl_home_store::{CommandOutcome, HomeCommand, ReconciliationResolution};
use beryl_model::{DomainRevision, WindowBounds, WindowDisplayState, WindowId, WindowPlacement};
use beryl_state::InitializeThreadlessWindow;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};

#[test]
fn cancelled_initial_preparation_explicitly_closes_before_or_after_workers_exist() {
    for before in [true, false] {
        let (directory, candidate, state, syndic, _) = fixture();
        let reference = candidate.service_reference();
        let themes = state.themes();
        let mut owner = owner(&candidate);
        let cancellation = CommandCancellation::new();
        let called = Arc::new(AtomicBool::new(false));
        if before {
            cancellation.cancel();
        } else {
            let cancellation = cancellation.clone();
            let observed = called.clone();
            owner.test_before_initial_publication(move |candidate, state, _| {
                assert_eq!(candidate.health().state(), HomeHealthState::Opening);
                assert_eq!(state.themes().diagnostics().active_subscriptions(), 1);
                observed.store(true, Ordering::SeqCst);
                cancellation.cancel();
            });
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
        assert!(matches!(failure.error, AppServiceOpenError::Cancelled));
        assert!(failure.rejected_candidate.is_none());
        assert_eq!(called.load(Ordering::SeqCst), !before);
        assert!(owner.graph().is_none());
        assert!(owner.retained_close().is_none());
        assert_eq!(themes.diagnostics().active_subscriptions(), 0);
        assert!(reference.home_revision().is_err());
        assert_reopens(&directory);
    }
}

#[test]
fn cancelled_published_worker_release_joins_services_and_closes_the_home() {
    let (directory, candidate, state, syndic, _) = fixture();
    let reference = candidate.service_reference();
    let themes = state.themes();
    let mut owner = owner(&candidate);
    owner.test_cancel_initial_worker_release();
    let failure = owner
        .open_initial(
            candidate,
            state,
            syndic,
            configuration(),
            SyndicTimestamp::from_unix_millis(1),
            CommandCancellation::new(),
        )
        .unwrap_err();
    assert!(matches!(
        failure.error,
        AppServiceOpenError::StartupCancelled
    ));
    assert!(failure.rejected_candidate.is_none());
    assert!(owner.graph().is_none());
    assert!(owner.retained_close().is_none());
    assert_eq!(themes.diagnostics().active_subscriptions(), 0);
    assert_eq!(owner.enrollments.pending_count(), 0);
    assert_eq!(owner.settlements.pending_nondispatch_count(), 0);
    assert!(reference.home_revision().is_err());
    assert_reopens(&directory);
}

#[test]
fn failed_initial_disposal_retains_original_reconciliation_and_rejects_replacement() {
    let (directory, candidate, state, syndic, faults) = fixture();
    let home_id = candidate.home_id();
    let reference = candidate.service_reference();
    let themes = state.themes();
    let mut owner = owner(&candidate);
    let reconciliation = Arc::new(Mutex::new(None));
    let retained = reconciliation.clone();
    owner.test_before_initial_publication(move |candidate, state, _| {
        let access = candidate.recovery_access().unwrap();
        let mut command = HomeCommand::new(access.home_revision().unwrap());
        command
            .add(state.session().initialize_threadless(
                DomainRevision::new(1).unwrap(),
                InitializeThreadlessWindow::new(
                    WindowId::from_bytes([91; 16]),
                    WindowPlacement::new(
                        WindowBounds::new(0, 0, 800, 600).unwrap(),
                        WindowDisplayState::Normal,
                        None,
                        None,
                    ),
                ),
            ))
            .unwrap();
        faults.fail_next(FaultPoint::AfterCommitBeforePersist);
        let handle = match access.execute(command) {
            CommandOutcome::Indeterminate { reconciliation, .. } => {
                reconciliation.install_and_handle()
            }
            other => panic!("expected original uncertain session command: {other:?}"),
        };
        *retained.lock().unwrap() = Some(handle);
    });
    let failure = owner
        .open_initial(
            candidate,
            state,
            syndic,
            configuration(),
            SyndicTimestamp::from_unix_millis(1),
            CommandCancellation::new(),
        )
        .unwrap_err();
    assert!(matches!(failure.error, AppServiceOpenError::Candidate(_)));
    assert!(failure.rejected_candidate.is_none());
    assert!(owner.graph().is_none());
    assert_eq!(themes.diagnostics().active_subscriptions(), 0);
    assert!(reference.home_revision().is_err());
    let close = owner
        .retained_close()
        .expect("original close capability retained");
    assert_eq!(close.pending_reconciliation_scopes(), Some(1));
    let close_identity = close as *const _;
    assert!(
        HomeOpenCandidate::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT
        ))
        .is_err()
    );

    let (other_directory, incoming, state, syndic, _) = fixture();
    let incoming_id = incoming.home_id();
    let rejected = owner
        .open_initial(
            incoming,
            state,
            syndic,
            configuration(),
            SyndicTimestamp::from_unix_millis(2),
            CommandCancellation::new(),
        )
        .unwrap_err();
    assert!(matches!(
        rejected.error,
        AppServiceOpenError::AlreadyInstalled
    ));
    let incoming = rejected
        .rejected_candidate
        .expect("rejected original candidate returned");
    assert_eq!(incoming.home_id(), incoming_id);
    assert!(std::ptr::eq(
        owner.retained_close().unwrap(),
        close_identity
    ));
    assert_eq!(
        owner
            .retained_close()
            .unwrap()
            .pending_reconciliation_scopes(),
        Some(1)
    );
    incoming.close().unwrap();
    assert_reopens(&other_directory);

    let original = reconciliation.lock().unwrap().take().unwrap();
    let store = owner
        .failed_close
        .take()
        .unwrap()
        .into_open_store()
        .unwrap();
    assert_eq!(store.home_id(), home_id);
    let mut recovered = store.recover_same_home().unwrap();
    assert!(matches!(
        recovered
            .recovery_access()
            .unwrap()
            .reconcile(&original)
            .unwrap(),
        ReconciliationResolution::ExactNew { .. }
    ));
    recovered.abort().close().unwrap();
    assert_reopens(&directory);
}
