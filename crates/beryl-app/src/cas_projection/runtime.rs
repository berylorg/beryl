use std::{sync::Arc, time::Duration};

use beryl_model::{CasProcessGeneration, RuntimeId};

use super::{
    ProjectionExecutionError,
    connection::{ConnectionRequestSession, ExistingLease, ProjectionConnection, ThreadRetirement},
};

/// Release-admitted authority over one exact initialized backend session.
///
/// The value owns the connection that was synchronously admitted, so later
/// crate-internal projection requests cannot substitute a session from another
/// runtime or managed-process generation.
#[derive(Debug)]
pub struct AdmittedProjectionSession {
    connection: Arc<ProjectionConnection>,
    runtime_interest: Option<(Arc<super::RuntimeInterest>, super::RuntimeActivityPeriod)>,
}

impl AdmittedProjectionSession {
    pub(super) const fn from_admitted_connection(connection: Arc<ProjectionConnection>) -> Self {
        Self {
            connection,
            runtime_interest: None,
        }
    }

    pub(in crate::cas_projection) fn retain_runtime_interest(
        &mut self,
        interest: Arc<super::RuntimeInterest>,
        period: super::RuntimeActivityPeriod,
    ) -> Result<(), super::RuntimeSessionAdmissionError> {
        self.connection
            .retain_runtime_interest(Arc::clone(&interest))?;
        self.runtime_interest = Some((interest, period));
        Ok(())
    }

    pub(in crate::cas_projection) fn permits_execution_binding(
        &self,
        binding: &beryl_model::ExecutionBinding,
    ) -> bool {
        self.runtime_id() == binding.runtime_id()
            && self
                .runtime_interest
                .as_ref()
                .is_none_or(|(interest, period)| {
                    interest.binding() == binding && interest.is_current(*period)
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

    /// Returns named content-free metrics for the last foreground WebSocket message.
    #[cfg(feature = "test-faults")]
    #[doc(hidden)]
    pub fn last_websocket_ingress_test_snapshot(
        &self,
    ) -> Result<Option<super::test_faults::WebSocketIngressSnapshot>, super::ProjectionExecutionError>
    {
        self.call(|session| Ok(session.last_websocket_ingress_test_snapshot()))
    }

    #[cfg(feature = "test-faults")]
    pub(in crate::cas_projection) fn provider_test_key(
        &self,
    ) -> super::test_faults::ProviderTestKey {
        self.connection.provider_test_key()
    }

    #[cfg(feature = "test-faults")]
    pub(in crate::cas_projection) fn provider_broker_test_snapshot(
        &self,
    ) -> super::test_faults::ProviderBrokerSnapshot {
        self.connection.provider_broker_test_snapshot()
    }

    #[cfg(feature = "test-faults")]
    pub(in crate::cas_projection) fn fail_next_write_before_dispatch_for_test(
        &self,
    ) -> Result<(), super::ProjectionCoordinatorError> {
        self.connection.fail_next_write_before_dispatch_for_test()
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

    /// Returns content-free diagnostics for this connection's one-page provider pool.
    #[must_use]
    pub fn provider_page_diagnostics(&self) -> beryl_stream::PagePoolDiagnostics {
        self.connection.provider_page_diagnostics()
    }

    /// Returns content-free capacity and logical-progress diagnostics for the latest recovery
    /// replay attempted through this exact connection.
    #[must_use]
    pub fn recovery_replay_diagnostics(&self) -> Option<super::RecoveryReplayDiagnosticsSnapshot> {
        self.connection.recovery_replay_diagnostics()
    }

    /// Returns a weak observer for this connection's latest recovery-replay diagnostics.
    ///
    /// The observer remains usable while the admitted session is moved to a worker, but it does
    /// not retain the connection or any recovery allocation after that connection is released.
    #[must_use]
    pub fn recovery_replay_diagnostics_observer(&self) -> super::RecoveryReplayDiagnosticsObserver {
        self.connection.recovery_replay_diagnostics_observer()
    }

    /// Returns test-only authority to retire this exact admitted connection.
    #[cfg(feature = "test-faults")]
    #[doc(hidden)]
    #[must_use]
    pub fn connection_retirement_handle_for_test(
        &self,
    ) -> super::test_faults::ProjectionConnectionRetirementHandle {
        super::test_faults::ProjectionConnectionRetirementHandle::new(Arc::clone(&self.connection))
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

    pub(super) fn register_loaded(
        &self,
        cas_thread_id: beryl_model::CasThreadId,
        metadata: beryl_backend::ThreadSessionMetadata,
        owner: beryl_model::SyndicThreadId,
        timeout: Duration,
    ) -> Result<super::connection::LoadedProjectionLease, super::ProjectionCoordinatorError> {
        self.connection
            .register_new(cas_thread_id, metadata, owner, timeout)
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

    #[cfg(feature = "test-faults")]
    pub fn ingester_finished_for_test(&self) -> bool {
        self.connection.ingester_finished_for_test()
    }

    #[cfg(feature = "test-faults")]
    pub fn retained_worker_units_for_test(&self) -> Option<(bool, bool)> {
        self.connection.retained_worker_units_for_test()
    }
}

impl Drop for AdmittedProjectionSession {
    fn drop(&mut self) {
        self.connection.release_session_owner();
    }
}
