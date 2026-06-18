use std::{
    fmt,
    path::PathBuf,
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
    ManagedBackendClientConnector, ManagedBackendSession, ThreadRollbackResponse,
    ThreadSessionMetadata, ThreadStatus, ThreadSummary, TurnStartOptions, TurnStreamEvent,
};
use beryl_model::workspace::{BerylWorkspaceId, WorkspaceId};
use tracing::{debug, info, warn};

use super::graph::GraphMutationUpdate;
use super::resident_branch_edit;
use super::syndic_ingestion::{self, SyndicLiveTurnIngestor, SyndicTurnAdmission};
use super::thread_activation::prepare_storage_backed_transcript_activation;
use super::thread_title::{ThreadTitleCandidate, TurnThreadTitleMode};
use super::turn_input::UserInputFragment;
use crate::memory_diagnostics::MemoryMilestone;
use crate::{
    BerylWorkspacePersistence, WorkspaceGraphToolService,
    beryl_diagnostic_child_dynamic_tool_shell_response_timeout,
    diagnostic_bridge_unavailable_response, dispatch_beryl_dynamic_tool_call_with_metadata,
    is_beryl_diagnostic_child_dynamic_tool, is_beryl_diagnostic_dynamic_tool,
    is_beryl_settings_dynamic_tool, is_beryl_theme_dynamic_tool,
    is_beryl_threaded_decision_dynamic_tool,
};
use approval::deny_backend_approval_request;
use lifecycle_yield::ActiveTurnLifecycleYieldCapture;
pub(crate) use lifecycle_yield::{AcceptedLifecycleYield, HandledDynamicToolCall};
#[cfg(test)]
#[allow(unused_imports)]
pub(crate) use thread_start::ThreadActivationBackend;
pub(crate) use thread_start::activate_thread;
use title::automatic_thread_title_candidate;

pub(super) const TURN_STREAM_IDLE_POLL_INTERVAL: Duration = Duration::from_secs(10);
pub(super) const POST_COMPLETION_GRACE: Duration = Duration::from_millis(500);
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
        summary: ThreadSummary,
        status: ThreadStatus,
        session_metadata: Option<ThreadSessionMetadata>,
        prepared_transcript: super::syndic_transcript::PreparedTranscriptActivation,
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
        syndic_admission: Option<SyndicTurnAdmission>,
    },
    ThreadTitleEligible {
        execution_target: WorkspaceId,
        candidate: ThreadTitleCandidate,
        title_mode: TurnThreadTitleMode,
    },
    TurnAdmitted {
        thread_id: String,
        user_input_fragments: Vec<UserInputFragment>,
        syndic_admission: SyndicTurnAdmission,
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

#[derive(Clone)]
pub(super) enum TurnWorkerPreStartOperation {
    ResidentEditReplacement {
        proof: resident_branch_edit::ResidentEditProof,
        syndic_storage_dir: PathBuf,
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

pub(crate) trait ResidentEditRollbackBackend {
    type Error: fmt::Display;

    fn rollback_thread(
        &mut self,
        thread_id: &str,
        num_turns: u32,
        timeout: Duration,
    ) -> Result<ThreadRollbackResponse, Self::Error>;
}

impl ResidentEditRollbackBackend for ManagedBackendSession {
    type Error = beryl_backend::ManagedBackendError;

    fn rollback_thread(
        &mut self,
        thread_id: &str,
        num_turns: u32,
        timeout: Duration,
    ) -> Result<ThreadRollbackResponse, Self::Error> {
        ManagedBackendSession::rollback_thread(self, thread_id, num_turns, timeout)
    }
}

pub(super) fn spawn_turn_worker(
    persistence: BerylWorkspacePersistence,
    connector: ManagedBackendClientConnector,
    beryl_workspace_id: BerylWorkspaceId,
    workspace: WorkspaceId,
    selected_thread_id: Option<String>,
    title_mode: TurnThreadTitleMode,
    user_input_fragments: Vec<UserInputFragment>,
    syndic_admission: Option<SyndicTurnAdmission>,
    turn_options: TurnStartOptions,
    shell_tool_sender: Option<ShellDynamicToolRequestSender>,
    timeout: Duration,
) -> Receiver<TurnWorkerUpdate> {
    spawn_turn_worker_with_pre_start(
        persistence,
        connector,
        beryl_workspace_id,
        workspace,
        selected_thread_id,
        title_mode,
        user_input_fragments,
        syndic_admission,
        None,
        turn_options,
        shell_tool_sender,
        timeout,
    )
}

pub(super) fn spawn_turn_worker_with_pre_start(
    persistence: BerylWorkspacePersistence,
    connector: ManagedBackendClientConnector,
    beryl_workspace_id: BerylWorkspaceId,
    workspace: WorkspaceId,
    selected_thread_id: Option<String>,
    title_mode: TurnThreadTitleMode,
    user_input_fragments: Vec<UserInputFragment>,
    syndic_admission: Option<SyndicTurnAdmission>,
    pre_start: Option<TurnWorkerPreStartOperation>,
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
            title_mode,
            user_input_fragments,
            syndic_admission,
            pre_start,
            turn_options,
            shell_tool_sender,
            timeout,
            sender,
        )
    });
    receiver
}

pub(super) fn spawn_thread_activation_worker(
    beryl_workspace_id: BerylWorkspaceId,
    syndic_storage_dir: PathBuf,
    workspace: WorkspaceId,
    thread_id: String,
    syndic_view_id: String,
    label: String,
) -> Receiver<ThreadActivationUpdate> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        run_thread_activation_worker(
            beryl_workspace_id,
            syndic_storage_dir,
            workspace,
            thread_id,
            syndic_view_id,
            label,
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
    title_mode: TurnThreadTitleMode,
    user_input_fragments: Vec<UserInputFragment>,
    mut syndic_admission: Option<SyndicTurnAdmission>,
    pre_start: Option<TurnWorkerPreStartOperation>,
    turn_options: TurnStartOptions,
    shell_tool_sender: Option<ShellDynamicToolRequestSender>,
    timeout: Duration,
    sender: SyncSender<TurnWorkerUpdate>,
) {
    let mut syndic_ingestor = match open_syndic_ingestor(syndic_admission.clone(), &sender) {
        Some(ingestor) => ingestor,
        None => return,
    };
    let mut session = match connector.connect_client(timeout) {
        Ok(session) => session,
        Err(error) => {
            if let Some(ingestor) = syndic_ingestor.as_mut() {
                let _ = ingestor.mark_local_failure(format!(
                    "Beryl could not connect to the managed backend: {error}"
                ));
            }
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
            if let Some(ingestor) = syndic_ingestor.as_mut() {
                let _ = ingestor.mark_local_failure(message.clone());
            }
            let _ = send_turn_worker_update(
                &sender,
                TurnWorkerUpdate::Finished(TurnWorkerOutcome::Failed { message }),
            );
            return;
        }
    };

    if let Some(ingestor) = syndic_ingestor.as_mut()
        && let Err(error) = ingestor.bind_cas_thread(&activation.thread_id)
    {
        let message = format!("Beryl could not bind the CAS thread into Syndic: {error}");
        let _ = ingestor.mark_local_failure(message.clone());
        let _ = send_turn_worker_update(
            &sender,
            TurnWorkerUpdate::Finished(TurnWorkerOutcome::Failed { message }),
        );
        return;
    }

    if send_turn_worker_update(
        &sender,
        TurnWorkerUpdate::ThreadActivated {
            execution_target: workspace.clone(),
            thread: activation.summary.clone(),
            session_metadata: activation.session_metadata.clone(),
            syndic_admission: syndic_admission.clone(),
        },
    )
    .is_err()
    {
        return;
    }

    let mut admission_update = None;
    if let Some(pre_start) = pre_start {
        let admitted = match run_turn_pre_start_operation(
            &mut session,
            &persistence,
            &beryl_workspace_id,
            &workspace,
            &activation.thread_id,
            selected_thread_id.as_deref(),
            &user_input_fragments,
            pre_start,
            timeout,
        ) {
            Ok(admission) => admission,
            Err(message) => {
                let _ = send_turn_worker_update(
                    &sender,
                    TurnWorkerUpdate::Finished(TurnWorkerOutcome::Failed { message }),
                );
                return;
            }
        };
        syndic_admission = Some(admitted.clone());
        syndic_ingestor = match open_syndic_ingestor(syndic_admission.clone(), &sender) {
            Some(ingestor) => ingestor,
            None => return,
        };
        if let Some(ingestor) = syndic_ingestor.as_mut()
            && let Err(error) = ingestor.bind_cas_thread(&activation.thread_id)
        {
            let message = format!("Beryl could not bind the CAS thread into Syndic: {error}");
            let _ = ingestor.mark_local_failure(message.clone());
            let _ = send_turn_worker_update(
                &sender,
                TurnWorkerUpdate::Finished(TurnWorkerOutcome::Failed { message }),
            );
            return;
        }
        admission_update = Some(admitted);
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
            if let Some(ingestor) = syndic_ingestor.as_mut() {
                let _ = ingestor.mark_local_failure(format!("CAS rejected turn start: {error}"));
            }
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
    let turn_started_event = TurnStreamEvent::TurnStarted {
        thread_id: activation.thread_id.clone(),
        turn,
    };
    if let Some(ingestor) = syndic_ingestor.as_mut()
        && let Err(error) = ingestor.ingest_event(&turn_started_event)
    {
        let message = format!("Beryl could not persist CAS turn start in Syndic: {error}");
        let _ = ingestor.mark_local_failure(message.clone());
        let _ = send_turn_worker_update(
            &sender,
            TurnWorkerUpdate::Finished(TurnWorkerOutcome::Failed { message }),
        );
        return;
    }
    if let Some(syndic_admission) = admission_update
        && send_turn_worker_update(
            &sender,
            TurnWorkerUpdate::TurnAdmitted {
                thread_id: activation.thread_id.clone(),
                user_input_fragments: user_input_fragments.clone(),
                syndic_admission,
            },
        )
        .is_err()
    {
        return;
    }
    if send_turn_worker_update(&sender, TurnWorkerUpdate::Event(turn_started_event)).is_err() {
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
        title_mode,
    ) {
        if send_turn_worker_update(
            &sender,
            TurnWorkerUpdate::ThreadTitleEligible {
                execution_target: workspace.clone(),
                candidate,
                title_mode,
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
            if let Some(ingestor) = syndic_ingestor.as_mut() {
                ingestor.ingest_event(&event).map_err(|error| {
                    format!("Beryl could not persist a CAS turn event in Syndic: {error}")
                })?;
            }
            send_turn_worker_update(&sender, TurnWorkerUpdate::Event(event))
                .map_err(|_| "Beryl stopped receiving turn stream updates.".to_string())
        },
    ) {
        if let Some(ingestor) = syndic_ingestor.as_mut() {
            let _ = ingestor.mark_stream_lost(message.clone());
        }
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

fn open_syndic_ingestor(
    admission: Option<SyndicTurnAdmission>,
    sender: &SyncSender<TurnWorkerUpdate>,
) -> Option<Option<SyndicLiveTurnIngestor>> {
    match admission.map(SyndicLiveTurnIngestor::new).transpose() {
        Ok(ingestor) => Some(ingestor),
        Err(error) => {
            let _ = send_turn_worker_update(
                sender,
                TurnWorkerUpdate::Finished(TurnWorkerOutcome::Failed {
                    message: format!("Beryl could not open Syndic turn capture: {error}"),
                }),
            );
            None
        }
    }
}

fn run_turn_pre_start_operation(
    session: &mut ManagedBackendSession,
    persistence: &BerylWorkspacePersistence,
    beryl_workspace_id: &BerylWorkspaceId,
    workspace: &WorkspaceId,
    activation_thread_id: &str,
    selected_thread_id: Option<&str>,
    user_input_fragments: &[UserInputFragment],
    pre_start: TurnWorkerPreStartOperation,
    timeout: Duration,
) -> Result<SyndicTurnAdmission, String> {
    match pre_start {
        TurnWorkerPreStartOperation::ResidentEditReplacement {
            proof,
            syndic_storage_dir,
        } => {
            let selected_thread_id = selected_thread_id.ok_or_else(|| {
                "Beryl cannot replace-edit a pending new thread draft.".to_string()
            })?;
            if activation_thread_id != proof.source_thread_id {
                return Err(format!(
                    "Beryl reopened CAS thread {activation_thread_id}, but the resident edit proof targets {}.",
                    proof.source_thread_id
                ));
            }
            if selected_thread_id != proof.source_thread_id {
                return Err(format!(
                    "Beryl selected CAS thread {selected_thread_id}, but the resident edit proof targets {}.",
                    proof.source_thread_id
                ));
            }
            rollback_resident_edit_tail(session, &proof, timeout)?;
            resident_branch_edit::detach_resident_edit_tail(&syndic_storage_dir, &proof)
                .map_err(|error| {
                    format!(
                        "Beryl rolled back CAS thread {} but could not detach the selected Syndic transcript tail: {error:?}",
                        proof.source_thread_id
                    )
                })?;
            syndic_ingestion::admit_user_turn(
                persistence,
                beryl_workspace_id,
                workspace,
                Some(selected_thread_id),
                user_input_fragments,
            )
            .map_err(|error| {
                format!("Beryl could not durably admit the replacement input: {error}")
            })
        }
    }
}

pub(crate) fn rollback_resident_edit_tail<B>(
    backend: &mut B,
    proof: &resident_branch_edit::ResidentEditProof,
    timeout: Duration,
) -> Result<(), String>
where
    B: ResidentEditRollbackBackend,
{
    backend
        .rollback_thread(
            &proof.source_thread_id,
            proof.rollback_turns_including_target,
            timeout,
        )
        .map(|_| ())
        .map_err(|error| {
            format!(
                "Beryl could not roll back CAS thread {} by {} turn(s): {error}",
                proof.source_thread_id, proof.rollback_turns_including_target
            )
        })
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
) -> bool {
    title::automatic_thread_title_generation_is_eligible(automatic_title_generation_allowed)
}

#[cfg(test)]
pub(crate) fn thread_title_candidate_available_for_mode(title_mode: TurnThreadTitleMode) -> bool {
    title::automatic_thread_title_candidate("thread_id", "First real branch prompt", title_mode)
        .is_some()
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
        || is_beryl_threaded_decision_dynamic_tool(request)
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
    beryl_workspace_id: BerylWorkspaceId,
    syndic_storage_dir: PathBuf,
    workspace: WorkspaceId,
    thread_id: String,
    syndic_view_id: String,
    label: String,
    sender: Sender<ThreadActivationUpdate>,
) {
    let worker_started = Instant::now();
    MemoryMilestone::new("thread_activation_worker_start")
        .workspace_id(beryl_workspace_id.as_str())
        .runtime(workspace.runtime_mode().display_name())
        .thread_id(thread_id.as_str())
        .log();
    let summary = ThreadSummary {
        id: thread_id.clone(),
        forked_from_id: None,
        cwd: workspace.canonical_path().to_path_buf(),
        preview: String::new(),
        name: Some(label),
        agent_nickname: None,
        path: None,
        created_at: 0,
        updated_at: 0,
        model_provider: String::new(),
        ephemeral: false,
    };
    let prepared_transcript =
        prepare_storage_backed_transcript_activation(syndic_storage_dir, &syndic_view_id);
    debug!(
        thread_id = thread_id.as_str(),
        syndic_view_id = syndic_view_id.as_str(),
        worker_total_ms = elapsed_ms(worker_started.elapsed()),
        "thread activation worker prepared Syndic transcript activation"
    );
    MemoryMilestone::new("thread_activation_worker_done")
        .workspace_id(beryl_workspace_id.as_str())
        .runtime(workspace.runtime_mode().display_name())
        .thread_id(thread_id.as_str())
        .log();
    info!(
        thread_id = thread_id.as_str(),
        syndic_view_id = syndic_view_id.as_str(),
        "Prepared selected-thread activation from Syndic"
    );
    let _ = sender.send(ThreadActivationUpdate::Finished(
        ThreadActivationOutcome::Activated {
            execution_target: workspace,
            summary,
            status: ThreadStatus::Idle,
            session_metadata: None,
            prepared_transcript,
        },
    ));
}

fn elapsed_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}
