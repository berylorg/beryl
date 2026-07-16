use std::{
    fmt,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
};

use serde_json::Value;

use crate::JsonRpcError;

const COMMAND_EXECUTION_REQUEST_APPROVAL_METHOD: &str = "item/commandExecution/requestApproval";
const FILE_CHANGE_REQUEST_APPROVAL_METHOD: &str = "item/fileChange/requestApproval";
const PERMISSIONS_REQUEST_APPROVAL_METHOD: &str = "item/permissions/requestApproval";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveTurnNotSteerable {
    pub turn_kind: NonSteerableTurnKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NonSteerableTurnKind {
    Review,
    Compact,
    Other(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApprovalRequestKind {
    CommandExecution,
    FileChange,
    Permissions,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApprovalResponseDisposition {
    ResponseRequired,
    AutoDenied,
    Denied,
}

#[derive(Debug)]
struct ApprovalResponseState(AtomicU8);

impl ApprovalResponseState {
    fn new(disposition: ApprovalResponseDisposition) -> Self {
        Self(AtomicU8::new(disposition as u8))
    }

    fn load(&self) -> ApprovalResponseDisposition {
        match self.0.load(Ordering::Acquire) {
            0 => ApprovalResponseDisposition::ResponseRequired,
            1 => ApprovalResponseDisposition::AutoDenied,
            2 => ApprovalResponseDisposition::Denied,
            _ => unreachable!("approval response state is internally closed"),
        }
    }

    fn store(&self, disposition: ApprovalResponseDisposition) {
        self.0.store(disposition as u8, Ordering::Release);
    }
}

impl PartialEq for ApprovalResponseState {
    fn eq(&self, other: &Self) -> bool {
        self.load() == other.load()
    }
}

impl Eq for ApprovalResponseState {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApprovalRequest {
    request_id: Value,
    method: String,
    kind: ApprovalRequestKind,
    params: Value,
    thread_id: Option<String>,
    turn_id: Option<String>,
    item_id: Option<String>,
    command: Option<String>,
    cwd: Option<String>,
    reason: Option<String>,
    response_authority_generation: Option<u64>,
    response_state: Arc<ApprovalResponseState>,
}

impl ApprovalRequestKind {
    #[must_use]
    pub const fn method(self) -> &'static str {
        match self {
            Self::CommandExecution => COMMAND_EXECUTION_REQUEST_APPROVAL_METHOD,
            Self::FileChange => FILE_CHANGE_REQUEST_APPROVAL_METHOD,
            Self::Permissions => PERMISSIONS_REQUEST_APPROVAL_METHOD,
        }
    }

    #[must_use]
    pub const fn denial_response_interrupts_turn(self) -> bool {
        matches!(self, Self::CommandExecution | Self::FileChange)
    }
}

impl fmt::Display for NonSteerableTurnKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Review => formatter.write_str("review"),
            Self::Compact => formatter.write_str("compact"),
            Self::Other(kind) => formatter.write_str(kind),
        }
    }
}

impl NonSteerableTurnKind {
    fn from_wire(value: &str) -> Self {
        match value {
            "review" => Self::Review,
            "compact" => Self::Compact,
            other => Self::Other(other.to_string()),
        }
    }
}

#[must_use]
pub fn active_turn_not_steerable_error(error: &JsonRpcError) -> Option<ActiveTurnNotSteerable> {
    let data = error.data.as_ref()?;
    active_turn_not_steerable_from_value(data)
        .or_else(|| {
            data.get("codexErrorInfo")
                .and_then(active_turn_not_steerable_from_value)
        })
        .or_else(|| {
            data.get("error")
                .and_then(|error| error.get("codexErrorInfo"))
                .and_then(active_turn_not_steerable_from_value)
        })
}

fn active_turn_not_steerable_from_value(value: &Value) -> Option<ActiveTurnNotSteerable> {
    if value.as_str() == Some("activeTurnNotSteerable") {
        return Some(ActiveTurnNotSteerable {
            turn_kind: NonSteerableTurnKind::Other("unknown".to_string()),
        });
    }

    let info = value.get("activeTurnNotSteerable")?;
    let turn_kind = info
        .get("turnKind")
        .and_then(Value::as_str)
        .map(NonSteerableTurnKind::from_wire)
        .unwrap_or_else(|| NonSteerableTurnKind::Other("unknown".to_string()));
    Some(ActiveTurnNotSteerable { turn_kind })
}

impl ApprovalRequest {
    #[must_use]
    pub fn request_id(&self) -> &Value {
        &self.request_id
    }

    #[must_use]
    pub fn method(&self) -> &str {
        &self.method
    }

    #[must_use]
    pub const fn kind(&self) -> ApprovalRequestKind {
        self.kind
    }

    /// Returns whether this request still needs a JSON-RPC response.
    #[must_use]
    pub fn response_disposition(&self) -> ApprovalResponseDisposition {
        self.response_state.load()
    }

    pub(crate) fn bind_response_authority(
        &mut self,
        generation: u64,
        disposition: ApprovalResponseDisposition,
    ) {
        self.response_authority_generation = Some(generation);
        self.response_state.store(disposition);
    }

    pub(crate) fn response_authority_generation(&self) -> Option<u64> {
        self.response_authority_generation
    }

    pub(crate) fn set_response_disposition(&self, disposition: ApprovalResponseDisposition) {
        self.response_state.store(disposition);
    }

    #[must_use]
    pub fn params(&self) -> &Value {
        &self.params
    }

    #[must_use]
    pub fn thread_id(&self) -> Option<&str> {
        self.thread_id.as_deref()
    }

    #[must_use]
    pub fn turn_id(&self) -> Option<&str> {
        self.turn_id.as_deref()
    }

    #[must_use]
    pub fn item_id(&self) -> Option<&str> {
        self.item_id.as_deref()
    }

    #[must_use]
    pub fn command(&self) -> Option<&str> {
        self.command.as_deref()
    }

    #[must_use]
    pub fn cwd(&self) -> Option<&str> {
        self.cwd.as_deref()
    }

    #[must_use]
    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }

    #[must_use]
    pub fn pretty_params(&self) -> String {
        serde_json::to_string_pretty(&self.params).unwrap_or_else(|_| self.params.to_string())
    }

    #[must_use]
    pub fn summary(&self) -> String {
        let command = self.command.as_deref().unwrap_or("<none>");
        let cwd = self.cwd.as_deref().unwrap_or("<none>");
        let reason = self.reason.as_deref().unwrap_or("<none>");
        format!(
            "method={}, requestId={}, threadId={}, turnId={}, itemId={}, cwd={}, command={}, reason={}",
            self.method,
            self.request_id,
            self.thread_id.as_deref().unwrap_or("<unknown>"),
            self.turn_id.as_deref().unwrap_or("<unknown>"),
            self.item_id.as_deref().unwrap_or("<unknown>"),
            cwd,
            command,
            reason
        )
    }
}

#[must_use]
pub fn parse_approval_request(
    request_id: Value,
    method: &str,
    params: Option<Value>,
) -> Option<ApprovalRequest> {
    let kind = match method {
        COMMAND_EXECUTION_REQUEST_APPROVAL_METHOD => ApprovalRequestKind::CommandExecution,
        FILE_CHANGE_REQUEST_APPROVAL_METHOD => ApprovalRequestKind::FileChange,
        PERMISSIONS_REQUEST_APPROVAL_METHOD => ApprovalRequestKind::Permissions,
        _ => return None,
    };
    let params = params.unwrap_or(Value::Null);
    Some(ApprovalRequest {
        request_id,
        method: method.to_string(),
        kind,
        thread_id: string_field(&params, "threadId"),
        turn_id: string_field(&params, "turnId"),
        item_id: string_field(&params, "itemId"),
        command: string_field(&params, "command"),
        cwd: string_field(&params, "cwd"),
        reason: string_field(&params, "reason"),
        response_authority_generation: None,
        response_state: Arc::new(ApprovalResponseState::new(
            ApprovalResponseDisposition::ResponseRequired,
        )),
        params,
    })
}

fn string_field(value: &Value, field: &str) -> Option<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}
