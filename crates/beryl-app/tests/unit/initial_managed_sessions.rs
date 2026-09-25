use super::*;
use crate::cas_projection::{
    OrdinaryTurnExecutionRequest, ProcessScheduledExecutionProvider, RuntimeInterestConfig,
    RuntimeSessionPreparationConfig, RuntimeSessionPreparationError, RuntimeTokenDirectories,
    ScheduledOrdinaryRequestPolicy,
};
use crate::lifecycle_attention::ProcessLifecycleAttentionPool;
use beryl_backend::{ThreadStartOptions, TurnStartOptions};
use beryl_model::{AdmittedHostPath, PathFlavor, RuntimeId, RuntimeNativePath};
use std::num::NonZeroUsize;

fn session_config(state: &BerylState) -> RuntimeSessionPreparationConfig {
    RuntimeSessionPreparationConfig {
        runtime_roots: state.runtime_roots(),
        assets: state.assets(),
        policy: ScheduledOrdinaryRequestPolicy::new(
            ThreadStartOptions::persistent(),
            None,
            Duration::from_secs(1),
            OrdinaryTurnExecutionRequest::new(TurnStartOptions::default(), Duration::from_secs(1)),
        ),
        token_directories: Vec::new(),
    }
}

fn interest() -> RuntimeInterestConfig {
    RuntimeInterestConfig::new(
        NonZeroUsize::new(1).unwrap(),
        NonZeroUsize::new(1).unwrap(),
        Duration::from_secs(1),
    )
    .unwrap()
}

#[test]
fn attached_sessions_configure_without_releasing_startup() {
    let (directory, candidate, storage, _, state) = candidate_with_state();
    let reference = candidate.service_reference();
    let (provider, sessions) = ProcessScheduledExecutionProvider::new();
    let prepared = PreparedCasServices::prepare(
        Default::default(),
        candidate,
        storage,
        config(),
        Box::new(provider),
    )
    .unwrap();
    let attention = Arc::new(ProcessLifecycleAttentionPool::new());
    assert!(matches!(
        prepared
            .service
            .as_ref()
            .unwrap()
            .configure_runtime_session_preparation(&sessions, session_config(&state), &attention,),
        Err(RuntimeSessionPreparationError::ServiceUnavailable)
    ));
    let enrollments = crate::runtime_activity_enrollment::RuntimeActivityEnrollmentOperations::new(
        reference.home_id(),
        NonZeroUsize::new(1).unwrap(),
    );
    let prepared = prepared
        .configure_managed_sessions(
            &sessions,
            interest(),
            enrollments,
            session_config(&state),
            &attention,
        )
        .unwrap();
    let signal = prepared.service.as_ref().unwrap().scheduler_signal.clone();
    thread::sleep(Duration::from_millis(50));
    assert_eq!(signal.diagnostics().pass_count(), 0);
    assert_eq!(reference.health().state(), HomeHealthState::Opening);
    assert!(reference.home_revision().is_err());
    drop(prepared);
    assert!(signal.diagnostics().stopped());
    assert_reopens(&directory);
}

#[test]
fn configuration_rejection_retires_private_custody() {
    for failure in [
        "foreign_owner",
        "foreign_assets",
        "missing_runtime",
        "capacity",
        "confirmation",
    ] {
        let (directory, candidate, storage, faults, state) = candidate_with_state();
        let (_foreign_directory, _foreign_candidate, _, _, foreign_state) = candidate_with_state();
        let (provider, sessions) = ProcessScheduledExecutionProvider::new();
        let (_unused_provider, foreign_sessions) = ProcessScheduledExecutionProvider::new();
        let prepared = PreparedCasServices::prepare(
            Default::default(),
            candidate,
            storage,
            config(),
            Box::new(provider),
        )
        .unwrap();
        let signal = prepared.service.as_ref().unwrap().scheduler_signal.clone();
        let mut configuration = session_config(&state);
        if failure == "foreign_assets" {
            configuration.assets = foreign_state.assets();
        }
        if matches!(failure, "missing_runtime" | "capacity") {
            let tokens = RuntimeTokenDirectories {
                runtime_id: RuntimeId::from_bytes([1; 16]),
                host: AdmittedHostPath::from_admitted(PathFlavor::Windows, r"C:\tokens").unwrap(),
                runtime: RuntimeNativePath::from_admitted(
                    beryl_model::RuntimeMode::Host,
                    PathFlavor::Windows,
                    r"C:\tokens",
                )
                .unwrap(),
            };
            configuration.token_directories.push(tokens.clone());
            if failure == "capacity" {
                configuration.token_directories.push(tokens);
            }
        }
        if failure == "confirmation" {
            faults.fail_next(FaultPoint::BeforeReadConfirmation);
        }
        let selected = if failure == "foreign_owner" {
            &foreign_sessions
        } else {
            &sessions
        };
        let enrollments =
            crate::runtime_activity_enrollment::RuntimeActivityEnrollmentOperations::new(
                prepared.service.as_ref().unwrap().home_id,
                NonZeroUsize::new(1).unwrap(),
            );
        let result = prepared.configure_managed_sessions(
            selected,
            interest(),
            enrollments,
            configuration,
            &Arc::new(ProcessLifecycleAttentionPool::new()),
        );
        let expected = if failure == "foreign_owner" {
            RuntimeSessionPreparationError::OwnerMismatch
        } else {
            RuntimeSessionPreparationError::InvalidConfiguration
        };
        assert!(
            matches!(result, Err(CasPreparationError::Session(error)) if error == expected),
            "{failure}"
        );
        assert!(signal.diagnostics().stopped());
        assert_reopens(&directory);
    }
}
