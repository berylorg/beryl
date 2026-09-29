use super::*;
use crate::cas_projection::{MinimumTurnCaptureReserve, ScheduledExecutionProviderContext};
use beryl_home_store::{
    HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion,
    test_faults::{FaultController, FaultPoint},
};
use beryl_state::BerylState;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Default)]
struct Probe {
    attached: AtomicUsize,
    issued: AtomicUsize,
    shutdown: AtomicUsize,
}

struct Provider {
    home: HomeServiceReference,
    probe: Arc<Probe>,
    cancel_on_attach: Option<ProjectionCancellationToken>,
    fail_scheduler_start: bool,
    fail_cleanup: bool,
    shutdown_state: HomeHealthState,
}

impl ScheduledOrdinaryExecutionProvider for Provider {
    fn attach(&mut self, _: ScheduledExecutionProviderContext) {
        assert_eq!(self.home.health().state(), HomeHealthState::Reopening);
        assert!(self.home.home_revision().is_err());
        self.probe.attached.fetch_add(1, Ordering::SeqCst);
        if self.fail_scheduler_start {
            super::super::construction::fail_scheduler_start_for_test(self.fail_cleanup);
        }
        if let Some(cancellation) = &self.cancel_on_attach {
            cancellation.cancel();
        }
    }

    fn try_issue(
        &mut self,
        admission: ScheduledOrdinaryAdmission,
    ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
        self.probe.issued.fetch_add(1, Ordering::SeqCst);
        Ok(admission.decline(ScheduledOrdinaryExecutionUnavailable::RuntimeNotReady))
    }

    fn shutdown(&mut self) {
        assert_eq!(self.home.health().state(), self.shutdown_state);
        self.probe.shutdown.fetch_add(1, Ordering::SeqCst);
    }
}

fn fixture() -> (
    tempfile::TempDir,
    HomeRecoveryCandidate,
    SyndicStorage,
    SyndicStorage,
    FaultController,
) {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let stale = SyndicStorage::register(&mut candidate).unwrap();
    BerylState::register(&mut candidate).unwrap();
    let home = candidate
        .prepare_publication(
            BerylState::required_domains()
                .unwrap()
                .merge(SyndicStorage::required_domains().unwrap())
                .unwrap(),
        )
        .unwrap()
        .publish()
        .unwrap();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(home.home_revision().is_err());
    let candidate = home.recover_same_home().unwrap();
    let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    (directory, candidate, storage, stale, faults)
}

fn config() -> ProjectionServiceConfig {
    ProjectionServiceConfig::try_new(8, 4, MinimumTurnCaptureReserve::try_new(1).unwrap()).unwrap()
}

fn provider(candidate: &HomeRecoveryCandidate, probe: &Arc<Probe>) -> Provider {
    Provider {
        home: candidate.service_reference(),
        probe: Arc::clone(probe),
        cancel_on_attach: None,
        fail_scheduler_start: false,
        fail_cleanup: false,
        shutdown_state: HomeHealthState::Reopening,
    }
}

#[test]
fn recovery_handoff_preserves_exact_custody_until_explicit_release_or_cancellation() {
    for publish in [false, true] {
        let (directory, candidate, storage, _, _) = fixture();
        let reference = candidate.service_reference();
        let home_id = candidate.home_id();
        let generation = candidate.generation();
        let probe = Arc::new(Probe::default());
        let mut provider = provider(&candidate, &probe);
        if publish {
            provider.shutdown_state = HomeHealthState::Healthy;
        }
        let prepared = match PreparedRecoveryCasServices::prepare(
            Default::default(),
            candidate,
            storage.clone(),
            config(),
            Box::new(provider),
            &ProjectionCancellationToken::new(),
        ) {
            Ok(prepared) => prepared,
            Err(_) => panic!("fresh candidate preparation failed"),
        };
        let gate = prepared.initial_start.as_ref().unwrap().gate();
        let signal = prepared.service.as_ref().unwrap().scheduler_signal.clone();
        let service_generation = prepared.service.as_ref().unwrap().service_generation();
        let (mut candidate, service, start) = prepared.into_recovery_parts();
        assert_eq!(candidate.home_id(), home_id);
        assert_eq!(candidate.generation(), generation);
        assert_eq!(service.home_id(), home_id);
        assert_eq!(service.home_generation(), generation);
        assert_eq!(service.service_generation(), service_generation);
        assert!(Arc::ptr_eq(&gate, &start.gate()));
        assert!(
            service
                .submission_execution_wake()
                .matches_binding(home_id, generation)
        );
        assert!(
            storage
                .revision_candidate(&candidate.recovery_access().unwrap())
                .is_ok()
        );
        assert_eq!(reference.health().state(), HomeHealthState::Reopening);
        assert!(reference.home_revision().is_err());
        assert_eq!(signal.diagnostics().pass_count(), 0);
        assert_eq!(probe.attached.load(Ordering::SeqCst), 1);
        assert_eq!(probe.issued.load(Ordering::SeqCst), 0);
        assert_eq!(probe.shutdown.load(Ordering::SeqCst), 0);
        assert!(
            HomeOpenCandidate::open(HomeOpenOptions::new(
                directory.path(),
                HomeSchemaVersion::CURRENT,
            ))
            .is_err()
        );
        let home = if publish {
            let home = candidate.publish().unwrap();
            assert!(start.release());
            assert!(gate.wait());
            service.close().unwrap();
            home
        } else {
            drop(start);
            assert!(!gate.wait());
            service.close().unwrap();
            assert_eq!(reference.health().state(), HomeHealthState::Reopening);
            candidate.abort()
        };
        assert!(signal.diagnostics().stopped());
        assert_eq!(probe.shutdown.load(Ordering::SeqCst), 1);
        home.close().unwrap();
        HomeOpenCandidate::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap()
        .close()
        .unwrap();
    }
}

#[test]
fn reopening_services_remain_fenced_and_cancel_before_candidate_abort() {
    let (_directory, candidate, storage, _, _) = fixture();
    let generation = candidate.generation();
    let reference = candidate.service_reference();
    let probe = Arc::new(Probe::default());
    let provider = provider(&candidate, &probe);
    let prepared = match PreparedRecoveryCasServices::prepare(
        Default::default(),
        candidate,
        storage,
        config(),
        Box::new(provider),
        &ProjectionCancellationToken::new(),
    ) {
        Ok(prepared) => prepared,
        Err(_) => panic!("fresh candidate preparation failed"),
    };
    assert_eq!(
        prepared.service.as_ref().unwrap().home_generation(),
        generation
    );
    assert!(
        prepared
            .service
            .as_ref()
            .unwrap()
            .live_home_command()
            .unwrap()
            .home()
            .home_revision()
            .is_err()
    );
    assert_eq!(probe.attached.load(Ordering::SeqCst), 1);
    assert_eq!(probe.issued.load(Ordering::SeqCst), 0);
    assert_eq!(reference.health().state(), HomeHealthState::Reopening);
    let failed = prepared.cancel();
    assert_eq!(probe.shutdown.load(Ordering::SeqCst), 1);
    let home = match failed.into_retry_home() {
        Ok(home) => home,
        Err(_) => panic!("joined preparation must allow retry"),
    };
    assert_eq!(home.health().state(), HomeHealthState::Failed);
    let candidate = home.recover_same_home().unwrap();
    assert!(candidate.generation().get() > generation.get());
    candidate.abort().close().unwrap();
}

#[test]
fn stale_storage_and_failed_candidate_reads_return_original_failure_and_custody() {
    for stale in [false, true] {
        let (_directory, candidate, storage, old_storage, faults) = fixture();
        let probe = Arc::new(Probe::default());
        let provider = provider(&candidate, &probe);
        if !stale {
            faults.fail_next(FaultPoint::BeforeReadConfirmation);
        }
        let failure = match PreparedRecoveryCasServices::prepare(
            Default::default(),
            candidate,
            if stale { old_storage } else { storage },
            config(),
            Box::new(provider),
            &ProjectionCancellationToken::new(),
        ) {
            Err(failure) => failure,
            Ok(_) => panic!("candidate read must fail"),
        };
        assert!(matches!(
            failure.error(),
            CasPreparationError::Service(ProjectionCoordinatorError::AcceptedDeliveryRecoveryRead)
        ));
        assert_eq!(probe.attached.load(Ordering::SeqCst), 0);
        let home = match failure.into_retry_home() {
            Ok(home) => home,
            Err(_) => panic!("no workers were started"),
        };
        home.recover_same_home().unwrap().abort().close().unwrap();
    }
}

#[test]
fn cancellation_before_or_during_construction_never_releases_ordinary_work() {
    for during in [false, true] {
        let (_directory, candidate, storage, _, _) = fixture();
        let probe = Arc::new(Probe::default());
        let cancellation = ProjectionCancellationToken::new();
        let mut provider = provider(&candidate, &probe);
        if during {
            provider.cancel_on_attach = Some(cancellation.clone());
        } else {
            cancellation.cancel();
        }
        let failure = match PreparedRecoveryCasServices::prepare(
            Default::default(),
            candidate,
            storage,
            config(),
            Box::new(provider),
            &cancellation,
        ) {
            Err(failure) => failure,
            Ok(_) => panic!("cancelled preparation succeeded"),
        };
        assert!(matches!(failure.error(), CasPreparationError::Cancelled));
        assert_eq!(probe.issued.load(Ordering::SeqCst), 0);
        assert_eq!(probe.shutdown.load(Ordering::SeqCst), usize::from(during));
        failure.close().unwrap();
    }
}

#[test]
fn unconfirmed_disposal_cannot_return_a_retry_home() {
    let (_directory, candidate, storage, _, _) = fixture();
    let probe = Arc::new(Probe::default());
    let provider = provider(&candidate, &probe);
    let prepared = match PreparedRecoveryCasServices::prepare(
        Default::default(),
        candidate,
        storage,
        config(),
        Box::new(provider),
        &ProjectionCancellationToken::new(),
    ) {
        Ok(prepared) => prepared,
        Err(_) => panic!("fresh candidate preparation failed"),
    };
    let retained_provider = Arc::clone(
        prepared
            .service
            .as_ref()
            .unwrap()
            .scheduled_ordinary_provider
            .as_ref()
            .unwrap(),
    );
    let failure = match prepared.cancel().into_retry_home() {
        Err(failure) => failure,
        Ok(_) => panic!("unconfirmed shared-provider disposal granted retry"),
    };
    assert_eq!(probe.shutdown.load(Ordering::SeqCst), 1);
    drop(retained_provider);
    failure.close().unwrap();
}

#[test]
fn returned_constructor_failure_joins_partial_workers_before_abort_and_retry() {
    for cleanup_fails in [false, true] {
        let (_directory, candidate, storage, _, _) = fixture();
        let reference = candidate.service_reference();
        let probe = Arc::new(Probe::default());
        let mut provider = provider(&candidate, &probe);
        provider.fail_scheduler_start = true;
        provider.fail_cleanup = cleanup_fails;
        let failure = match PreparedRecoveryCasServices::prepare(
            Default::default(),
            candidate,
            storage,
            config(),
            Box::new(provider),
            &ProjectionCancellationToken::new(),
        ) {
            Err(failure) => failure,
            Ok(_) => panic!("scheduler construction fault was ignored"),
        };
        if cleanup_fails {
            assert!(
                matches!(failure.error(), CasPreparationError::Service(ProjectionCoordinatorError::ServiceConstructionDisposal { source }) if matches!(**source, ProjectionCoordinatorError::AcceptedInputSchedulerSpawn { .. }))
            );
            let failure = match failure.into_retry_home() {
                Err(failure) => failure,
                Ok(_) => panic!("failed construction cleanup granted retry"),
            };
            assert_eq!(probe.shutdown.load(Ordering::SeqCst), 1);
            failure.close().unwrap();
            continue;
        }
        assert!(matches!(
            failure.error(),
            CasPreparationError::Service(
                ProjectionCoordinatorError::AcceptedInputSchedulerSpawn { .. }
            )
        ));
        assert_eq!(probe.shutdown.load(Ordering::SeqCst), 1);
        assert_eq!(reference.health().state(), HomeHealthState::Failed);
        let home = match failure.into_retry_home() {
            Ok(home) => home,
            Err(_) => panic!("partial constructor did not confirm cleanup"),
        };
        home.recover_same_home().unwrap().abort().close().unwrap();
    }
}

#[test]
fn recovered_managed_session_configuration_keeps_work_fenced_and_rejects_foreign_owner() {
    use crate::cas_projection::{
        OrdinaryTurnExecutionRequest, ProcessScheduledExecutionProvider, RuntimeInterestConfig,
        RuntimeSessionPreparationConfig, ScheduledOrdinaryRequestPolicy,
    };
    use std::{num::NonZeroUsize, time::Duration};
    for mode in ["valid", "foreign", "cancelled"] {
        let foreign = mode == "foreign";
        let (_directory, candidate, storage, _, _) = fixture();
        let state = BerylState::reacquire_candidate(&candidate).unwrap();
        let reference = candidate.service_reference();
        let (provider, sessions) = ProcessScheduledExecutionProvider::new();
        let (_foreign_provider, foreign_sessions) = ProcessScheduledExecutionProvider::new();
        let cancellation = ProjectionCancellationToken::new();
        let prepared = match PreparedRecoveryCasServices::prepare(
            Default::default(),
            candidate,
            storage,
            config(),
            Box::new(provider),
            &cancellation,
        ) {
            Ok(prepared) => prepared,
            Err(_) => panic!("fresh CAS preparation failed"),
        };
        let signal = prepared.service.as_ref().unwrap().scheduler_signal.clone();
        if mode == "cancelled" {
            cancellation.cancel();
        }
        let enrollments =
            crate::runtime_activity_enrollment::RuntimeActivityEnrollmentOperations::new(
                reference.home_id(),
                NonZeroUsize::new(1).unwrap(),
            );
        let result = prepared.configure_managed_sessions(
            if foreign {
                &foreign_sessions
            } else {
                &sessions
            },
            RuntimeInterestConfig::new(
                NonZeroUsize::new(1).unwrap(),
                NonZeroUsize::new(1).unwrap(),
                Duration::from_secs(1),
            )
            .unwrap(),
            enrollments,
            RuntimeSessionPreparationConfig {
                runtime_roots: state.runtime_roots(),
                assets: state.assets(),
                token_directories: Vec::new(),
                policy: ScheduledOrdinaryRequestPolicy::new(
                    beryl_backend::ThreadStartOptions::persistent(),
                    None,
                    Duration::from_secs(1),
                    OrdinaryTurnExecutionRequest::new(
                        beryl_backend::TurnStartOptions::default(),
                        Duration::from_secs(1),
                    ),
                ),
            },
            &Arc::new(crate::lifecycle_attention::ProcessLifecycleAttentionPool::new()),
            &cancellation,
        );
        let failure = match result {
            Ok(prepared) => {
                assert_eq!(mode, "valid");
                assert_eq!(signal.diagnostics().pass_count(), 0);
                assert_eq!(reference.health().state(), HomeHealthState::Reopening);
                prepared.cancel()
            }
            Err(failure) => {
                if mode == "cancelled" {
                    assert!(matches!(failure.error(), CasPreparationError::Cancelled));
                } else {
                    assert!(foreign);
                    assert!(matches!(
                        failure.error(),
                        CasPreparationError::Session(
                            crate::cas_projection::RuntimeSessionPreparationError::OwnerMismatch
                        )
                    ));
                }
                failure
            }
        };
        assert!(signal.diagnostics().stopped());
        let home = match failure.into_retry_home() {
            Ok(home) => home,
            Err(_) => panic!("configuration cleanup must settle"),
        };
        home.close().unwrap();
    }
}
