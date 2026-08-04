use beryl_backend::ThreadStatus;
use beryl_home_store::HomeStore;
use beryl_model::{CasNativeTurnCount, CasTurnId};
use syndic_storage::{
    CasLineageProof, NativeCasLineage, NativeProjectionBasis, NativeProjectionRequest,
    NativeProjectionSource, PublishStaleBinding, PublishValidBinding, StaleCasBinding,
    SyndicStorage,
};

use crate::cas_projection::connection::{ExistingLease, LoadedProjectionLease};
use crate::cas_projection::model::NativeLineageRetryOperation;
use crate::cas_projection::{
    AdmittedProjectionSession, CasProjectionCoordinator, CasProjectionRequest, LoadedCasProjection,
    NativeLineageRecoveryDecision, ProjectionCancellationToken, ProjectionCoordinatorError,
    ProjectionExecutionError, ProjectionPublicationFailure, connection::ThreadRetirement,
    publication,
};

use super::{
    cleanup::StaleObservation,
    native_retry::{NativeCallFailure, call_native_with_retry},
    support::{point_limit, thread_load_options},
};

impl CasProjectionCoordinator {
    pub(super) fn publish_existing_loaded(
        &self,
        home: &HomeStore,
        storage: SyndicStorage,
        request: &CasProjectionRequest,
        basis: NativeProjectionBasis,
        source: NativeProjectionSource,
        lease: LoadedProjectionLease,
    ) -> Result<LoadedCasProjection, ProjectionExecutionError> {
        self.ensure_home(home)?;
        let publication = PublishValidBinding::new(
            request.thread_id(),
            basis.expected_binding_revision(),
            basis.selected_path(),
            request.execution_binding().clone(),
            source.binding().cas_thread_id().clone(),
            basis.represented_prefix(),
            source.binding().native_turn_count(),
            basis.tool_profile(),
            source.binding().lineage(),
        );
        let revision = publication::publish_valid(home, storage, &publication, point_limit())?;
        Ok(self.capability(
            request,
            revision,
            source.binding().cas_thread_id().clone(),
            lease,
            source.binding().lineage(),
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn resume_remote_source(
        &self,
        home: &HomeStore,
        storage: SyndicStorage,
        session: &mut AdmittedProjectionSession,
        request: &CasProjectionRequest,
        cancellation: &ProjectionCancellationToken,
        basis: NativeProjectionBasis,
        source: NativeProjectionSource,
    ) -> Result<LoadedCasProjection, ProjectionExecutionError> {
        let lineage = resumed_lineage(&source, basis)?;
        if cancellation.is_cancelled() {
            return Err(ProjectionExecutionError::Cancelled);
        }
        let cas_thread_id = source.binding().cas_thread_id().clone();
        let load_options = thread_load_options(request);
        let resume_thread_id = cas_thread_id.clone();
        let timeout = request.timeout();
        let loaded = match call_native_with_retry(session, cancellation, move |backend| {
            backend.resume_thread(&resume_thread_id, &load_options, timeout)
        }) {
            Ok(loaded) => loaded,
            Err(NativeCallFailure::Terminal(error)) => return Err(error),
            Err(NativeCallFailure::RetryExhausted {
                failed_attempts,
                last_failure,
            }) => {
                return Err(ProjectionExecutionError::NativeLineageRecoveryRequired {
                    decision: Box::new(NativeLineageRecoveryDecision::new(
                        self,
                        request.clone(),
                        basis,
                        source,
                        NativeLineageRetryOperation::Resume,
                        failed_attempts,
                        *last_failure,
                    )),
                });
            }
        };
        let lease = match session.register_loaded(
            cas_thread_id.clone(),
            request.thread_id(),
            request.timeout(),
        ) {
            Ok(lease) => lease,
            Err(error) => {
                session.invalidate_connection();
                return Err(self.abandon_projection_target(
                    home,
                    storage,
                    session,
                    request,
                    basis,
                    cas_thread_id,
                    StaleObservation::exact(
                        source.binding().execution().clone(),
                        source.binding().represented_prefix(),
                        source.binding().tool_profile(),
                        source.binding().lineage(),
                        source.binding().native_turn_count(),
                        source.binding().lineage().recovered_injection_generation(),
                    ),
                    "resumed CAS thread could not enter the loaded registry",
                    ProjectionExecutionError::Coordinator(error),
                    None,
                ));
            }
        };
        if loaded.status() != &ThreadStatus::Idle {
            let primary = ProjectionExecutionError::ProjectionThreadNotIdle {
                thread_id: cas_thread_id.clone(),
                status: loaded.status().clone(),
            };
            return Err(self.abandon_projection_target(
                home,
                storage,
                session,
                request,
                basis,
                cas_thread_id,
                StaleObservation::exact(
                    source.binding().execution().clone(),
                    source.binding().represented_prefix(),
                    source.binding().tool_profile(),
                    source.binding().lineage(),
                    source.binding().native_turn_count(),
                    Some(lease.generation()),
                ),
                "CAS resume returned a non-idle thread",
                primary,
                Some(lease),
            ));
        }

        self.ensure_home(home)?;
        let publication = PublishValidBinding::new(
            request.thread_id(),
            basis.expected_binding_revision(),
            basis.selected_path(),
            request.execution_binding().clone(),
            cas_thread_id.clone(),
            basis.represented_prefix(),
            source.binding().native_turn_count(),
            basis.tool_profile(),
            lineage,
        );
        match publication::publish_valid(home, storage, &publication, point_limit()) {
            Ok(revision) => Ok(self.capability(request, revision, cas_thread_id, lease, lineage)),
            Err(error) => {
                let primary = ProjectionExecutionError::from(error);
                Err(self.forget_after_publication_failure(
                    session,
                    request,
                    &cas_thread_id,
                    lease,
                    primary,
                ))
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn fork_native_projection(
        &self,
        home: &HomeStore,
        storage: SyndicStorage,
        session: &mut AdmittedProjectionSession,
        request: &CasProjectionRequest,
        cancellation: &ProjectionCancellationToken,
        basis: NativeProjectionBasis,
        source: NativeProjectionSource,
        through_turn: Option<&CasTurnId>,
        native_turn_count: CasNativeTurnCount,
    ) -> Result<LoadedCasProjection, ProjectionExecutionError> {
        let lineage = CasLineageProof::native(NativeCasLineage::Fork, basis.represented_prefix())?;
        let source_lease = if let Some(required) =
            source.binding().lineage().recovered_injection_generation()
        {
            let observed = session.acquire_loaded(
                source.binding().cas_thread_id(),
                source.thread_id(),
                request.timeout(),
            )?;
            match observed {
                ExistingLease::Exact(lease)
                    if lease.generation().process() == required.process() =>
                {
                    Some(lease)
                }
                ExistingLease::Exact(lease) => {
                    drop(lease);
                    return self.retire_recovered_fork_source_then_replan(
                        home,
                        storage,
                        session,
                        request,
                        cancellation,
                        basis,
                        source,
                        "recovered fork source managed process no longer matches",
                    );
                }
                ExistingLease::AnotherConnection => {
                    return Err(
                        ProjectionExecutionError::LoadedProjectionConnectionMismatch {
                            thread_id: source.binding().cas_thread_id().clone(),
                        },
                    );
                }
                ExistingLease::Quarantined => {
                    return Err(ProjectionExecutionError::ReacquisitionInProgress {
                        thread_id: source.binding().cas_thread_id().clone(),
                    });
                }
                ExistingLease::AnotherOwner { existing_owner } => {
                    return Err(crate::cas_projection::ProjectionCoordinatorError::CasThreadOwnerCollision {
                        runtime_id: session.runtime_id(),
                        process_generation: session.process_generation(),
                        cas_thread_id: source.binding().cas_thread_id().clone(),
                        existing_owner,
                        offered_owner: source.thread_id(),
                    }.into());
                }
                ExistingLease::Absent => {
                    return self.retire_recovered_fork_source_then_replan(
                        home,
                        storage,
                        session,
                        request,
                        cancellation,
                        basis,
                        source,
                        "recovered fork source loaded-session authority was lost",
                    );
                }
            }
        } else {
            None
        };
        if cancellation.is_cancelled() {
            return Err(ProjectionExecutionError::Cancelled);
        }
        let options = thread_load_options(request);
        let source_thread_id = source.binding().cas_thread_id().clone();
        let operation_through_turn = through_turn.cloned();
        let timeout = request.timeout();
        let fresh = match call_native_with_retry(session, cancellation, move |backend| {
            match operation_through_turn.as_ref() {
                Some(turn_id) => {
                    backend.fork_thread_through_turn(&source_thread_id, turn_id, &options, timeout)
                }
                None => backend.fork_thread(&source_thread_id, &options, timeout),
            }
        }) {
            Ok(fresh) => fresh,
            Err(NativeCallFailure::Terminal(error)) => return Err(error),
            Err(NativeCallFailure::RetryExhausted {
                failed_attempts,
                last_failure,
            }) => {
                return Err(ProjectionExecutionError::NativeLineageRecoveryRequired {
                    decision: Box::new(NativeLineageRecoveryDecision::new(
                        self,
                        request.clone(),
                        basis,
                        source,
                        NativeLineageRetryOperation::Fork {
                            through_turn: through_turn.cloned(),
                            native_turn_count,
                        },
                        failed_attempts,
                        *last_failure,
                    )),
                });
            }
        };
        let child = self.publish_fresh_native_target(
            home,
            storage,
            session,
            request,
            basis,
            fresh,
            native_turn_count,
            lineage,
            "forked CAS thread could not be published",
        )?;
        if let Some(source_lease) = source_lease {
            source_lease
                .release()
                .map_err(|error| ProjectionExecutionError::LeaseRelease(Box::new(error)))?;
        }
        Ok(child)
    }

    #[allow(clippy::too_many_arguments)]
    fn retire_recovered_fork_source_then_replan(
        &self,
        home: &HomeStore,
        storage: SyndicStorage,
        session: &mut AdmittedProjectionSession,
        request: &CasProjectionRequest,
        cancellation: &ProjectionCancellationToken,
        basis: NativeProjectionBasis,
        source: NativeProjectionSource,
        reason: &'static str,
    ) -> Result<LoadedCasProjection, ProjectionExecutionError> {
        let source_is_target = source.thread_id() == request.thread_id();
        let retirement_basis_revision = if source_is_target {
            basis.expected_binding_revision()
        } else {
            source.binding_revision()
        };
        let expected_retired_source_revision = retirement_basis_revision
            .checked_next()
            .map_err(|_| ProjectionPublicationFailure::BindingRevisionExhausted)?;
        let expected_target_revision = if source_is_target {
            expected_retired_source_revision
        } else {
            basis.expected_binding_revision()
        };
        let _source_flight = (source.thread_id() != request.thread_id())
            .then(|| self.begin_projection(source.thread_id()))
            .transpose()?;
        let retirement = session.retire_loaded_thread(
            source.binding().cas_thread_id(),
            source.thread_id(),
            request.timeout(),
        );
        let (release_error, retired_loaded_generation) = match retirement {
            Ok(ThreadRetirement::Absent) => (None, None),
            Ok(ThreadRetirement::Retired {
                generation,
                release_error,
            }) => (release_error, Some(generation)),
            Ok(ThreadRetirement::AnotherConnection) => {
                return Err(
                    ProjectionExecutionError::LoadedProjectionConnectionMismatch {
                        thread_id: source.binding().cas_thread_id().clone(),
                    },
                );
            }
            Ok(ThreadRetirement::AnotherOwner { existing_owner }) => {
                return Err(ProjectionCoordinatorError::CasThreadOwnerCollision {
                    runtime_id: session.runtime_id(),
                    process_generation: session.process_generation(),
                    cas_thread_id: source.binding().cas_thread_id().clone(),
                    existing_owner,
                    offered_owner: source.thread_id(),
                }
                .into());
            }
            Err(error) => (Some(error), None),
        };
        let stale = StaleCasBinding::new(
            source.binding().execution().clone(),
            source.binding().cas_thread_id().clone(),
            Some(source.binding().tool_profile()),
            Some(source.binding().represented_prefix()),
            Some(source.binding().lineage()),
            Some(source.binding().native_turn_count()),
            retired_loaded_generation,
            reason,
            request.observed_at(),
        )?;
        let retired_source_revision = publication::publish_stale(
            home,
            storage,
            &PublishStaleBinding::new(
                source.thread_id(),
                retirement_basis_revision,
                if source_is_target {
                    basis.selected_path()
                } else {
                    source.selected_path()
                },
                stale,
            ),
            point_limit(),
        )?;
        if retired_source_revision != expected_retired_source_revision {
            return Err(ProjectionExecutionError::ProjectionBasisChanged {
                thread_id: request.thread_id(),
            });
        }
        if let Some(error) = release_error {
            return Err(ProjectionExecutionError::LeaseRelease(Box::new(error)));
        }
        self.ensure_home(home)?;
        if cancellation.is_cancelled() {
            return Err(ProjectionExecutionError::Cancelled);
        }
        let replanned = storage.prepare_native_projection(
            home,
            &NativeProjectionRequest::new(
                request.thread_id(),
                request.selected_path(),
                request.execution_binding().clone(),
                basis.tool_profile(),
            ),
            point_limit(),
        )?;
        if replanned.basis().expected_binding_revision() != expected_target_revision {
            return Err(ProjectionExecutionError::ProjectionBasisChanged {
                thread_id: request.thread_id(),
            });
        }
        self.execute_plan(home, storage, session, request, cancellation, replanned)
    }
}

fn resumed_lineage(
    source: &NativeProjectionSource,
    basis: NativeProjectionBasis,
) -> Result<CasLineageProof, syndic_storage::SyndicValueError> {
    if basis.represented_prefix().tail().is_none() {
        return Ok(source.binding().lineage());
    }
    CasLineageProof::native(
        NativeCasLineage::Resume,
        source.binding().represented_prefix(),
    )
}
