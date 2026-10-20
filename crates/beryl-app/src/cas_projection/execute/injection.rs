use super::recovery::source::{
    map_recovery_cursor_error, map_recovery_page, recovery_source_identity,
};
use super::{
    cleanup::StaleObservation,
    support::{completion_timestamp, point_limit},
};
use crate::cas_projection::connection::PreparedRecoverySource;
use crate::cas_projection::{
    AdmittedProjectionSession, CasProjectionCoordinator, CasProjectionRequest, LoadedCasProjection,
    ProjectionCancellationToken, ProjectionCoordinatorError, ProjectionExecutionError,
    connection::{ConnectionAttachmentIdentity, ConnectionRoutingFailure},
    publication,
};
use beryl_backend::{
    FreshLoadedThreadSession, ThreadInjectionOutcome, ThreadInjectionPreflight,
    ThreadInjectionSourceError, ThreadInjectionSourceIdentity, ThreadInjectionSourceRevision,
    ThreadStatus,
};
use beryl_home_store::HomeStore;
use beryl_model::CasNativeTurnCount;
use syndic_storage::{
    CasLineageProof, ContextInjectionObservation, NativeCasLineage, NativeProjectionBasis,
    PublishValidBinding, RecoveryProjection, SyndicStorage,
};

pub(super) struct PreparedProjectionInjection {
    projection: RecoveryProjection,
    source_identity: ThreadInjectionSourceIdentity,
    source_revision: ThreadInjectionSourceRevision,
    preflight: ThreadInjectionPreflight,
    source: PreparedRecoverySource,
    attachment: ConnectionAttachmentIdentity,
}

mod completion;

#[derive(Clone, Copy)]
struct ProjectionInjectionBinding {
    basis: NativeProjectionBasis,
    projection: RecoveryProjection,
    establishment: ProjectionInjectionEstablishment,
    attachment: ConnectionAttachmentIdentity,
}
#[derive(Clone, Copy)]
pub(super) enum ProjectionInjectionEstablishment {
    Replay,
    NativeSeed {
        native_turn_count: CasNativeTurnCount,
        native_lineage: CasLineageProof,
    },
}

impl ProjectionInjectionEstablishment {
    fn native_turn_count(self) -> CasNativeTurnCount {
        match self {
            Self::Replay => CasNativeTurnCount::ZERO,
            Self::NativeSeed {
                native_turn_count, ..
            } => native_turn_count,
        }
    }

    fn native_lineage(self) -> Option<CasLineageProof> {
        match self {
            Self::Replay => None,
            Self::NativeSeed { native_lineage, .. } => Some(native_lineage),
        }
    }
}

impl CasProjectionCoordinator {
    pub(super) fn prepare_projection_injection(
        &self,
        session: &AdmittedProjectionSession,
        projection: RecoveryProjection,
    ) -> Result<PreparedProjectionInjection, ProjectionExecutionError> {
        let attachment = session.connection().projection_source_attachment()?;
        let source_identity = recovery_source_identity(
            self.home_id(),
            projection,
            attachment.home_generation().get(),
            attachment.service_generation().get(),
        );
        let source_revision =
            ThreadInjectionSourceRevision::new(projection.source_revision().get());
        let preflight = ThreadInjectionPreflight::new(
            source_identity,
            source_revision,
            projection.item_count().get().into(),
            projection.utf8_bytes().get(),
            projection.sequence_digest(),
        )?;
        let source = session.connection().prepare_recovery_source()?;
        Ok(PreparedProjectionInjection {
            projection,
            source_identity,
            source_revision,
            preflight,
            source,
            attachment,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn inject_projection_target(
        &self,
        home: &HomeStore,
        storage: &SyndicStorage,
        session: &mut AdmittedProjectionSession,
        request: &CasProjectionRequest,
        cancellation: &ProjectionCancellationToken,
        basis: NativeProjectionBasis,
        fresh: FreshLoadedThreadSession,
        prepared: PreparedProjectionInjection,
        establishment: ProjectionInjectionEstablishment,
    ) -> Result<LoadedCasProjection, ProjectionExecutionError> {
        let PreparedProjectionInjection {
            projection,
            source_identity,
            source_revision,
            preflight,
            source: prepared_source,
            attachment,
        } = prepared;
        let cas_thread_id = fresh.thread_id().clone();
        let lease = match session.register_loaded(
            cas_thread_id.clone(),
            fresh.metadata().clone(),
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
        let generation = lease.generation();
        if let Err(error) = self.ensure_home(home) {
            return Err(self.abandon_projection_target(
                home,
                storage,
                session,
                request,
                basis,
                cas_thread_id,
                StaleObservation::unknown(Some(generation)),
                "injection target Home changed before source admission",
                ProjectionExecutionError::Coordinator(error),
                Some(lease),
            ));
        }
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

        #[cfg(feature = "test-faults")]
        crate::cas_projection::test_faults::pause_recovery_source(request.thread_id(), u64::MAX);
        let mut cursor = match storage.open_recovery_cursor(home, projection) {
            Ok(cursor) => cursor,
            Err(error) => {
                return Err(self.abandon_projection_target(
                    home,
                    storage,
                    session,
                    request,
                    basis,
                    cas_thread_id,
                    StaleObservation::unknown(Some(generation)),
                    "recovery source could not be reconfirmed for the fresh target",
                    ProjectionExecutionError::from(error),
                    Some(lease),
                ));
            }
        };
        if cancellation.is_cancelled() {
            return Err(self.abandon_projection_target(
                home,
                storage,
                session,
                request,
                basis,
                cas_thread_id,
                StaleObservation::unknown(Some(generation)),
                "recovery was cancelled before injection dispatch",
                ProjectionExecutionError::Cancelled,
                Some(lease),
            ));
        }

        let timeout = request.timeout();
        #[cfg(feature = "test-faults")]
        let recovery_thread_id = request.thread_id();
        #[cfg(feature = "test-faults")]
        let mut served_pages = 0_u64;
        let command = session.connection().inject_thread_items_with_source(
            target,
            preflight,
            prepared_source,
            timeout,
            |max_utf8_bytes, page_lease| {
                #[cfg(feature = "test-faults")]
                crate::cas_projection::test_faults::pause_recovery_source(
                    recovery_thread_id,
                    served_pages,
                );
                if cancellation.is_cancelled() {
                    return Err(ThreadInjectionSourceError::Cancelled);
                }
                let page = storage
                    .read_recovery_cursor_page(home, &mut cursor, page_lease, max_utf8_bytes)
                    .map_err(map_recovery_cursor_error)?;
                let mapped = page
                    .map(|page| {
                        map_recovery_page(page, source_identity, source_revision, max_utf8_bytes)
                    })
                    .transpose()?;
                #[cfg(feature = "test-faults")]
                if mapped.is_some() {
                    crate::cas_projection::test_faults::pause_recovery_page_handoff(
                        recovery_thread_id,
                        served_pages,
                    );
                    served_pages = served_pages.saturating_add(1);
                }
                Ok(mapped)
            },
        );
        let command = match command {
            Ok(command) => command,
            Err(error) => {
                return Err(self.abandon_projection_target(
                    home,
                    storage,
                    session,
                    request,
                    basis,
                    cas_thread_id,
                    StaleObservation::unknown(Some(generation)),
                    "recovery target connection stopped before injection completed",
                    ProjectionExecutionError::Coordinator(error),
                    Some(lease),
                ));
            }
        };
        let (outcome, routing_failure) = command.into_parts();
        match outcome {
            ThreadInjectionOutcome::Succeeded { thread } => self.complete_projection_injection(
                home,
                storage,
                session,
                request,
                cancellation,
                ProjectionInjectionBinding {
                    basis,
                    projection,
                    establishment,
                    attachment,
                },
                thread,
                lease,
                routing_failure,
            ),
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
            ThreadInjectionOutcome::ProvenNotDispatched { thread_id, error } => {
                let primary = ProjectionExecutionError::InjectionNotDispatched {
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
                    "recovery injection was proven not dispatched",
                    primary,
                    Some(lease),
                ))
            }
            ThreadInjectionOutcome::TransportLost { thread_id, error } => {
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

fn routing_failure_error(
    session: &AdmittedProjectionSession,
    failure: ConnectionRoutingFailure,
) -> ProjectionExecutionError {
    match failure {
        ConnectionRoutingFailure::Backend | ConnectionRoutingFailure::Router => {
            ProjectionCoordinatorError::ProjectionConnectionUnavailable {
                runtime_id: session.runtime_id(),
                process_generation: session.process_generation(),
            }
            .into()
        }
        ConnectionRoutingFailure::Target { thread_id, reason } => {
            ProjectionExecutionError::LiveEventRouting { thread_id, reason }
        }
    }
}
