use super::access::Access;
use super::*;
use beryl_state::{
    BranchHandoffJobLifecycle, HandoffFailureEvidence, HandoffFailureKind, HandoffJobTransition,
};
use syndic_storage::{
    DiscussionChildSettlement, DiscussionChildSettlementDisposition, DiscussionHandoffGateState,
    DiscussionParentDisposition, DiscussionParentEligibility,
};

impl DiscussionSettlementService {
    pub fn prepare(
        &self,
        job_id: JobId,
        cancellation: CommandCancellation,
    ) -> Result<Option<PreparedDiscussionSettlement<'static>>, DiscussionSettlementError> {
        let permit = self.operations.permit();
        let flight = permit.commit(|| self.operations.acquire(job_id))??;
        let Some((command, audit)) = prepare(
            Access::Ordinary(&self.store),
            &self.state,
            &self.syndic,
            job_id,
            cancellation,
            flight,
        )?
        else {
            return Ok(None);
        };
        Ok(Some(PreparedDiscussionSettlement {
            command: Some(command),
            execution: Execution::Ordinary(self.store.clone()),
            permit: Some(permit),
            audit,
        }))
    }
}

impl DiscussionSettlementOperations {
    pub fn prepare_candidate<'a>(
        &self,
        access: &'a HomeCandidateRecoveryAccess<'a>,
        state: &BerylState,
        syndic: &SyndicStorage,
        job_id: JobId,
        cancellation: CommandCancellation,
    ) -> Result<Option<PreparedDiscussionSettlement<'a>>, DiscussionSettlementError> {
        let flight = self.acquire(job_id)?;
        let Some((command, audit)) = prepare(
            Access::Candidate(access),
            state,
            syndic,
            job_id,
            cancellation,
            flight,
        )?
        else {
            return Ok(None);
        };
        Ok(Some(PreparedDiscussionSettlement {
            command: Some(command),
            execution: Execution::Candidate(access),
            permit: None,
            audit,
        }))
    }
}

fn prepare(
    access: Access<'_>,
    state: &BerylState,
    syndic: &SyndicStorage,
    job_id: JobId,
    cancellation: CommandCancellation,
    flight: Flight,
) -> Result<Option<(HomeCommand, DiscussionSettlementAudit)>, DiscussionSettlementError> {
    if cancellation.is_cancelled() {
        return Err(DiscussionSettlementError::Cancelled);
    }
    let before = access.revision()?;
    let job = access.job(state, job_id)?;
    if job.job_id() != job_id
        || !matches!(
            job.lifecycle(),
            BranchHandoffJobLifecycle::WaitingResolvingTurn
                | BranchHandoffJobLifecycle::WaitingParent
        )
    {
        return Err(DiscussionSettlementError::IdentityMismatch);
    }
    let gate = access.gate(syndic, job.discussion_thread_id())?;
    if gate.thread_id() != job.discussion_thread_id()
        || gate.state()
            != (DiscussionHandoffGateState::Pending {
                intent_id: job.intent_id(),
                job_id,
                resolving_turn_id: job.resolving_turn_id(),
            })
    {
        return Err(DiscussionSettlementError::IdentityMismatch);
    }
    let parent = access.parent(syndic, &job, gate)?;
    let mut command = HomeCommand::new(before).with_cancellation(cancellation);
    let (transition, gate, result) = if let DiscussionParentEligibility::Proven(parent) = parent
        && parent.disposition() == DiscussionParentDisposition::Archived
    {
        let release = parent.into_archived_release()?;
        let intent = release.intent().clone();
        command.add(release.contribution())?;
        let evidence = HandoffFailureEvidence::new(HandoffFailureKind::ParentArchived, None)
            .expect("empty failure detail is bounded");
        (
            HandoffJobTransition::ParentArchived(evidence),
            Some(intent),
            DiscussionSettlementResult::ParentArchived,
        )
    } else {
        if job.lifecycle() == BranchHandoffJobLifecycle::WaitingParent {
            if access.revision()? != before {
                return Err(DiscussionSettlementError::ConcurrentChange);
            }
            return Ok(None);
        }
        let child = access.child(syndic, gate)?;
        if access.revision()? != before {
            return Err(DiscussionSettlementError::ConcurrentChange);
        }
        let DiscussionChildSettlement::Settled(child) = child else {
            return Ok(None);
        };
        match child.disposition() {
            DiscussionChildSettlementDisposition::Ready => {
                command.add_validation(child.into_ready_validation()?)?;
                (
                    HandoffJobTransition::CompleteResolving,
                    None,
                    DiscussionSettlementResult::ReadyForParent,
                )
            }
            DiscussionChildSettlementDisposition::QueuedInput => {
                let release = child.into_queued_release()?;
                let intent = release.intent().clone();
                command.add(release.contribution())?;
                let evidence =
                    HandoffFailureEvidence::new(HandoffFailureKind::ChildInputPending, None)
                        .expect("empty failure detail is bounded");
                (
                    HandoffJobTransition::ChildInputPending(evidence),
                    Some(intent),
                    DiscussionSettlementResult::ChildInputPending,
                )
            }
        }
    };
    let prepared = access.transition(state, &job, transition)?;
    let witness = prepared.witness().clone();
    command.add(prepared.contribution())?;
    if access.revision()? != before {
        return Err(DiscussionSettlementError::ConcurrentChange);
    }
    Ok(Some((
        command,
        DiscussionSettlementAudit(Arc::new(Attempt {
            home_id: access.home_id(),
            job: witness,
            gate,
            result,
            disposition: Mutex::new(Disposition::Prepared),
            _flight: flight,
        })),
    )))
}
