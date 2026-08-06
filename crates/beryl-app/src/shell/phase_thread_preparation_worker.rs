use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
    time::Duration,
};

use beryl_backend::{
    ManagedBackendClientConnector, ManagedBackendError, ManagedBackendSession, ThreadForkFailure,
    ThreadForkResponse, ThreadReadOptions, ThreadReadResponse, ThreadRollbackResponse,
};
use beryl_model::workspace::WorkspaceId;

use super::phase_thread_preparation_core::{
    PhaseThreadCleanupError, PhaseThreadForkError, PhaseThreadPreparationBackend,
    PhaseThreadPreparationCancellation, PhaseThreadPreparationOutcome,
    PhaseThreadPreparationRequest, PhaseThreadPreparationResult, run_phase_thread_preparation,
};

impl PhaseThreadPreparationBackend for ManagedBackendSession {
    type Error = ManagedBackendError;

    fn fork_root(
        &mut self,
        root_id: &str,
        timeout: Duration,
    ) -> Result<ThreadForkResponse, PhaseThreadForkError<Self::Error>> {
        self.fork_thread_with_commitment(root_id, timeout)
            .map_err(|error| match error {
                ThreadForkFailure::NotCommitted { source } => {
                    PhaseThreadForkError::NotCommitted(source)
                }
                ThreadForkFailure::Indeterminate { source } => {
                    PhaseThreadForkError::Indeterminate(source)
                }
            })
    }

    fn rollback_child(
        &mut self,
        child_id: &str,
        num_turns: u32,
        timeout: Duration,
    ) -> Result<ThreadRollbackResponse, Self::Error> {
        self.rollback_thread(child_id, num_turns, timeout)
    }

    fn read_child(
        &mut self,
        child_id: &str,
        timeout: Duration,
    ) -> Result<ThreadReadResponse, Self::Error> {
        self.read_thread(child_id, ThreadReadOptions::include_turns(), timeout)
    }

    fn delete_child(
        &mut self,
        child_id: &str,
        timeout: Duration,
    ) -> Result<(), PhaseThreadCleanupError<Self::Error>> {
        self.delete_thread(child_id, timeout).map_err(|error| {
            if matches!(error, ManagedBackendError::RequestFailed { .. }) {
                PhaseThreadCleanupError::ChildRemains(error)
            } else {
                PhaseThreadCleanupError::Indeterminate(error)
            }
        })
    }
}

impl PhaseThreadPreparationCancellation for Arc<AtomicBool> {
    fn is_cancelled(&self) -> bool {
        self.load(Ordering::Acquire)
    }
}

pub(crate) trait PhaseThreadPreparationConnector {
    type Backend: PhaseThreadPreparationBackend;
    type Error: std::fmt::Display;

    fn execution_target(&self) -> WorkspaceId;

    fn connect_request_client(&self, timeout: Duration) -> Result<Self::Backend, Self::Error>;
}

impl PhaseThreadPreparationConnector for ManagedBackendClientConnector {
    type Backend = ManagedBackendSession;
    type Error = ManagedBackendError;

    fn execution_target(&self) -> WorkspaceId {
        WorkspaceId::from_parts(
            self.launch_spec().runtime_mode().clone(),
            self.launch_spec().cwd().to_path_buf(),
        )
    }

    fn connect_request_client(&self, timeout: Duration) -> Result<Self::Backend, Self::Error> {
        ManagedBackendClientConnector::connect_request_client(self, timeout)
    }
}

pub(crate) enum PhaseThreadPreparationUpdate {
    Finished(PhaseThreadPreparationOutcome),
}

pub(crate) fn spawn_phase_thread_preparation_worker(
    connector: ManagedBackendClientConnector,
    request: PhaseThreadPreparationRequest,
    cancellation: Arc<AtomicBool>,
    timeout: Duration,
) -> Receiver<PhaseThreadPreparationUpdate> {
    spawn_phase_thread_preparation_worker_with(connector, request, cancellation, timeout)
}

pub(crate) fn spawn_phase_thread_preparation_worker_with<C>(
    connector: C,
    request: PhaseThreadPreparationRequest,
    cancellation: Arc<AtomicBool>,
    timeout: Duration,
) -> Receiver<PhaseThreadPreparationUpdate>
where
    C: PhaseThreadPreparationConnector + Send + 'static,
    C::Backend: Send + 'static,
    C::Error: Send + 'static,
{
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let outcome =
            run_phase_thread_preparation_worker(&connector, request, cancellation, timeout);
        let _ = sender.send(PhaseThreadPreparationUpdate::Finished(outcome));
    });
    receiver
}

pub(crate) fn run_phase_thread_preparation_worker<C>(
    connector: &C,
    request: PhaseThreadPreparationRequest,
    cancellation: Arc<AtomicBool>,
    timeout: Duration,
) -> PhaseThreadPreparationOutcome
where
    C: PhaseThreadPreparationConnector,
{
    if cancellation.is_cancelled() {
        return PhaseThreadPreparationOutcome {
            request,
            result: PhaseThreadPreparationResult::CancelledBeforeFork,
        };
    }
    if connector.execution_target() != *request.execution_target() {
        return PhaseThreadPreparationOutcome {
            request,
            result: PhaseThreadPreparationResult::DefinitiveForkFailure {
                detail: "independent backend connector execution target does not match the frozen phase request".to_string(),
            },
        };
    }
    let mut backend = match connector.connect_request_client(timeout) {
        Ok(backend) => backend,
        Err(error) => {
            return PhaseThreadPreparationOutcome {
                request,
                result: PhaseThreadPreparationResult::DefinitiveForkFailure {
                    detail: format!(
                        "could not connect independent managed-backend request client: {error}"
                    ),
                },
            };
        }
    };
    run_phase_thread_preparation(&mut backend, request, &cancellation, timeout)
}
