use super::task::Cancellation;
use beryl_backend::{
    ApprovalRequest, CompactionReceipt, CompactionReceiptState, CompactionUnknownReason,
    ThreadStatus, ThreadTokenUsage, TurnStreamEvent,
};
use beryl_model::workspace::WorkspaceId;
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ObserverTarget {
    pub workspace_id: String,
    pub execution_target: WorkspaceId,
    pub generation: u64,
    pub thread_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OperationIdentity {
    pub thread_id: String,
    pub operation_id: String,
    pub observation_session_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Receipt {
    pub operation: OperationIdentity,
    pub turn_id: Option<String>,
    pub state: CompactionReceiptState,
}

impl From<CompactionReceipt> for Receipt {
    fn from(receipt: CompactionReceipt) -> Self {
        Self {
            operation: OperationIdentity {
                thread_id: receipt.thread_id.to_string(),
                operation_id: receipt.operation_id.to_string(),
                observation_session_id: receipt.observation_session_id.to_string(),
            },
            turn_id: receipt.turn_id.map(|id| id.to_string()),
            state: receipt.state,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UnconfirmedReason {
    Unavailable,
    InvalidEvidence,
    Receipt(CompactionUnknownReason),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Activity {
    Started,
    CompactionItemStarted,
    CompactionItemCompleted,
    Retrying,
    CompletedAwaitingIdle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Succeeded,
    Failed {
        message: String,
    },
    Interrupted,
    /// No mutation was dispatched, or the backend explicitly rejected it.
    Rejected {
        message: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum UpdateKind {
    Prepared,
    TurnKnown {
        turn_id: String,
    },
    Warning,
    Unconfirmed(UnconfirmedReason),
    Activity(Activity),
    TokenUsage {
        turn_id: String,
        usage: ThreadTokenUsage,
    },
    Finished(Outcome),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ObserverUpdate {
    pub target: ObserverTarget,
    pub operation: Option<OperationIdentity>,
    pub kind: UpdateKind,
}

impl ObserverUpdate {
    /// The shell must check this before consuming an update from retained work.
    pub(crate) fn belongs_to(&self, target: &ObserverTarget) -> bool {
        &self.target == target
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PortError {
    Unavailable,
    Rejected(String),
}

/// All blocking operations must respect the supplied ordinary request timeout.
/// `connect` returns the advertised process session before any thread resume.
pub(crate) trait BackendPort {
    fn connect(&mut self, timeout: Duration) -> Result<String, PortError>;
    fn disconnect(&mut self);
    fn prepare(&mut self, thread_id: &str) -> Result<OperationIdentity, PortError>;
    fn subscribe(&mut self, thread_id: &str, timeout: Duration) -> Result<(), PortError>;
    fn start(&mut self, timeout: Duration) -> Result<Receipt, PortError>;
    fn read(&mut self, turn_id: Option<&str>, timeout: Duration) -> Result<Receipt, PortError>;
    fn status(
        &mut self,
        thread_id: &str,
        timeout: Duration,
    ) -> Result<(String, ThreadStatus), PortError>;
    fn poll(&mut self, timeout: Duration) -> Result<Option<TurnStreamEvent>, PortError>;
    fn deny_approval(
        &mut self,
        request: &ApprovalRequest,
        thread_id: &str,
        turn_id: Option<&str>,
        timeout: Duration,
    ) -> Result<(), PortError>;
}

pub(crate) trait Clock {
    fn now(&self) -> Duration;
    fn wait(&mut self, duration: Duration, cancellation: &Cancellation);
}

pub(crate) trait UpdateSink {
    /// False stops observation without claiming a backend outcome.
    fn publish(&mut self, update: ObserverUpdate, cancellation: &Cancellation) -> bool;
}
