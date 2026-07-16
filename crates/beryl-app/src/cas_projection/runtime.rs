use std::{path::Path, sync::Arc, time::Duration};

use beryl_backend::{ManagedBackendProbeReport, ManagedBackendSession};
use beryl_model::{CasProcessGeneration, RuntimeId};

use super::{
    ProjectionExecutionError, ProjectionSessionAdmissionError,
    connection::{ConnectionRequestSession, ExistingLease, ProjectionConnection, ThreadRetirement},
};

/// Compatibility-admitted authority over one exact initialized backend session.
///
/// The value owns the session that was synchronously probed, so later
/// crate-internal projection requests cannot substitute a session from another
/// runtime or managed-process generation. No constructor accepts a precomputed
/// report or boolean compatibility claim.
#[derive(Debug)]
pub struct AdmittedProjectionSession {
    connection: Arc<ProjectionConnection>,
    compatibility_report: ManagedBackendProbeReport,
}

impl AdmittedProjectionSession {
    /// Probes and admits one exact owned initialized backend session.
    ///
    /// The compatibility probe runs synchronously on `backend` before this
    /// opaque admission proof can exist. A session that was not initialized,
    /// disagrees with the pinned contract, or fails any required method probe is
    /// returned only as a typed admission failure and cannot be used through
    /// this boundary.
    pub fn admit(
        mut backend: ManagedBackendSession,
        runtime_id: RuntimeId,
        process_generation: CasProcessGeneration,
        config_cwd: &Path,
        timeout: Duration,
    ) -> Result<Self, ProjectionSessionAdmissionError> {
        if !backend.has_full_turn_stream() {
            return Err(ProjectionSessionAdmissionError::full_turn_stream_required(
                runtime_id,
                process_generation,
            ));
        }
        let compatibility_report =
            backend
                .probe_compatibility(config_cwd, timeout)
                .map_err(|source| {
                    ProjectionSessionAdmissionError::new(runtime_id, process_generation, source)
                })?;
        let connection = ProjectionConnection::new(backend, runtime_id, process_generation)
            .map_err(|source| {
                ProjectionSessionAdmissionError::connection_ownership(
                    runtime_id,
                    process_generation,
                    source,
                )
            })?;
        Ok(Self {
            connection,
            compatibility_report,
        })
    }

    /// Returns the exact configured runtime associated with the owned session.
    #[must_use]
    pub fn runtime_id(&self) -> RuntimeId {
        self.connection.runtime_id()
    }

    /// Returns the exact managed-process generation associated with the session.
    #[must_use]
    pub fn process_generation(&self) -> CasProcessGeneration {
        self.connection.process_generation()
    }

    /// Returns the successful report retained from this exact session's probe.
    #[must_use]
    pub const fn compatibility_report(&self) -> &ManagedBackendProbeReport {
        &self.compatibility_report
    }

    pub(in crate::cas_projection) fn call<T>(
        &self,
        operation: impl FnOnce(
            &mut ConnectionRequestSession<'_>,
        ) -> Result<T, beryl_backend::ManagedBackendError>
        + Send
        + 'static,
    ) -> Result<T, ProjectionExecutionError>
    where
        T: Send + 'static,
    {
        self.connection.call(operation)
    }

    /// Returns a bounded content-free snapshot of this exact connection's live-event router.
    pub fn live_event_snapshot(
        &self,
    ) -> Result<super::LiveEventRouterSnapshot, super::ProjectionCoordinatorError> {
        self.connection.live_event_snapshot()
    }

    /// Returns bounded facts shared by every connection to this runtime process generation.
    pub fn live_event_process_snapshot(
        &self,
    ) -> Result<super::LiveEventProcessSnapshot, super::ProjectionCoordinatorError> {
        self.connection.live_event_process_snapshot()
    }

    pub(super) fn connection(&self) -> &Arc<ProjectionConnection> {
        &self.connection
    }

    pub(super) fn acquire_loaded(
        &self,
        cas_thread_id: &beryl_model::CasThreadId,
        owner: beryl_model::SyndicThreadId,
        timeout: Duration,
    ) -> Result<ExistingLease, super::ProjectionCoordinatorError> {
        self.connection
            .acquire_existing(cas_thread_id, owner, timeout)
    }

    pub(super) fn retire_loaded_thread(
        &self,
        cas_thread_id: &beryl_model::CasThreadId,
        owner: beryl_model::SyndicThreadId,
        timeout: Duration,
    ) -> Result<ThreadRetirement, super::LoadedProjectionReleaseError> {
        self.connection.retire_thread(cas_thread_id, owner, timeout)
    }

    /// Retires this exact connection and revokes every loaded projection it owns.
    pub fn invalidate_connection(&self) {
        self.connection.retire();
    }
}
