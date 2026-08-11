#[cfg(test)]
use beryl_stream::FixedChannelObserver;
use beryl_stream::{
    FixedChannelReceiver, FixedChannelSender, PageLease, ReceiveError, SendError, fixed_channel,
};
use std::{
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::JoinHandle,
    time::Duration,
};

use beryl_backend::{
    ApprovalRequest, CallerNoSuccessorFence, ClientUserMessageId, CompactThreadOutcome,
    CompactionAttemptCorrelation, DynamicToolCall, DynamicToolCallResponse, ExactForegroundThread,
    FreshIdleThread, FreshLoadedThreadSession, LoadedThreadSession, ManagedBackendError,
    ManagedBackendSession, OrderedTurnStreamProgress, StreamedInputSource, ThreadInjectionOutcome,
    ThreadInjectionPreflight, ThreadInjectionSource, ThreadInjectionSourceError,
    ThreadInjectionSourcePage, ThreadLoadOptions, ThreadStartOptions, ThreadUnsubscribeResponse,
    TurnStartOptions, TurnStartOutcome, TurnSteerOutcome,
};
use beryl_model::{CasThreadId, CasTurnId};

use super::{
    ConnectionAttachment, ConnectionAttachmentIdentity, ConnectionRegistryAuthority, ForwardingHub,
    driver_outcome::{ConnectionCommandOutcome, ConnectionRoutingFailure},
    persistent_failure::dispatch_next_persistent_failure,
    provider_broker::ProviderBrokerControl,
    recovery_source_broker::{self, PreparedRecoverySource},
    registry,
    router::{EventRouter, LiveEventTargetCloseReason, RouteOutcome},
    source_broker::{
        self, RemoteStreamedInputSource, SourceBrokerEvent, StreamedInputBrokerService,
    },
};
use crate::cas_projection::stop::{
    StopCoordinationError, StopDispatchOwner, StopDispatchSettlement,
};
use crate::cas_projection::{
    ProjectionCoordinatorError,
    persistent_failure::{
        LiveCommandPermit, PersistentFailureCommandFrontier, PersistentFailureCutIdentity,
        PersistentFailureGeneration,
    },
    service_config::ProjectionWorkerPermit,
};

const CONNECTION_COMMAND_QUEUE_LIMIT: usize = 64;
const STREAM_IDLE_POLL_INTERVAL: Duration = Duration::from_millis(20);

type DriverOperation = Box<
    dyn FnOnce(
            &mut ConnectionRequestSession<'_>,
            &DriverContext,
            LiveCommandPermit,
        ) -> DriverOperationDisposition
        + Send
        + 'static,
>;

type DriverRejection = Box<dyn FnOnce(DriverCommandNondispatch) + Send + 'static>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DriverCommandNondispatch {
    PersistentHomeFailure {
        cut: PersistentFailureCutIdentity,
        frontier: PersistentFailureCommandFrontier,
    },
    OrdinaryShutdown,
    AttachmentMismatch,
    WorkerUnavailable,
}

enum DriverOperationDisposition {
    Settled,
    Rejected(DriverCommandNondispatch),
}

struct DriverCommand {
    admitted_attachment: Option<ConnectionAttachmentIdentity>,
    permit: LiveCommandPermit,
    operation: DriverOperation,
    rejection: DriverRejection,
}

impl DriverCommand {
    fn reject(self, cause: DriverCommandNondispatch) {
        (self.rejection)(cause);
    }

    fn execute(self, session: &mut ConnectionRequestSession<'_>, context: &DriverContext) {
        let Self {
            permit,
            operation,
            rejection,
            ..
        } = self;
        match operation(session, context, permit) {
            DriverOperationDisposition::Settled => {}
            DriverOperationDisposition::Rejected(cause) => rejection(cause),
        }
    }
}

impl DriverCommandNondispatch {
    fn into_coordinator_error(self) -> ProjectionCoordinatorError {
        match self {
            Self::PersistentHomeFailure { cut, frontier }
                if frontier.matches_cut(cut.service_generation, cut.failure_generation) =>
            {
                ProjectionCoordinatorError::LiveCommandPersistentHomeFailure {
                    generation: cut.home_generation,
                }
            }
            Self::PersistentHomeFailure { .. }
            | Self::OrdinaryShutdown
            | Self::AttachmentMismatch
            | Self::WorkerUnavailable => ProjectionCoordinatorError::ProjectionWorkerStopped,
        }
    }
}

struct StopDriverOutcome {
    settlement: Result<StopDispatchSettlement, StopCoordinationError>,
    invalidates_connection: bool,
}

#[cfg(test)]
struct DriverWorkGuardPause {
    connection_generation: u64,
    reached: mpsc::SyncSender<()>,
    release: mpsc::Receiver<()>,
}

#[cfg(test)]
struct DriverWorkGuardPauseController {
    reached: mpsc::Receiver<()>,
    release: Option<mpsc::SyncSender<()>>,
}

#[cfg(test)]
struct DriverPreCyclePause {
    connection_generation: u64,
    reached: mpsc::SyncSender<()>,
    release: mpsc::Receiver<()>,
}

#[cfg(test)]
pub(in crate::cas_projection) struct DriverPreCyclePauseController {
    reached: mpsc::Receiver<()>,
    release: Option<mpsc::SyncSender<()>>,
    commands: FixedChannelObserver<DriverCommand>,
}

#[cfg(test)]
pub(in crate::cas_projection) struct DriverCommandQueueObserver {
    commands: FixedChannelObserver<DriverCommand>,
}

#[cfg(test)]
static DRIVER_WORK_GUARD_PAUSE: std::sync::OnceLock<Mutex<Option<DriverWorkGuardPause>>> =
    std::sync::OnceLock::new();

#[cfg(test)]
static DRIVER_PRE_CYCLE_PAUSE: std::sync::OnceLock<Mutex<Option<DriverPreCyclePause>>> =
    std::sync::OnceLock::new();

fn connection_command_channel() -> Result<
    (
        FixedChannelSender<DriverCommand>,
        FixedChannelReceiver<DriverCommand>,
    ),
    ProjectionCoordinatorError,
> {
    fixed_channel(
        std::num::NonZeroUsize::new(CONNECTION_COMMAND_QUEUE_LIMIT)
            .expect("driver command queue is nonzero"),
    )
    .map_err(
        |error| ProjectionCoordinatorError::ProviderBrokerAdmission {
            message: error.to_string(),
        },
    )
}

#[cfg(test)]
fn test_driver_command(operation: DriverOperation) -> DriverCommand {
    let gate = crate::cas_projection::persistent_failure::MasterCommandGate::new(
        crate::cas_projection::persistent_failure::ProjectionServiceGeneration::allocate()
            .expect("test service generation is available"),
        None,
    );
    DriverCommand {
        admitted_attachment: None,
        permit: gate
            .authorizer()
            .authorize()
            .expect("test command is admitted"),
        operation,
        rejection: Box::new(|_| {}),
    }
}

/// Request-only view of the connection worker's backend session.
///
/// The view deliberately exposes no stream polling or buffered-message drain.
/// All normalized events remain owned by the connection driver and router.
pub(in crate::cas_projection) struct ConnectionRequestSession<'a> {
    pub(super) backend: &'a mut ManagedBackendSession,
}

impl ConnectionRequestSession<'_> {
    pub(in crate::cas_projection) fn compact_exact_foreground_thread(
        &mut self,
        target: ExactForegroundThread,
        attempt: CompactionAttemptCorrelation,
        timeout: Duration,
    ) -> ExactContextCompactionDispatch {
        if let Err(error) = self.backend.bind_exact_foreground_thread(target.clone()) {
            return ExactContextCompactionDispatch::before_dispatch(error, None);
        }
        let authorization = match self.backend.authorize_exact_foreground_thread(
            target,
            attempt,
            CallerNoSuccessorFence::issue(),
        ) {
            Ok(authorization) => authorization,
            Err(error) => {
                let unbind_failure = self.backend.unbind_exact_foreground_thread().err();
                return ExactContextCompactionDispatch::before_dispatch(error, unbind_failure);
            }
        };
        let outcome = self
            .backend
            .compact_exact_foreground_thread(authorization, timeout);
        let unbind_failure = self.backend.unbind_exact_foreground_thread().err();
        ExactContextCompactionDispatch {
            outcome: Some(outcome),
            before_dispatch_failure: None,
            unbind_failure,
        }
    }

    #[cfg(feature = "test-faults")]
    pub(in crate::cas_projection) fn fail_next_write_before_dispatch_for_test(&mut self) {
        self.backend
            .fail_next_write_before_dispatch_for_lifecycle_test();
    }

    #[cfg(feature = "test-faults")]
    pub(in crate::cas_projection) fn last_websocket_ingress_test_snapshot(
        &self,
    ) -> Option<crate::cas_projection::test_faults::WebSocketIngressSnapshot> {
        self.backend
            .last_websocket_ingress_test_metrics()
            .map(crate::cas_projection::test_faults::WebSocketIngressSnapshot::from_backend)
    }

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
        preflight: &ThreadInjectionPreflight,
        source: &mut dyn ThreadInjectionSource,
        timeout: Duration,
    ) -> ThreadInjectionOutcome {
        self.backend
            .inject_thread_items(target, preflight, source, timeout)
    }

    pub(in crate::cas_projection) fn unsubscribe_thread(
        &mut self,
        thread_id: &CasThreadId,
        timeout: Duration,
    ) -> Result<ThreadUnsubscribeResponse, ManagedBackendError> {
        self.backend.unsubscribe_thread(thread_id, timeout)
    }

    pub(in crate::cas_projection) fn start_turn_with_streamed_input(
        &mut self,
        thread_id: &CasThreadId,
        input: Box<dyn StreamedInputSource>,
        options: TurnStartOptions,
        timeout: Duration,
    ) -> TurnStartOutcome {
        self.backend
            .start_turn_with_streamed_input_options(thread_id, input, options, timeout)
    }

    pub(in crate::cas_projection) fn steer_turn_with_streamed_input(
        &mut self,
        thread_id: &CasThreadId,
        expected_turn_id: &CasTurnId,
        correlation: &ClientUserMessageId,
        input: Box<dyn StreamedInputSource>,
        timeout: Duration,
    ) -> TurnSteerOutcome {
        self.backend.steer_turn_with_streamed_input(
            thread_id,
            expected_turn_id,
            correlation,
            input,
            timeout,
        )
    }

    pub(in crate::cas_projection) fn respond_dynamic_tool_call(
        &mut self,
        call: &DynamicToolCall,
        response: &DynamicToolCallResponse,
    ) -> Result<(), ManagedBackendError> {
        self.backend.respond_dynamic_tool_call(call, response)
    }
}

/// Sole-session result that never drops request evidence when binding cleanup also fails.
#[derive(Debug)]
pub(in crate::cas_projection) struct ExactContextCompactionDispatch {
    outcome: Option<CompactThreadOutcome>,
    before_dispatch_failure: Option<ManagedBackendError>,
    unbind_failure: Option<ManagedBackendError>,
}

impl ExactContextCompactionDispatch {
    fn before_dispatch(
        error: ManagedBackendError,
        unbind_failure: Option<ManagedBackendError>,
    ) -> Self {
        Self {
            outcome: None,
            before_dispatch_failure: Some(error),
            unbind_failure,
        }
    }

    pub(in crate::cas_projection) const fn outcome(&self) -> Option<&CompactThreadOutcome> {
        self.outcome.as_ref()
    }

    pub(in crate::cas_projection) const fn before_dispatch_failure(
        &self,
    ) -> Option<&ManagedBackendError> {
        self.before_dispatch_failure.as_ref()
    }

    pub(in crate::cas_projection) const fn unbind_failure(&self) -> Option<&ManagedBackendError> {
        self.unbind_failure.as_ref()
    }

    pub(in crate::cas_projection) fn invalidates_connection(&self) -> bool {
        self.unbind_failure.is_some()
            || self
                .before_dispatch_failure
                .as_ref()
                .is_some_and(ManagedBackendError::invalidates_connection_authority)
            || self.outcome.as_ref().is_some_and(|outcome| {
                matches!(
                    outcome.disposition(),
                    beryl_backend::CompactThreadDisposition::CompletionUnknown { .. }
                )
            })
    }
}

struct DriverContext {
    authority: Arc<ConnectionRegistryAuthority>,
    stop: Arc<AtomicBool>,
    forwarding_hub: Arc<ForwardingHub>,
}

impl DriverContext {
    fn attachment(&self) -> Result<Arc<ConnectionAttachment>, ProjectionCoordinatorError> {
        self.forwarding_hub.current_attachment()
    }

    fn attachment_for(&self, command: &LiveCommandPermit) -> Option<Arc<ConnectionAttachment>> {
        let attachment = self.attachment().ok()?;
        (attachment.identity.service_generation() == command.service_generation())
            .then_some(attachment)
    }

    fn retire(&self, reason: LiveEventTargetCloseReason) {
        let Ok(attachment) = self.attachment() else {
            return;
        };
        self.retire_attachment(&attachment, reason);
    }

    fn retire_attachment(
        &self,
        attachment: &ConnectionAttachment,
        reason: LiveEventTargetCloseReason,
    ) {
        let command = match attachment.commands.authorize() {
            Ok(command) => command,
            Err(_) => return,
        };
        let mut authority = match self.authority.lock() {
            Ok(authority) => authority,
            Err(_) => {
                let elected = command
                    .commit_if_current(|| attachment.begin_ordinary_retirement())
                    .unwrap_or(false);
                drop(command);
                if elected {
                    self.stop.store(true, Ordering::Release);
                    attachment.broker.request_cancel();
                    attachment.router.retire(reason);
                }
                return;
            }
        };
        let elected = command
            .commit_if_current(|| {
                let elected = attachment.begin_ordinary_retirement();
                if elected {
                    self.authority.retire_locked(&mut authority);
                }
                elected
            })
            .unwrap_or(false);
        drop(authority);
        drop(command);
        if elected {
            self.stop.store(true, Ordering::Release);
            attachment.broker.request_cancel();
            attachment.router.retire(reason);
        }
    }

    fn retire_backend(&self, reason: LiveEventTargetCloseReason) {
        let Ok(attachment) = self.attachment() else {
            return;
        };
        attachment.broker.record_backend_failure();
        self.retire_attachment(&attachment, reason);
    }

    fn retire_router(&self, reason: LiveEventTargetCloseReason) {
        let Ok(attachment) = self.attachment() else {
            return;
        };
        attachment.broker.record_router_failure();
        self.retire_attachment(&attachment, reason);
    }

    fn drain_approval_interruptions(
        &self,
        session: &mut ConnectionRequestSession<'_>,
        command: &LiveCommandPermit,
        expected_target_failure: Option<&ApprovalRequest>,
    ) -> bool {
        let Some(attachment) = self.attachment_for(command) else {
            return false;
        };
        let expects_permission = expected_target_failure
            .is_some_and(|request| request.kind().separate_interruption_required());
        let mut first = true;
        while let Some(mut pending) = attachment.broker.take_approval_interruption() {
            if first
                && expected_target_failure
                    .is_some_and(|request| !pending.obligation().matches_request(request))
            {
                drop(pending);
                self.retire_router(LiveEventTargetCloseReason::StreamFailure);
                return false;
            }
            first = false;
            let Some(owner) = pending.obligation_mut().take_primary() else {
                drop(pending);
                continue;
            };
            let dispatched = dispatch_stop_owner(session, owner);
            if dispatched.invalidates_connection {
                drop(pending);
                self.retire_backend(LiveEventTargetCloseReason::StreamFailure);
                return false;
            }
            match dispatched.settlement {
                Ok(StopDispatchSettlement::Stopping(_)) => {}
                Ok(StopDispatchSettlement::SafelyReopened(_)) => {
                    unreachable!("interrupting approval cannot safely reopen its durable stop")
                }
                Ok(StopDispatchSettlement::Abandoned(_)) | Err(_) => {
                    match attachment
                        .router
                        .fail_approval_interruption(command, pending.obligation())
                    {
                        RouteOutcome::Continue => {}
                        RouteOutcome::InvalidateTarget(target) => {
                            if registry::invalidate_exact_generation(
                                &target.key,
                                self.authority.generation,
                                target.owner,
                                target.loaded_generation,
                            )
                            .is_err()
                            {
                                drop(pending);
                                self.retire_router(LiveEventTargetCloseReason::StreamFailure);
                                return false;
                            }
                        }
                        RouteOutcome::RetireConnection(reason) => {
                            drop(pending);
                            self.retire_router(reason);
                            return false;
                        }
                    }
                }
            }
            drop(pending);
        }
        if first && expects_permission {
            self.retire_router(LiveEventTargetCloseReason::StreamFailure);
            return false;
        }
        true
    }

    fn admitted_attachment(
        &self,
        command: &LiveCommandPermit,
    ) -> Result<Arc<ConnectionAttachment>, DriverCommandNondispatch> {
        let attachment = self
            .attachment_for(command)
            .ok_or(DriverCommandNondispatch::WorkerUnavailable)?;
        if !attachment.commands.is_open() {
            return Err(command_nondispatch(&attachment));
        }
        if !self.stop.load(Ordering::Acquire)
            && !self.authority.is_retired()
            && attachment.broker.routing_failure().is_none()
        {
            return Ok(attachment);
        }
        self.retire_attachment(&attachment, LiveEventTargetCloseReason::StreamFailure);
        Err(DriverCommandNondispatch::WorkerUnavailable)
    }
}

pub(super) struct ConnectionDriver {
    sender: FixedChannelSender<DriverCommand>,
    stop: Arc<AtomicBool>,
    handle: Mutex<Option<JoinHandle<()>>>,
    forwarding_hub: Arc<ForwardingHub>,
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
        forwarding_hub: Arc<ForwardingHub>,
        worker: ProjectionWorkerPermit,
    ) -> Result<Self, ProjectionCoordinatorError> {
        let (sender, receiver) = connection_command_channel()?;
        let stop = Arc::new(AtomicBool::new(false));
        let context = DriverContext {
            authority,
            stop: Arc::clone(&stop),
            forwarding_hub: Arc::clone(&forwarding_hub),
        };
        let handle = std::thread::Builder::new()
            .name("beryl-cas-connection".to_string())
            .spawn(move || run_driver(backend, receiver, context, worker))
            .map_err(|error| ProjectionCoordinatorError::ProjectionWorkerSpawn {
                message: error.to_string(),
            })?;
        Ok(Self {
            sender,
            stop,
            handle: Mutex::new(Some(handle)),
            forwarding_hub,
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

    pub(super) fn dispatch_exact_stop(
        &self,
        owner: StopDispatchOwner,
    ) -> Result<Result<StopDispatchSettlement, StopCoordinationError>, ProjectionCoordinatorError>
    {
        let command = self.call_classified(
            move |session| dispatch_stop_owner(session, owner),
            |outcome| outcome.invalidates_connection,
        )?;
        Ok(command.into_parts().0.settlement)
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
            |_, _| Ok::<(), std::convert::Infallible>(()),
            operation,
            invalidates_connection,
        )? {
            Ok(outcome) => Ok(outcome),
            Err(never) => match never {},
        }
    }

    pub(super) fn call_classified_checked<T, E>(
        &self,
        authorize: impl FnOnce(&EventRouter, &LiveCommandPermit) -> Result<(), E> + Send + 'static,
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
        let rejection_sender = result_sender.clone();
        self.send_command(
            Box::new(move |session, context, command| {
                let attachment = match context.admitted_attachment(&command) {
                    Ok(attachment) => attachment,
                    Err(cause) => return DriverOperationDisposition::Rejected(cause),
                };
                if let Err(error) = authorize(&attachment.router, &command) {
                    let _ = result_sender.send(Ok(Err(error)));
                    return DriverOperationDisposition::Settled;
                }
                let operation = operation(session);
                if invalidates_connection(&operation) {
                    context
                        .retire_attachment(&attachment, LiveEventTargetCloseReason::StreamFailure);
                } else if attachment.commands.is_open() {
                    let _ = context.drain_approval_interruptions(session, &command, None);
                }
                let routing_failure = attachment.broker.routing_failure();
                if matches!(
                    routing_failure,
                    Some(ConnectionRoutingFailure::Backend | ConnectionRoutingFailure::Router)
                ) {
                    context
                        .retire_attachment(&attachment, LiveEventTargetCloseReason::StreamFailure);
                }
                let _ = result_sender.send(Ok(Ok(ConnectionCommandOutcome::new(
                    operation,
                    routing_failure,
                ))));
                DriverOperationDisposition::Settled
            }),
            Box::new(move |cause| {
                let _ = rejection_sender.send(Err(cause));
            }),
        )?;
        receive_driver_result(result_receiver)
    }

    pub(super) fn call_classified_checked_with_source<T, E, O, F>(
        &self,
        authorize: impl FnOnce(&EventRouter, &LiveCommandPermit) -> Result<(), E> + Send + 'static,
        build_operation: impl FnOnce(RemoteStreamedInputSource<(T, F), E>) -> O,
        classify_operation: impl FnOnce(&ProviderBrokerControl, &T) -> F + Send + 'static,
        service: impl StreamedInputBrokerService,
        invalidates_connection: impl FnOnce(&T) -> bool + Send + 'static,
    ) -> Result<Result<ConnectionCommandOutcome<(T, F)>, E>, ProjectionCoordinatorError>
    where
        T: Send + 'static,
        E: Send + 'static,
        F: Send + 'static,
        O: FnOnce(&mut ConnectionRequestSession<'_>) -> T + Send + 'static,
    {
        if self.stop.load(Ordering::Acquire) {
            return Err(ProjectionCoordinatorError::ProjectionWorkerStopped);
        }
        let (source, events, receiver) = source_broker::channel(service.header());
        let operation = build_operation(source);
        let (rejection_sender, rejection_receiver) = mpsc::sync_channel(1);
        self.send_command(
            Box::new(move |session, context, command| {
                let attachment = match context.admitted_attachment(&command) {
                    Ok(attachment) => attachment,
                    Err(cause) => return DriverOperationDisposition::Rejected(cause),
                };
                if let Err(error) = authorize(&attachment.router, &command) {
                    let _ = events.send(SourceBrokerEvent::Finished(Err(error)));
                    return DriverOperationDisposition::Settled;
                }
                let operation = operation(session);
                let classification = classify_operation(&attachment.broker, &operation);
                if invalidates_connection(&operation) {
                    context
                        .retire_attachment(&attachment, LiveEventTargetCloseReason::StreamFailure);
                } else if attachment.commands.is_open() {
                    let _ = context.drain_approval_interruptions(session, &command, None);
                }
                let routing_failure = attachment.broker.routing_failure();
                if matches!(
                    routing_failure,
                    Some(ConnectionRoutingFailure::Backend | ConnectionRoutingFailure::Router)
                ) {
                    context
                        .retire_attachment(&attachment, LiveEventTargetCloseReason::StreamFailure);
                }
                let _ = events.send(SourceBrokerEvent::Finished(Ok(
                    ConnectionCommandOutcome::new((operation, classification), routing_failure),
                )));
                DriverOperationDisposition::Settled
            }),
            Box::new(move |cause| {
                let _ = rejection_sender.send(cause);
            }),
        )?;
        match source_broker::service_until_finished(receiver, service) {
            Err(ProjectionCoordinatorError::ProjectionWorkerStopped) => rejection_receiver
                .try_recv()
                .map_err(|_| ProjectionCoordinatorError::ProjectionWorkerStopped)
                .and_then(|cause| Err(cause.into_coordinator_error())),
            outcome => outcome,
        }
    }

    pub(super) fn call_classified_with_recovery_source(
        &self,
        operation: impl FnOnce(
            &mut ConnectionRequestSession<'_>,
            &mut dyn ThreadInjectionSource,
        ) -> ThreadInjectionOutcome
        + Send
        + 'static,
        prepared: PreparedRecoverySource,
        next_page: impl FnMut(
            usize,
            PageLease,
        )
            -> Result<Option<ThreadInjectionSourcePage>, ThreadInjectionSourceError>,
        invalidates_connection: impl FnOnce(&ThreadInjectionOutcome) -> bool + Send + 'static,
    ) -> Result<ConnectionCommandOutcome<ThreadInjectionOutcome>, ProjectionCoordinatorError> {
        if self.stop.load(Ordering::Acquire) {
            return Err(ProjectionCoordinatorError::ProjectionWorkerStopped);
        }
        let diagnostics = prepared.diagnostics();
        let (mut source, service) = prepared.into_parts();
        let (rejection_sender, rejection_receiver) = mpsc::sync_channel(1);
        self.send_command(
            Box::new(move |session, context, command| {
                let attachment = match context.admitted_attachment(&command) {
                    Ok(attachment) => attachment,
                    Err(cause) => return DriverOperationDisposition::Rejected(cause),
                };
                let operation = operation(session, &mut source);
                if invalidates_connection(&operation) {
                    context
                        .retire_attachment(&attachment, LiveEventTargetCloseReason::StreamFailure);
                } else if attachment.commands.is_open() {
                    let _ = context.drain_approval_interruptions(session, &command, None);
                }
                let routing_failure = attachment.broker.routing_failure();
                if matches!(
                    routing_failure,
                    Some(ConnectionRoutingFailure::Backend | ConnectionRoutingFailure::Router)
                ) {
                    context
                        .retire_attachment(&attachment, LiveEventTargetCloseReason::StreamFailure);
                }
                source.finish(ConnectionCommandOutcome::new(operation, routing_failure));
                DriverOperationDisposition::Settled
            }),
            Box::new(move |cause| {
                let _ = rejection_sender.send(cause);
            }),
        )?;
        match recovery_source_broker::service_until_finished(service, &diagnostics, next_page) {
            Err(ProjectionCoordinatorError::ProjectionWorkerStopped) => rejection_receiver
                .try_recv()
                .map_err(|_| ProjectionCoordinatorError::ProjectionWorkerStopped)
                .and_then(|cause| Err(cause.into_coordinator_error())),
            outcome => outcome,
        }
    }

    pub(super) fn request_stop(&self) {
        self.stop.store(true, Ordering::Release);
    }

    pub(super) fn is_finished(&self) -> bool {
        self.handle
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .as_ref()
            .is_none_or(std::thread::JoinHandle::is_finished)
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn pause_before_next_cycle_for_test(
        &self,
        connection_generation: u64,
    ) -> DriverPreCyclePauseController {
        let (reached, reached_receiver) = mpsc::sync_channel(1);
        let (release, release_receiver) = mpsc::sync_channel(1);
        let slot = DRIVER_PRE_CYCLE_PAUSE.get_or_init(|| Mutex::new(None));
        let mut slot = slot.lock().unwrap_or_else(|poison| poison.into_inner());
        assert!(
            slot.is_none(),
            "only one pre-cycle driver pause may be armed"
        );
        *slot = Some(DriverPreCyclePause {
            connection_generation,
            reached,
            release: release_receiver,
        });
        DriverPreCyclePauseController {
            reached: reached_receiver,
            release: Some(release),
            commands: self.sender.observer(),
        }
    }

    fn send_command(
        &self,
        operation: DriverOperation,
        rejection: DriverRejection,
    ) -> Result<(), ProjectionCoordinatorError> {
        let attachment = self.forwarding_hub.current_attachment()?;
        let permit = attachment
            .commands
            .authorize()
            .map_err(|_| ProjectionCoordinatorError::ProjectionWorkerStopped)?;
        let mut command = DriverCommand {
            admitted_attachment: Some(attachment.identity),
            permit,
            operation,
            rejection,
        };
        loop {
            if self.stop.load(Ordering::Acquire) {
                command.reject(DriverCommandNondispatch::WorkerUnavailable);
                return Ok(());
            }
            match self.sender.send_timeout(command, STREAM_IDLE_POLL_INTERVAL) {
                Ok(()) => return Ok(()),
                Err(SendError::Full(returned) | SendError::Timeout(returned)) => {
                    command = returned;
                }
                Err(SendError::Closed(returned)) => {
                    returned.reject(DriverCommandNondispatch::WorkerUnavailable);
                    return Ok(());
                }
            }
        }
    }

    pub(super) fn join(&self) -> Result<(), ProjectionCoordinatorError> {
        let handle = self
            .handle
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .take();
        if handle.is_some_and(|handle| handle.join().is_err()) {
            return Err(ProjectionCoordinatorError::ProjectionWorkerStopped);
        }
        Ok(())
    }
}

impl Drop for ConnectionDriver {
    fn drop(&mut self) {
        self.request_stop();
        let handle = match self.handle.get_mut() {
            Ok(handle) => handle.take(),
            Err(poisoned) => poisoned.into_inner().take(),
        };
        drop(handle);
    }
}

fn dispatch_stop_owner(
    session: &mut ConnectionRequestSession<'_>,
    owner: StopDispatchOwner,
) -> StopDriverOutcome {
    if let Err(error) = owner.begin_dispatch() {
        return StopDriverOutcome {
            settlement: Err(error),
            invalidates_connection: false,
        };
    }
    let target = owner.exact_target();
    if let Err(error) = session.backend.bind_exact_foreground_turn(target.clone()) {
        let mut invalidates_connection = error.invalidates_connection_authority();
        let settlement = owner.settle_before_dispatch();
        invalidates_connection |= stop_settlement_retires_projection(&settlement);
        return StopDriverOutcome {
            settlement,
            invalidates_connection,
        };
    }
    let authorization = match session.backend.authorize_exact_foreground_turn(
        target,
        owner.operation_correlation(),
        owner.attempt_correlation(),
        CallerNoSuccessorFence::issue(),
    ) {
        Ok(authorization) => authorization,
        Err(error) => {
            let mut invalidates_connection = error.invalidates_connection_authority();
            let unbind = session.backend.unbind_exact_foreground_turn();
            invalidates_connection |= unbind
                .as_ref()
                .is_err_and(ManagedBackendError::invalidates_connection_authority);
            let settlement = owner.settle_before_dispatch();
            invalidates_connection |= stop_settlement_retires_projection(&settlement);
            return StopDriverOutcome {
                settlement,
                invalidates_connection,
            };
        }
    };
    let outcome = session
        .backend
        .interrupt_exact_foreground_turn(authorization, owner.timeout());
    let invalidates_connection = match outcome.disposition() {
        beryl_backend::TurnInterruptDisposition::CompletionUnknown { .. } => true,
        beryl_backend::TurnInterruptDisposition::ProvenNotDispatched { error } => {
            error.invalidates_connection_authority()
        }
        beryl_backend::TurnInterruptDisposition::RequestAccepted
        | beryl_backend::TurnInterruptDisposition::RejectedBeforeCoreInterrupt => false,
    };
    let settlement = owner.settle_interrupt(&outcome);
    let unbind = session.backend.unbind_exact_foreground_turn();
    let invalidates_connection = invalidates_connection
        || unbind
            .as_ref()
            .is_err_and(ManagedBackendError::invalidates_connection_authority);
    let invalidates_connection =
        invalidates_connection || stop_settlement_retires_projection(&settlement);
    StopDriverOutcome {
        settlement,
        invalidates_connection,
    }
}

fn stop_settlement_retires_projection(
    settlement: &Result<StopDispatchSettlement, StopCoordinationError>,
) -> bool {
    matches!(settlement, Ok(StopDispatchSettlement::Abandoned(_)))
}

fn persistent_failure_nondispatch(
    attachment: &ConnectionAttachment,
) -> Option<DriverCommandNondispatch> {
    let failure_generation = PersistentFailureGeneration::FIRST;
    let frontier = attachment
        .commands
        .persistent_failure_frontier(attachment.identity.service_generation(), failure_generation)
        .ok()?;
    Some(DriverCommandNondispatch::PersistentHomeFailure {
        cut: PersistentFailureCutIdentity::new(
            attachment.identity.home_id(),
            attachment.identity.home_generation(),
            attachment.identity.service_generation(),
            failure_generation,
        ),
        frontier,
    })
}

fn command_nondispatch(attachment: &ConnectionAttachment) -> DriverCommandNondispatch {
    if attachment.commands.is_persistent_failure_cut() {
        persistent_failure_nondispatch(attachment)
            .unwrap_or(DriverCommandNondispatch::WorkerUnavailable)
    } else {
        DriverCommandNondispatch::OrdinaryShutdown
    }
}

fn receive_driver_result<T>(
    receiver: mpsc::Receiver<Result<T, DriverCommandNondispatch>>,
) -> Result<T, ProjectionCoordinatorError> {
    receiver
        .recv()
        .map_err(|_| ProjectionCoordinatorError::ProjectionWorkerStopped)?
        .map_err(DriverCommandNondispatch::into_coordinator_error)
}

#[cfg(test)]
fn pause_before_driver_cycle_if_requested(connection_generation: u64) {
    let slot = DRIVER_PRE_CYCLE_PAUSE.get_or_init(|| Mutex::new(None));
    let pause = {
        let mut slot = slot.lock().unwrap_or_else(|poison| poison.into_inner());
        if slot
            .as_ref()
            .is_some_and(|pause| pause.connection_generation == connection_generation)
        {
            slot.take()
        } else {
            None
        }
    };
    let Some(pause) = pause else {
        return;
    };
    pause
        .reached
        .send(())
        .expect("the pre-cycle driver test observer remains available");
    pause
        .release
        .recv_timeout(Duration::from_secs(30))
        .expect("the pre-cycle driver test releases the exact paused cycle");
}

#[cfg(test)]
impl DriverPreCyclePauseController {
    pub(in crate::cas_projection) fn wait_until_reached(&self) {
        self.reached
            .recv_timeout(Duration::from_secs(10))
            .expect("the exact connection driver reaches its pre-cycle pause");
    }

    pub(in crate::cas_projection) fn wait_until_one_command_is_queued(&self) {
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        loop {
            let diagnostics = self
                .commands
                .diagnostics()
                .expect("the paused driver retains its command ring");
            if diagnostics.len == 1 && diagnostics.sends == 1 && diagnostics.receives == 0 {
                return;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "one admitted command must reach the paused driver ring: {diagnostics:?}"
            );
            std::thread::yield_now();
        }
    }

    pub(in crate::cas_projection) fn diagnostics(&self) -> beryl_stream::ChannelDiagnostics {
        self.commands
            .diagnostics()
            .expect("the driver command ring remains observable")
    }

    pub(in crate::cas_projection) fn release(mut self) -> DriverCommandQueueObserver {
        let commands = self.commands.clone();
        self.release
            .take()
            .expect("the pre-cycle driver pause releases once")
            .send(())
            .expect("the exact paused driver remains available");
        DriverCommandQueueObserver { commands }
    }
}

#[cfg(test)]
impl DriverCommandQueueObserver {
    pub(in crate::cas_projection) fn diagnostics(
        &self,
    ) -> Option<beryl_stream::ChannelDiagnostics> {
        self.commands.diagnostics()
    }
}

#[cfg(test)]
impl Drop for DriverPreCyclePauseController {
    fn drop(&mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
    }
}

#[cfg(test)]
fn install_driver_work_guard_pause(connection_generation: u64) -> DriverWorkGuardPauseController {
    let (reached, reached_receiver) = mpsc::sync_channel(1);
    let (release, release_receiver) = mpsc::sync_channel(1);
    let slot = DRIVER_WORK_GUARD_PAUSE.get_or_init(|| Mutex::new(None));
    let mut slot = slot.lock().unwrap_or_else(|poison| poison.into_inner());
    assert!(
        slot.is_none(),
        "only one driver Work-guard pause may be armed"
    );
    *slot = Some(DriverWorkGuardPause {
        connection_generation,
        reached,
        release: release_receiver,
    });
    DriverWorkGuardPauseController {
        reached: reached_receiver,
        release: Some(release),
    }
}

#[cfg(test)]
fn pause_after_driver_work_guard_if_requested(connection_generation: u64) {
    let slot = DRIVER_WORK_GUARD_PAUSE.get_or_init(|| Mutex::new(None));
    let pause = {
        let mut slot = slot.lock().unwrap_or_else(|poison| poison.into_inner());
        if slot
            .as_ref()
            .is_some_and(|pause| pause.connection_generation == connection_generation)
        {
            slot.take()
        } else {
            None
        }
    };
    let Some(pause) = pause else {
        return;
    };
    pause
        .reached
        .send(())
        .expect("the driver Work-guard test observer remains available");
    pause
        .release
        .recv_timeout(Duration::from_secs(10))
        .expect("the driver Work-guard test releases the exact paused cycle");
}

#[cfg(test)]
impl DriverWorkGuardPauseController {
    fn wait_until_reached(&self) {
        self.reached
            .recv_timeout(Duration::from_secs(10))
            .expect("the exact connection driver reaches its Work-guard pause");
    }

    fn release(mut self) {
        self.release
            .take()
            .expect("the driver Work-guard pause releases once")
            .send(())
            .expect("the exact paused driver remains available");
    }
}

#[cfg(test)]
impl Drop for DriverWorkGuardPauseController {
    fn drop(&mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
    }
}

fn run_driver(
    mut backend: ManagedBackendSession,
    receiver: FixedChannelReceiver<DriverCommand>,
    context: DriverContext,
    worker: ProjectionWorkerPermit,
) {
    let _retirement = DriverRetirementGuard { context: &context };
    let _worker = worker;
    let mut initial_approval_drain_pending = true;
    'driver: loop {
        #[cfg(test)]
        pause_before_driver_cycle_if_requested(context.authority.generation.get());

        #[cfg(test)]
        pause_after_driver_work_guard_if_requested(context.authority.generation.get());

        let attachment = match context.attachment() {
            Ok(attachment) => attachment,
            Err(_) => break 'driver,
        };

        if initial_approval_drain_pending {
            initial_approval_drain_pending = false;
            if let Ok(initial) = attachment.commands.authorize() {
                let mut requests = ConnectionRequestSession {
                    backend: &mut backend,
                };
                if initial.is_current()
                    && !context.drain_approval_interruptions(&mut requests, &initial, None)
                {
                    continue;
                }
            }
        }

        if !attachment.commands.is_open() {
            let rejection = command_nondispatch(&attachment);
            drain_invalidated_driver_commands(&receiver, rejection);
            if context.stop.load(Ordering::Acquire) {
                break 'driver;
            }
            if !attachment.commands.is_persistent_failure_cut() {
                break 'driver;
            }
            if attachment.persistent_failure.ordinary_retirement_won() {
                break 'driver;
            }
            let mut requests = ConnectionRequestSession {
                backend: &mut backend,
            };
            if dispatch_next_persistent_failure(
                &mut requests,
                &attachment.router,
                &context.authority,
                &attachment.persistent_failure,
            ) {
                continue;
            }
            attachment
                .persistent_failure
                .wait_for_change(STREAM_IDLE_POLL_INTERVAL);
            continue;
        }
        if context.authority.is_retired() {
            break 'driver;
        }
        if context.stop.load(Ordering::Acquire) {
            context.retire_attachment(&attachment, LiveEventTargetCloseReason::WorkerStopped);
            if context.authority.is_retired()
                || attachment.persistent_failure.ordinary_retirement_won()
            {
                break 'driver;
            }
            continue;
        }
        match receiver.try_receive() {
            Ok(command) => {
                if command.admitted_attachment != Some(attachment.identity)
                    || !command.permit.is_current()
                {
                    command.reject(DriverCommandNondispatch::AttachmentMismatch);
                    continue;
                }
                let mut requests = ConnectionRequestSession {
                    backend: &mut backend,
                };
                command.execute(&mut requests, &context);
            }
            Err(ReceiveError::Empty) => {
                let Ok(poll_permit) = attachment.commands.authorize() else {
                    continue;
                };
                if checked_steering_blocks_stream_poll(
                    &attachment.broker,
                    STREAM_IDLE_POLL_INTERVAL,
                ) {
                    continue;
                }
                let progress = backend.poll_ordered_turn_stream_progress(STREAM_IDLE_POLL_INTERVAL);
                if !poll_permit.is_current() {
                    continue;
                }
                match progress {
                    Ok(progress) => {
                        let mut requests = ConnectionRequestSession {
                            backend: &mut backend,
                        };
                        if !context.drain_approval_interruptions(&mut requests, &poll_permit, None)
                        {
                            continue;
                        }
                        if progress == OrderedTurnStreamProgress::Quiet {
                            attachment.router.record_quiet_poll(&poll_permit);
                        }
                    }
                    Err(ManagedBackendError::ApprovalTargetFailed { request, cause }) => {
                        let mut requests = ConnectionRequestSession {
                            backend: &mut backend,
                        };
                        let _ = context.drain_approval_interruptions(
                            &mut requests,
                            &poll_permit,
                            Some(&request),
                        );
                        drop((request, cause));
                    }
                    Err(error) if error.invalidates_connection_authority() => {
                        context.retire_backend(LiveEventTargetCloseReason::StreamFailure);
                    }
                    Err(_) => {
                        attachment.broker.clear_approval_interruption();
                    }
                }
                match attachment.broker.routing_failure() {
                    Some(ConnectionRoutingFailure::Backend | ConnectionRoutingFailure::Router) => {
                        context.retire(LiveEventTargetCloseReason::StreamFailure);
                    }
                    Some(ConnectionRoutingFailure::Target { .. }) | None => {}
                }
            }
            Err(ReceiveError::Closed) => context.retire(LiveEventTargetCloseReason::WorkerStopped),
        }
    }
    drain_invalidated_driver_commands(&receiver, DriverCommandNondispatch::WorkerUnavailable);
    let _ = backend.shutdown();
}

fn drain_invalidated_driver_commands(
    receiver: &FixedChannelReceiver<DriverCommand>,
    cause: DriverCommandNondispatch,
) {
    loop {
        match receiver.try_receive() {
            Ok(command) => command.reject(cause),
            Err(ReceiveError::Empty | ReceiveError::Closed) => return,
        }
    }
}

pub(in crate::cas_projection::connection) fn checked_steering_blocks_stream_poll(
    broker: &ProviderBrokerControl,
    timeout: Duration,
) -> bool {
    if !broker.has_checked_steering_lifecycle() {
        return false;
    }
    broker.wait_for_checked_steering_consumption(timeout);
    broker.has_checked_steering_lifecycle()
}

struct DriverRetirementGuard<'a> {
    context: &'a DriverContext,
}

impl Drop for DriverRetirementGuard<'_> {
    fn drop(&mut self) {
        if let Ok(attachment) = self.context.attachment() {
            attachment.persistent_failure.driver_stopped();
            if attachment.commands.is_open() {
                self.context
                    .retire_attachment(&attachment, LiveEventTargetCloseReason::WorkerStopped);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/connection_driver_command_queue.rs"
    ));
}
