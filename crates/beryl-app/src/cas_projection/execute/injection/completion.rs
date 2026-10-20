use super::*;
use crate::cas_projection::connection::LoadedProjectionLease;
use beryl_backend::LoadedThreadSession;

impl CasProjectionCoordinator {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn complete_projection_injection(
        &self,
        home: &HomeStore,
        storage: &SyndicStorage,
        session: &mut AdmittedProjectionSession,
        request: &CasProjectionRequest,
        cancellation: &ProjectionCancellationToken,
        binding: ProjectionInjectionBinding,
        thread: LoadedThreadSession,
        lease: LoadedProjectionLease,
        routing_failure: Option<ConnectionRoutingFailure>,
    ) -> Result<LoadedCasProjection, ProjectionExecutionError> {
        let ProjectionInjectionBinding {
            basis,
            projection,
            establishment,
            attachment,
        } = binding;
        let generation = lease.generation();
        let native_turn_count = establishment.native_turn_count();
        let completed_at = match completion_timestamp(
            self.home_id(),
            request.thread_id(),
            thread.thread_id(),
            generation.process(),
        ) {
            Ok(completed_at) => completed_at,
            Err(primary) => {
                return Err(self.abandon_projection_target(
                    home,
                    storage,
                    session,
                    request,
                    basis,
                    thread.thread_id().clone(),
                    StaleObservation {
                        execution: Some(request.execution_binding().clone()),
                        represented_prefix: Some(projection.represented_prefix()),
                        tool_profile: Some(basis.tool_profile()),
                        lineage: establishment.native_lineage(),
                        native_turn_count: Some(native_turn_count),
                        loaded_generation: Some(generation),
                    },
                    "recovery injection completed but completion time could not be observed",
                    primary,
                    Some(lease),
                ));
            }
        };
        let lineage = match establishment {
            ProjectionInjectionEstablishment::Replay => {
                projection.completed_lineage(completed_at, generation)
            }
            ProjectionInjectionEstablishment::NativeSeed { .. } => {
                ContextInjectionObservation::new(
                    projection.item_count(),
                    projection.utf8_bytes(),
                    projection.sequence_digest(),
                    completed_at,
                )
                .and_then(|observation| {
                    basis
                        .context()
                        .expect("native seed requires the prepared context witness")
                        .complete_native(NativeCasLineage::Fork, generation, Some(observation))
                })
            }
        };
        let lineage = match lineage {
            Ok(proof) => proof,
            Err(error) => {
                return Err(self.abandon_projection_target(
                    home,
                    storage,
                    session,
                    request,
                    basis,
                    thread.thread_id().clone(),
                    StaleObservation {
                        execution: Some(request.execution_binding().clone()),
                        represented_prefix: Some(projection.represented_prefix()),
                        tool_profile: Some(basis.tool_profile()),
                        lineage: establishment.native_lineage(),
                        native_turn_count: Some(native_turn_count),
                        loaded_generation: Some(generation),
                    },
                    "recovery injection completed but its lineage proof was invalid",
                    ProjectionExecutionError::from(error),
                    Some(lease),
                ));
            }
        };

        if let Some(failure) = routing_failure {
            let primary = routing_failure_error(session, failure);
            return Err(self.abandon_projection_target(
                home,
                storage,
                session,
                request,
                basis,
                thread.thread_id().clone(),
                StaleObservation::exact(
                    request.execution_binding().clone(),
                    projection.represented_prefix(),
                    basis.tool_profile(),
                    lineage,
                    native_turn_count,
                    Some(generation),
                ),
                "recovery injection completed but ordered routing authority was lost",
                primary,
                Some(lease),
            ));
        }
        if let Err(error) = self.ensure_home(home) {
            let primary = ProjectionExecutionError::Coordinator(error);
            return Err(self.abandon_projection_target(
                home,
                storage,
                session,
                request,
                basis,
                thread.thread_id().clone(),
                StaleObservation::exact(
                    request.execution_binding().clone(),
                    projection.represented_prefix(),
                    basis.tool_profile(),
                    lineage,
                    native_turn_count,
                    Some(generation),
                ),
                "recovery injection completed after home authority was lost",
                primary,
                Some(lease),
            ));
        }
        let current_attachment = session.connection().projection_source_attachment();
        let failure = if cancellation.is_cancelled() {
            Some(ProjectionExecutionError::Cancelled)
        } else {
            match current_attachment {
                Ok(current) if current == attachment => None,
                Ok(_) => Some(
                    ProjectionCoordinatorError::ProjectionConnectionUnavailable {
                        runtime_id: session.runtime_id(),
                        process_generation: session.process_generation(),
                    }
                    .into(),
                ),
                Err(error) => Some(error.into()),
            }
        };
        if let Some(primary) = failure {
            return Err(self.abandon_projection_target(
                home,
                storage,
                session,
                request,
                basis,
                thread.thread_id().clone(),
                StaleObservation::exact(
                    request.execution_binding().clone(),
                    projection.represented_prefix(),
                    basis.tool_profile(),
                    lineage,
                    native_turn_count,
                    Some(generation),
                ),
                "injection completed after its original operation authority ended",
                primary,
                Some(lease),
            ));
        }
        let publication = match establishment {
            ProjectionInjectionEstablishment::NativeSeed { .. } => {
                PublishValidBinding::from_native(
                    basis,
                    request.execution_binding().clone(),
                    thread.thread_id().clone(),
                    native_turn_count,
                    lineage,
                )
            }
            ProjectionInjectionEstablishment::Replay => PublishValidBinding::from_recovery(
                projection,
                basis,
                request.execution_binding().clone(),
                thread.thread_id().clone(),
                native_turn_count,
                lineage,
            ),
        };
        match publication::publish_valid(home, storage, &publication, point_limit()) {
            Ok(revision) => Ok(self.capability(
                request,
                revision,
                thread.thread_id().clone(),
                lease,
                lineage,
            )),
            Err(error) => {
                let primary = ProjectionExecutionError::from(error);
                Err(self.abandon_projection_target(
                    home,
                    storage,
                    session,
                    request,
                    basis,
                    thread.thread_id().clone(),
                    StaleObservation::exact(
                        request.execution_binding().clone(),
                        projection.represented_prefix(),
                        basis.tool_profile(),
                        lineage,
                        native_turn_count,
                        Some(generation),
                    ),
                    "recovered CAS thread could not be published",
                    primary,
                    Some(lease),
                ))
            }
        }
    }
}
