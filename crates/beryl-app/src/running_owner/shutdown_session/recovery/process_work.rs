use super::*;
use beryl_home_store::CommandCancellation;
use settlement::{CandidateSettlement, CandidateSettlementError};

impl RunningProcessOwner {
    pub(crate) fn settle_interrupted_exit_process_work(
        owner: &Rc<RefCell<Self>>,
        request: &impl RecoveryIdentity,
        cancellation: CommandCancellation,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, &mut App) + 'static,
    ) -> Result<(), String> {
        Self::settle_interrupted_exit_process_work_with(
            owner,
            request,
            cancellation,
            app,
            completed,
            || {},
        )
    }

    fn settle_interrupted_exit_process_work_with(
        owner: &Rc<RefCell<Self>>,
        request: &impl RecoveryIdentity,
        cancellation: CommandCancellation,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, &mut App) + 'static,
        before_settle: impl FnOnce() + Send + 'static,
    ) -> Result<(), String> {
        let (session_slot, settlement_slot, original, candidate, services) = {
            let mut owner = owner.borrow_mut();
            owner.interrupted_exit_graph_retirement_result(request)?;
            let recovery = owner.interrupted_exit.as_ref().unwrap();
            if recovery.resident.is_some()
                || recovery.session.borrow().is_none()
                || recovery
                    .pending_resident_frame
                    .as_ref()
                    .is_some_and(|wake| wake.strong_count() != 0)
                || owner.process.services.is_none()
            {
                return Err("Interrupted Exit recovery custody is unavailable".into());
            }
            let session_slot = recovery.session.clone();
            let settlement_slot = recovery.settlement.clone();
            let mut settlement = settlement_slot.borrow_mut();
            if !matches!(
                settlement.as_ref(),
                Some(CandidateSettlement::Returned { result: Ok(()), .. })
            ) {
                return Err("Interrupted Exit candidate has no successful settlement".into());
            }
            let Some(CandidateSettlement::Returned { candidate, .. }) =
                settlement.replace(CandidateSettlement::Pending)
            else {
                unreachable!()
            };
            drop(settlement);
            let original = session_slot.borrow_mut().take().unwrap();
            (
                session_slot,
                settlement_slot,
                original,
                candidate,
                owner.process.services.take().unwrap(),
            )
        };
        Self::run_interrupted_exit_candidate_pass(
            owner,
            session_slot,
            settlement_slot,
            original,
            candidate,
            Some(services),
            app,
            completed,
            move |_, candidate, services| {
                before_settle();
                let state = beryl_state::BerylState::reacquire_candidate(&candidate.candidate)
                    .map_err(|error| CandidateSettlementError::Candidate(error.to_string()))?;
                let syndic =
                    syndic_storage::SyndicStorage::reacquire_candidate(&candidate.candidate)
                        .map_err(|error| CandidateSettlementError::Candidate(error.to_string()))?;
                let access = candidate
                    .candidate
                    .recovery_access()
                    .map_err(|error| CandidateSettlementError::Candidate(error.to_string()))?;
                services
                    .unwrap()
                    .settle_retired_process_work(&access, &state, &syndic, &cancellation)
                    .map_err(CandidateSettlementError::ProcessWork)
            },
        );
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn test_settle_interrupted_exit_process_work(
        owner: &Rc<RefCell<Self>>,
        request: &impl RecoveryIdentity,
        cancellation: CommandCancellation,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, &mut App) + 'static,
        before_settle: impl FnOnce() + Send + 'static,
    ) -> Result<(), String> {
        Self::settle_interrupted_exit_process_work_with(
            owner,
            request,
            cancellation,
            app,
            completed,
            before_settle,
        )
    }

    #[cfg(test)]
    pub(crate) fn test_interrupted_exit_process_work_cancelled(&self) -> bool {
        matches!(
            self.interrupted_exit
                .as_ref()
                .unwrap()
                .settlement
                .borrow()
                .as_ref(),
            Some(CandidateSettlement::Returned {
                result: Err(CandidateSettlementError::ProcessWork(
                    crate::app_services::RetiredProcessWorkError::Cancelled
                )),
                ..
            })
        )
    }
}
