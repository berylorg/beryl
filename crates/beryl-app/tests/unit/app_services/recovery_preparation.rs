use super::*;
use crate::{
    app_services::recovery_preparation::PreparedRecoveryAppServices,
    cas_projection::{PreparedRecoveryCasServices, ProcessScheduledExecutionProvider},
};

#[test]
fn recovery_app_services_remain_private_and_dispose_before_retry() {
    for mode in [
        "cancel",
        "terminal_close",
        "drop",
        "precancelled",
        "missing_handoff",
        "missing_runtime",
        "stale_state",
        "stale_syndic",
        "theme_failure",
    ] {
        let (directory, candidate, stale_state, stale_syndic, faults) = fixture();
        let owner = owner(&candidate);
        let home = candidate.publish().unwrap();
        faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(home.home_revision().is_err());
        let candidate = home.recover_same_home().unwrap();
        let reference = candidate.service_reference();
        let generation = candidate.generation();
        let state = BerylState::reacquire_candidate(&candidate).unwrap();
        let syndic = SyndicStorage::reacquire_candidate(&candidate).unwrap();
        let observer = state.themes();
        let (provider, sessions) = ProcessScheduledExecutionProvider::new();
        let attention = Arc::new(ProcessLifecycleAttentionPool::new());
        let config = configuration();
        let mut cas = PreparedRecoveryCasServices::prepare(
            owner.process.clone(),
            candidate,
            syndic.clone(),
            config.projection.clone(),
            Box::new(provider),
            &ProjectionCancellationToken::new(),
        )
        .unwrap_or_else(|failure| panic!("CAS preparation: {}", failure.error()));
        if mode != "missing_runtime" {
            cas = cas
                .configure_managed_sessions(
                    &sessions,
                    config.runtime_interest.clone(),
                    owner.enrollments.clone(),
                    RuntimeSessionPreparationConfig {
                        runtime_roots: state.runtime_roots(),
                        assets: state.assets(),
                        policy: config.session_policy.clone(),
                        token_directory: crate::cas_projection::RuntimeTokenDirectory::from_admitted(beryl_model::AdmittedHostPath::from_admitted(beryl_model::PathFlavor::Windows, r"C:\tokens").unwrap()),
                    },
                    &attention,
                    &ProjectionCancellationToken::new(),
                )
                .unwrap_or_else(|failure| panic!("runtime preparation: {}", failure.error()));
        }
        if mode != "missing_handoff" {
            cas = cas
                .prepare_handoff(
                    owner.settlements.clone(),
                    state.clone(),
                    config.handoff,
                    SyndicTimestamp::from_unix_millis(1),
                    CommandCancellation::new(),
                )
                .unwrap_or_else(|failure| panic!("handoff preparation: {}", failure.error()));
            assert_eq!(
                cas.app_preparation_parts()
                    .unwrap()
                    .1
                    .accepted_input_scheduler_diagnostics()
                    .pass_count(),
                0
            );
        }
        let cancellation = CommandCancellation::new();
        if mode == "precancelled" {
            cancellation.cancel();
        }
        if mode == "theme_failure" {
            faults.fail_next(FaultPoint::BeforeThemeWatchSpawn);
        }
        let observation = faults.block_next(FaultPoint::BeforeThemeWatchObservation);
        let result = PreparedRecoveryAppServices::prepare(
            cas,
            if mode == "stale_state" {
                &stale_state
            } else {
                &state
            },
            if mode == "stale_syndic" {
                stale_syndic
            } else {
                syndic
            },
            config,
            &cancellation,
        );
        let failure = match result {
            Ok(prepared) => {
                assert!(matches!(mode, "cancel" | "terminal_close" | "drop"));
                assert_eq!(reference.health().state(), HomeHealthState::Reopening);
                assert!(reference.home_revision().is_err());
                assert_eq!(observer.diagnostics().active_subscriptions(), 1);
                assert!(!observation.wait_until_reached(Duration::from_millis(60)));
                if mode == "drop" {
                    drop(prepared);
                    assert_eq!(observer.diagnostics().active_subscriptions(), 0);
                    observation.release();
                    assert!(
                        HomeOpenCandidate::open(HomeOpenOptions::new(
                            directory.path(),
                            HomeSchemaVersion::CURRENT,
                        ))
                        .is_err()
                    );
                    println!("abandoned recovery fixture: {}", directory.path().display());
                    attention.close();
                    continue;
                }
                prepared.cancel()
            }
            Err(failure) => failure,
        };
        match mode {
            "cancel" | "terminal_close" | "precancelled" => {
                assert!(matches!(failure.error(), AppServiceOpenError::Cancelled));
            }
            "missing_handoff" => {
                assert!(matches!(
                    failure.error(),
                    AppServiceOpenError::HandoffUnavailable
                ));
            }
            "missing_runtime" => {
                assert!(matches!(
                    failure.error(),
                    AppServiceOpenError::RuntimeUnavailable
                ));
            }
            "stale_state" | "stale_syndic" => {
                assert!(matches!(failure.error(), AppServiceOpenError::Marker(_)));
            }
            "theme_failure" => {
                assert!(matches!(failure.error(), AppServiceOpenError::Theme(_)));
            }
            _ => unreachable!(),
        }
        assert_eq!(observer.diagnostics().active_subscriptions(), 0);
        assert!(!observation.wait_until_reached(Duration::from_millis(60)));
        observation.release();
        if mode == "terminal_close" {
            failure.close().unwrap();
            assert_reopens(&directory);
            attention.close();
            continue;
        }
        let (home, _, cas_error) = failure.into_retry_parts().unwrap();
        assert!(matches!(cas_error, CasPreparationError::Cancelled));
        assert_eq!(home.health().state(), HomeHealthState::Failed);
        assert!(
            HomeOpenCandidate::open(HomeOpenOptions::new(
                directory.path(),
                HomeSchemaVersion::CURRENT,
            ))
            .is_err()
        );
        let candidate = home.recover_same_home().unwrap();
        assert_ne!(candidate.generation(), generation);
        candidate.abort().close().unwrap();
        attention.close();
    }
}
