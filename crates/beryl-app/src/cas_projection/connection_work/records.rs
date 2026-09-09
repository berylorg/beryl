use beryl_backend::{ApprovalRequestKind, ResponseWorkSnapshot};
use beryl_model::{
    CasLoadedSessionGeneration, CasProcessGeneration, CasThreadId, CasTurnId, RuntimeId,
    SyndicThreadId,
};

use crate::cas_projection::LiveEventTargetCloseReason;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConnectionWorkTargetIdentity {
    pub(in crate::cas_projection) runtime_id: RuntimeId,
    pub(in crate::cas_projection) process_generation: CasProcessGeneration,
    pub(in crate::cas_projection) connection_generation: u64,
    pub(in crate::cas_projection) registration_serial: u64,
    pub(in crate::cas_projection) thread_id: SyndicThreadId,
    pub(in crate::cas_projection) cas_thread_id: CasThreadId,
    pub(in crate::cas_projection) loaded_generation: CasLoadedSessionGeneration,
    pub(in crate::cas_projection) home_generation: u64,
}

impl ConnectionWorkTargetIdentity {
    pub const fn runtime_id(&self) -> RuntimeId {
        self.runtime_id
    }
    pub const fn process_generation(&self) -> CasProcessGeneration {
        self.process_generation
    }
    pub const fn connection_generation(&self) -> u64 {
        self.connection_generation
    }
    pub const fn registration_serial(&self) -> u64 {
        self.registration_serial
    }
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }
    pub const fn cas_thread_id(&self) -> &CasThreadId {
        &self.cas_thread_id
    }
    pub const fn loaded_generation(&self) -> CasLoadedSessionGeneration {
        self.loaded_generation
    }
    pub const fn home_generation(&self) -> u64 {
        self.home_generation
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectionTargetWorkState {
    AwaitingStart,
    AwaitingCompactionTurn,
    Executing,
    Terminal,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConnectionTargetWorkFact {
    pub(in crate::cas_projection) identity: ConnectionWorkTargetIdentity,
    pub(in crate::cas_projection) turn_id: Option<CasTurnId>,
    pub(in crate::cas_projection) state: ConnectionTargetWorkState,
    pub(in crate::cas_projection) start_dispatched: bool,
    pub(in crate::cas_projection) activation_durable: bool,
    pub(in crate::cas_projection) publication_pending: bool,
    pub(in crate::cas_projection) closing: Option<LiveEventTargetCloseReason>,
    pub(in crate::cas_projection) loss_requested: bool,
    pub(in crate::cas_projection) queued_operations: usize,
    pub(in crate::cas_projection) connection_retired: bool,
}

impl ConnectionTargetWorkFact {
    pub const fn identity(&self) -> &ConnectionWorkTargetIdentity {
        &self.identity
    }
    pub const fn turn_id(&self) -> Option<&CasTurnId> {
        self.turn_id.as_ref()
    }
    pub const fn state(&self) -> ConnectionTargetWorkState {
        self.state
    }
    pub const fn start_dispatched(&self) -> bool {
        self.start_dispatched
    }
    pub const fn activation_durable(&self) -> bool {
        self.activation_durable
    }
    pub const fn publication_pending(&self) -> bool {
        self.publication_pending
    }
    pub const fn closing(&self) -> Option<LiveEventTargetCloseReason> {
        self.closing
    }
    pub const fn loss_requested(&self) -> bool {
        self.loss_requested
    }
    pub const fn queued_operations(&self) -> usize {
        self.queued_operations
    }
    pub const fn connection_retired(&self) -> bool {
        self.connection_retired
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectionRequestWorkKind {
    Approval(ApprovalRequestKind),
    DynamicTool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectionRequestWorkStage {
    Ingress,
    Queued,
    Handling,
    ResponseAdmitted,
    Rejected,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConnectionRequestWorkFact {
    pub(in crate::cas_projection) identity: ConnectionWorkTargetIdentity,
    pub(in crate::cas_projection) request_serial: u64,
    pub(in crate::cas_projection) turn_id: CasTurnId,
    pub(in crate::cas_projection) kind: ConnectionRequestWorkKind,
    pub(in crate::cas_projection) stage: ConnectionRequestWorkStage,
    pub(in crate::cas_projection) response: ResponseWorkSnapshot,
    pub(in crate::cas_projection) target_registered: bool,
    pub(in crate::cas_projection) connection_retired: bool,
}

impl ConnectionRequestWorkFact {
    pub const fn identity(&self) -> &ConnectionWorkTargetIdentity {
        &self.identity
    }
    pub const fn request_serial(&self) -> u64 {
        self.request_serial
    }
    pub const fn turn_id(&self) -> &CasTurnId {
        &self.turn_id
    }
    pub const fn kind(&self) -> ConnectionRequestWorkKind {
        self.kind
    }
    pub const fn stage(&self) -> ConnectionRequestWorkStage {
        self.stage
    }
    pub const fn response(&self) -> &ResponseWorkSnapshot {
        &self.response
    }
    pub const fn target_registered(&self) -> bool {
        self.target_registered
    }
    pub const fn connection_retired(&self) -> bool {
        self.connection_retired
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConnectionWorkRecord {
    Target(ConnectionTargetWorkFact),
    Request(ConnectionRequestWorkFact),
}

impl ConnectionWorkRecord {
    pub fn identity(&self) -> &ConnectionWorkTargetIdentity {
        match self {
            Self::Target(fact) => fact.identity(),
            Self::Request(fact) => fact.identity(),
        }
    }

    pub(super) fn bytes(&self) -> usize {
        let turn_bytes = match self {
            Self::Target(fact) => fact.turn_id.as_ref().map_or(0, |turn| turn.as_str().len()),
            Self::Request(fact) => fact.turn_id.as_str().len(),
        };
        std::mem::size_of::<Self>() + self.identity().cas_thread_id.as_str().len() + turn_bytes
    }
}
