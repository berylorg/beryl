use super::*;

struct StoppingProof {
    source_frontier: u64,
    current_frontier: u64,
}

impl CompactionOperationRecord {
    /// Authenticates one retained stopping descendant against its exact handoff revisions.
    pub(crate) fn stopping_descendant_is_exact(
        &self,
        stop_nonce: StopOperationNonce,
        admission_source: CompactionOperationRevision,
        admission_successor: CompactionOperationRevision,
    ) -> bool {
        if self.state != CompactionOperationState::Stopping(stop_nonce) || self.terminal.is_some() {
            return false;
        }
        let Some(proof) =
            self.stopping_source_proof(admission_source, admission_successor, self.revision, None)
        else {
            return false;
        };
        proof.source_frontier == proof.current_frontier
            && Some(self.revision.get())
                == canonical_revision(u64::from(self.request.is_some()), proof.current_frontier, 1)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn safe_reopen_descendant_is_exact(
        &self,
        admission_source: CompactionOperationRevision,
        admission_successor: CompactionOperationRevision,
        source: CompactionOperationRevision,
        successor: CompactionOperationRevision,
        receipt: Option<&crate::CompactionSettlementReceiptRecord>,
    ) -> bool {
        if source.checked_next().ok() != Some(successor) {
            return false;
        }
        let Some(proof) = self.stopping_source_proof(
            admission_source,
            admission_successor,
            source,
            Some(successor),
        ) else {
            return false;
        };
        if self
            .terminal
            .is_some_and(|terminal| terminal.sequence().get() <= proof.source_frontier)
        {
            return false;
        }
        let expected =
            canonical_revision(u64::from(self.request.is_some()), proof.current_frontier, 2);
        self.post_stop_descendant_is_exact(expected, receipt, false)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn matching_terminal_descendant_is_exact(
        &self,
        admission_source: CompactionOperationRevision,
        admission_successor: CompactionOperationRevision,
        source: CompactionOperationRevision,
        successor: CompactionOperationRevision,
        successor_turn_state: TurnStateRevision,
        receipt: Option<&crate::CompactionSettlementReceiptRecord>,
    ) -> bool {
        if source.checked_next().ok() != Some(successor) {
            return false;
        }
        let Some(proof) = self.stopping_source_proof(
            admission_source,
            admission_successor,
            source,
            Some(successor),
        ) else {
            return false;
        };
        let terminal_sequence = proof.source_frontier.checked_add(1);
        if self.terminal.is_none_or(|terminal| {
            Some(terminal.sequence().get()) != terminal_sequence
                || terminal.turn_state_revision() != successor_turn_state
        }) {
            return false;
        }
        let expected =
            canonical_revision(u64::from(self.request.is_some()), proof.current_frontier, 1);
        self.post_stop_descendant_is_exact(expected, receipt, true)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn stop_abandonment_successor_is_exact(
        &self,
        admission_source: CompactionOperationRevision,
        admission_successor: CompactionOperationRevision,
        source: CompactionOperationRevision,
        successor: CompactionOperationRevision,
        receipt: &crate::CompactionSettlementReceiptRecord,
    ) -> bool {
        if source.checked_next().ok() != Some(successor)
            || self.revision != successor
            || self.terminal.is_some()
        {
            return false;
        }
        let Some(proof) =
            self.stopping_source_proof(admission_source, admission_successor, source, None)
        else {
            return false;
        };
        proof.source_frontier == proof.current_frontier
            && Some(successor.get())
                == canonical_revision(u64::from(self.request.is_some()), proof.current_frontier, 2)
            && receipt.source_operation_revision() == source
            && receipt.successor_operation_revision() == successor
            && self.consumed_receipt_is_exact(receipt)
    }

    fn post_stop_descendant_is_exact(
        &self,
        expected_live_revision: Option<u64>,
        receipt: Option<&crate::CompactionSettlementReceiptRecord>,
        terminal_required: bool,
    ) -> bool {
        let Some(expected_live_revision) = expected_live_revision else {
            return false;
        };
        match &self.state {
            CompactionOperationState::DispatchClaimed | CompactionOperationState::Live => {
                !terminal_required
                    && self.terminal.is_none()
                    && self.revision.get() == expected_live_revision
                    && receipt.is_none()
            }
            CompactionOperationState::Finalizing => {
                self.terminal.is_some()
                    && self.revision.get() == expected_live_revision
                    && receipt.is_none()
            }
            CompactionOperationState::Consumed(_) => {
                let Some(expected_consumed_revision) = expected_live_revision.checked_add(1) else {
                    return false;
                };
                self.revision.get() == expected_consumed_revision
                    && receipt.is_some_and(|receipt| self.consumed_receipt_is_exact(receipt))
            }
            CompactionOperationState::Admitted | CompactionOperationState::Stopping(_) => false,
        }
    }

    fn stopping_source_proof(
        &self,
        admission_source: CompactionOperationRevision,
        admission_successor: CompactionOperationRevision,
        stopping_source: CompactionOperationRevision,
        late_request_after: Option<CompactionOperationRevision>,
    ) -> Option<StoppingProof> {
        if admission_source.checked_next().ok() != Some(admission_successor)
            || admission_successor > stopping_source
        {
            return None;
        }
        let (Some(claim), Some(frontier), Some(cas_turn)) = (
            self.dispatch_claim,
            self.provider_frontier,
            self.cas_turn.as_ref(),
        ) else {
            return None;
        };
        if claim.source_revision() != CompactionOperationRevision::FIRST
            || self.latest_provider_sequence() != Some(frontier)
        {
            return None;
        }

        let request_before_admission = match self.request {
            None => false,
            Some(request)
                if request.revision().get()
                    >= CompactionOperationRevision::FIRST.get().checked_add(2)?
                    && request.revision() <= admission_source =>
            {
                true
            }
            Some(request)
                if late_request_after.is_some_and(|successor| request.revision() > successor) =>
            {
                false
            }
            Some(_) => return None,
        };
        let pre_handoff_fixed = CompactionOperationRevision::FIRST
            .get()
            .checked_add(1)?
            .checked_add(u64::from(request_before_admission))?;
        let admission_frontier = admission_source.get().checked_sub(pre_handoff_fixed)?;
        let stopping_fixed = pre_handoff_fixed.checked_add(1)?;
        let source_frontier = stopping_source.get().checked_sub(stopping_fixed)?;
        if admission_frontier > source_frontier
            || source_frontier > frontier.get()
            || cas_turn.sequence().get() > admission_frontier
        {
            return None;
        }
        Some(StoppingProof {
            source_frontier,
            current_frontier: frontier.get(),
        })
    }

    fn latest_provider_sequence(&self) -> Option<CompactionProviderSequence> {
        [
            self.status.map(|value| value.sequence()),
            self.cas_turn.as_ref().map(|value| value.sequence()),
            self.marker.as_ref().map(|value| value.sequence()),
            self.terminal.map(|value| value.sequence()),
        ]
        .into_iter()
        .flatten()
        .max()
    }
}

fn canonical_revision(
    request_count: u64,
    provider_count: u64,
    fixed_after_claim: u64,
) -> Option<u64> {
    CompactionOperationRevision::FIRST
        .get()
        .checked_add(1)
        .and_then(|revision| revision.checked_add(request_count))
        .and_then(|revision| revision.checked_add(provider_count))
        .and_then(|revision| revision.checked_add(fixed_after_claim))
}
