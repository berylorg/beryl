use beryl_home_store::HomeStore;
use syndic_storage::{
    NativeProjectionBasis, NativeProjectionPlan, NativeProjectionRequest, NativeProjectionSource,
    NativeProjectionUnavailable, SyndicStorage,
};

use self::cleanup::StaleObservation;
use super::{
    AdmittedProjectionSession, CasProjectionCoordinator, CasProjectionRequest, LoadedCasProjection,
    ProjectionCancellationToken, ProjectionCoordinatorError, ProjectionExecutionError,
    ProjectionPublicationFailure, service::ProjectionFlight,
};
use crate::cas_projection::connection::{ExistingLease, ThreadRetirement};
use crate::conversation_tools::ConversationToolRegistry;

mod cleanup;
mod decision;
mod fresh;
mod native;
pub(super) mod native_retry;
mod recovery;
pub(super) mod support;

use support::{point_limit, recovered_process_matches};

impl CasProjectionCoordinator {
    /// Obtains one exact loaded projection and publishes durable authority before returning it.
    pub fn obtain_projection(
        &self,
        home: &HomeStore,
        storage: SyndicStorage,
        session: &mut AdmittedProjectionSession,
        request: &CasProjectionRequest,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<LoadedCasProjection, ProjectionExecutionError> {
        let (request, tool_profile) =
            self.prepare_projection_request(home, session, request, cancellation)?;
        let flight = self.begin_projection(request.thread_id())?;
        self.obtain_projection_with_prepared_flight(
            home,
            storage,
            session,
            &request,
            cancellation,
            tool_profile,
            &flight,
        )
    }

    pub(super) fn obtain_projection_in_flight(
        &self,
        home: &HomeStore,
        storage: SyndicStorage,
        session: &mut AdmittedProjectionSession,
        request: &CasProjectionRequest,
        cancellation: &ProjectionCancellationToken,
        flight: &ProjectionFlight,
    ) -> Result<LoadedCasProjection, ProjectionExecutionError> {
        let (request, tool_profile) =
            self.prepare_projection_request(home, session, request, cancellation)?;
        self.obtain_projection_with_prepared_flight(
            home,
            storage,
            session,
            &request,
            cancellation,
            tool_profile,
            flight,
        )
    }

    fn prepare_projection_request(
        &self,
        home: &HomeStore,
        session: &AdmittedProjectionSession,
        request: &CasProjectionRequest,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<
        (
            CasProjectionRequest,
            beryl_model::CasConversationToolProfile,
        ),
        ProjectionExecutionError,
    > {
        self.ensure_home(home)?;
        let tools = ConversationToolRegistry::canonical();
        let request =
            request.with_installed_thread_options(tools.install(request.thread_options().clone())?);
        if session.runtime_id() != request.execution_binding().runtime_id() {
            return Err(ProjectionExecutionError::RuntimeMismatch {
                requested: request.execution_binding().runtime_id(),
                admitted: session.runtime_id(),
            });
        }
        if request.thread_options().is_ephemeral() {
            return Err(ProjectionExecutionError::EphemeralProjectionThread);
        }
        if cancellation.is_cancelled() {
            return Err(ProjectionExecutionError::Cancelled);
        }
        Ok((request, tools.profile()))
    }

    #[allow(clippy::too_many_arguments)]
    fn obtain_projection_with_prepared_flight(
        &self,
        home: &HomeStore,
        storage: SyndicStorage,
        session: &mut AdmittedProjectionSession,
        request: &CasProjectionRequest,
        cancellation: &ProjectionCancellationToken,
        tool_profile: beryl_model::CasConversationToolProfile,
        flight: &ProjectionFlight,
    ) -> Result<LoadedCasProjection, ProjectionExecutionError> {
        self.ensure_projection_flight(flight, request.thread_id())?;
        self.ensure_home(home)?;
        let native = storage.prepare_native_projection(
            home,
            &NativeProjectionRequest::new(
                request.thread_id(),
                request.selected_path(),
                request.execution_binding().clone(),
                tool_profile,
            ),
            point_limit(),
        )?;
        self.ensure_home(home)?;
        if cancellation.is_cancelled() {
            return Err(ProjectionExecutionError::Cancelled);
        }

        self.execute_plan(home, storage, session, request, cancellation, native)
    }

    fn execute_plan(
        &self,
        home: &HomeStore,
        storage: SyndicStorage,
        session: &mut AdmittedProjectionSession,
        request: &CasProjectionRequest,
        cancellation: &ProjectionCancellationToken,
        native: NativeProjectionPlan,
    ) -> Result<LoadedCasProjection, ProjectionExecutionError> {
        match native {
            NativeProjectionPlan::Current { basis, source } => self.use_current_source(
                home,
                storage,
                session,
                request,
                cancellation,
                basis,
                source,
            ),
            NativeProjectionPlan::Resume { basis, source } => {
                self.use_resume_source(home, storage, session, request, cancellation, basis, source)
            }
            NativeProjectionPlan::Fork {
                basis,
                source,
                through_turn,
                native_turn_count,
            } => self.fork_native_projection(
                home,
                storage,
                session,
                request,
                cancellation,
                basis,
                source,
                through_turn.as_ref(),
                native_turn_count,
            ),
            NativeProjectionPlan::Fresh { basis } => {
                self.start_fresh_native(home, storage, session, request, cancellation, basis)
            }
            NativeProjectionPlan::Unavailable {
                basis,
                source,
                reason,
            } => match source {
                Some(source) => self.retire_current_then_recover(
                    home,
                    storage,
                    session,
                    request,
                    cancellation,
                    basis,
                    source,
                    unavailable_source_reason(reason),
                ),
                None => {
                    self.recover_projection(home, storage, session, request, cancellation, basis)
                }
            },
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn use_current_source(
        &self,
        home: &HomeStore,
        storage: SyndicStorage,
        session: &mut AdmittedProjectionSession,
        request: &CasProjectionRequest,
        cancellation: &ProjectionCancellationToken,
        basis: NativeProjectionBasis,
        source: NativeProjectionSource,
    ) -> Result<LoadedCasProjection, ProjectionExecutionError> {
        match session.acquire_loaded(
            source.binding().cas_thread_id(),
            request.thread_id(),
            request.timeout(),
        )? {
            ExistingLease::Exact(lease) => {
                let generation = lease.generation();
                if recovered_process_matches(source.binding().lineage(), generation) {
                    return Ok(self.capability(
                        request,
                        source.binding_revision(),
                        source.binding().cas_thread_id().clone(),
                        lease,
                        source.binding().lineage(),
                    ));
                }
                drop(lease);
                return self.retire_current_then_recover(
                    home,
                    storage,
                    session,
                    request,
                    cancellation,
                    basis,
                    source,
                    "recovered CAS managed process no longer matches",
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
                return Err(ProjectionCoordinatorError::CasThreadOwnerCollision {
                    runtime_id: session.runtime_id(),
                    process_generation: session.process_generation(),
                    cas_thread_id: source.binding().cas_thread_id().clone(),
                    existing_owner,
                    offered_owner: request.thread_id(),
                }
                .into());
            }
            ExistingLease::Absent => {}
        }
        if source
            .binding()
            .lineage()
            .recovered_injection_generation()
            .is_some()
        {
            return self.retire_current_then_recover(
                home,
                storage,
                session,
                request,
                cancellation,
                basis,
                source,
                "recovered CAS loaded-session authority was lost",
            );
        }
        self.resume_remote_source(home, storage, session, request, cancellation, basis, source)
    }

    #[allow(clippy::too_many_arguments)]
    fn use_resume_source(
        &self,
        home: &HomeStore,
        storage: SyndicStorage,
        session: &mut AdmittedProjectionSession,
        request: &CasProjectionRequest,
        cancellation: &ProjectionCancellationToken,
        basis: NativeProjectionBasis,
        source: NativeProjectionSource,
    ) -> Result<LoadedCasProjection, ProjectionExecutionError> {
        match session.acquire_loaded(
            source.binding().cas_thread_id(),
            request.thread_id(),
            request.timeout(),
        )? {
            ExistingLease::Exact(lease) => {
                let generation = lease.generation();
                if !recovered_process_matches(source.binding().lineage(), generation) {
                    drop(lease);
                    return self.retire_current_then_recover(
                        home,
                        storage,
                        session,
                        request,
                        cancellation,
                        basis,
                        source,
                        "recovered CAS managed process no longer matches",
                    );
                }
                return self.publish_existing_loaded(home, storage, request, basis, source, lease);
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
                return Err(ProjectionCoordinatorError::CasThreadOwnerCollision {
                    runtime_id: session.runtime_id(),
                    process_generation: session.process_generation(),
                    cas_thread_id: source.binding().cas_thread_id().clone(),
                    existing_owner,
                    offered_owner: request.thread_id(),
                }
                .into());
            }
            ExistingLease::Absent => {}
        }
        if source
            .binding()
            .lineage()
            .recovered_injection_generation()
            .is_some()
        {
            return self.retire_current_then_recover(
                home,
                storage,
                session,
                request,
                cancellation,
                basis,
                source,
                "recovered CAS loaded-session authority was lost",
            );
        }
        self.resume_remote_source(home, storage, session, request, cancellation, basis, source)
    }

    #[allow(clippy::too_many_arguments)]
    fn retire_current_then_recover(
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
        if source.thread_id() != request.thread_id() {
            return Err(ProjectionExecutionError::ProjectionBasisChanged {
                thread_id: request.thread_id(),
            });
        }
        let expected_retired_revision = basis
            .expected_binding_revision()
            .checked_next()
            .map_err(|_| ProjectionPublicationFailure::BindingRevisionExhausted)?;
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
        let observed_loaded_generation = retired_loaded_generation
            .or_else(|| source.binding().lineage().recovered_injection_generation());
        let retired_revision = self.publish_abandoned_target(
            home,
            storage,
            request,
            basis,
            source.binding().cas_thread_id().clone(),
            StaleObservation::exact(
                source.binding().execution().clone(),
                source.binding().represented_prefix(),
                source.binding().tool_profile(),
                source.binding().lineage(),
                source.binding().native_turn_count(),
                observed_loaded_generation,
            ),
            reason,
        )?;
        if retired_revision != expected_retired_revision {
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
        if replanned.basis().expected_binding_revision() != retired_revision {
            return Err(ProjectionExecutionError::ProjectionBasisChanged {
                thread_id: request.thread_id(),
            });
        }
        self.execute_plan(home, storage, session, request, cancellation, replanned)
    }
}

const fn unavailable_source_reason(reason: NativeProjectionUnavailable) -> &'static str {
    match reason {
        NativeProjectionUnavailable::MissingCasTurnCorrelation => {
            "native CAS turn correlation is unavailable"
        }
        NativeProjectionUnavailable::SourceProjectionUnavailable => {
            "native CAS source projection is unavailable"
        }
        NativeProjectionUnavailable::SourceExecutionMismatch => {
            "native CAS execution binding is not reusable"
        }
        NativeProjectionUnavailable::SourceToolProfileMismatch => {
            "CAS conversation-tool profile is not reusable"
        }
        NativeProjectionUnavailable::SourcePrefixMismatch => {
            "native CAS source prefix is not reusable"
        }
    }
}
