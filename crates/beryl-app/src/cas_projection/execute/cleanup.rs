use beryl_home_store::HomeStore;
use beryl_model::{
    CasConversationToolProfile, CasLoadedSessionGeneration, CasNativeTurnCount, CasThreadId,
    ExecutionBinding,
};
use syndic_storage::{
    CasLineageProof, CasRepresentedPrefixProof, NativeProjectionBasis, PublishStaleBinding,
    StaleCasBinding, SyndicStorage,
};

use crate::cas_projection::{
    AdmittedProjectionSession, CasProjectionCoordinator, CasProjectionRequest,
    ProjectionExecutionError, ProjectionPublicationFailure, connection::LoadedProjectionLease,
    publication,
};

use super::support::point_limit;

pub(super) struct StaleObservation {
    pub(super) execution: Option<ExecutionBinding>,
    pub(super) represented_prefix: Option<CasRepresentedPrefixProof>,
    pub(super) tool_profile: Option<CasConversationToolProfile>,
    pub(super) lineage: Option<CasLineageProof>,
    pub(super) native_turn_count: Option<CasNativeTurnCount>,
    pub(super) loaded_generation: Option<CasLoadedSessionGeneration>,
}

impl StaleObservation {
    pub(super) const fn unknown(loaded_generation: Option<CasLoadedSessionGeneration>) -> Self {
        Self {
            execution: None,
            represented_prefix: None,
            tool_profile: None,
            lineage: None,
            native_turn_count: None,
            loaded_generation,
        }
    }

    pub(super) const fn exact(
        execution: ExecutionBinding,
        represented_prefix: CasRepresentedPrefixProof,
        tool_profile: CasConversationToolProfile,
        lineage: CasLineageProof,
        native_turn_count: CasNativeTurnCount,
        loaded_generation: Option<CasLoadedSessionGeneration>,
    ) -> Self {
        Self {
            execution: Some(execution),
            represented_prefix: Some(represented_prefix),
            tool_profile: Some(tool_profile),
            lineage: Some(lineage),
            native_turn_count: Some(native_turn_count),
            loaded_generation,
        }
    }
}

impl CasProjectionCoordinator {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn abandon_projection_target(
        &self,
        home: &HomeStore,
        storage: &SyndicStorage,
        _session: &AdmittedProjectionSession,
        request: &CasProjectionRequest,
        basis: NativeProjectionBasis,
        cas_thread_id: CasThreadId,
        observation: StaleObservation,
        reason: &'static str,
        primary: ProjectionExecutionError,
        lease: Option<LoadedProjectionLease>,
    ) -> ProjectionExecutionError {
        let release = lease.and_then(|lease| lease.release().err().map(Box::new));
        let publication = self
            .publish_abandoned_target(
                home,
                storage,
                request,
                basis,
                cas_thread_id,
                observation,
                reason,
            )
            .err();

        if release.is_none() && publication.is_none() {
            primary
        } else {
            ProjectionExecutionError::AbandonmentFailed {
                primary: Box::new(primary),
                release,
                publication: publication.map(Box::new),
            }
        }
    }

    pub(super) fn forget_after_publication_failure(
        &self,
        session: &AdmittedProjectionSession,
        request: &CasProjectionRequest,
        cas_thread_id: &CasThreadId,
        lease: LoadedProjectionLease,
        primary: ProjectionExecutionError,
    ) -> ProjectionExecutionError {
        let _ = (session, request, cas_thread_id);
        match lease.release() {
            Ok(_) => primary,
            Err(release) => ProjectionExecutionError::AbandonmentFailed {
                primary: Box::new(primary),
                release: Some(Box::new(release)),
                publication: None,
            },
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn publish_abandoned_target(
        &self,
        home: &HomeStore,
        storage: &SyndicStorage,
        request: &CasProjectionRequest,
        basis: NativeProjectionBasis,
        cas_thread_id: CasThreadId,
        observation: StaleObservation,
        reason: &'static str,
    ) -> Result<beryl_model::BindingRevision, ProjectionPublicationFailure> {
        self.ensure_home(home)
            .map_err(ProjectionPublicationFailure::HomeAuthorityLost)?;
        let stale = StaleCasBinding::new(
            observation
                .execution
                .unwrap_or_else(|| request.execution_binding().clone()),
            cas_thread_id,
            observation.tool_profile,
            observation.represented_prefix,
            observation.lineage,
            observation.native_turn_count,
            observation.loaded_generation,
            reason,
            request.observed_at(),
        )
        .map_err(ProjectionPublicationFailure::StaleRecord)?;
        publication::publish_stale(
            home,
            storage,
            &PublishStaleBinding::new(
                request.thread_id(),
                basis.expected_binding_revision(),
                basis.selected_path(),
                stale,
            ),
            point_limit(),
        )
    }
}
