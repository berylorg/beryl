use std::{
    fmt,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
        mpsc::{self, Receiver, Sender, SyncSender, TrySendError},
    },
    thread,
    time::{Duration, Instant},
};

#[path = "turn_worker/approval.rs"]
mod approval;
#[path = "turn_worker/lifecycle_yield.rs"]
mod lifecycle_yield;
#[path = "turn_worker/thread_start.rs"]
mod thread_start;
#[path = "turn_worker/title.rs"]
mod title;

use beryl_backend::{
    ApprovalRequest, DynamicToolCallRequest, DynamicToolCallResponse,
    ManagedBackendClientConnector, ManagedBackendSession, ThreadInfo, ThreadSessionMetadata,
    ThreadStatus, ThreadSummary, TurnStartOptions, TurnStreamEvent,
};
use beryl_model::workspace::{BerylWorkspaceId, WorkspaceId};
use tracing::{debug, warn};

use super::execution_detail::{TranscriptImagePathResolver, UserInputFragment};
use super::graph::GraphMutationUpdate;
use super::thread_activation::{ExistingThreadActivationError, activate_existing_thread_direct};
use super::thread_title::ThreadTitleCandidate;
use super::transcript_history::TranscriptHistoryWindow;
use super::transcript_image_sources::transcript_image_path_resolver_for_turns;
use crate::memory_diagnostics::MemoryMilestone;
use crate::{
    BerylWorkspacePersistence, WorkspaceGraphToolService,
    beryl_diagnostic_child_dynamic_tool_shell_response_timeout,
    diagnostic_bridge_unavailable_response, dispatch_beryl_dynamic_tool_call_with_metadata,
    is_beryl_diagnostic_child_dynamic_tool, is_beryl_diagnostic_dynamic_tool,
    is_beryl_settings_dynamic_tool, is_beryl_theme_dynamic_tool,
};
use approval::deny_backend_approval_request;
use lifecycle_yield::ActiveTurnLifecycleYieldCapture;
pub(crate) use lifecycle_yield::{AcceptedLifecycleYield, HandledDynamicToolCall};
#[cfg(test)]
#[allow(unused_imports)]
pub(crate) use thread_start::ThreadActivationBackend;
pub(crate) use thread_start::activate_thread;
use title::automatic_thread_title_candidate;

const TURN_STREAM_IDLE_POLL_INTERVAL: Duration = Duration::from_secs(10);
const POST_COMPLETION_GRACE: Duration = Duration::from_millis(500);
const TURN_WORKER_UPDATE_QUEUE_CAPACITY: usize = 1024;
const DYNAMIC_TOOL_SHELL_REQUEST_QUEUE_CAPACITY: usize = 8;
const DYNAMIC_TOOL_SHELL_RESPONSE_TIMEOUT: Duration = Duration::from_secs(2);
const DYNAMIC_THEME_DURABLE_TOOL_RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);
const SHELL_DYNAMIC_TOOL_REQUEST_PENDING: u8 = 0;
const SHELL_DYNAMIC_TOOL_REQUEST_CANCELLED: u8 = 1;
const SHELL_DYNAMIC_TOOL_REQUEST_CLAIMED: u8 = 2;

#[derive(Clone)]
pub(crate) struct ShellDynamicToolRequestSender {
    sender: SyncSender<ShellDynamicToolRequest>,
    response_timeout: Duration,
}

pub(crate) struct ShellDynamicToolRequest {
    request: DynamicToolCallRequest,
    response_sender: SyncSender<DynamicToolCallResponse>,
    control: Arc<ShellDynamicToolRequestControl>,
}

struct ShellDynamicToolRequestControl {
    state: AtomicU8,
    expires_at: Instant,
}

pub(super) enum ThreadActivationUpdate {
    Finished(ThreadActivationOutcome),
}

pub(super) enum ThreadActivationOutcome {
    Activated {
        execution_target: WorkspaceId,
        thread: ThreadInfo,
        session_metadata: ThreadSessionMetadata,
        history_window: TranscriptHistoryWindow,
        image_resolver: TranscriptImagePathResolver,
    },
    RequiresRebind {
        detail: String,
    },
    Failed {
        message: String,
    },
}

pub(super) enum TurnWorkerUpdate {
    ThreadActivated {
        execution_target: WorkspaceId,
        thread: ThreadSummary,
        session_metadata: ThreadSessionMetadata,
    },
    ThreadTitleEligible {
        execution_target: WorkspaceId,
        candidate: ThreadTitleCandidate,
    },
    GraphMutationFinished(GraphMutationUpdate),
    LifecycleYieldAccepted(AcceptedLifecycleYield),
    Event(TurnStreamEvent),
    Finished(TurnWorkerOutcome),
}

pub(super) enum TurnWorkerOutcome {
    Finished {
        execution_target: WorkspaceId,
        known_threads: Option<Vec<ThreadSummary>>,
        active_thread_id: String,
    },
    Failed {
        message: String,
    },
}

pub(crate) fn shell_dynamic_tool_request_channel() -> (
    ShellDynamicToolRequestSender,
    Receiver<ShellDynamicToolRequest>,
) {
    let (sender, receiver) = mpsc::sync_channel(DYNAMIC_TOOL_SHELL_REQUEST_QUEUE_CAPACITY);
    (
        ShellDynamicToolRequestSender {
            sender,
            response_timeout: DYNAMIC_TOOL_SHELL_RESPONSE_TIMEOUT,
        },
        receiver,
    )
}

impl ShellDynamicToolRequestSender {
    pub(crate) fn request(&self, request: &DynamicToolCallRequest) -> DynamicToolCallResponse {
        let (response_sender, response_receiver) = mpsc::sync_channel(1);
        let response_timeout = self.response_timeout_for_request(request);
        let control = Arc::new(ShellDynamicToolRequestControl::new(response_timeout));
        let shell_request = ShellDynamicToolRequest {
            request: request.clone(),
            response_sender,
            control: control.clone(),
        };
        match self.sender.try_send(shell_request) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                return diagnostic_bridge_unavailable_response(
                    request,
                    "Beryl live shell dynamic tool request bridge is busy.",
                );
            }
            Err(TrySendError::Disconnected(_)) => {
                return diagnostic_bridge_unavailable_response(
                    request,
                    "Beryl shell stopped receiving live shell dynamic tool requests.",
                );
            }
        }
        match response_receiver.recv_timeout(response_timeout) {
            Ok(response) => response,
            Err(_) => {
                control.cancel();
                diagnostic_bridge_unavailable_response(
                    request,
                    "Timed out waiting for Beryl shell dynamic tool response.",
                )
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn with_response_timeout_for_test(mut self, response_timeout: Duration) -> Self {
        self.response_timeout = response_timeout;
        self
    }

    fn response_timeout_for_request(&self, request: &DynamicToolCallRequest) -> Duration {
        let timeout = beryl_diagnostic_child_dynamic_tool_shell_response_timeout(
            request,
            self.response_timeout,
        );
        beryl_theme_dynamic_tool_shell_response_timeout(request, timeout)
    }

    #[cfg(test)]
    pub(crate) fn response_timeout_for_request_for_test(
        &self,
        request: &DynamicToolCallRequest,
    ) -> Duration {
        self.response_timeout_for_request(request)
    }
}

fn beryl_theme_dynamic_tool_shell_response_timeout(
    request: &DynamicToolCallRequest,
    default_timeout: Duration,
) -> Duration {
    if request
        .namespace()
        .is_none_or(|namespace| namespace == "beryl")
        && matches!(
            request.tool(),
            "install_theme" | "update_theme" | "save_theme_as" | "activate_theme"
        )
    {
        default_timeout.max(DYNAMIC_THEME_DURABLE_TOOL_RESPONSE_TIMEOUT)
    } else {
        default_timeout
    }
}

impl ShellDynamicToolRequest {
    pub(crate) fn request(&self) -> &DynamicToolCallRequest {
        &self.request
    }

    pub(crate) fn try_claim(&self) -> bool {
        self.control.try_claim()
    }

    pub(crate) fn respond(self, response: DynamicToolCallResponse) {
        let _ = self.response_sender.send(response);
    }
}

impl ShellDynamicToolRequestControl {
    fn new(timeout: Duration) -> Self {
        Self {
            state: AtomicU8::new(SHELL_DYNAMIC_TOOL_REQUEST_PENDING),
            expires_at: Instant::now() + timeout,
        }
    }

    fn cancel(&self) {
        let _ = self.state.compare_exchange(
            SHELL_DYNAMIC_TOOL_REQUEST_PENDING,
            SHELL_DYNAMIC_TOOL_REQUEST_CANCELLED,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }

    fn try_claim(&self) -> bool {
        if Instant::now() >= self.expires_at {
            self.cancel();
            return false;
        }
        self.state
            .compare_exchange(
                SHELL_DYNAMIC_TOOL_REQUEST_PENDING,
                SHELL_DYNAMIC_TOOL_REQUEST_CLAIMED,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }
}

#[cfg(test)]
pub(crate) fn shell_dynamic_tool_request_channel_with_capacity_for_test(
    capacity: usize,
) -> (
    ShellDynamicToolRequestSender,
    Receiver<ShellDynamicToolRequest>,
) {
    let (sender, receiver) = mpsc::sync_channel(capacity);
    (
        ShellDynamicToolRequestSender {
            sender,
            response_timeout: DYNAMIC_TOOL_SHELL_RESPONSE_TIMEOUT,
        },
        receiver,
    )
}

pub(crate) trait TurnStreamBackend {
    type Error: fmt::Display;

    fn next_turn_stream_event(
        &mut self,
        idle_timeout: Duration,
    ) -> Result<Option<TurnStreamEvent>, Self::Error>;

    fn deny_approval_request(&mut self, request: &ApprovalRequest) -> Result<(), Self::Error>;

    fn respond_dynamic_tool_call(
        &mut self,
        request: &DynamicToolCallRequest,
        response: &DynamicToolCallResponse,
    ) -> Result<(), Self::Error>;

    fn interrupt_turn(
        &mut self,
        thread_id: &str,
        turn_id: &str,
        timeout: Duration,
    ) -> Result<(), Self::Error>;
}

impl TurnStreamBackend for ManagedBackendSession {
    type Error = beryl_backend::ManagedBackendError;

    fn next_turn_stream_event(
        &mut self,
        idle_timeout: Duration,
    ) -> Result<Option<TurnStreamEvent>, Self::Error> {
        ManagedBackendSession::next_turn_stream_event(self, idle_timeout)
    }

    fn deny_approval_request(&mut self, request: &ApprovalRequest) -> Result<(), Self::Error> {
        ManagedBackendSession::deny_approval_request(self, request)
    }

    fn respond_dynamic_tool_call(
        &mut self,
        request: &DynamicToolCallRequest,
        response: &DynamicToolCallResponse,
    ) -> Result<(), Self::Error> {
        ManagedBackendSession::respond_dynamic_tool_call(self, request, response)
    }

    fn interrupt_turn(
        &mut self,
        thread_id: &str,
        turn_id: &str,
        timeout: Duration,
    ) -> Result<(), Self::Error> {
        ManagedBackendSession::interrupt_turn(self, thread_id, turn_id, timeout)
    }
}

pub(super) fn spawn_turn_worker(
    persistence: BerylWorkspacePersistence,
    connector: ManagedBackendClientConnector,
    beryl_workspace_id: BerylWorkspaceId,
    workspace: WorkspaceId,
    selected_thread_id: Option<String>,
    automatic_title_generation_allowed: bool,
    user_input_fragments: Vec<UserInputFragment>,
    turn_options: TurnStartOptions,
    shell_tool_sender: Option<ShellDynamicToolRequestSender>,
    timeout: Duration,
) -> Receiver<TurnWorkerUpdate> {
    let (sender, receiver) = mpsc::sync_channel(TURN_WORKER_UPDATE_QUEUE_CAPACITY);
    thread::spawn(move || {
        run_turn_worker(
            persistence,
            connector,
            beryl_workspace_id,
            workspace,
            selected_thread_id,
            automatic_title_generation_allowed,
            user_input_fragments,
            turn_options,
            shell_tool_sender,
            timeout,
            sender,
        )
    });
    receiver
}

pub(super) fn spawn_thread_activation_worker(
    persistence: BerylWorkspacePersistence,
    connector: ManagedBackendClientConnector,
    beryl_workspace_id: BerylWorkspaceId,
    workspace: WorkspaceId,
    thread_id: String,
    label: String,
    timeout: Duration,
) -> Receiver<ThreadActivationUpdate> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        run_thread_activation_worker(
            persistence,
            connector,
            beryl_workspace_id,
            workspace,
            thread_id,
            label,
            timeout,
            sender,
        )
    });
    receiver
}

fn run_turn_worker(
    persistence: BerylWorkspacePersistence,
    connector: ManagedBackendClientConnector,
    beryl_workspace_id: BerylWorkspaceId,
    workspace: WorkspaceId,
    selected_thread_id: Option<String>,
    automatic_title_generation_allowed: bool,
    user_input_fragments: Vec<UserInputFragment>,
    turn_options: TurnStartOptions,
    shell_tool_sender: Option<ShellDynamicToolRequestSender>,
    timeout: Duration,
    sender: SyncSender<TurnWorkerUpdate>,
) {
    let mut session = match connector.connect_client(timeout) {
        Ok(session) => session,
        Err(error) => {
            let _ = send_turn_worker_update(
                &sender,
                TurnWorkerUpdate::Finished(TurnWorkerOutcome::Failed {
                    message: format!("Beryl could not connect to the managed backend: {error}"),
                }),
            );
            return;
        }
    };

    let activation = match activate_thread(
        &mut session,
        &workspace,
        selected_thread_id.as_deref(),
        timeout,
    ) {
        Ok(activation) => activation,
        Err(message) => {
            let _ = send_turn_worker_update(
                &sender,
                TurnWorkerUpdate::Finished(TurnWorkerOutcome::Failed { message }),
            );
            return;
        }
    };

    if send_turn_worker_update(
        &sender,
        TurnWorkerUpdate::ThreadActivated {
            execution_target: workspace.clone(),
            thread: activation.summary.clone(),
            session_metadata: activation.session_metadata.clone(),
        },
    )
    .is_err()
    {
        return;
    }

    let graph_tool_service = WorkspaceGraphToolService::new(persistence.clone());

    let turn = match session.start_turn_with_user_input_options(
        &activation.thread_id,
        backend_input_for_user_input_fragments(&user_input_fragments),
        turn_options,
        timeout,
    ) {
        Ok(response) => response.turn,
        Err(error) => {
            let _ = send_turn_worker_update(
                &sender,
                TurnWorkerUpdate::Finished(TurnWorkerOutcome::Failed {
                    message: format!("Beryl could not start the turn: {error}"),
                }),
            );
            return;
        }
    };
    let active_turn_id = turn.id.clone();
    if send_turn_worker_update(
        &sender,
        TurnWorkerUpdate::Event(TurnStreamEvent::TurnStarted {
            thread_id: activation.thread_id.clone(),
            turn,
        }),
    )
    .is_err()
    {
        return;
    }

    let first_user_input_fragment = user_input_fragments
        .iter()
        .find(|fragment| !fragment.is_blank());
    if let Some(candidate) = automatic_thread_title_candidate(
        &activation.thread_id,
        first_user_input_fragment
            .map(|fragment| fragment.text.as_str())
            .unwrap_or_default(),
        automatic_title_generation_allowed,
        activation.summary.name.as_deref(),
    ) {
        if send_turn_worker_update(
            &sender,
            TurnWorkerUpdate::ThreadTitleEligible {
                execution_target: workspace.clone(),
                candidate,
            },
        )
        .is_err()
        {
            return;
        }
    }

    let graph_update_sender = sender.clone();
    let lifecycle_update_sender = sender.clone();
    if let Err(message) = stream_active_turn_events(
        &mut session,
        &activation.thread_id,
        &active_turn_id,
        TURN_STREAM_IDLE_POLL_INTERVAL,
        POST_COMPLETION_GRACE,
        |request| {
            handle_beryl_dynamic_tool_call_with_shell_tools(
                &graph_tool_service,
                &beryl_workspace_id,
                shell_tool_sender.as_ref(),
                request,
                |update| {
                    let _ = send_turn_worker_update(
                        &graph_update_sender,
                        TurnWorkerUpdate::GraphMutationFinished(update),
                    );
                },
            )
        },
        |yielded| {
            let _ = send_turn_worker_update(
                &lifecycle_update_sender,
                TurnWorkerUpdate::LifecycleYieldAccepted(yielded),
            );
        },
        |event| {
            send_turn_worker_update(&sender, TurnWorkerUpdate::Event(event))
                .map_err(|_| "Beryl stopped receiving turn stream updates.".to_string())
        },
    ) {
        let _ = send_turn_worker_update(
            &sender,
            TurnWorkerUpdate::Finished(TurnWorkerOutcome::Failed { message }),
        );
        return;
    }

    let _ = send_turn_worker_update(
        &sender,
        TurnWorkerUpdate::Finished(TurnWorkerOutcome::Finished {
            execution_target: workspace,
            known_threads: None,
            active_thread_id: activation.thread_id,
        }),
    );
}

fn send_turn_worker_update(
    sender: &SyncSender<TurnWorkerUpdate>,
    update: TurnWorkerUpdate,
) -> Result<(), ()> {
    sender.send(update).map_err(|_| ())
}

pub(super) fn backend_input_for_user_input_fragments(
    fragments: &[UserInputFragment],
) -> Vec<beryl_backend::UserInput> {
    fragments
        .iter()
        .flat_map(|fragment| fragment.backend_input().iter().cloned())
        .collect()
}

#[cfg(test)]
pub(crate) fn automatic_thread_title_generation_is_eligible(
    automatic_title_generation_allowed: bool,
    backend_thread_name: Option<&str>,
) -> bool {
    title::automatic_thread_title_generation_is_eligible(
        automatic_title_generation_allowed,
        backend_thread_name,
    )
}

pub(crate) fn handle_beryl_dynamic_tool_call(
    service: &WorkspaceGraphToolService,
    workspace_id: &BerylWorkspaceId,
    request: &DynamicToolCallRequest,
    mut publish_graph_mutation: impl FnMut(GraphMutationUpdate),
) -> HandledDynamicToolCall {
    let dispatch = dispatch_beryl_dynamic_tool_call_with_metadata(service, workspace_id, request);
    let graph_write = dispatch.graph_write();
    let graph_failure = dispatch.graph_failure();
    let lifecycle_yield = dispatch.lifecycle_yield();
    if let Some(write) = graph_write {
        let commit = write.into_commit();
        publish_graph_mutation(GraphMutationUpdate::commit(commit, ""));
    } else if let Some(message) = graph_failure {
        publish_graph_mutation(GraphMutationUpdate::failure(workspace_id.clone(), message));
    }

    HandledDynamicToolCall::new(dispatch.into_response(), lifecycle_yield)
}

pub(crate) fn handle_beryl_dynamic_tool_call_with_shell_tools(
    service: &WorkspaceGraphToolService,
    workspace_id: &BerylWorkspaceId,
    shell_tool_sender: Option<&ShellDynamicToolRequestSender>,
    request: &DynamicToolCallRequest,
    publish_graph_mutation: impl FnMut(GraphMutationUpdate),
) -> HandledDynamicToolCall {
    if is_beryl_diagnostic_dynamic_tool(request)
        || is_beryl_diagnostic_child_dynamic_tool(request)
        || is_beryl_theme_dynamic_tool(request)
        || is_beryl_settings_dynamic_tool(request)
    {
        let response = shell_tool_sender.map_or_else(
            || {
                diagnostic_bridge_unavailable_response(
                    request,
                    "Beryl live shell dynamic tools are unavailable for this turn.",
                )
            },
            |sender| sender.request(request),
        );
        return HandledDynamicToolCall::new(response, None);
    }

    handle_beryl_dynamic_tool_call(service, workspace_id, request, publish_graph_mutation)
}

pub(crate) fn stream_active_turn_events<B, F, H, R>(
    backend: &mut B,
    active_thread_id: &str,
    active_turn_id: &str,
    idle_poll_interval: Duration,
    post_completion_grace: Duration,
    mut handle_dynamic_tool_call: H,
    mut emit_lifecycle_yield: impl FnMut(AcceptedLifecycleYield),
    mut emit_event: F,
) -> Result<(), String>
where
    B: TurnStreamBackend,
    F: FnMut(TurnStreamEvent) -> Result<(), String>,
    H: FnMut(&DynamicToolCallRequest) -> R,
    R: Into<HandledDynamicToolCall>,
{
    let mut saw_turn_completion = false;
    let mut lifecycle_yields = ActiveTurnLifecycleYieldCapture::default();
    loop {
        let event_timeout = if saw_turn_completion {
            post_completion_grace
        } else {
            idle_poll_interval
        };

        let event = match backend.next_turn_stream_event(event_timeout) {
            Ok(Some(TurnStreamEvent::ProtocolError { error })) => {
                return Err(format!(
                    "Beryl received a protocol error while streaming the turn: {}",
                    error.message
                ));
            }
            Ok(Some(TurnStreamEvent::ApprovalRequested(request))) => {
                deny_backend_approval_request(backend, &request, idle_poll_interval)?;
                continue;
            }
            Ok(Some(TurnStreamEvent::DynamicToolCallRequested(request))) => {
                let handled = handle_dynamic_tool_call(&request).into();
                let (response, accepted_lifecycle_yield) = lifecycle_yields
                    .handle_dynamic_tool_call(active_thread_id, active_turn_id, &request, handled)
                    .into_parts();
                backend
                    .respond_dynamic_tool_call(&request, &response)
                    .map_err(|error| {
                        format!("Beryl could not return the dynamic tool result: {error}")
                    })?;
                if let Some(accepted_lifecycle_yield) = accepted_lifecycle_yield {
                    emit_lifecycle_yield(accepted_lifecycle_yield);
                }
                continue;
            }
            Ok(Some(event)) => event,
            Ok(None) if saw_turn_completion => break,
            Ok(None) => continue,
            Err(error) if saw_turn_completion => {
                warn!(error = %error, "turn stream ended after completion grace window");
                break;
            }
            Err(error) => {
                return Err(format!(
                    "Beryl lost the execution stream for the active turn: {error}"
                ));
            }
        };

        if matches!(
            &event,
            TurnStreamEvent::TurnCompleted { turn, .. } if turn.id == active_turn_id
        ) {
            saw_turn_completion = true;
        }

        let finish_after_event = saw_turn_completion
            && matches!(
                &event,
                TurnStreamEvent::ThreadStatusChanged { thread_id, status }
                    if thread_id == active_thread_id
                        && (status.waiting_on_user_input() || matches!(status, ThreadStatus::Idle))
            );

        emit_event(event)?;

        if finish_after_event {
            break;
        }
    }

    Ok(())
}

fn run_thread_activation_worker(
    persistence: BerylWorkspacePersistence,
    connector: ManagedBackendClientConnector,
    beryl_workspace_id: BerylWorkspaceId,
    workspace: WorkspaceId,
    thread_id: String,
    label: String,
    timeout: Duration,
    sender: Sender<ThreadActivationUpdate>,
) {
    let worker_started = Instant::now();
    MemoryMilestone::new("thread_activation_worker_start")
        .workspace_id(beryl_workspace_id.as_str())
        .runtime(workspace.runtime_mode().display_name())
        .thread_id(thread_id.as_str())
        .log();
    let connect_started = Instant::now();
    let mut session = match connector.connect_client(timeout) {
        Ok(session) => session,
        Err(error) => {
            MemoryMilestone::new("backend_client_connect_failed")
                .workspace_id(beryl_workspace_id.as_str())
                .runtime(workspace.runtime_mode().display_name())
                .thread_id(thread_id.as_str())
                .log();
            debug!(
                thread_id = thread_id.as_str(),
                backend_connect_ms = elapsed_ms(connect_started.elapsed()),
                worker_total_ms = elapsed_ms(worker_started.elapsed()),
                "thread activation worker failed to connect backend client"
            );
            let _ = sender.send(ThreadActivationUpdate::Finished(
                ThreadActivationOutcome::Failed {
                    message: format!("Beryl could not connect to the managed backend: {error}"),
                },
            ));
            return;
        }
    };
    debug!(
        thread_id = thread_id.as_str(),
        backend_connect_ms = elapsed_ms(connect_started.elapsed()),
        "thread activation worker connected backend client"
    );
    MemoryMilestone::new("backend_client_connected")
        .workspace_id(beryl_workspace_id.as_str())
        .runtime(workspace.runtime_mode().display_name())
        .thread_id(thread_id.as_str())
        .log();

    let activation_started = Instant::now();
    match activate_existing_thread_direct(&mut session, &workspace, &thread_id, &label, timeout) {
        Ok(activation) => {
            let history_turn_count = activation.thread.turns.len();
            let history_item_count = activation
                .thread
                .turns
                .iter()
                .map(|turn| turn.items.len())
                .sum::<usize>();
            let history_generated_image_count = activation
                .thread
                .turns
                .iter()
                .flat_map(|turn| turn.items.iter())
                .filter(|item| matches!(item, beryl_backend::ThreadItem::ImageGeneration(_)))
                .count();
            MemoryMilestone::new("thread_activation_worker_loaded_history")
                .workspace_id(beryl_workspace_id.as_str())
                .runtime(workspace.runtime_mode().display_name())
                .thread_id(thread_id.as_str())
                .history_counts(
                    history_turn_count,
                    history_item_count,
                    history_generated_image_count,
                )
                .log();
            debug!(
                thread_id = thread_id.as_str(),
                backend_activation_ms = elapsed_ms(activation_started.elapsed()),
                "thread activation worker received backend activation"
            );
            let resolver_started = Instant::now();
            let image_resolver = match transcript_image_path_resolver_for_turns(
                &persistence,
                &beryl_workspace_id,
                workspace.runtime_mode(),
                &activation.thread.turns,
                &mut session,
                timeout,
            ) {
                Ok(resolver) => resolver,
                Err(error) => {
                    warn!(
                        workspace_id = beryl_workspace_id.as_str(),
                        error = %error,
                        "failed to prepare transcript image source resolver during thread activation"
                    );
                    TranscriptImagePathResolver::default()
                }
            };
            debug!(
                thread_id = thread_id.as_str(),
                image_resolver_prepare_ms = elapsed_ms(resolver_started.elapsed()),
                worker_total_ms = elapsed_ms(worker_started.elapsed()),
                "thread activation worker prepared image resolver"
            );
            MemoryMilestone::new("thread_activation_worker_done")
                .workspace_id(beryl_workspace_id.as_str())
                .runtime(workspace.runtime_mode().display_name())
                .thread_id(thread_id.as_str())
                .history_counts(
                    history_turn_count,
                    history_item_count,
                    history_generated_image_count,
                )
                .log();
            let _ = sender.send(ThreadActivationUpdate::Finished(
                ThreadActivationOutcome::Activated {
                    execution_target: workspace,
                    thread: activation.thread,
                    session_metadata: activation.session_metadata,
                    history_window: activation.history_window,
                    image_resolver,
                },
            ));
        }
        Err(ExistingThreadActivationError::RequiresRebind { detail }) => {
            MemoryMilestone::new("thread_activation_worker_requires_rebind")
                .workspace_id(beryl_workspace_id.as_str())
                .runtime(workspace.runtime_mode().display_name())
                .thread_id(thread_id.as_str())
                .log();
            debug!(
                thread_id = thread_id.as_str(),
                backend_activation_ms = elapsed_ms(activation_started.elapsed()),
                worker_total_ms = elapsed_ms(worker_started.elapsed()),
                "thread activation worker requires rebind"
            );
            let _ = sender.send(ThreadActivationUpdate::Finished(
                ThreadActivationOutcome::RequiresRebind { detail },
            ));
        }
        Err(ExistingThreadActivationError::Failed { message }) => {
            MemoryMilestone::new("thread_activation_worker_failed")
                .workspace_id(beryl_workspace_id.as_str())
                .runtime(workspace.runtime_mode().display_name())
                .thread_id(thread_id.as_str())
                .log();
            debug!(
                thread_id = thread_id.as_str(),
                backend_activation_ms = elapsed_ms(activation_started.elapsed()),
                worker_total_ms = elapsed_ms(worker_started.elapsed()),
                "thread activation worker failed"
            );
            let _ = sender.send(ThreadActivationUpdate::Finished(
                ThreadActivationOutcome::Failed { message },
            ));
        }
    }
}

fn elapsed_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}
