#![allow(dead_code)]

use std::{fmt, time::Duration};

use beryl_backend::{
    ApprovalRequest, DynamicToolCallRequest, DynamicToolCallResponse, ManagedBackendSession,
    ThreadInfo, ThreadItem, ThreadReadOptions, ThreadSummary, TurnInfo, TurnStartOptions,
    TurnStartResponse, TurnStatus, TurnStreamEvent, UserInput,
};
use beryl_model::conversation::{ConversationThreadId, ConversationTurnId};

#[path = "branch_bootstrap_core/backend.rs"]
mod backend;
#[path = "branch_bootstrap_core/error.rs"]
mod error;
#[path = "branch_bootstrap_core/message.rs"]
mod message;
#[path = "branch_bootstrap_core/proof.rs"]
mod proof;

#[allow(unused_imports)]
pub(crate) use message::beryl_thread_link_destination;
pub(crate) use message::{branch_bootstrap_message, parse_beryl_thread_link};
#[allow(unused_imports)]
pub(crate) use proof::bootstrap_dynamic_tool_unavailable_response;
pub(crate) use proof::{
    prove_branch_thread_completed_bootstrap_from_history,
    prove_branch_thread_durable_with_bootstrap_turn,
};
use proof::{
    validate_durable_thread_summary, validate_thread_info_with_completed_bootstrap_turn,
    wait_for_bootstrap_turn_terminal,
};

pub(crate) const BERYL_THREAD_LINK_SCHEME: &str = "beryl_threadid://";
const UNTITLED_THREAD_LABEL: &str = "Untitled thread";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BranchBootstrapMessageInput<'a> {
    pub(crate) parent_thread_id: &'a ConversationThreadId,
    pub(crate) parent_thread_title: Option<&'a str>,
    pub(crate) branch_context: Option<&'a str>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BranchBootstrapTurn {
    thread: ThreadSummary,
    bootstrap_turn_id: Option<ConversationTurnId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BranchBootstrapStartedTurn {
    turn: TurnInfo,
    bootstrap_turn_id: ConversationTurnId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BranchBootstrapHistoryCompletion {
    thread: ThreadSummary,
    turn: TurnInfo,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum BootstrapTerminalProof {
    Streamed(TurnInfo),
    History { thread: ThreadInfo, turn: TurnInfo },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum BranchBootstrapError {
    EmptyMessage {
        thread_id: ConversationThreadId,
    },
    TurnStartFailed {
        thread_id: ConversationThreadId,
        error: String,
    },
    BootstrapTurnMissingId {
        thread_id: ConversationThreadId,
    },
    BootstrapStreamFailed {
        thread_id: ConversationThreadId,
        turn_id: ConversationTurnId,
        error: String,
    },
    BootstrapTurnFailed {
        thread_id: ConversationThreadId,
        turn_id: ConversationTurnId,
        status: TurnStatus,
        error: Option<String>,
    },
    BootstrapUnexpectedApprovalRequest {
        thread_id: ConversationThreadId,
        turn_id: ConversationTurnId,
        request: String,
    },
    BootstrapApprovalDenialFailed {
        thread_id: ConversationThreadId,
        turn_id: ConversationTurnId,
        error: String,
    },
    BootstrapUnexpectedDynamicToolRequest {
        thread_id: ConversationThreadId,
        turn_id: ConversationTurnId,
        request: String,
    },
    BootstrapDynamicToolResponseFailed {
        thread_id: ConversationThreadId,
        turn_id: ConversationTurnId,
        error: String,
    },
    DurabilityProofFailed {
        thread_id: ConversationThreadId,
        error: String,
    },
    DurableThreadIdMismatch {
        expected_thread_id: ConversationThreadId,
        actual_thread_id: String,
    },
    DurableThreadMarkedEphemeral {
        thread_id: ConversationThreadId,
    },
    BootstrapTurnMissingFromHistory {
        thread_id: ConversationThreadId,
        turn_id: ConversationTurnId,
    },
    BootstrapTurnNotCompletedInHistory {
        thread_id: ConversationThreadId,
        turn_id: ConversationTurnId,
        status: TurnStatus,
    },
    BootstrapTurnMissingVisibleMessage {
        thread_id: ConversationThreadId,
        turn_id: ConversationTurnId,
    },
}

pub(crate) trait BranchBootstrapBackend {
    type Error: fmt::Display;

    fn start_turn_with_options(
        &mut self,
        thread_id: &str,
        text: &str,
        options: TurnStartOptions,
        timeout: Duration,
    ) -> Result<TurnStartResponse, Self::Error>;

    fn read_thread_metadata(
        &mut self,
        thread_id: &str,
        timeout: Duration,
    ) -> Result<ThreadSummary, Self::Error>;

    fn read_thread_with_turns(
        &mut self,
        thread_id: &str,
        timeout: Duration,
    ) -> Result<ThreadInfo, Self::Error>;

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
}

impl BranchBootstrapTurn {
    pub(crate) fn thread(&self) -> &ThreadSummary {
        &self.thread
    }

    pub(crate) fn bootstrap_turn_id(&self) -> Option<&ConversationTurnId> {
        self.bootstrap_turn_id.as_ref()
    }
}

impl BranchBootstrapStartedTurn {
    pub(crate) fn turn(&self) -> &TurnInfo {
        &self.turn
    }

    pub(crate) fn bootstrap_turn_id(&self) -> &ConversationTurnId {
        &self.bootstrap_turn_id
    }
}

impl BranchBootstrapHistoryCompletion {
    pub(crate) fn new(thread: ThreadSummary, turn: TurnInfo) -> Self {
        Self { thread, turn }
    }

    pub(crate) fn thread(&self) -> &ThreadSummary {
        &self.thread
    }

    pub(crate) fn turn(&self) -> &TurnInfo {
        &self.turn
    }
}

impl BootstrapTerminalProof {
    fn turn(&self) -> &TurnInfo {
        match self {
            Self::Streamed(turn) | Self::History { turn, .. } => turn,
        }
    }
}

pub(crate) fn start_branch_bootstrap_turn<B>(
    backend: &mut B,
    thread_id: &ConversationThreadId,
    message: &str,
    timeout: Duration,
) -> Result<BranchBootstrapTurn, BranchBootstrapError>
where
    B: BranchBootstrapBackend,
{
    let started = start_branch_bootstrap_turn_only(backend, thread_id, message, timeout)?;
    let bootstrap_turn_id = started.bootstrap_turn_id().clone();

    let terminal_proof = wait_for_bootstrap_turn_terminal(
        backend,
        thread_id,
        &bootstrap_turn_id,
        started.turn,
        timeout,
    )?;
    let terminal_status = terminal_proof.turn().status;
    let terminal_error = terminal_proof.turn().error.clone();
    if terminal_status != TurnStatus::Completed {
        return Err(BranchBootstrapError::BootstrapTurnFailed {
            thread_id: thread_id.clone(),
            turn_id: bootstrap_turn_id,
            status: terminal_status,
            error: terminal_error.map(|error| error.message),
        });
    }

    let thread = match terminal_proof {
        BootstrapTerminalProof::Streamed(_) => prove_branch_thread_durable_with_bootstrap_turn(
            backend,
            thread_id,
            &bootstrap_turn_id,
            message,
            timeout,
        )?,
        BootstrapTerminalProof::History { thread, .. } => {
            validate_thread_info_with_completed_bootstrap_turn(
                thread,
                thread_id,
                &bootstrap_turn_id,
                message,
            )?
        }
    };
    Ok(BranchBootstrapTurn {
        thread,
        bootstrap_turn_id: Some(bootstrap_turn_id),
    })
}

pub(crate) fn start_branch_bootstrap_turn_only<B>(
    backend: &mut B,
    thread_id: &ConversationThreadId,
    message: &str,
    timeout: Duration,
) -> Result<BranchBootstrapStartedTurn, BranchBootstrapError>
where
    B: BranchBootstrapBackend,
{
    let message = message.trim();
    if message.is_empty() {
        return Err(BranchBootstrapError::EmptyMessage {
            thread_id: thread_id.clone(),
        });
    }

    let turn = backend
        .start_turn_with_options(
            thread_id.as_str(),
            message,
            TurnStartOptions::default().without_developer_instructions_context(),
            timeout,
        )
        .map_err(|error| BranchBootstrapError::TurnStartFailed {
            thread_id: thread_id.clone(),
            error: error.to_string(),
        })?
        .turn;

    let Some(bootstrap_turn_id) = bootstrap_turn_id(&turn) else {
        return Err(BranchBootstrapError::BootstrapTurnMissingId {
            thread_id: thread_id.clone(),
        });
    };

    Ok(BranchBootstrapStartedTurn {
        turn,
        bootstrap_turn_id,
    })
}

pub(crate) fn prove_branch_thread_durable<B>(
    backend: &mut B,
    thread_id: &ConversationThreadId,
    timeout: Duration,
) -> Result<ThreadSummary, BranchBootstrapError>
where
    B: BranchBootstrapBackend,
{
    let thread = backend
        .read_thread_metadata(thread_id.as_str(), timeout)
        .map_err(|error| BranchBootstrapError::DurabilityProofFailed {
            thread_id: thread_id.clone(),
            error: error.to_string(),
        })?;

    validate_durable_thread_summary(thread, thread_id)
}

fn bootstrap_turn_id(turn: &TurnInfo) -> Option<ConversationTurnId> {
    let turn_id = turn.id.trim();
    (!turn_id.is_empty()).then(|| ConversationTurnId::new(turn_id.to_string()))
}
