use beryl_model::SyndicTurnId;
use syndic_storage::CompactionOperationId;

/// Compact durable authority attached to one pre-turn router target.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) struct ContextCompactionTargetAuthority {
    operation_id: CompactionOperationId,
    provider_turn_id: SyndicTurnId,
}

impl ContextCompactionTargetAuthority {
    pub(in crate::cas_projection) const fn new(
        operation_id: CompactionOperationId,
        provider_turn_id: SyndicTurnId,
    ) -> Self {
        Self {
            operation_id,
            provider_turn_id,
        }
    }

    pub(in crate::cas_projection) const fn operation_id(self) -> CompactionOperationId {
        self.operation_id
    }

    pub(in crate::cas_projection) const fn provider_turn_id(self) -> SyndicTurnId {
        self.provider_turn_id
    }
}
