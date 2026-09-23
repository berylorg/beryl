use super::access::Access;
use super::*;
use beryl_home_store::ReconciliationResolution;
use beryl_state::ResolvingTransitionStatus;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiscussionSettlementAuditOutcome {
    Pending,
    NotCommitted,
    Settled(DiscussionSettlementResult),
    Collision,
}
impl DiscussionSettlementAudit {
    pub fn reconcile(
        &self,
        store: &HomeStore,
        syndic: &SyndicStorage,
        state: &BerylState,
    ) -> Result<DiscussionSettlementAuditOutcome, DiscussionSettlementError> {
        self.reconcile_access(Access::Ordinary(store), syndic, state)
    }
    pub fn reconcile_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        syndic: &SyndicStorage,
        state: &BerylState,
    ) -> Result<DiscussionSettlementAuditOutcome, DiscussionSettlementError> {
        self.reconcile_access(Access::Candidate(access), syndic, state)
    }
    fn reconcile_access(
        &self,
        access: Access<'_>,
        syndic: &SyndicStorage,
        state: &BerylState,
    ) -> Result<DiscussionSettlementAuditOutcome, DiscussionSettlementError> {
        let mut disposition = self
            .0
            .disposition
            .lock()
            .map_err(|_| DiscussionSettlementError::CustodyUnavailable)?;
        if access.home_id() != self.0.home_id {
            return Err(DiscussionSettlementError::ForeignHome);
        }
        let natural = access.natural(self, syndic, state)?;
        if matches!(*disposition, Disposition::Prepared) {
            return Ok(DiscussionSettlementAuditOutcome::Pending);
        }
        if matches!(*disposition, Disposition::Collision) {
            return Ok(DiscussionSettlementAuditOutcome::Collision);
        }
        if let Disposition::Indeterminate(handle) = &*disposition {
            match (natural, access.reconcile(handle)?) {
                (ResolvingTransitionStatus::ExactOld, ReconciliationResolution::ExactOld) => {
                    *disposition = Disposition::NotCommitted
                }
                (
                    ResolvingTransitionStatus::ExactNew,
                    ReconciliationResolution::ExactNew { .. },
                ) => *disposition = Disposition::Committed,
                _ => *disposition = Disposition::Collision,
            }
        }
        Ok(match (&*disposition, natural) {
            (Disposition::NotCommitted, ResolvingTransitionStatus::ExactOld) => {
                DiscussionSettlementAuditOutcome::NotCommitted
            }
            (Disposition::Committed, ResolvingTransitionStatus::ExactNew) => {
                DiscussionSettlementAuditOutcome::Settled(self.0.result)
            }
            _ => DiscussionSettlementAuditOutcome::Collision,
        })
    }
}
