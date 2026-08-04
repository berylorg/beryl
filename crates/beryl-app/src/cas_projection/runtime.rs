use std::{sync::Arc, time::Duration};

use beryl_backend::ManagedBackendProbeReport;
#[cfg(test)]
use beryl_backend::{BackendConfigDefaults, CompatibilityProbeSet, ThreadBranchCapabilities};
use beryl_model::{CasProcessGeneration, RuntimeId};

use super::{
    ProjectionExecutionError,
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
    compatibility_evidence: SessionCompatibilityEvidence,
    preactivation_surrender_issuer:
        Option<super::service_config::ProjectionPreactivationSurrenderIssuer>,
}

impl AdmittedProjectionSession {
    pub(super) const fn from_production_connection(
        connection: Arc<ProjectionConnection>,
        compatibility_report: ManagedBackendProbeReport,
    ) -> Self {
        Self {
            connection,
            compatibility_evidence: SessionCompatibilityEvidence::Production(compatibility_report),
            preactivation_surrender_issuer: None,
        }
    }

    #[cfg(test)]
    pub(super) const fn from_lifecycle_test_connection(
        connection: Arc<ProjectionConnection>,
        lifecycle_facts: LifecycleTestCompatibilityFacts,
    ) -> Self {
        Self {
            connection,
            compatibility_evidence: SessionCompatibilityEvidence::LifecycleTest(lifecycle_facts),
            preactivation_surrender_issuer: None,
        }
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
        match &self.compatibility_evidence {
            SessionCompatibilityEvidence::Production(report) => report,
            #[cfg(test)]
            SessionCompatibilityEvidence::LifecycleTest(_) => {
                panic!("lifecycle-test session has no production compatibility report")
            }
        }
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

    pub(super) fn install_preactivation_surrender_issuer(
        &mut self,
        issuer: super::service_config::ProjectionPreactivationSurrenderIssuer,
    ) {
        self.preactivation_surrender_issuer = Some(issuer);
    }

    pub(super) fn preactivation_surrender_issuer(
        &self,
    ) -> Option<&super::service_config::ProjectionPreactivationSurrenderIssuer> {
        self.preactivation_surrender_issuer.as_ref()
    }

    pub(super) fn acquire_loaded(
        &self,
        cas_thread_id: &beryl_model::CasThreadId,
        owner: beryl_model::SyndicThreadId,
        timeout: Duration,
    ) -> Result<ExistingLease, super::ProjectionCoordinatorError> {
        self.connection.acquire_existing(
            cas_thread_id,
            owner,
            timeout,
            self.preactivation_surrender_issuer(),
        )
    }

    pub(super) fn register_loaded(
        &self,
        cas_thread_id: beryl_model::CasThreadId,
        owner: beryl_model::SyndicThreadId,
        timeout: Duration,
    ) -> Result<super::connection::LoadedProjectionLease, super::ProjectionCoordinatorError> {
        self.connection.register_new(
            cas_thread_id,
            owner,
            timeout,
            self.preactivation_surrender_issuer(),
        )
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

#[derive(Debug)]
enum SessionCompatibilityEvidence {
    Production(ManagedBackendProbeReport),
    #[cfg(test)]
    LifecycleTest(LifecycleTestCompatibilityFacts),
}

#[cfg(test)]
#[derive(Debug)]
pub(super) struct LifecycleTestCompatibilityFacts {
    probe_successes: CompatibilityProbeSet,
    config_defaults: BackendConfigDefaults,
    thread_branch_capabilities: ThreadBranchCapabilities,
}

#[cfg(test)]
impl LifecycleTestCompatibilityFacts {
    pub(super) const fn new(
        probe_successes: CompatibilityProbeSet,
        config_defaults: BackendConfigDefaults,
        thread_branch_capabilities: ThreadBranchCapabilities,
    ) -> Self {
        Self {
            probe_successes,
            config_defaults,
            thread_branch_capabilities,
        }
    }
}

impl Drop for AdmittedProjectionSession {
    fn drop(&mut self) {
        self.connection.release_session_owner();
    }
}
