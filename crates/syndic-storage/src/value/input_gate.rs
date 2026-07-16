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
    WorkerCapacity,
    ProjectionLost,
}

/// Current delivery target retained by one accepted input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AcceptedInputDisposition {
    AwaitingSteering(PendingSteeringTargetProof),
    SteerActiveTurn(SteeringTargetProof),
    NextTurn(NextTurnReason),
}

/// Exact per-thread mode against which input admission is revision-checked.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InputGateState {
    Idle,
    PendingTurn(SyndicTurnId),
    AwaitingSteering(PendingSteeringTargetProof),
    Steerable(SteeringTargetProof),
    Compacting(SyndicTurnId),
    Stopping(SteeringTargetProof),
}

impl InputGateState {
    #[must_use]
    pub const fn blocking_turn_id(&self) -> Option<SyndicTurnId> {
        match self {
            Self::Idle => None,
            Self::PendingTurn(turn) | Self::Compacting(turn) => Some(*turn),
            Self::AwaitingSteering(target) => Some(target.active_turn_id()),
            Self::Steerable(target) | Self::Stopping(target) => {
                Some(target.pending().active_turn_id())
            }
        }
    }

    #[must_use]
    pub fn admitted_disposition(&self) -> Option<AcceptedInputDisposition> {
        match self {
            Self::Idle => None,
            Self::PendingTurn(_) => Some(AcceptedInputDisposition::NextTurn(
                NextTurnReason::PendingTurn,
            )),
            Self::AwaitingSteering(target) => {
                Some(AcceptedInputDisposition::AwaitingSteering(target.clone()))
            }
            Self::Steerable(target) => {
                Some(AcceptedInputDisposition::SteerActiveTurn(target.clone()))
            }
            Self::Compacting(_) => Some(AcceptedInputDisposition::NextTurn(
                NextTurnReason::Compaction,
            )),
            Self::Stopping(_) => Some(AcceptedInputDisposition::NextTurn(NextTurnReason::Stop)),
        }
    }
}
