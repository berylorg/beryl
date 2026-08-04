use beryl_model::BindingRevision;

use crate::{
    CompactionOperationId, CompactionOperationRevision, CompactionSettlement, ContentReference,
    ConversationParent, InputGateRecord, InputGateState, SelectedPathProof, SyndicRecordError,
};

/// Immutable topology created with one lifecycle-continuation settlement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompactionContinuationReceipt {
    parent: ConversationParent,
    selected_path: SelectedPathProof,
    binding_revision: BindingRevision,
    content: ContentReference,
}

impl CompactionContinuationReceipt {
    #[must_use]
    pub const fn new(
        parent: ConversationParent,
        selected_path: SelectedPathProof,
        binding_revision: BindingRevision,
        content: ContentReference,
    ) -> Self {
        Self {
            parent,
            selected_path,
            binding_revision,
            content,
        }
    }

    #[must_use]
    pub const fn parent(self) -> ConversationParent {
        self.parent
    }

    #[must_use]
    pub const fn selected_path(self) -> SelectedPathProof {
        self.selected_path
    }

    #[must_use]
    pub const fn binding_revision(self) -> BindingRevision {
        self.binding_revision
    }

    #[must_use]
    pub const fn content(self) -> ContentReference {
        self.content
    }
}

/// Independent immutable proof of one consumed compaction's exact gate transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompactionSettlementReceiptRecord {
    operation_id: CompactionOperationId,
    source_operation_revision: CompactionOperationRevision,
    successor_operation_revision: CompactionOperationRevision,
    source_gate: InputGateRecord,
    successor_gate: InputGateRecord,
    settlement: CompactionSettlement,
    continuation: Option<CompactionContinuationReceipt>,
}

impl CompactionSettlementReceiptRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        operation_id: CompactionOperationId,
        source_operation_revision: CompactionOperationRevision,
        successor_operation_revision: CompactionOperationRevision,
        source_gate: InputGateRecord,
        successor_gate: InputGateRecord,
        settlement: CompactionSettlement,
        continuation: Option<CompactionContinuationReceipt>,
    ) -> Result<Self, SyndicRecordError> {
        let record = Self {
            operation_id,
            source_operation_revision,
            successor_operation_revision,
            source_gate,
            successor_gate,
            settlement,
            continuation,
        };
        if !record.transition_shape_is_exact() {
            return Err(SyndicRecordError::InvalidCompactionOperation);
        }
        Ok(record)
    }

    #[must_use]
    pub const fn operation_id(&self) -> CompactionOperationId {
        self.operation_id
    }

    #[must_use]
    pub const fn source_operation_revision(&self) -> CompactionOperationRevision {
        self.source_operation_revision
    }

    #[must_use]
    pub const fn successor_operation_revision(&self) -> CompactionOperationRevision {
        self.successor_operation_revision
    }

    #[must_use]
    pub const fn source_gate(&self) -> &InputGateRecord {
        &self.source_gate
    }

    #[must_use]
    pub const fn successor_gate(&self) -> &InputGateRecord {
        &self.successor_gate
    }

    #[must_use]
    pub const fn settlement(&self) -> &CompactionSettlement {
        &self.settlement
    }

    #[must_use]
    pub const fn continuation(&self) -> Option<CompactionContinuationReceipt> {
        self.continuation
    }

    #[must_use]
    pub(crate) fn current_gate_is_descendant(&self, current: &InputGateRecord) -> bool {
        if current.thread_id() != self.successor_gate.thread_id()
            || current.revision() < self.successor_gate.revision()
            || current.accepted_high_water() < self.successor_gate.accepted_high_water()
            || generation(current.route_generation_high_water())
                < generation(self.successor_gate.route_generation_high_water())
        {
            return false;
        }
        current.revision() > self.successor_gate.revision() || current == &self.successor_gate
    }

    fn transition_shape_is_exact(&self) -> bool {
        let thread = self.operation_id.thread_id();
        if self.source_operation_revision.checked_next().ok()
            != Some(self.successor_operation_revision)
            || self.source_gate.thread_id() != thread
            || self.successor_gate.thread_id() != thread
            || self.source_gate.revision().checked_next().ok()
                != Some(self.successor_gate.revision())
            || !self.source_gate_selects_operation()
            || self.source_gate.live_steering_count() != 0
            || self.successor_gate.live_steering_count() != 0
            || self.source_gate.accepted_high_water() != self.successor_gate.accepted_high_water()
            || self.source_gate.route_generation_high_water()
                != self.successor_gate.route_generation_high_water()
        {
            return false;
        }
        match (&self.settlement, self.continuation) {
            (
                CompactionSettlement::LifecycleContinuation {
                    turn_id,
                    item_id: _,
                    content_id,
                },
                Some(continuation),
            ) => {
                continuation.selected_path().tail() == Some(*turn_id)
                    && continuation.content().id() == *content_id
                    && self.source_gate.live_next_turn_count() == 0
                    && self.source_gate.live_logical_utf8_bytes() == 0
                    && self.successor_gate.state() == &InputGateState::PendingTurn(*turn_id)
                    && self.successor_gate.selected_route().is_none()
                    && self.successor_gate.live_next_turn_count() == 0
                    && self.successor_gate.live_logical_utf8_bytes() == 0
            }
            (CompactionSettlement::LifecycleContinuation { .. }, None) | (_, Some(_)) => false,
            (settlement, None) => {
                self.successor_gate.state() == &InputGateState::Idle
                    && self.successor_gate.selected_route() == self.source_gate.selected_route()
                    && self.successor_gate.live_next_turn_count()
                        == self.source_gate.live_next_turn_count()
                    && self.successor_gate.live_logical_utf8_bytes()
                        == self.source_gate.live_logical_utf8_bytes()
                    && (!matches!(settlement, CompactionSettlement::LifecycleUserWorkWon)
                        || self.source_gate.live_next_turn_count() > 0)
            }
        }
    }

    fn source_gate_selects_operation(&self) -> bool {
        match self.source_gate.state() {
            InputGateState::Compacting {
                turn_id,
                operation_nonce,
            } => {
                *turn_id == self.operation_id.provider_turn_id()
                    && *operation_nonce == self.operation_id.nonce()
            }
            InputGateState::Stopping { turn_id, .. } => {
                *turn_id == self.operation_id.provider_turn_id()
                    && matches!(self.settlement, CompactionSettlement::Abandoned(_))
            }
            _ => false,
        }
    }
}

const fn generation(value: Option<crate::AcceptedRouteGeneration>) -> u64 {
    match value {
        Some(value) => value.get(),
        None => 0,
    }
}
