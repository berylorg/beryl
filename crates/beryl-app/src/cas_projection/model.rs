use std::time::Duration;

use beryl_backend::{ManagedBackendError, ThreadStartOptions};
use beryl_home_store::HomeGeneration;
use beryl_model::{
    BerylHomeId, BindingRevision, CasLoadedSessionGeneration, CasNativeTurnCount, CasThreadId,
    CasTurnId, ExecutionBinding, SyndicThreadId,
};
use syndic_storage::{
    CasLineageProof, NativeProjectionBasis, NativeProjectionPlan, NativeProjectionSource,
    SelectedPathProof, SyndicTimestamp,
};

use super::{
    LiveEventTarget, LiveEventTargetRegistrationError, LoadedProjectionReleaseError,
    LoadedProjectionReleaseOutcome, connection::LoadedProjectionLease,
    service::CasProjectionCoordinator,
};

/// Exact caller-owned inputs for obtaining one pending turn's CAS projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CasProjectionRequest {
    thread_id: SyndicThreadId,
    selected_path: SelectedPathProof,
    execution_binding: ExecutionBinding,
    thread_options: ThreadStartOptions,
    model_context_window_tokens: Option<u64>,
    observed_at: SyndicTimestamp,
    timeout: Duration,
}

impl CasProjectionRequest {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        selected_path: SelectedPathProof,
        execution_binding: ExecutionBinding,
        thread_options: ThreadStartOptions,
        model_context_window_tokens: Option<u64>,
        observed_at: SyndicTimestamp,
        timeout: Duration,
    ) -> Self {
        Self {
            thread_id,
            selected_path,
            execution_binding,
            thread_options,
            model_context_window_tokens,
            observed_at,
            timeout,
        }
    }

    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }

    #[must_use]
    pub const fn selected_path(&self) -> SelectedPathProof {
        self.selected_path
    }

    #[must_use]
    pub const fn execution_binding(&self) -> &ExecutionBinding {
        &self.execution_binding
    }

    #[must_use]
    pub const fn thread_options(&self) -> &ThreadStartOptions {
        &self.thread_options
    }

    #[must_use]
    pub const fn model_context_window_tokens(&self) -> Option<u64> {
        self.model_context_window_tokens
    }

    #[must_use]
    pub const fn observed_at(&self) -> SyndicTimestamp {
        self.observed_at
    }

    #[must_use]
    pub const fn timeout(&self) -> Duration {
        self.timeout
    }

    pub(crate) fn with_installed_thread_options(&self, thread_options: ThreadStartOptions) -> Self {
        let mut request = self.clone();
        request.thread_options = thread_options;
        request
    }
}

/// Native CAS operation whose unclassified rejection exhausted automatic retry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeLineageOperation {
    /// Reattach the selected thread to its own persistent CAS thread.
    Resume,
    /// Create a distinct CAS child from an exact native source prefix.
    Fork,
}

#[derive(Debug)]
pub(super) enum NativeLineageRetryOperation {
    Resume,
    Fork {
        through_turn: Option<CasTurnId>,
        native_turn_count: CasNativeTurnCount,
    },
}

/// Non-cloneable authority for one exact Retry-or-recover decision.
///
/// The capability preserves the native source after an unclassified CAS
/// rejection. Consuming it through the coordinator either retries that exact
/// native operation or explicitly authorizes a fresh target projection from
/// Syndic history.
pub struct NativeLineageRecoveryDecision {
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    request: CasProjectionRequest,
    basis: NativeProjectionBasis,
    source: NativeProjectionSource,
    operation: NativeLineageRetryOperation,
    failed_attempts: u8,
    last_failure: Box<ManagedBackendError>,
}

impl NativeLineageRecoveryDecision {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        coordinator: &CasProjectionCoordinator,
        request: CasProjectionRequest,
        basis: NativeProjectionBasis,
        source: NativeProjectionSource,
        operation: NativeLineageRetryOperation,
        failed_attempts: u8,
        last_failure: ManagedBackendError,
    ) -> Self {
        Self {
            home_id: coordinator.home_id(),
            home_generation: coordinator.home_generation(),
            request,
            basis,
            source,
            operation,
            failed_attempts,
            last_failure: Box::new(last_failure),
        }
    }

    /// Returns the exact Beryl home owning this decision.
    #[must_use]
    pub const fn home_id(&self) -> BerylHomeId {
        self.home_id
    }

    /// Returns the exact healthy home generation owning this decision.
    #[must_use]
    pub const fn home_generation(&self) -> HomeGeneration {
        self.home_generation
    }

    /// Returns the selected Syndic thread whose projection is blocked.
    #[must_use]
    pub const fn target_thread_id(&self) -> SyndicThreadId {
        self.request.thread_id()
    }

    /// Returns the Syndic thread owning the retained native source.
    #[must_use]
    pub const fn source_thread_id(&self) -> SyndicThreadId {
        self.source.thread_id()
    }

    /// Returns the exact retained source binding revision.
    #[must_use]
    pub const fn source_binding_revision(&self) -> BindingRevision {
        self.source.binding_revision()
    }

    /// Returns the rejected native operation.
    #[must_use]
    pub const fn operation(&self) -> NativeLineageOperation {
        match &self.operation {
            NativeLineageRetryOperation::Resume => NativeLineageOperation::Resume,
            NativeLineageRetryOperation::Fork { .. } => NativeLineageOperation::Fork,
        }
    }

    /// Returns the number of failed automatic request attempts in this cycle.
    #[must_use]
    pub const fn failed_attempts(&self) -> u8 {
        self.failed_attempts
    }

    /// Returns the final unclassified backend rejection for bounded diagnostics.
    #[must_use]
    pub fn last_failure(&self) -> &ManagedBackendError {
        &self.last_failure
    }

    pub(super) const fn request(&self) -> &CasProjectionRequest {
        &self.request
    }

    pub(super) const fn basis(&self) -> NativeProjectionBasis {
        self.basis
    }

    pub(super) const fn source(&self) -> &NativeProjectionSource {
        &self.source
    }

    pub(super) fn matches_plan(&self, plan: &NativeProjectionPlan) -> bool {
        match (&self.operation, plan) {
            (
                NativeLineageRetryOperation::Resume,
                NativeProjectionPlan::Resume { basis, source },
            ) => self.basis == *basis && &self.source == source,
            (
                NativeLineageRetryOperation::Fork {
                    through_turn,
                    native_turn_count,
                },
                NativeProjectionPlan::Fork {
                    basis,
                    source,
                    through_turn: planned_turn,
                    native_turn_count: planned_count,
                },
            ) => {
                self.basis == *basis
                    && &self.source == source
                    && through_turn == planned_turn
                    && native_turn_count == planned_count
            }
            _ => false,
        }
    }
}

impl std::fmt::Debug for NativeLineageRecoveryDecision {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NativeLineageRecoveryDecision")
            .field("home_id", &self.home_id)
            .field("home_generation", &self.home_generation)
            .field("target_thread_id", &self.target_thread_id())
            .field("source_thread_id", &self.source_thread_id())
            .field("source_binding_revision", &self.source_binding_revision())
            .field("operation", &self.operation())
            .field("failed_attempts", &self.failed_attempts)
            .finish_non_exhaustive()
    }
}

/// Opaque non-cloneable capability for one exact loaded CAS projection.
///
/// The value correlates the app coordinator's exact home generation with the
/// durable binding revision and the loaded backend session that may execute the
/// Syndic thread. External callers can inspect these facts but cannot construct
/// or clone the capability.
#[derive(Debug)]
pub struct LoadedCasProjection {
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    syndic_thread_id: SyndicThreadId,
    binding_revision: BindingRevision,
    execution_binding: ExecutionBinding,
    cas_thread_id: CasThreadId,
    loaded_session_generation: CasLoadedSessionGeneration,
    lineage_proof: CasLineageProof,
    lease: Option<LoadedProjectionLease>,
}

impl LoadedCasProjection {
    pub(super) fn new(
        coordinator: &CasProjectionCoordinator,
        syndic_thread_id: SyndicThreadId,
        binding_revision: BindingRevision,
        execution_binding: ExecutionBinding,
        cas_thread_id: CasThreadId,
        lease: LoadedProjectionLease,
        lineage_proof: CasLineageProof,
    ) -> Self {
        let loaded_session_generation = lease.generation();
        Self {
            home_id: coordinator.home_id(),
            home_generation: coordinator.home_generation(),
            syndic_thread_id,
            binding_revision,
            execution_binding,
            cas_thread_id,
            loaded_session_generation,
            lineage_proof,
            lease: Some(lease),
        }
    }

    /// Returns the exact durable Beryl-home identity.
    #[must_use]
    pub const fn home_id(&self) -> BerylHomeId {
        self.home_id
    }

    /// Returns the exact healthy home generation used by the coordinator.
    #[must_use]
    pub const fn home_generation(&self) -> HomeGeneration {
        self.home_generation
    }

    /// Returns the exact Syndic thread owning this projection.
    #[must_use]
    pub const fn syndic_thread_id(&self) -> SyndicThreadId {
        self.syndic_thread_id
    }

    /// Returns the exact durable CAS-binding revision represented by the result.
    #[must_use]
    pub const fn binding_revision(&self) -> BindingRevision {
        self.binding_revision
    }

    pub(super) fn with_binding_revision(mut self, binding_revision: BindingRevision) -> Self {
        self.binding_revision = binding_revision;
        self
    }

    /// Returns the immutable runtime/root execution binding.
    #[must_use]
    pub const fn execution_binding(&self) -> &ExecutionBinding {
        &self.execution_binding
    }

    /// Returns the exact exclusive CAS thread.
    #[must_use]
    pub const fn cas_thread_id(&self) -> &CasThreadId {
        &self.cas_thread_id
    }

    /// Returns the exact managed-process and loaded-thread generation pair.
    #[must_use]
    pub const fn loaded_session_generation(&self) -> CasLoadedSessionGeneration {
        self.loaded_session_generation
    }

    /// Returns the exact durable proof of native or recovered lineage establishment.
    #[must_use]
    pub const fn lineage_proof(&self) -> CasLineageProof {
        self.lineage_proof
    }

    /// Returns whether this exact lease still owns live local subscription authority.
    pub fn is_live(&self) -> Result<bool, super::ProjectionCoordinatorError> {
        match &self.lease {
            Some(lease) => lease.is_live(),
            None => Ok(false),
        }
    }

    /// Consumes this projection into the sole provisional target registered before `turn/start`.
    pub fn into_pending_live_event_target(
        self,
    ) -> Result<LiveEventTarget, LiveEventTargetRegistrationError> {
        self.into_live_event_target_inner(None)
    }

    /// Consumes this projection into the sole target for an already proven active CAS turn.
    pub fn into_active_live_event_target(
        self,
        turn_id: CasTurnId,
    ) -> Result<LiveEventTarget, LiveEventTargetRegistrationError> {
        self.into_live_event_target_inner(Some(turn_id))
    }

    fn into_live_event_target_inner(
        self,
        turn_id: Option<CasTurnId>,
    ) -> Result<LiveEventTarget, LiveEventTargetRegistrationError> {
        let (connection, registration) = self
            .lease
            .as_ref()
            .expect("loaded projection owns exactly one lease")
            .register_event_target(turn_id)?;
        Ok(LiveEventTarget::new(self, connection, registration))
    }

    /// Consumes the lease, revokes local authority, and classifies exact unsubscribe cleanup.
    pub fn release(
        mut self,
    ) -> Result<LoadedProjectionReleaseOutcome, LoadedProjectionReleaseError> {
        self.lease
            .take()
            .expect("loaded projection owns exactly one lease")
            .release()
    }
}
