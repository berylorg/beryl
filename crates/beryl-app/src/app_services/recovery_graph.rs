use super::recovery_preparation::{
    PreparedRecoveryAppServices, RecoveryAppServicePreparationFailure,
};
use super::recovery_retirement::ServiceGraphRetirementError;
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
    private_clipboard: Option<crate::main_window::MainWindowPrivateClipboardOwner>,
    pub(super) failed_residents: Vec<super::recovery_failed_residents::FailedResidentSource>,
    process: ProcessAdmissionGate,
    pub(super) services: Option<PreparedRecoveryAppServices>,
    sessions: ScheduledExecutionSessions,
    attention: Option<Arc<ProcessLifecycleAttentionPool>>,
    pub(super) state: BerylState,
    pub(super) syndic: SyndicStorage,
    pub(super) recovered_window: Option<beryl_state::SessionWindowRemovalEvidence>,
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
    pub(crate) fn return_recovery_preparation_home(
        &mut self,
        expected: HomeGeneration,
        failure: &mut RecoveryServicePreparationError,
    ) -> Result<(), ServiceGraphRetirementError> {
        let home = match failure {
            RecoveryServicePreparationError::Refused(_) => None,
            RecoveryServicePreparationError::Cas(failure) => failure.retry_home_custody(),
            RecoveryServicePreparationError::App(failure) => failure.retry_home_custody(),
        }
        .ok_or(ServiceGraphRetirementError::InvalidHomeReturn)?;
        self.return_retired_service_home(expected, home)
    }

    pub(crate) fn prepare_recovery_service_graph(
        &mut self,
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
        if self.failed_markers.is_some() {
            return Err(reject("original marker custody is not settled".into()));
        }
        let state =
            BerylState::reacquire_candidate(retained).map_err(|error| reject(error.to_string()))?;
        let syndic = SyndicStorage::reacquire_candidate(retained)
            .map_err(|error| reject(error.to_string()))?;
        let (provider, sessions) = ProcessScheduledExecutionProvider::new();
        let mut prepared = PreparedRecoveryServiceGraph {
            private_clipboard: Some(
                crate::main_window::MainWindowPrivateClipboardOwner::with_paste_resources(
                    configuration.paste_resources,
                ),
            ),
            failed_residents: Vec::new(),
            process: self.process.clone(),
            services: None,
            sessions,
            attention: Some(Arc::new(ProcessLifecycleAttentionPool::new())),
            state,
            syndic,
            recovered_window: None,
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
                    token_directory: configuration.token_directory.clone(),
                },
                prepared
                    .attention
                    .as_ref()
                    .expect("prepared attention owner"),
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
        prepared.failed_residents = std::mem::take(&mut self.failed_residents);
        Ok(prepared)
    }
}

impl PreparedRecoveryServiceGraph {
    pub(crate) fn retain_recovered_window(
        &mut self,
        original: &crate::running_owner::RunningShutdownSession,
    ) {
        self.recovered_window = match original {
            crate::running_owner::RunningShutdownSession::RemovedWindow(close) => {
                close.restored_evidence().cloned()
            }
            _ => None,
        };
    }

    pub(crate) fn revalidate_interrupted_exit_session(
        &mut self,
        home: BerylHomeId,
        generation: HomeGeneration,
        original: &crate::running_owner::RunningShutdownSession,
    ) -> Result<(), String> {
        let (candidate, _) = self
            .services
            .as_mut()
            .expect("prepared recovery services")
            .cas
            .as_mut()
            .expect("prepared recovery CAS custody")
            .app_preparation_parts()
            .ok_or("recovery handoff is unavailable")?;
        if candidate.home_id() != home || candidate.generation() != generation {
            return Err("session validation belongs to another recovery candidate".into());
        }
        original.revalidate_candidate(candidate, &self.state.session())?;
        self.recovered_window = match original {
            crate::running_owner::RunningShutdownSession::RemovedWindow(close) => {
                close.restored_evidence().cloned()
            }
            _ => None,
        };
        Ok(())
    }

    pub(crate) fn composer_recovery_read(
        &mut self,
        source: &crate::main_window::MainWindowComposerCandidateSource,
        effect: &gpui_text_input::RangePrepublicationEffect,
    ) -> Result<crate::main_window::MainWindowComposerCandidateRead, String> {
        self.services
            .as_mut()
            .expect("prepared recovery services")
            .composer_recovery_read(source, effect)
    }

    pub(crate) fn composer_recovery_source(
        &mut self,
        retired: crate::main_window::MainWindowComposerRetiredClose,
        seed: gpui_text_input::RangeRestorationSeed,
    ) -> Result<
        crate::main_window::MainWindowComposerCandidateSource,
        (crate::main_window::MainWindowComposerRetiredClose, String),
    > {
        self.services
            .as_mut()
            .expect("prepared recovery services")
            .composer_recovery_source(
                &self.state,
                self.syndic.clone(),
                retired,
                seed,
                self.recovered_window.as_ref(),
            )
    }

    pub(crate) fn appearance(&self) -> Arc<crate::theme_runtime::AppearanceGeneration> {
        self.services
            .as_ref()
            .expect("prepared recovery services")
            .appearance()
    }

    pub(crate) fn matches_candidate(
        &mut self,
        home: BerylHomeId,
        generation: HomeGeneration,
    ) -> bool {
        self.services
            .as_mut()
            .expect("prepared recovery services")
            .matches_candidate(home, generation)
    }

    pub(crate) fn threadless_recovery_window(
        &mut self,
        retired_home: BerylHomeId,
        retired_generation: HomeGeneration,
        window: beryl_model::WindowId,
    ) -> Result<super::recovery_threadless::ThreadlessRecoveryWindow, String> {
        self.services
            .as_mut()
            .expect("prepared recovery services")
            .threadless_recovery_window(&self.state, retired_home, retired_generation, window)
    }

    pub(crate) fn composer_recovery_adapters(
        &mut self,
        home: BerylHomeId,
        generation: HomeGeneration,
        requirement: beryl_home_store::TurnStartAdmissionRequirement,
    ) -> Result<super::recovery_composer::PreparedComposerRecoveryAdapters, String> {
        self.services
            .as_mut()
            .expect("prepared recovery services")
            .composer_recovery_adapters(home, generation, self.state.assets(), requirement)
            .map(|adapters| {
                adapters.with_private_clipboard_owner(
                    self.private_clipboard
                        .as_ref()
                        .expect("prepared clipboard owner")
                        .clone(),
                )
            })
    }

    pub(crate) fn cancel(mut self) -> RecoveryAppServicePreparationFailure {
        self.services
            .take()
            .expect("prepared recovery services")
            .cancel()
    }
}

impl Drop for PreparedRecoveryServiceGraph {
    fn drop(&mut self) {
        if let Some(private_clipboard) = self.private_clipboard.take() {
            private_clipboard.retire();
        }
        drop(self.services.take());
        if let Some(attention) = self.attention.take() {
            attention.close();
        }
    }
}

mod publication;

#[cfg(all(test, feature = "test-faults", target_os = "windows"))]
#[path = "../../tests/unit/app_services/recovery_graph_resident_support.rs"]
pub(crate) mod resident_test_support;

#[cfg(all(test, feature = "test-faults", target_os = "windows"))]
#[path = "../../tests/unit/app_services/recovery_graph_resident.rs"]
mod resident_tests;
