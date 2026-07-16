use std::{
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError},
    },
    thread::JoinHandle,
    time::Duration,
};

use beryl_backend::{
    DynamicToolCallRequest, DynamicToolCallResponse, FreshIdleThread, FreshLoadedThreadSession,
    LoadedThreadSession, ManagedBackendError, ManagedBackendSession, ThreadInjectionBatch,
    ThreadInjectionOutcome, ThreadLoadOptions, ThreadStartOptions, ThreadUnsubscribeResponse,
    TurnStartOptions, TurnStartOutcome, TurnStreamEnvelope, UserInput,
};
use beryl_model::{CasThreadId, CasTurnId};

use super::{
    ConnectionRegistryAuthority, registry,
    router::{EventRouter, LiveEventTargetCloseReason, RouteOutcome, TargetInvalidation},
};
use crate::cas_projection::ProjectionCoordinatorError;

const CONNECTION_COMMAND_QUEUE_LIMIT: usize = 64;
const STREAM_IDLE_POLL_INTERVAL: Duration = Duration::from_millis(20);

type DriverCommand =
    Box<dyn FnOnce(&mut ConnectionRequestSession<'_>, &DriverContext) + Send + 'static>;

/// Request-only view of the connection worker's backend session.
///
/// The view deliberately exposes no stream polling or buffered-message drain.
/// All normalized events remain owned by the connection driver and router.
pub(in crate::cas_projection) struct ConnectionRequestSession<'a> {
    backend: &'a mut ManagedBackendSession,
}

impl ConnectionRequestSession<'_> {
    pub(in crate::cas_projection) fn start_thread_with_options(
        &mut self,
        cwd: &Path,
        options: ThreadStartOptions,
        timeout: Duration,
    ) -> Result<FreshLoadedThreadSession, ManagedBackendError> {
        self.backend
            .start_thread_with_options(cwd, options, timeout)
    }

    pub(in crate::cas_projection) fn resume_thread(
        &mut self,
        thread_id: &CasThreadId,
        options: &ThreadLoadOptions,
        timeout: Duration,
    ) -> Result<LoadedThreadSession, ManagedBackendError> {
        self.backend.resume_thread(thread_id, options, timeout)
    }

    pub(in crate::cas_projection) fn fork_thread(
        &mut self,
        thread_id: &CasThreadId,
        options: &ThreadLoadOptions,
        timeout: Duration,
    ) -> Result<FreshLoadedThreadSession, ManagedBackendError> {
        self.backend.fork_thread(thread_id, options, timeout)
    }

    pub(in crate::cas_projection) fn fork_thread_through_turn(
        &mut self,
        thread_id: &CasThreadId,
        last_turn_id: &CasTurnId,
        options: &ThreadLoadOptions,
        timeout: Duration,
    ) -> Result<FreshLoadedThreadSession, ManagedBackendError> {
        self.backend
            .fork_thread_through_turn(thread_id, last_turn_id, options, timeout)
    }

    pub(in crate::cas_projection) fn inject_thread_items(
        &mut self,
        target: FreshIdleThread,
        batch: &ThreadInjectionBatch,
        timeout: Duration,
    ) -> ThreadInjectionOutcome {
        self.backend.inject_thread_items(target, batch, timeout)
    }

    pub(in crate::cas_projection) fn unsubscribe_thread(
        &mut self,
        thread_id: &CasThreadId,
        timeout: Duration,
    ) -> Result<ThreadUnsubscribeResponse, ManagedBackendError> {
        self.backend.unsubscribe_thread(thread_id, timeout)
    }

    pub(in crate::cas_projection) fn start_turn_with_user_input(
        &mut self,
        thread_id: &CasThreadId,
        input: Vec<UserInput>,
        options: TurnStartOptions,
        timeout: Duration,
    ) -> TurnStartOutcome {
        self.backend
            .start_turn_with_user_input_options(thread_id, input, options, timeout)
    }

    pub(in crate::cas_projection) fn respond_dynamic_tool_call(
        &mut self,
        request: &DynamicToolCallRequest,
        response: &DynamicToolCallResponse,
    ) -> Result<(), ManagedBackendError> {
        self.backend.respond_dynamic_tool_call(request, response)
    }
}

#[derive(Debug)]
pub(in crate::cas_projection) enum ConnectionRoutingFailure {
    Backend,
    Router,
    Target {
        thread_id: CasThreadId,
        reason: LiveEventTargetCloseReason,
    },
}

#[derive(Debug)]
pub(in crate::cas_projection) struct ConnectionCommandOutcome<T> {
    operation: T,
    routing_failure: Option<ConnectionRoutingFailure>,
}

impl<T> ConnectionCommandOutcome<T> {
    pub(in crate::cas_projection) fn into_parts(self) -> (T, Option<ConnectionRoutingFailure>) {
        (self.operation, self.routing_failure)
    }

    pub(in crate::cas_projection) const fn operation(&self) -> &T {
        &self.operation
    }

    pub(super) fn record_routing_failure(&mut self, failure: ConnectionRoutingFailure) {
        if self.routing_failure.is_none() {
            self.routing_failure = Some(failure);
        }
    }
}

#[derive(Debug)]
struct DriverContext {
    authority: Arc<ConnectionRegistryAuthority>,
    router: Arc<EventRouter>,
    stop: Arc<AtomicBool>,
}

impl DriverContext {
    fn route(&self, envelope: TurnStreamEnvelope) -> Result<(), ConnectionRoutingFailure> {
        let retained_bytes = envelope.approximate_retained_bytes();
        let outcome = self.router.route(envelope.into_event(), retained_bytes);
        match outcome {
            RouteOutcome::Continue => Ok(()),
            RouteOutcome::InvalidateTarget(target) => self.invalidate_target(target),
            RouteOutcome::RetireConnection(reason) => {
                self.retire(reason);
                Err(ConnectionRoutingFailure::Router)
            }
        }
    }

    fn invalidate_target(
        &self,
        target: TargetInvalidation,
    ) -> Result<(), ConnectionRoutingFailure> {
        let thread_id = target.key.cas_thread_id.clone();
        let reason = target.reason;
        if registry::invalidate_exact_generation(
            &target.key,
            self.authority.generation,
            target.owner,
            target.loaded_generation,
        )
        .is_err()
        {
            self.retire(LiveEventTargetCloseReason::StreamFailure);
            return Err(ConnectionRoutingFailure::Router);
        }
        if reason == LiveEventTargetCloseReason::ThreadClosed {
            Ok(())
        } else {
            Err(ConnectionRoutingFailure::Target { thread_id, reason })
        }
    }

    fn drain_buffered(
        &self,
        session: &mut ManagedBackendSession,
    ) -> Result<(), ConnectionRoutingFailure> {
        while let Some(envelope) = session
            .drain_buffered_turn_stream_envelope()
            .map_err(|_| ConnectionRoutingFailure::Backend)?
        {
            self.route(envelope)?;
            if self.authority.is_retired() {
                break;
            }
        }
        Ok(())
    }

    fn retire(&self, reason: LiveEventTargetCloseReason) {
        self.stop.store(true, Ordering::Release);
        self.authority.retire();
        self.router.retire(reason);
    }
}

pub(super) struct ConnectionDriver {
    sender: SyncSender<DriverCommand>,
    stop: Arc<AtomicBool>,
    handle: Mutex<Option<JoinHandle<()>>>,
}

impl std::fmt::Debug for ConnectionDriver {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ConnectionDriver")
            .field("stopped", &self.stop.load(Ordering::Acquire))
            .finish_non_exhaustive()
    }
}

impl ConnectionDriver {
    pub(super) fn start(
        backend: ManagedBackendSession,
        authority: Arc<ConnectionRegistryAuthority>,
        router: Arc<EventRouter>,
    ) -> Result<Self, ProjectionCoordinatorError> {
        let (sender, receiver) = mpsc::sync_channel(CONNECTION_COMMAND_QUEUE_LIMIT);
        let stop = Arc::new(AtomicBool::new(false));
        let context = DriverContext {
            authority,
            router,
            stop: Arc::clone(&stop),
        };
        let handle = std::thread::Builder::new()
            .name("beryl-cas-connection".to_string())
            .spawn(move || run_driver(backend, receiver, context))
            .map_err(|error| ProjectionCoordinatorError::ProjectionWorkerSpawn {
                message: error.to_string(),
            })?;
        Ok(Self {
            sender,
            stop,
            handle: Mutex::new(Some(handle)),
        })
    }

    pub(super) fn call<T>(
        &self,
        operation: impl FnOnce(&mut ConnectionRequestSession<'_>) -> Result<T, ManagedBackendError>
        + Send
        + 'static,
    ) -> Result<ConnectionCommandOutcome<Result<T, ManagedBackendError>>, ProjectionCoordinatorError>
    where
        T: Send + 'static,
    {
        self.call_classified(operation, |result| {
            result
                .as_ref()
                .is_err_and(ManagedBackendError::invalidates_connection_authority)
        })
    }

    pub(super) fn call_classified<T>(
        &self,
        operation: impl FnOnce(&mut ConnectionRequestSession<'_>) -> T + Send + 'static,
        invalidates_connection: impl FnOnce(&T) -> bool + Send + 'static,
    ) -> Result<ConnectionCommandOutcome<T>, ProjectionCoordinatorError>
    where
        T: Send + 'static,
    {
        match self.call_classified_checked(
            |_| Ok::<(), std::convert::Infallible>(()),
            operation,
            invalidates_connection,
        )? {
            Ok(outcome) => Ok(outcome),
            Err(never) => match never {},
        }
    }

    pub(super) fn call_classified_checked<T, E>(
        &self,
        authorize: impl FnOnce(&EventRouter) -> Result<(), E> + Send + 'static,
        operation: impl FnOnce(&mut ConnectionRequestSession<'_>) -> T + Send + 'static,
        invalidates_connection: impl FnOnce(&T) -> bool + Send + 'static,
    ) -> Result<Result<ConnectionCommandOutcome<T>, E>, ProjectionCoordinatorError>
    where
        T: Send + 'static,
        E: Send + 'static,
    {
        if self.stop.load(Ordering::Acquire) {
            return Err(ProjectionCoordinatorError::ProjectionWorkerStopped);
        }
        let (result_sender, result_receiver) = mpsc::sync_channel(1);
        self.sender
            .send(Box::new(move |session, context| {
                if let Err(error) = authorize(&context.router) {
                    let _ = result_sender.send(Err(error));
                    return;
                }
                let operation = operation(session);
                let routing_failure = context.drain_buffered(session.backend).err();
                if matches!(routing_failure, Some(ConnectionRoutingFailure::Backend)) {
                    context.retire(LiveEventTargetCloseReason::StreamFailure);
                }
                if invalidates_connection(&operation) {
                    context.retire(LiveEventTargetCloseReason::StreamFailure);
                }
                let _ = result_sender.send(Ok(ConnectionCommandOutcome {
                    operation,
                    routing_failure,
                }));
            }))
            .map_err(|_| ProjectionCoordinatorError::ProjectionWorkerStopped)?;
        result_receiver
            .recv()
            .map_err(|_| ProjectionCoordinatorError::ProjectionWorkerStopped)
    }

    pub(super) fn request_stop(&self) {
        self.stop.store(true, Ordering::Release);
    }
}

impl Drop for ConnectionDriver {
    fn drop(&mut self) {
        self.request_stop();
        let handle = match self.handle.get_mut() {
            Ok(handle) => handle.take(),
            Err(poisoned) => poisoned.into_inner().take(),
        };
        if let Some(handle) = handle {
            let _ = handle.join();
        }
    }
}

fn run_driver(
    mut backend: ManagedBackendSession,
    receiver: Receiver<DriverCommand>,
    context: DriverContext,
) {
    let _retirement = DriverRetirementGuard { context: &context };
    while !context.stop.load(Ordering::Acquire) && !context.authority.is_retired() {
        match receiver.try_recv() {
            Ok(command) => {
                let mut requests = ConnectionRequestSession {
                    backend: &mut backend,
                };
                command(&mut requests, &context);
            }
            Err(TryRecvError::Empty) => {
                match backend.poll_turn_stream_envelope(STREAM_IDLE_POLL_INTERVAL) {
                    Ok(Some(envelope)) => {
                        let _ = context.route(envelope);
                    }
                    Ok(None) => context.router.record_quiet_poll(),
                    Err(_) => context.retire(LiveEventTargetCloseReason::StreamFailure),
                }
            }
            Err(TryRecvError::Disconnected) => {
                context.retire(LiveEventTargetCloseReason::WorkerStopped)
            }
        }
    }
    let _ = backend.shutdown();
}

struct DriverRetirementGuard<'a> {
    context: &'a DriverContext,
}

impl Drop for DriverRetirementGuard<'_> {
    fn drop(&mut self) {
        self.context
            .retire(LiveEventTargetCloseReason::WorkerStopped);
    }
}
