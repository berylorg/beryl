use super::*;
use crate::cas_projection::{
    OrdinaryTurnExecutionRequest, ProcessScheduledExecutionProvider, RuntimeInterestConfig,
    RuntimeSessionPreparationConfig, RuntimeSessionPreparationError,
    ScheduledOrdinaryRequestPolicy,
};
use crate::lifecycle_attention::ProcessLifecycleAttentionPool;
use beryl_backend::{ThreadStartOptions, TurnStartOptions};
use std::num::NonZeroUsize;

fn session_config(state: &BerylState) -> RuntimeSessionPreparationConfig {
    RuntimeSessionPreparationConfig {
        wsl_supervisor_artifact: None,
        runtime_roots: state.runtime_roots(),
        assets: state.assets(),
        policy: ScheduledOrdinaryRequestPolicy::new(
            ThreadStartOptions::persistent(),
            None,
            Duration::from_secs(1),
            OrdinaryTurnExecutionRequest::new(TurnStartOptions::default(), Duration::from_secs(1)),
        ),
        token_directory: crate::cas_projection::RuntimeTokenDirectory::from_admitted(
            beryl_model::AdmittedHostPath::from_admitted(
                beryl_model::PathFlavor::Windows,
                r"C:\tokens",
            )
            .unwrap(),
        ),
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
    let (directory, mut candidate, storage, _, state) = candidate_with_state();
    let reference = candidate.service_reference();
    let (provider, sessions) = ProcessScheduledExecutionProvider::new();
    let prepared = PreparedCasServices::prepare(
        Default::default(),
        &mut candidate,
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
            &mut candidate,
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
    drop(candidate);
    assert_reopens(&directory);
}

#[test]
fn reopened_candidate_cannot_configure_services_from_retired_custody() {
    let (directory, mut candidate, storage, _, _) = candidate_with_state();
    let home_id = candidate.home_id();
    let generation = candidate.generation();
    let (provider, sessions) = ProcessScheduledExecutionProvider::new();
    let prepared = PreparedCasServices::prepare(
        Default::default(),
        &mut candidate,
        storage,
        config(),
        Box::new(provider),
    )
    .unwrap();
    let signal = prepared.service.as_ref().unwrap().scheduler_signal.clone();
    drop(candidate);
    let mut candidate = HomeOpenCandidate::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let _storage = SyndicStorage::register(&mut candidate).unwrap();
    let state = BerylState::register(&mut candidate).unwrap();
    let mut candidate = candidate
        .prepare_publication(
            BerylState::required_domains()
                .unwrap()
                .merge(SyndicStorage::required_domains().unwrap())
                .unwrap(),
        )
        .unwrap();
    assert_eq!(candidate.home_id(), home_id);
    assert_eq!(candidate.generation(), generation);
    let enrollments = crate::runtime_activity_enrollment::RuntimeActivityEnrollmentOperations::new(
        home_id,
        NonZeroUsize::new(1).unwrap(),
    );
    let result = prepared.configure_managed_sessions(
        &mut candidate,
        &sessions,
        interest(),
        enrollments,
        session_config(&state),
        &Arc::new(ProcessLifecycleAttentionPool::new()),
    );
    assert!(matches!(
        result,
        Err(CasPreparationError::Service(
            ProjectionCoordinatorError::SyndicRevisionUnavailable { .. }
        ))
    ));
    assert!(signal.diagnostics().stopped());
    assert_eq!(candidate.health().state(), HomeHealthState::Opening);
    assert!(candidate.recovery_access().is_ok());
    drop(candidate);
    assert_reopens(&directory);
}

#[test]
fn configuration_rejection_retires_private_custody() {
    for failure in [
        "foreign_owner",
        "foreign_candidate",
        "foreign_assets",
        "foreign_runtime_domain",
        "confirmation",
    ] {
        let (directory, mut candidate, storage, faults, state) = candidate_with_state();
        let (_foreign_directory, mut foreign_candidate, _, _, foreign_state) =
            candidate_with_state();
        let (provider, sessions) = ProcessScheduledExecutionProvider::new();
        let (_unused_provider, foreign_sessions) = ProcessScheduledExecutionProvider::new();
        let prepared = PreparedCasServices::prepare(
            Default::default(),
            &mut candidate,
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
        if failure == "foreign_runtime_domain" {
            configuration.runtime_roots = foreign_state.runtime_roots();
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
            if failure == "foreign_candidate" {
                &mut foreign_candidate
            } else {
                &mut candidate
            },
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
        match failure {
            "foreign_candidate" => assert!(matches!(
                result,
                Err(CasPreparationError::CandidateIdentity)
            )),
            "confirmation" => assert!(matches!(
                result,
                Err(CasPreparationError::Service(
                    ProjectionCoordinatorError::SyndicRevisionUnavailable { .. }
                ))
            )),
            _ => assert!(
                matches!(result, Err(CasPreparationError::Session(error)) if error == expected),
                "{failure}"
            ),
        }
        assert!(signal.diagnostics().stopped());
        drop(candidate);
        assert_reopens(&directory);
    }
}
