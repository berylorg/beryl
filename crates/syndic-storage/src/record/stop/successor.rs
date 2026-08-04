use beryl_model::{BindingRevision, InputGateRevision};

use crate::{
    AcceptedRouteHeadProof, CompactionOperationRevision, StopAbandonmentReason,
    StopOperationRevision, TurnStateRevision,
};

/// Exact source revisions consumed by one closed stop disposition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StopDispositionSource {
    gate_revision: InputGateRevision,
    stop_revision: StopOperationRevision,
}

impl StopDispositionSource {
    #[must_use]
    pub const fn new(
        gate_revision: InputGateRevision,
        stop_revision: StopOperationRevision,
    ) -> Self {
        Self {
            gate_revision,
            stop_revision,
        }
    }

    #[must_use]
    pub const fn gate_revision(self) -> InputGateRevision {
        self.gate_revision
    }

    #[must_use]
    pub const fn stop_revision(self) -> StopOperationRevision {
        self.stop_revision
    }
}

/// Exact bounded successor published after proven local nondispatch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StopSafeReopenWitness {
    Ordinary {
        source: StopDispositionSource,
        successor_gate_revision: InputGateRevision,
        successor_route: AcceptedRouteHeadProof,
    },
    ProviderOperation {
        source: StopDispositionSource,
        successor_gate_revision: InputGateRevision,
        source_compaction_revision: CompactionOperationRevision,
        successor_compaction_revision: CompactionOperationRevision,
    },
}

impl StopSafeReopenWitness {
    #[must_use]
    pub const fn new(
        source: StopDispositionSource,
        successor_gate_revision: InputGateRevision,
        successor_route: AcceptedRouteHeadProof,
    ) -> Self {
        Self::Ordinary {
            source,
            successor_gate_revision,
            successor_route,
        }
    }

    #[must_use]
    pub const fn provider_operation(
        source: StopDispositionSource,
        successor_gate_revision: InputGateRevision,
        source_compaction_revision: CompactionOperationRevision,
        successor_compaction_revision: CompactionOperationRevision,
    ) -> Self {
        Self::ProviderOperation {
            source,
            successor_gate_revision,
            source_compaction_revision,
            successor_compaction_revision,
        }
    }

    #[must_use]
    pub const fn source(self) -> StopDispositionSource {
        match self {
            Self::Ordinary { source, .. } | Self::ProviderOperation { source, .. } => source,
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
    pub const fn successor_route(self) -> AcceptedRouteHeadProof {
        match self {
            Self::Ordinary {
                successor_route, ..
            } => successor_route,
            Self::ProviderOperation { .. } => panic!("provider-operation reopen has no route"),
        }
    }

    #[must_use]
    pub const fn is_provider_operation(self) -> bool {
        matches!(self, Self::ProviderOperation { .. })
    }
}

/// Exact bounded successor published by matching terminal ingestion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StopMatchingTerminalWitness {
    Ordinary {
        source: StopDispositionSource,
        successor_gate_revision: InputGateRevision,
        successor_turn_state_revision: TurnStateRevision,
    },
    ProviderOperation {
        source: StopDispositionSource,
        successor_gate_revision: InputGateRevision,
        successor_turn_state_revision: TurnStateRevision,
        source_compaction_revision: CompactionOperationRevision,
        successor_compaction_revision: CompactionOperationRevision,
    },
}

impl StopMatchingTerminalWitness {
    #[must_use]
    pub const fn new(
        source: StopDispositionSource,
        successor_gate_revision: InputGateRevision,
        successor_turn_state_revision: TurnStateRevision,
    ) -> Self {
        Self::Ordinary {
            source,
            successor_gate_revision,
            successor_turn_state_revision,
        }
    }

    #[must_use]
    pub const fn provider_operation(
        source: StopDispositionSource,
        successor_gate_revision: InputGateRevision,
        successor_turn_state_revision: TurnStateRevision,
        source_compaction_revision: CompactionOperationRevision,
        successor_compaction_revision: CompactionOperationRevision,
    ) -> Self {
        Self::ProviderOperation {
            source,
            successor_gate_revision,
            successor_turn_state_revision,
            source_compaction_revision,
            successor_compaction_revision,
        }
    }

    #[must_use]
    pub const fn source(self) -> StopDispositionSource {
        match self {
            Self::Ordinary { source, .. } | Self::ProviderOperation { source, .. } => source,
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
    pub const fn successor_turn_state_revision(self) -> TurnStateRevision {
        match self {
            Self::Ordinary {
                successor_turn_state_revision,
                ..
            }
            | Self::ProviderOperation {
                successor_turn_state_revision,
                ..
            } => successor_turn_state_revision,
        }
    }

    #[must_use]
    pub const fn is_provider_operation(self) -> bool {
        matches!(self, Self::ProviderOperation { .. })
    }
}

/// Exact bounded successor published by classified stop abandonment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StopAbandonmentWitness {
    Ordinary {
        source: StopDispositionSource,
        reason: StopAbandonmentReason,
        successor_gate_revision: InputGateRevision,
        retired_binding_revision: BindingRevision,
        successor_turn_state_revision: TurnStateRevision,
    },
    ProviderOperation {
        source: StopDispositionSource,
        reason: StopAbandonmentReason,
        successor_gate_revision: InputGateRevision,
        retired_binding_revision: BindingRevision,
        successor_turn_state_revision: TurnStateRevision,
        source_compaction_revision: CompactionOperationRevision,
        successor_compaction_revision: CompactionOperationRevision,
    },
}

impl StopAbandonmentWitness {
    #[must_use]
    pub const fn new(
        source: StopDispositionSource,
        reason: StopAbandonmentReason,
        successor_gate_revision: InputGateRevision,
        retired_binding_revision: BindingRevision,
        successor_turn_state_revision: TurnStateRevision,
    ) -> Self {
        Self::Ordinary {
            source,
            reason,
            successor_gate_revision,
            retired_binding_revision,
            successor_turn_state_revision,
        }
    }

    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn provider_operation(
        source: StopDispositionSource,
        reason: StopAbandonmentReason,
        successor_gate_revision: InputGateRevision,
        retired_binding_revision: BindingRevision,
        successor_turn_state_revision: TurnStateRevision,
        source_compaction_revision: CompactionOperationRevision,
        successor_compaction_revision: CompactionOperationRevision,
    ) -> Self {
        Self::ProviderOperation {
            source,
            reason,
            successor_gate_revision,
            retired_binding_revision,
            successor_turn_state_revision,
            source_compaction_revision,
            successor_compaction_revision,
        }
    }

    #[must_use]
    pub const fn source(self) -> StopDispositionSource {
        match self {
            Self::Ordinary { source, .. } | Self::ProviderOperation { source, .. } => source,
        }
    }

    #[must_use]
    pub const fn reason(self) -> StopAbandonmentReason {
        match self {
            Self::Ordinary { reason, .. } | Self::ProviderOperation { reason, .. } => reason,
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
    pub const fn retired_binding_revision(self) -> BindingRevision {
        match self {
            Self::Ordinary {
                retired_binding_revision,
                ..
            }
            | Self::ProviderOperation {
                retired_binding_revision,
                ..
            } => retired_binding_revision,
        }
    }

    #[must_use]
    pub const fn successor_turn_state_revision(self) -> TurnStateRevision {
        match self {
            Self::Ordinary {
                successor_turn_state_revision,
                ..
            }
            | Self::ProviderOperation {
                successor_turn_state_revision,
                ..
            } => successor_turn_state_revision,
        }
    }

    #[must_use]
    pub const fn is_provider_operation(self) -> bool {
        matches!(self, Self::ProviderOperation { .. })
    }
}
