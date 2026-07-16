use std::path::Path;

use beryl_backend::{
    ThreadInjectionBatch, ThreadInjectionItem, ThreadInjectionOutcome, ThreadStatus,
};
use beryl_home_store::HomeStore;
use beryl_model::CasNativeTurnCount;
use syndic_storage::{
    CasLineageProof, NativeProjectionBasis, PublishValidBinding, RecoveredInjectionProof,
    RecoveryAssembly, RecoveryItem, RecoveryProjectionRequest, SyndicStorage,
};

use crate::cas_projection::{
    AdmittedProjectionSession, CasProjectionCoordinator, CasProjectionRequest, LoadedCasProjection,
    ProjectionCancellationToken, ProjectionExecutionError, publication,
};

use super::{
    cleanup::StaleObservation,
    support::{completion_timestamp, point_limit},
};

impl CasProjectionCoordinator {
    pub(super) fn recover_projection(
        &self,
        home: &HomeStore,
        storage: SyndicStorage,
        session: &mut AdmittedProjectionSession,
        request: &CasProjectionRequest,
        cancellation: &ProjectionCancellationToken,
        basis: NativeProjectionBasis,
    ) -> Result<LoadedCasProjection, ProjectionExecutionError> {
        let assembly = storage.prepare_recovery_projection(
            home,
            RecoveryProjectionRequest::for_pending_selected_turn_parent(
                request.thread_id(),
                request.selected_path(),
                request.model_context_window_tokens(),
            ),
        )?;
        let RecoveryAssembly::Ready(projection) = assembly else {
            return Err(ProjectionExecutionError::UnexpectedNativeEmptyRecovery);
        };
        let batch = recovery_batch(projection.items())?;
        if cancellation.is_cancelled() {
            return Err(ProjectionExecutionError::Cancelled);
        }

        let root_path = request.execution_binding().root_path().as_str().to_owned();
        let thread_options = request.thread_options().clone();
        let timeout = request.timeout();
        let fresh = session.call(move |backend| {
            backend.start_thread_with_options(Path::new(&root_path), thread_options, timeout)
        })?;
        let cas_thread_id = fresh.thread_id().clone();
        let lease = match session.connection().register_new(
            cas_thread_id.clone(),
            request.thread_id(),
            request.timeout(),
        ) {
            Ok(lease) => lease,
            Err(error) => {
                session.invalidate_connection();
                let primary = ProjectionExecutionError::Coordinator(error);
                return Err(self.abandon_projection_target(
                    home,
                    storage,
                    session,
                    request,
                    basis,
                    cas_thread_id,
                    StaleObservation::unknown(None),
                    "recovery target could not enter the loaded registry",
                    primary,
                    None,
                ));
            }
        };
        if fresh.status() != &ThreadStatus::Idle {
            let primary = ProjectionExecutionError::ProjectionThreadNotIdle {
                thread_id: cas_thread_id.clone(),
                status: fresh.status().clone(),
            };
            return Err(self.abandon_projection_target(
                home,
                storage,
                session,
                request,
                basis,
                cas_thread_id,
                StaleObservation::unknown(None),
                "recovery target was not idle",
                primary,
                Some(lease),
            ));
        }

        let target = match fresh.into_idle() {
            Ok(target) => target,
            Err(error) => {
                let primary = ProjectionExecutionError::ProjectionThreadNotIdle {
                    thread_id: error.thread_id().clone(),
                    status: error.status().clone(),
                };
                return Err(self.abandon_projection_target(
                    home,
                    storage,
                    session,
                    request,
                    basis,
                    cas_thread_id,
                    StaleObservation::unknown(Some(lease.generation())),
                    "recovery target idle typestate conversion failed",
                    primary,
                    Some(lease),
                ));
            }
        };
        self.ensure_home(home)?;
        let generation = lease.generation();
        if cancellation.is_cancelled() {
            return Err(self.abandon_projection_target(
                home,
                storage,
                session,
                request,
                basis,
                cas_thread_id,
                StaleObservation::unknown(Some(generation)),
                "recovery was cancelled before injection",
                ProjectionExecutionError::Cancelled,
                Some(lease),
            ));
        }

        let timeout = request.timeout();
        let outcome = match session.call(move |backend| {
            Ok::<_, beryl_backend::ManagedBackendError>(
                backend.inject_thread_items(target, &batch, timeout),
            )
        }) {
            Ok(outcome) => outcome,
            Err(primary) => {
                return Err(self.abandon_projection_target(
                    home,
                    storage,
                    session,
                    request,
                    basis,
                    cas_thread_id,
                    StaleObservation::unknown(Some(generation)),
                    "recovery target connection was lost before injection dispatch",
                    primary,
                    Some(lease),
                ));
            }
        };
        match outcome {
            ThreadInjectionOutcome::Succeeded { thread } => {
                let completed_at = match completion_timestamp() {
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
                                lineage: None,
                                native_turn_count: Some(CasNativeTurnCount::ZERO),
                                loaded_generation: Some(generation),
                            },
                            "recovery injection completed but completion time could not be observed",
                            primary,
                            Some(lease),
                        ));
                    }
                };
                let proof = match RecoveredInjectionProof::new(
                    projection.version(),
                    projection.represented_prefix(),
                    projection.sequence_digest(),
                    projection.item_count(),
                    projection.utf8_bytes(),
                    completed_at,
                    generation,
                ) {
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
                                lineage: None,
                                native_turn_count: Some(CasNativeTurnCount::ZERO),
                                loaded_generation: Some(generation),
                            },
                            "recovery injection completed but its lineage proof was invalid",
                            ProjectionExecutionError::from(error),
                            Some(lease),
                        ));
                    }
                };
                let lineage = CasLineageProof::recovered(proof);
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
                            CasNativeTurnCount::ZERO,
                            Some(generation),
                        ),
                        "recovery injection completed after home authority was lost",
                        primary,
                        Some(lease),
                    ));
                }
                let publication = PublishValidBinding::new(
                    request.thread_id(),
                    basis.expected_binding_revision(),
                    basis.selected_path(),
                    request.execution_binding().clone(),
                    thread.thread_id().clone(),
                    projection.represented_prefix(),
                    CasNativeTurnCount::ZERO,
                    basis.tool_profile(),
                    lineage,
                );
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
                                CasNativeTurnCount::ZERO,
                                Some(generation),
                            ),
                            "recovered CAS thread could not be published",
                            primary,
                            Some(lease),
                        ))
                    }
                }
            }
            ThreadInjectionOutcome::Rejected {
                thread_id,
                rejection,
            } => {
                let primary = ProjectionExecutionError::InjectionRejected {
                    thread_id: thread_id.clone(),
                    code: rejection.code(),
                    message: rejection.message().into(),
                    data_was_present: rejection.data_was_present(),
                };
                Err(self.abandon_projection_target(
                    home,
                    storage,
                    session,
                    request,
                    basis,
                    thread_id,
                    StaleObservation::unknown(Some(generation)),
                    "recovery injection was rejected",
                    primary,
                    Some(lease),
                ))
            }
            ThreadInjectionOutcome::TransportLost { thread_id, error } => {
                session.invalidate_connection();
                let primary = ProjectionExecutionError::InjectionTransportLost {
                    thread_id: thread_id.clone(),
                    source: error,
                };
                Err(self.abandon_projection_target(
                    home,
                    storage,
                    session,
                    request,
                    basis,
                    thread_id,
                    StaleObservation::unknown(Some(generation)),
                    "recovery injection lost transport",
                    primary,
                    Some(lease),
                ))
            }
            ThreadInjectionOutcome::CompletionUnknown { thread_id, error } => {
                if error.invalidates_connection_authority() {
                    session.invalidate_connection();
                }
                let primary = ProjectionExecutionError::InjectionCompletionUnknown {
                    thread_id: thread_id.clone(),
                    source: error,
                };
                Err(self.abandon_projection_target(
                    home,
                    storage,
                    session,
                    request,
                    basis,
                    thread_id,
                    StaleObservation::unknown(Some(generation)),
                    "recovery injection completion was unknown",
                    primary,
                    Some(lease),
                ))
            }
        }
    }
}

fn recovery_batch(
    items: &[RecoveryItem],
) -> Result<ThreadInjectionBatch, ProjectionExecutionError> {
    let items = items
        .iter()
        .map(|item| match item {
            RecoveryItem::UserInputText(text) => {
                ThreadInjectionItem::user_input_text(text.as_ref())
            }
            RecoveryItem::AssistantOutputText(text) => {
                ThreadInjectionItem::assistant_output_text(text.as_ref())
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ThreadInjectionBatch::new(items)?)
}
