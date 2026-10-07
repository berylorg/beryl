use super::*;
use beryl_app::cas_projection::{
    CasProjectionRequest, ExactSoftStopAvailability, ExactStopAttemptKind, ExactStopFeedbackState,
    ExactStopRequestError, StopWorkPageLimits,
};
use beryl_home_store::test_faults::{FaultController, FaultPoint};

#[test]
fn exact_stop_feedback_rejects_target_after_connection_generation_retires() {
    run_admission_failure(None);
}

#[test]
fn exact_stop_feedback_proven_admission_noncommit_is_request_failure() {
    run_admission_failure(Some(FaultPoint::BeforeCommit));
}

#[test]
fn exact_stop_feedback_indeterminate_admission_survives_service_disposal() {
    run_admission_failure(Some(FaultPoint::AfterCommitBeforePersist));
}

fn run_admission_failure(fault: Option<FaultPoint>) {
    let _guard = TEST_LOCK.lock().unwrap();
    let faults = FaultController::new();
    let mut fixture = Fixture::with_faults(183, faults.clone());
    eprintln!(
        "exact-stop fault fixture home: {}",
        fixture.home_path().display()
    );
    let submitted = fixture.submit_text(SUBMITTED_TEXT);
    let server = NormalTerminalServer::spawn_controlled_connection_loss();
    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), AUTHORIZATION);
    let mut session = fixture
        .store
        .admit_runtime_lifecycle_test_candidate(
            &connector,
            execution_binding(),
            CasProcessGeneration::new(38_183).unwrap(),
            Path::new(EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    let coordinator = CasProjectionCoordinator::for_healthy_home(&*fixture.home()).unwrap();
    let request = CasProjectionRequest::new(
        fixture.thread,
        fixture.selected_path(fixture.thread),
        execution_binding(),
        ThreadStartOptions::persistent(),
        Some(2_000_000),
        SyndicTimestamp::from_unix_millis(38_100),
        TIMEOUT,
    );
    let projection = coordinator
        .obtain_projection(
            &*fixture.home(),
            &fixture.storage,
            &mut session,
            &request,
            &fixture.cancellation,
        )
        .unwrap();
    server.wait_for_projection();
    let feedback = thread::scope(|scope| {
        let capture = scope.spawn(|| {
            coordinator.execute_ordinary_turn(
                &*fixture.home(),
                &fixture.storage,
                &fixture.state.assets(),
                None,
                projection,
                &fixture.cancellation,
                &OrdinaryTurnExecutionRequest::new(TurnStartOptions::default(), TIMEOUT),
                OrdinaryDynamicToolHandlers::new(
                    &mut NoopLifecycle::default(),
                    &mut NoopBranch::default(),
                ),
            )
        });
        super::stop_retention::wait_for_active(&fixture, submitted.turn);
        let ExactSoftStopAvailability::Eligible(token) =
            fixture.store.exact_soft_stop_eligibility(fixture.thread)
        else {
            panic!("exact eligibility required");
        };
        let feedback = fault.map(|fault| {
            faults.fail_next(fault);
            let feedback = fixture.store.request_exact_soft_stop(&token).unwrap();
            if fault == FaultPoint::BeforeCommit {
                assert_eq!(
                    feedback.snapshot().state,
                    ExactStopFeedbackState::RequestNotAdmitted
                );
            } else {
                assert_eq!(feedback.snapshot().state, ExactStopFeedbackState::Waiting);
            }
            assert!(fixture.store.request_exact_soft_stop(&token).unwrap() == feedback);
            feedback
        });
        server.close_connection();
        session.invalidate_connection();
        let _ = capture.join().unwrap();
        if fault.is_none() {
            assert!(matches!(
                fixture.store.request_exact_soft_stop(&token),
                Err(ExactStopRequestError::Revoked)
            ));
        }
        feedback
    });
    session.invalidate_connection();
    drop(session);
    server.join();
    let (directory, service) = fixture.into_service();
    let close = service.close();
    if fault == Some(FaultPoint::AfterCommitBeforePersist) {
        assert!(matches!(
            close,
            Err(ref failure) if matches!(failure.error(), beryl_app::cas_projection::ProjectionConnectionServiceCloseError::HomeClose(_))
        ));
    } else {
        let _ = close.unwrap();
    }
    if let Some(feedback) = feedback {
        let expected = if fault == Some(FaultPoint::BeforeCommit) {
            ExactStopFeedbackState::RequestNotAdmitted
        } else {
            ExactStopFeedbackState::AuthorityLost
        };
        assert_eq!(feedback.snapshot().state, expected);
    }
    drop(directory);
}

#[test]
fn exact_stop_feedback_preserves_interrupted_terminal_without_error_payload() {
    run_terminal("interrupted", ExactStopFeedbackState::Interrupted, false);
}

#[test]
fn exact_stop_feedback_preserves_successful_terminal_race() {
    run_terminal("completed", ExactStopFeedbackState::Completed, false);
}

#[test]
fn exact_stop_feedback_preserves_failed_terminal_race() {
    run_terminal("failed", ExactStopFeedbackState::Failed, false);
}

#[test]
fn exact_stop_feedback_drop_does_not_reopen_waiting_control() {
    run_terminal("interrupted", ExactStopFeedbackState::Interrupted, true);
}

fn run_terminal(status: &'static str, expected: ExactStopFeedbackState, drop_waiting: bool) {
    run_terminal_using(status, expected, drop_waiting, false);
}

fn run_terminal_using(
    status: &'static str,
    expected: ExactStopFeedbackState,
    drop_waiting: bool,
    use_worker: bool,
) {
    run_terminal_scenario(status, expected, drop_waiting, use_worker, false);
}

fn run_terminal_scenario(
    status: &'static str,
    expected: ExactStopFeedbackState,
    drop_waiting: bool,
    use_worker: bool,
    retire_publication: bool,
) {
    let _guard = TEST_LOCK.lock().unwrap();
    let mut fixture = Fixture::new(181);
    let worker = fixture.store.exact_stop_worker();
    let submitted = fixture.submit_text(SUBMITTED_TEXT);
    let caller = if use_worker {
        ExactStopCaller::Worker(worker.clone())
    } else {
        ExactStopCaller::Service(&fixture.store)
    };
    let server = NormalTerminalServer::spawn_stop_feedback_terminal(status);
    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), AUTHORIZATION);
    let mut session = fixture
        .store
        .admit_runtime_lifecycle_test_candidate(
            &connector,
            execution_binding(),
            CasProcessGeneration::new(38_181).unwrap(),
            Path::new(EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    let coordinator = CasProjectionCoordinator::for_healthy_home(&*fixture.home()).unwrap();
    let request = CasProjectionRequest::new(
        fixture.thread,
        fixture.selected_path(fixture.thread),
        execution_binding(),
        ThreadStartOptions::persistent(),
        Some(2_000_000),
        SyndicTimestamp::from_unix_millis(38_100),
        TIMEOUT,
    );
    let projection = coordinator
        .obtain_projection(
            &*fixture.home(),
            &fixture.storage,
            &mut session,
            &request,
            &fixture.cancellation,
        )
        .unwrap();
    server.wait_for_projection();
    let feedback = thread::scope(|scope| {
        let capture = scope.spawn(|| {
            coordinator.execute_ordinary_turn(
                &*fixture.home(),
                &fixture.storage,
                &fixture.state.assets(),
                None,
                projection,
                &fixture.cancellation,
                &OrdinaryTurnExecutionRequest::new(TurnStartOptions::default(), TIMEOUT),
                OrdinaryDynamicToolHandlers::new(
                    &mut NoopLifecycle::default(),
                    &mut NoopBranch::default(),
                ),
            )
        });
        super::stop_retention::wait_for_active(&fixture, submitted.turn);
        let ExactSoftStopAvailability::Eligible(token) = caller.eligibility(fixture.thread) else {
            panic!("exact stop eligibility required");
        };
        let foreign = Fixture::new(182);
        assert!(matches!(
            foreign
                .store
                .exact_stop_worker()
                .request_exact_soft_stop(&token),
            Err(ExactStopRequestError::Revoked)
        ));
        drop(foreign);
        let feedback = if retire_publication {
            use beryl_app::cas_projection::test_faults::{
                install_stop_handoff_barrier, request_published_exact_soft_stop_for_test,
            };
            let publication = std::sync::Arc::new(());
            let expired = std::sync::Arc::downgrade(&publication);
            drop(publication);
            let stale_worker = worker.clone();
            let stale_token = token.clone();
            assert!(matches!(
                std::thread::spawn(move || request_published_exact_soft_stop_for_test(
                    stale_worker,
                    expired,
                    &stale_token
                ))
                .join()
                .unwrap(),
                Err(ExactStopRequestError::Revoked)
            ));
            assert!(!fixture.store.has_local_stop_for_test(fixture.thread));
            assert!(matches!(
                caller.eligibility(fixture.thread),
                ExactSoftStopAvailability::Eligible(_)
            ));
            let publication = std::sync::Arc::new(());
            let lifetime = std::sync::Arc::downgrade(&publication);
            let barrier = install_stop_handoff_barrier(fixture.thread);
            let request_worker = worker.clone();
            let request_token = token.clone();
            let request = std::thread::spawn(move || {
                request_published_exact_soft_stop_for_test(request_worker, lifetime, &request_token)
            });
            barrier.wait();
            let retained = caller.request(&token).unwrap();
            drop(publication);
            barrier.release(false);
            let returned = request.join().unwrap().unwrap();
            assert_eq!(returned, retained);
            assert!(matches!(
                caller.eligibility(fixture.thread),
                ExactSoftStopAvailability::Unavailable(_)
            ));
            returned
        } else {
            caller.request(&token).unwrap()
        };
        let first = feedback.snapshot();
        assert_eq!(first.state, ExactStopFeedbackState::Waiting);
        assert_eq!(first.attempt, ExactStopAttemptKind::Durable);
        assert!(fixture.store.request_exact_soft_stop(&token).unwrap() == feedback);
        assert!(caller.request(&token).unwrap() == feedback);
        assert!(matches!(
            caller.eligibility(fixture.thread),
            ExactSoftStopAvailability::Unavailable(_)
        ));
        if drop_waiting {
            drop(feedback);
            assert!(matches!(
                caller.request(&token),
                Err(ExactStopRequestError::Revoked)
            ));
            assert!(matches!(
                caller.eligibility(fixture.thread),
                ExactSoftStopAvailability::Unavailable(
                    beryl_app::cas_projection::ExactSoftStopUnavailable::RequestInProgress
                )
            ));
            server.release_stop_terminal();
            let _ = capture.join().unwrap().unwrap();
            return token;
        }
        server.release_stop_terminal();
        let _ = capture.join().unwrap().unwrap();
        let final_state = feedback.snapshot();
        assert_eq!(final_state.state, expected);
        assert!(final_state.revision > first.revision);
        assert!(!fixture.store.has_local_stop_for_test(fixture.thread));
        let revision = fixture.store.stop_work_revision().unwrap();
        assert!(
            fixture
                .store
                .stop_work_page(&revision, None, StopWorkPageLimits::new(8, 65_536).unwrap())
                .unwrap()
                .records()
                .is_empty()
        );
        assert!(caller.request(&token).unwrap() == feedback);
        let retained = feedback.clone();
        drop(feedback);
        assert_eq!(retained.snapshot(), final_state);
        drop(retained);
        assert!(matches!(
            caller.request(&token),
            Err(ExactStopRequestError::Revoked)
        ));
        token
    });
    session.invalidate_connection();
    drop(session);
    server.join();
    assert!(matches!(
        fixture.store.request_exact_soft_stop(&feedback),
        Err(ExactStopRequestError::Revoked)
    ));
    drop(caller);
    let (directory, service) = fixture.into_service();
    let _ = service.close().unwrap();
    assert_eq!(worker.test_resource_strong_counts(), [0, 0, 0]);
    assert!(matches!(
        worker.request_exact_soft_stop(&feedback),
        Err(ExactStopRequestError::Revoked)
    ));
    drop(directory);
}

#[test]
fn exact_stop_worker_invokes_shared_interrupted_terminal_and_duplicate_identity_on_threads() {
    run_terminal_using(
        "interrupted",
        ExactStopFeedbackState::Interrupted,
        false,
        true,
    );
}

#[test]
fn exact_stop_worker_invokes_shared_completed_terminal_on_threads() {
    run_terminal_using("completed", ExactStopFeedbackState::Completed, false, true);
}

#[test]
fn exact_stop_worker_invokes_shared_failed_terminal_on_threads() {
    run_terminal_using("failed", ExactStopFeedbackState::Failed, false, true);
}

#[test]
fn exact_stop_worker_disposal_does_not_reopen_waiting_control() {
    run_terminal_using(
        "interrupted",
        ExactStopFeedbackState::Interrupted,
        true,
        true,
    );
}

#[test]
fn exact_stop_published_worker_retains_feedback_after_midrequest_retirement() {
    run_terminal_scenario(
        "interrupted",
        ExactStopFeedbackState::Interrupted,
        false,
        true,
        true,
    );
}

enum ExactStopCaller<'a> {
    Service(&'a beryl_app::cas_projection::ProjectionConnectionService),
    Worker(beryl_app::cas_projection::ExactStopWorker),
}

impl ExactStopCaller<'_> {
    fn eligibility(&self, thread: beryl_model::SyndicThreadId) -> ExactSoftStopAvailability {
        match self {
            Self::Service(service) => service.exact_soft_stop_eligibility(thread),
            Self::Worker(worker) => {
                let worker = worker.clone();
                std::thread::spawn(move || worker.exact_soft_stop_eligibility(thread))
                    .join()
                    .unwrap()
            }
        }
    }

    fn request(
        &self,
        token: &beryl_app::cas_projection::ExactSoftStopEligibility,
    ) -> Result<beryl_app::cas_projection::ExactStopFeedback, ExactStopRequestError> {
        match self {
            Self::Service(service) => service.request_exact_soft_stop(token),
            Self::Worker(worker) => {
                let worker = worker.clone();
                let token = token.clone();
                std::thread::spawn(move || worker.request_exact_soft_stop(&token))
                    .join()
                    .unwrap()
            }
        }
    }
}

#[test]
fn exact_stop_worker_clones_do_not_pin_resources_and_retirement_is_inert() {
    let _guard = TEST_LOCK.lock().unwrap();
    let fixture = Fixture::new(184);
    let worker = fixture.store.exact_stop_worker();
    let counts = worker.test_resource_strong_counts();
    let retained = worker.clone();
    assert_eq!(retained.test_resource_strong_counts(), counts);
    assert_eq!(worker.home_id(), fixture.store.home_id());
    assert_eq!(worker.home_generation(), fixture.store.home_generation());
    assert_eq!(
        worker.service_generation(),
        fixture.store.service_generation()
    );
    let thread = fixture.thread;
    let call = retained.clone();
    assert!(matches!(
        std::thread::spawn(move || call.exact_soft_stop_eligibility(thread))
            .join()
            .unwrap(),
        ExactSoftStopAvailability::Unavailable(
            beryl_app::cas_projection::ExactSoftStopUnavailable::NoExactTarget
        )
    ));
    let (directory, service) = fixture.into_service();
    let _ = service.close().unwrap();
    assert_eq!(worker.test_resource_strong_counts(), [0, 0, 0]);
    assert_eq!(retained.test_resource_strong_counts(), [0, 0, 0]);
    assert!(matches!(
        worker.exact_soft_stop_eligibility(thread),
        ExactSoftStopAvailability::Unavailable(
            beryl_app::cas_projection::ExactSoftStopUnavailable::AuthorityUnavailable
        )
    ));
    drop(directory);
}
