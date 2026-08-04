use beryl_model::{
    BindingRevision, CasThreadId, CasTurnId, SyndicExecutionSnapshotId, SyndicTurnId,
};

/// Exact durable correlation available while CAS has not returned its turn id.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingSteeringTargetProof {
    binding_revision: BindingRevision,
    snapshot_id: SyndicExecutionSnapshotId,
    active_turn_id: SyndicTurnId,
    cas_thread_id: CasThreadId,
}

impl PendingSteeringTargetProof {
    #[must_use]
    pub const fn new(
        binding_revision: BindingRevision,
        snapshot_id: SyndicExecutionSnapshotId,
        active_turn_id: SyndicTurnId,
        cas_thread_id: CasThreadId,
    ) -> Self {
        Self {
            binding_revision,
            snapshot_id,
            active_turn_id,
            cas_thread_id,
        }
    }

    #[must_use]
    pub const fn binding_revision(&self) -> BindingRevision {
        self.binding_revision
    }
    #[must_use]
    pub const fn snapshot_id(&self) -> SyndicExecutionSnapshotId {
        self.snapshot_id
    }
    #[must_use]
    pub const fn active_turn_id(&self) -> SyndicTurnId {
        self.active_turn_id
    }
    #[must_use]
    pub const fn cas_thread_id(&self) -> &CasThreadId {
        &self.cas_thread_id
    }
}

/// Exact durable steering target after CAS returned its active turn id.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SteeringTargetProof {
    pending: PendingSteeringTargetProof,
    cas_turn_id: CasTurnId,
}

impl SteeringTargetProof {
    #[must_use]
    pub const fn new(pending: PendingSteeringTargetProof, cas_turn_id: CasTurnId) -> Self {
        Self {
            pending,
            cas_turn_id,
        }
    }

    #[must_use]
    pub const fn pending(&self) -> &PendingSteeringTargetProof {
        &self.pending
    }
    #[must_use]
    pub const fn cas_turn_id(&self) -> &CasTurnId {
        &self.cas_turn_id
    }
}

/// Why one admitted fragment is waiting for a later ordinary turn.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum NextTurnReason {
    PendingTurn,
    Compaction,
    Stop,
    SteeringRejected,
    ProjectionLost,
    TerminalHistory,
    UnknownTerminal,
}

/// Exact per-thread mode against which input admission is revision-checked.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InputGateState {
    Idle,
    PendingTurn(SyndicTurnId),
    AwaitingSteering(SyndicTurnId),
    Steerable(SyndicTurnId),
    AwaitingTerminal(SyndicTurnId),
    Compacting {
        turn_id: SyndicTurnId,
        operation_nonce: crate::CompactionOperationNonce,
    },
    Stopping {
        turn_id: SyndicTurnId,
        operation_nonce: crate::StopOperationNonce,
    },
    FinalizingHistory(SyndicTurnId),
}

impl InputGateState {
    /// Constructs a compacting gate selecting one exact retained operation.
    #[must_use]
    pub const fn compacting(
        turn_id: SyndicTurnId,
        operation_nonce: crate::CompactionOperationNonce,
    ) -> Self {
        Self::Compacting {
            turn_id,
            operation_nonce,
        }
    }

    /// Constructs a stopping gate selecting one exact retained stop operation.
    #[must_use]
    pub const fn stopping(
        turn_id: SyndicTurnId,
        operation_nonce: crate::StopOperationNonce,
    ) -> Self {
        Self::Stopping {
            turn_id,
            operation_nonce,
        }
    }

    #[must_use]
    pub const fn blocking_turn_id(&self) -> Option<SyndicTurnId> {
        match self {
            Self::Idle => None,
            Self::PendingTurn(turn)
            | Self::AwaitingSteering(turn)
            | Self::Steerable(turn)
            | Self::AwaitingTerminal(turn)
            | Self::FinalizingHistory(turn) => Some(*turn),
            Self::Compacting { turn_id, .. } => Some(*turn_id),
            Self::Stopping { turn_id, .. } => Some(*turn_id),
        }
    }

    #[must_use]
    pub const fn compaction_operation_nonce(&self) -> Option<crate::CompactionOperationNonce> {
        match self {
            Self::Compacting {
                operation_nonce, ..
            } => Some(*operation_nonce),
            _ => None,
        }
    }

    /// Returns the caller-owned operation nonce selected by a stopping gate.
    #[must_use]
    pub const fn stop_operation_nonce(&self) -> Option<crate::StopOperationNonce> {
        match self {
            Self::Stopping {
                operation_nonce, ..
            } => Some(*operation_nonce),
            _ => None,
        }
    }
}
