use beryl_model::{
    BindingRevision, CasLoadedSessionGeneration, CasThreadId, CasTurnId, InputGateRevision,
    RuntimeId, SyndicExecutionSnapshotId, SyndicThreadId, SyndicTurnId,
};

use crate::{
    AcceptedRouteHeadProof, StopAttemptNonce, StopCause, StopCauseFirstRevisions, StopCauseSet,
    StopDispatchClaimWitness, StopOperationId, StopOperationRevision, TurnKind,
};

mod operation;
mod provider;
mod successor;

pub use operation::*;
pub use successor::*;

/// Immutable exact provider operation selected by one durable stop.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StopOperationTarget {
    thread_id: SyndicThreadId,
    turn_id: SyndicTurnId,
    turn_kind: TurnKind,
    binding_revision: BindingRevision,
    snapshot_id: SyndicExecutionSnapshotId,
    runtime_id: RuntimeId,
    loaded_generation: CasLoadedSessionGeneration,
    cas_thread_id: CasThreadId,
    cas_turn_id: CasTurnId,
}

impl StopOperationTarget {
    /// Captures the complete immutable execution authority selected for interruption.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        turn_id: SyndicTurnId,
        turn_kind: TurnKind,
        binding_revision: BindingRevision,
        snapshot_id: SyndicExecutionSnapshotId,
        runtime_id: RuntimeId,
        loaded_generation: CasLoadedSessionGeneration,
        cas_thread_id: CasThreadId,
        cas_turn_id: CasTurnId,
    ) -> Self {
        Self {
            thread_id,
            turn_id,
            turn_kind,
            binding_revision,
            snapshot_id,
            runtime_id,
            loaded_generation,
            cas_thread_id,
            cas_turn_id,
        }
    }

    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }

    #[must_use]
    pub const fn turn_id(&self) -> SyndicTurnId {
        self.turn_id
    }

    #[must_use]
    pub const fn turn_kind(&self) -> TurnKind {
        self.turn_kind
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
    pub const fn runtime_id(&self) -> RuntimeId {
        self.runtime_id
    }

    #[must_use]
    pub const fn loaded_generation(&self) -> CasLoadedSessionGeneration {
        self.loaded_generation
    }

    #[must_use]
    pub const fn cas_thread_id(&self) -> &CasThreadId {
        &self.cas_thread_id
    }

    #[must_use]
    pub const fn cas_turn_id(&self) -> &CasTurnId {
        &self.cas_turn_id
    }
}

/// Immutable exact source and successor selected by stop admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StopAdmissionWitness {
    Ordinary {
        source_gate_revision: InputGateRevision,
        source_selected_route: AcceptedRouteHeadProof,
        successor_gate_revision: InputGateRevision,
        successor_stopped_route: AcceptedRouteHeadProof,
    },
    ProviderOperation {
        source_gate_revision: InputGateRevision,
        successor_gate_revision: InputGateRevision,
        source_compaction_revision: crate::CompactionOperationRevision,
        successor_compaction_revision: crate::CompactionOperationRevision,
    },
}

impl StopAdmissionWitness {
    #[must_use]
    pub const fn new(
        source_gate_revision: InputGateRevision,
        source_selected_route: AcceptedRouteHeadProof,
        successor_gate_revision: InputGateRevision,
        successor_stopped_route: AcceptedRouteHeadProof,
    ) -> Self {
        Self::Ordinary {
            source_gate_revision,
            source_selected_route,
            successor_gate_revision,
            successor_stopped_route,
        }
    }

    #[must_use]
    pub const fn provider_operation(
        source_gate_revision: InputGateRevision,
        successor_gate_revision: InputGateRevision,
        source_compaction_revision: crate::CompactionOperationRevision,
        successor_compaction_revision: crate::CompactionOperationRevision,
    ) -> Self {
        Self::ProviderOperation {
            source_gate_revision,
            successor_gate_revision,
            source_compaction_revision,
            successor_compaction_revision,
        }
    }

    #[must_use]
    pub const fn source_gate_revision(self) -> InputGateRevision {
        match self {
            Self::Ordinary {
                source_gate_revision,
                ..
            }
            | Self::ProviderOperation {
                source_gate_revision,
                ..
            } => source_gate_revision,
        }
    }

    #[must_use]
    pub const fn source_selected_route(self) -> AcceptedRouteHeadProof {
        match self {
            Self::Ordinary {
                source_selected_route,
                ..
            } => source_selected_route,
            Self::ProviderOperation { .. } => panic!("provider-operation stop has no route"),
        }
    }

    #[must_use]
    pub const fn source_selected_route_option(self) -> Option<AcceptedRouteHeadProof> {
        match self {
            Self::Ordinary {
                source_selected_route,
                ..
            } => Some(source_selected_route),
            Self::ProviderOperation { .. } => None,
        }
    }

    #[must_use]
    pub const fn successor_gate_revision(self) -> InputGateRevision {
        match self {
            Self::Ordinary {
                successor_gate_revision,
                ..
            }
            | Self::ProviderOperation {
                successor_gate_revision,
                ..
            } => successor_gate_revision,
        }
    }

    #[must_use]
    pub const fn successor_stopped_route(self) -> AcceptedRouteHeadProof {
        match self {
            Self::Ordinary {
                successor_stopped_route,
                ..
            } => successor_stopped_route,
            Self::ProviderOperation { .. } => panic!("provider-operation stop has no route"),
        }
    }

    #[must_use]
    pub const fn successor_stopped_route_option(self) -> Option<AcceptedRouteHeadProof> {
        match self {
            Self::Ordinary {
                successor_stopped_route,
                ..
            } => Some(successor_stopped_route),
            Self::ProviderOperation { .. } => None,
        }
    }

    #[must_use]
    pub const fn is_provider_operation(self) -> bool {
        matches!(self, Self::ProviderOperation { .. })
    }

    #[must_use]
    pub const fn source_compaction_revision(self) -> Option<crate::CompactionOperationRevision> {
        match self {
            Self::Ordinary { .. } => None,
            Self::ProviderOperation {
                source_compaction_revision,
                ..
            } => Some(source_compaction_revision),
        }
    }

    #[must_use]
    pub const fn successor_compaction_revision(self) -> Option<crate::CompactionOperationRevision> {
        match self {
            Self::Ordinary { .. } => None,
            Self::ProviderOperation {
                successor_compaction_revision,
                ..
            } => Some(successor_compaction_revision),
        }
    }
}
