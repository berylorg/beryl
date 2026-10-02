use super::*;

impl PreparedRecoveryServiceGraph {
    pub(crate) fn test_retain_recovery_provider(&self) -> impl Send + 'static + use<> {
        self.services
            .as_ref()
            .unwrap()
            .cas
            .as_ref()
            .unwrap()
            .test_retain_recovery_provider()
    }
}

pub(crate) fn prepared(candidate: HomeRecoveryCandidate) -> PreparedRecoveryServiceGraph {
    let one = NonZeroUsize::new(1).unwrap();
    let owner = ProcessServiceOwner::new(candidate.home_id(), one, one);
    let state = BerylState::reacquire_candidate(&candidate).unwrap();
    let syndic = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let configuration = crate::app_services::tests::configuration();
    let (provider, sessions) = ProcessScheduledExecutionProvider::new();
    let attention = Arc::new(ProcessLifecycleAttentionPool::new());
    let cas = PreparedRecoveryCasServices::prepare(
        owner.process.clone(),
        candidate,
        syndic.clone(),
        configuration.projection.clone(),
        Box::new(provider),
        &ProjectionCancellationToken::new(),
    )
    .unwrap_or_else(|error| panic!("CAS preparation: {}", error.error()));
    let cas = cas
        .configure_managed_sessions(
            &sessions,
            configuration.runtime_interest.clone(),
            owner.enrollments.clone(),
            RuntimeSessionPreparationConfig {
                runtime_roots: state.runtime_roots(),
                assets: state.assets(),
                policy: configuration.session_policy.clone(),
                token_directories: Vec::new(),
            },
            &attention,
            &ProjectionCancellationToken::new(),
        )
        .unwrap_or_else(|error| panic!("session preparation: {}", error.error()));
    let cas = cas
        .prepare_handoff(
            owner.settlements.clone(),
            state.clone(),
            configuration.handoff,
            SyndicTimestamp::from_unix_millis(2),
            CommandCancellation::new(),
        )
        .unwrap_or_else(|error| panic!("handoff preparation: {}", error.error()));
    let services = PreparedRecoveryAppServices::prepare(
        cas,
        &state,
        syndic.clone(),
        configuration,
        &CommandCancellation::new(),
    )
    .unwrap();
    PreparedRecoveryServiceGraph {
        failed_residents: Vec::new(),
        recovered_window: None,
        process: owner.process.clone(),
        services: Some(services),
        sessions,
        attention: Some(attention),
        state,
        syndic,
    }
}
