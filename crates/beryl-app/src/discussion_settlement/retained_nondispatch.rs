use super::access::Access;
use super::prepare::Selection;
use super::*;

impl DiscussionSettlementOperations {
    pub fn settle_retained_nondispatch_candidate(
        &self,
        candidate: &HomeCandidateRecoveryAccess<'_>,
        state: &BerylState,
        syndic: &SyndicStorage,
        cancellation: CommandCancellation,
    ) -> Result<u64, HandoffCandidateConvergenceError> {
        let mut settled = 0_u64;
        while let Some(proof) = self.next_nondispatch() {
            if cancellation.is_cancelled() {
                return Err(HandoffCandidateConvergenceError::Cancelled);
            }
            if proof.home_id != candidate.home_id() || proof.generation == candidate.generation() {
                return Err(DiscussionSettlementError::ForeignHome.into());
            }
            let mut last = proof
                .last
                .lock()
                .map_err(|_| DiscussionSettlementError::CustodyUnavailable)?;
            if proof.owner.upgrade().is_some() {
                return Err(DiscussionSettlementError::DuplicateIdentity.into());
            }
            if let Some(attempt) = last.upgrade() {
                let audit = DiscussionSettlementAudit(attempt);
                match audit.reconcile_candidate(candidate, syndic, state)? {
                    DiscussionSettlementAuditOutcome::Settled(
                        DiscussionSettlementResult::ParentRetryable { .. },
                    ) => {
                        settled = settled
                            .checked_add(1)
                            .ok_or(HandoffCandidateConvergenceError::CounterExhausted)?;
                        continue;
                    }
                    DiscussionSettlementAuditOutcome::NotCommitted => {}
                    DiscussionSettlementAuditOutcome::Pending => {
                        return Err(DiscussionSettlementError::DuplicateIdentity.into());
                    }
                    _ => return Err(DiscussionSettlementError::IdentityMismatch.into()),
                }
                drop(audit);
                if last.upgrade().is_some() {
                    return Err(DiscussionSettlementError::DuplicateIdentity.into());
                }
            }
            let (command, audit) = super::prepare::prepare(
                Access::Candidate(candidate),
                state,
                syndic,
                proof.job_id,
                Selection::ParentNondispatch(proof.evidence.clone(), Some(proof.job_revision)),
                cancellation.clone(),
                Arc::clone(&proof.flight),
            )?
            .ok_or(DiscussionSettlementError::IdentityMismatch)?;
            *last = Arc::downgrade(&audit.0);
            let prepared = PreparedDiscussionSettlement {
                command: Some(command),
                execution: Execution::Candidate(candidate),
                permit: None,
                audit,
            };
            drop(last);
            match prepared.execute() {
                DiscussionSettlementOutcome::Committed {
                    later_failure: None,
                    local_finalization: None,
                    ..
                } => {
                    settled = settled
                        .checked_add(1)
                        .ok_or(HandoffCandidateConvergenceError::CounterExhausted)?;
                }
                outcome => {
                    return Err(HandoffCandidateConvergenceError::Outcome {
                        job_id: proof.job_id,
                        outcome: Box::new(outcome),
                    });
                }
            }
        }
        Ok(settled)
    }
}
