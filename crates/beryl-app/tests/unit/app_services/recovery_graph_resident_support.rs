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
    let runtime_setup = super::super::runtime_setup::RuntimeSetupService::prepare(
        Arc::new(candidate.service_reference()),
        state.clone(),
        syndic.clone(),
        owner.windows.clone(),
        &configuration,
    )
    .unwrap();
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
                wsl_supervisor_artifact: None,
                runtime_roots: state.runtime_roots(),
                assets: state.assets(),
                policy: configuration.session_policy.clone(),
                token_directory: crate::cas_projection::RuntimeTokenDirectory::from_admitted(
                    beryl_model::AdmittedHostPath::from_admitted(
                        beryl_model::PathFlavor::Windows,
                        r"C:\tokens",
                    )
                    .unwrap(),
                ),
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
        runtime_setup: Some(runtime_setup),
        first_composer: None,
        first_configurator: None,
        first_transcript: None,
        private_clipboard: Some(crate::main_window::MainWindowPrivateClipboardOwner::new()),
        failed_residents: Vec::new(),
        failed_claims: Vec::new(),
        claim_targets: Vec::new(),
        return_slot: owner.recovery_graph_return_slot(),
        recovered_window: None,
        process: owner.process.clone(),
        services: Some(services),
        sessions,
        attention: Some(attention),
        state,
        syndic,
    }
}
