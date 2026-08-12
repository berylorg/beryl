use std::{sync::Arc, time::Duration};

use beryl_backend::ThreadStartOptions;
use beryl_home_store::{HomeGeneration, ReadError};
use beryl_model::{BerylHomeId, CasProcessGeneration, ExecutionBinding, RuntimeId, SyndicThreadId};
use beryl_state::AssetState;
use thiserror::Error;

use super::{
    AdmittedProjectionSession, OrdinaryDynamicToolHandlers, OrdinaryTurnExecutionRequest,
    connection::ProjectionConnection, service::ProjectionFlight,
    service_config::ProjectionWorkerPermit,
};

/// Late-bound process-shell policy for one scheduled ordinary execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduledOrdinaryRequestPolicy {
    thread_options: ThreadStartOptions,
    model_context_window_tokens: Option<u64>,
    projection_timeout: Duration,
    turn: OrdinaryTurnExecutionRequest,
}

impl ScheduledOrdinaryRequestPolicy {
    /// Creates one complete policy snapshot. There is deliberately no default policy.
    #[must_use]
    pub const fn new(
        thread_options: ThreadStartOptions,
        model_context_window_tokens: Option<u64>,
        projection_timeout: Duration,
        turn: OrdinaryTurnExecutionRequest,
    ) -> Self {
        Self {
            thread_options,
            model_context_window_tokens,
            projection_timeout,
            turn,
        }
    }

    /// Returns the persistent CAS thread-start policy.
    #[must_use]
    pub const fn thread_options(&self) -> &ThreadStartOptions {
        &self.thread_options
    }

    /// Returns the optional model context-window value selected by the process shell.
    #[must_use]
    pub const fn model_context_window_tokens(&self) -> Option<u64> {
        self.model_context_window_tokens
    }

    /// Returns the timeout for establishing the exact loaded projection.
    #[must_use]
    pub const fn projection_timeout(&self) -> Duration {
        self.projection_timeout
    }

    /// Returns the late-bound ordinary turn-start policy.
    #[must_use]
    pub const fn turn(&self) -> &OrdinaryTurnExecutionRequest {
        &self.turn
    }
}

/// Process-shell checkout that retains one exact admitted session until lease release.
pub trait ScheduledProjectionSessionAuthority: Send + 'static {
    /// Borrows the exact compatibility-admitted session selected for this execution.
    fn session(&mut self) -> &mut AdmittedProjectionSession;
}

/// Owned feature authority that lends narrowed dynamic-tool handlers inside the worker.
pub trait OrdinaryDynamicToolAuthority: Send + 'static {
    /// Borrows both feature-owned handlers for the duration of ordinary execution.
    fn handlers(&mut self) -> OrdinaryDynamicToolHandlers<'_>;
}

/// Why the process shell could not issue a complete execution lease.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ScheduledOrdinaryExecutionUnavailable {
    #[error("the selected runtime or root is not ready")]
    RuntimeNotReady,
    #[error("the exact admitted projection session is already in use")]
    SessionBusy,
    #[error("late-bound ordinary request policy is unavailable")]
    RequestPolicyUnavailable,
    #[error("feature-owned ordinary dynamic-tool authority is unavailable")]
    DynamicToolsUnavailable,
    #[error("the process shell is shutting down")]
    ShuttingDown,
}

/// Synchronous process-shell provider for scheduled ordinary execution.
pub trait ScheduledOrdinaryExecutionProvider: Send + 'static {
    /// Consumes the service-issued admission token into one complete lease or a typed decline.
    fn try_issue(
        &mut self,
        admission: ScheduledOrdinaryAdmission,
    ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError>;

    /// Fences further issuance and releases every idle or returned session checkout.
    ///
    /// The service calls this only after its scheduler workers have joined and
    /// before it closes the owned Beryl home.
    fn shutdown(&mut self);
}

/// Opaque service-issued authority to complete or decline one exact admission.
#[must_use = "scheduled ordinary admission must be completed or explicitly declined"]
pub struct ScheduledOrdinaryAdmission {
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    thread_id: SyndicThreadId,
    execution_binding: ExecutionBinding,
    worker: ProjectionWorkerPermit,
    terminal_disposer: super::persistent_failure::PersistentFailureTerminalDisposer,
    flight: ProjectionFlight,
}

impl ScheduledOrdinaryAdmission {
    pub(super) fn new(
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        thread_id: SyndicThreadId,
        execution_binding: ExecutionBinding,
        worker: ProjectionWorkerPermit,
        terminal_disposer: super::persistent_failure::PersistentFailureTerminalDisposer,
        flight: ProjectionFlight,
    ) -> Self {
        Self {
            home_id,
            home_generation,
            thread_id,
            execution_binding,
            worker,
            terminal_disposer,
            flight,
        }
    }

    /// Returns the exact Beryl-home identity selected by the service.
    #[must_use]
    pub const fn home_id(&self) -> BerylHomeId {
        self.home_id
    }

    /// Returns the exact healthy home generation selected by the service.
    #[must_use]
    pub const fn home_generation(&self) -> HomeGeneration {
        self.home_generation
    }

    /// Returns the Syndic thread selected for this admission.
    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }

    /// Returns the durable runtime and execution-root binding selected for this admission.
    #[must_use]
    pub const fn execution_binding(&self) -> &ExecutionBinding {
        &self.execution_binding
    }

    /// Completes this token with every process-shell-owned execution authority.
    pub fn issue(
        self,
        mut session: Box<dyn ScheduledProjectionSessionAuthority>,
        policy: ScheduledOrdinaryRequestPolicy,
        assets: AssetState,
        tools: Box<dyn OrdinaryDynamicToolAuthority>,
    ) -> Result<ScheduledOrdinaryExecutionLease, ScheduledOrdinaryAdmissionError> {
        let requested = self.execution_binding.runtime_id();
        let admitted_session = session.session();
        let admitted = admitted_session.runtime_id();
        if admitted != requested {
            return Err(ScheduledOrdinaryAdmissionError::RuntimeMismatch {
                requested,
                admitted,
            });
        }
        let process_generation = admitted_session.process_generation();
        let connection = Arc::clone(admitted_session.connection());
        if policy.thread_options().is_ephemeral() {
            return Err(ScheduledOrdinaryAdmissionError::EphemeralThreadPolicy);
        }
        Ok(ScheduledOrdinaryExecutionLease {
            home_id: self.home_id,
            home_generation: self.home_generation,
            thread_id: self.thread_id,
            execution_binding: self.execution_binding,
            process_generation,
            connection,
            policy,
            assets,
            session,
            tools,
            _worker: self.worker,
            flight: self.flight,
        })
    }

    /// Consumes this token without durable mutation or retained local work.
    #[must_use]
    pub fn decline(
        self,
        reason: ScheduledOrdinaryExecutionUnavailable,
    ) -> ScheduledOrdinaryAdmissionResult {
        ScheduledOrdinaryAdmissionResult::Unavailable(reason)
    }
}

impl std::fmt::Debug for ScheduledOrdinaryAdmission {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ScheduledOrdinaryAdmission")
            .field("home_id", &self.home_id)
            .field("home_generation", &self.home_generation)
            .field("thread_id", &self.thread_id)
            .field("execution_binding", &self.execution_binding)
            .finish_non_exhaustive()
    }
}

/// Complete non-cloneable authority for one scheduled ordinary execution.
///
/// Dropping the value returns process-shell session authority and releases the
/// retained same-thread flight and long-lived worker permit. It never retains
/// an accepted-input identity or payload.
#[must_use = "dropping an unused lease releases its exact execution admission"]
pub struct ScheduledOrdinaryExecutionLease {
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    thread_id: SyndicThreadId,
    execution_binding: ExecutionBinding,
    process_generation: CasProcessGeneration,
    connection: Arc<ProjectionConnection>,
    policy: ScheduledOrdinaryRequestPolicy,
    assets: AssetState,
    session: Box<dyn ScheduledProjectionSessionAuthority>,
    tools: Box<dyn OrdinaryDynamicToolAuthority>,
    _worker: ProjectionWorkerPermit,
    flight: ProjectionFlight,
}

/// Result of one synchronous provider call.
#[must_use]
#[derive(Debug)]
pub enum ScheduledOrdinaryAdmissionResult {
    Issued(ScheduledOrdinaryExecutionLease),
    Unavailable(ScheduledOrdinaryExecutionUnavailable),
}

impl ScheduledOrdinaryExecutionLease {
    /// Returns the exact Beryl-home identity retained by this lease.
    #[must_use]
    pub const fn home_id(&self) -> BerylHomeId {
        self.home_id
    }

    /// Returns the exact healthy home generation retained by this lease.
    #[must_use]
    pub const fn home_generation(&self) -> HomeGeneration {
        self.home_generation
    }

    /// Returns the Syndic thread whose same-thread flight is retained.
    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }

    /// Returns the durable runtime and execution-root binding retained by this lease.
    #[must_use]
    pub const fn execution_binding(&self) -> &ExecutionBinding {
        &self.execution_binding
    }

    /// Returns the exact admitted managed-process generation retained by this lease.
    #[must_use]
    pub const fn process_generation(&self) -> CasProcessGeneration {
        self.process_generation
    }

    pub(super) const fn connection(&self) -> &Arc<ProjectionConnection> {
        &self.connection
    }

    /// Returns the complete late-bound request policy retained by this lease.
    #[must_use]
    pub const fn policy(&self) -> &ScheduledOrdinaryRequestPolicy {
        &self.policy
    }

    /// Returns the exact asset domain authority retained by this lease.
    #[must_use]
    pub const fn assets(&self) -> AssetState {
        self.assets
    }

    pub(super) fn session(&mut self) -> &mut AdmittedProjectionSession {
        self.session.session()
    }

    pub(super) fn handlers(&mut self) -> OrdinaryDynamicToolHandlers<'_> {
        self.tools.handlers()
    }

    pub(super) const fn flight(&self) -> &ProjectionFlight {
        &self.flight
    }

    pub(super) fn with_execution_authority<R>(
        &mut self,
        use_authority: impl FnOnce(
            &mut AdmittedProjectionSession,
            &ScheduledOrdinaryRequestPolicy,
            AssetState,
            OrdinaryDynamicToolHandlers<'_>,
            &ProjectionFlight,
        ) -> R,
    ) -> R {
        let Self {
            policy,
            assets,
            session,
            tools,
            flight,
            ..
        } = self;
        use_authority(session.session(), policy, *assets, tools.handlers(), flight)
    }
}

impl std::fmt::Debug for ScheduledOrdinaryExecutionLease {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ScheduledOrdinaryExecutionLease")
            .field("home_id", &self.home_id)
            .field("home_generation", &self.home_generation)
            .field("thread_id", &self.thread_id)
            .field("execution_binding", &self.execution_binding)
            .field("process_generation", &self.process_generation)
            .field("policy", &self.policy)
            .finish_non_exhaustive()
    }
}

/// A process-shell decline or invalid authority offered for one exact admission.
#[derive(Debug, Error)]
pub enum ScheduledOrdinaryAdmissionError {
    #[error(transparent)]
    Authority(#[from] super::ProjectionCoordinatorError),
    #[error(
        "scheduled ordinary session runtime {admitted} does not match requested runtime {requested}"
    )]
    RuntimeMismatch {
        requested: RuntimeId,
        admitted: RuntimeId,
    },
    #[error("scheduled ordinary execution cannot use an ephemeral projection thread policy")]
    EphemeralThreadPolicy,
    #[error(
        "scheduled ordinary session for runtime {runtime_id} process {process_generation:?} is not one live connection owned by this service"
    )]
    SessionAuthorityUnavailable {
        runtime_id: RuntimeId,
        process_generation: CasProcessGeneration,
    },
    #[error("scheduled ordinary asset authority does not belong to the owned home: {source}")]
    AssetAuthority {
        #[source]
        source: ReadError,
    },
    #[error("the scheduled ordinary execution provider is poisoned")]
    ProviderPoisoned,
    #[error("the provider returned a lease for different authority than thread {thread_id}")]
    LeaseMismatch { thread_id: SyndicThreadId },
}

#[cfg(test)]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/scheduled_ordinary_admission.rs"
    ));
}
