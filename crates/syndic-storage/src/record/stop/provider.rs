use super::*;

impl StopOperationRecord {
    pub(crate) fn provider_abandonment_authenticates(
        &self,
        operation: &crate::CompactionOperationRecord,
        receipt: &crate::CompactionSettlementReceiptRecord,
    ) -> bool {
        let StopAdmissionWitness::ProviderOperation {
            source_compaction_revision: admission_source,
            successor_compaction_revision: admission_successor,
            ..
        } = self.admission()
        else {
            return false;
        };
        let StopOperationState::Abandoned(StopAbandonmentWitness::ProviderOperation {
            source,
            reason,
            successor_gate_revision,
            source_compaction_revision,
            successor_compaction_revision,
            ..
        }) = self.state()
        else {
            return false;
        };
        let expected_reason = match reason {
            crate::StopAbandonmentReason::StartupProcessGenerationLost => {
                crate::CompactionAbandonmentReason::StartupProcessGenerationLost
            }
            crate::StopAbandonmentReason::ProviderRejectedBeforeCoreInterrupt
            | crate::StopAbandonmentReason::TargetAuthorityLost => {
                crate::CompactionAbandonmentReason::TargetAuthorityLost
            }
        };
        self.target().turn_kind()
            == TurnKind::ProviderOperation(crate::ProviderOperationKind::ContextCompaction)
            && operation.target().thread_id() == self.target().thread_id()
            && operation.target().turn_id() == self.target().turn_id()
            && receipt.source_gate().revision() == source.gate_revision()
            && receipt.successor_gate().revision() == successor_gate_revision
            && receipt.source_gate().state()
                == &crate::InputGateState::stopping(self.target().turn_id(), self.id().nonce())
            && receipt.settlement() == &crate::CompactionSettlement::Abandoned(expected_reason)
            && operation.stop_abandonment_successor_is_exact(
                admission_source,
                admission_successor,
                source_compaction_revision,
                successor_compaction_revision,
                receipt,
            )
    }
}
