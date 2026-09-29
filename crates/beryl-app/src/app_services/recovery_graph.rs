use super::recovery_preparation::{
    PreparedRecoveryAppServices, RecoveryAppServicePreparationFailure,
};
use super::*;
use crate::{
    cas_projection::{
        PreparedRecoveryCasServices, ProcessScheduledExecutionProvider,
        ProjectionCancellationToken, RecoveryCasPreparationFailure,
    },
    discussion_settlement::DiscussionSettlementService,
};
use beryl_home_store::{HomeGeneration, HomeRecoveryCandidate};

pub(crate) struct PreparedRecoveryServiceGraph {
    services: Option<PreparedRecoveryAppServices>,
    sessions: ScheduledExecutionSessions,
    attention: Arc<ProcessLifecycleAttentionPool>,
    state: BerylState,
    syndic: SyndicStorage,
}

pub(crate) enum RecoveryServicePreparationError {
    Refused(String),
    Cas(RecoveryCasPreparationFailure),
    App(RecoveryAppServicePreparationFailure),
}

impl std::fmt::Debug for RecoveryServicePreparationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Refused(error) => formatter.debug_tuple("Refused").field(error).finish(),
            Self::Cas(failure) => formatter.debug_tuple("Cas").field(failure.error()).finish(),
            Self::App(failure) => formatter.debug_tuple("App").field(failure).finish(),
        }
    }
}

impl ProcessServiceOwner {
    pub(crate) fn prepare_recovery_service_graph(
        &self,
        expected: HomeGeneration,
        candidate: &mut Option<HomeRecoveryCandidate>,
        configuration: AppServiceConfiguration,
        at: SyndicTimestamp,
        cancellation: &CommandCancellation,
    ) -> Result<PreparedRecoveryServiceGraph, RecoveryServicePreparationError> {
        let reject = |error: String| RecoveryServicePreparationError::Refused(error);
        let retained = candidate
            .as_ref()
            .ok_or_else(|| reject("recovery candidate is unavailable".into()))?;
        self.validate_retired_service_home_return(expected, Some(retained.home_id()))
            .map_err(|error| reject(error.to_string()))?;
        if retained.generation() == expected {
            return Err(reject(
                "replacement requires a fresh candidate generation".into(),
            ));
        }
        check_cancellation(cancellation).map_err(|error| reject(error.to_string()))?;
        self.require_settled_custody()
            .map_err(|error| reject(error.to_string()))?;
        let state =
            BerylState::reacquire_candidate(retained).map_err(|error| reject(error.to_string()))?;
        let syndic = SyndicStorage::reacquire_candidate(retained)
            .map_err(|error| reject(error.to_string()))?;
        let (provider, sessions) = ProcessScheduledExecutionProvider::new();
        let mut prepared = PreparedRecoveryServiceGraph {
            services: None,
            sessions,
            attention: Arc::new(ProcessLifecycleAttentionPool::new()),
            state,
            syndic,
        };
        let settlement = DiscussionSettlementService::new(
            self.settlements.clone(),
            retained.service_reference(),
            prepared.state.clone(),
            prepared.syndic.clone(),
        );
        let projection_cancellation = ProjectionCancellationToken::new();
        let cas = PreparedRecoveryCasServices::prepare(
            self.process.clone(),
            candidate.take().expect("validated recovery candidate"),
            prepared.syndic.clone(),
            configuration.projection.clone(),
            Box::new(provider.with_discussion_settlement(settlement)),
            &projection_cancellation,
        )
        .map_err(RecoveryServicePreparationError::Cas)?;
        if cancellation.is_cancelled() {
            return Err(RecoveryServicePreparationError::Cas(cas.cancel()));
        }
        let cas = cas
            .configure_managed_sessions(
                &prepared.sessions,
                configuration.runtime_interest.clone(),
                self.enrollments.clone(),
                RuntimeSessionPreparationConfig {
                    runtime_roots: prepared.state.runtime_roots(),
                    assets: prepared.state.assets(),
                    policy: configuration.session_policy.clone(),
                    token_directories: configuration.token_directories.clone(),
                },
                &prepared.attention,
                &projection_cancellation,
            )
            .map_err(RecoveryServicePreparationError::Cas)?;
        let cas = cas
            .prepare_handoff(
                self.settlements.clone(),
                prepared.state.clone(),
                configuration.handoff,
                at,
                cancellation.clone(),
            )
            .map_err(RecoveryServicePreparationError::Cas)?;
        prepared.services = Some(
            PreparedRecoveryAppServices::prepare(
                cas,
                &prepared.state,
                prepared.syndic.clone(),
                configuration,
                cancellation,
            )
            .map_err(RecoveryServicePreparationError::App)?,
        );
        Ok(prepared)
    }
}

impl PreparedRecoveryServiceGraph {
    pub(crate) fn cancel(mut self) -> RecoveryAppServicePreparationFailure {
        self.services
            .take()
            .expect("prepared recovery services")
            .cancel()
    }
}

impl Drop for PreparedRecoveryServiceGraph {
    fn drop(&mut self) {
        drop(self.services.take());
        self.attention.close();
    }
}
