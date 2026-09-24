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
        self.prepare_with_parent_input(job_id, None, cancellation)
    }

    pub fn prepare_parent_input(
        &self,
        job_id: JobId,
        request: DiscussionParentInputRequest,
        cancellation: CommandCancellation,
    ) -> Result<Option<PreparedDiscussionSettlement<'static>>, DiscussionSettlementError> {
        self.prepare_with_parent_input(job_id, Some(request), cancellation)
    }

    fn prepare_with_parent_input(
        &self,
        job_id: JobId,
        parent_input: Option<DiscussionParentInputRequest>,
        cancellation: CommandCancellation,
    ) -> Result<Option<PreparedDiscussionSettlement<'static>>, DiscussionSettlementError> {
        let permit = self.operations.permit();
        let flight = permit.commit(|| self.operations.acquire(job_id))??;
        let Some((command, audit)) = prepare(
            Access::Ordinary(&self.store),
            &self.state,
            &self.syndic,
            job_id,
            parent_input,
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
            None,
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
    parent_input: Option<DiscussionParentInputRequest>,
    cancellation: CommandCancellation,
    flight: Flight,
) -> Result<Option<(HomeCommand, DiscussionSettlementAudit)>, DiscussionSettlementError> {
    if cancellation.is_cancelled() {
        return Err(DiscussionSettlementError::Cancelled);
    }
    let before = access.revision()?;
    let job = access.job(state, job_id)?;
    if job.job_id() != job_id
        || (parent_input.is_some() && job.lifecycle() != BranchHandoffJobLifecycle::WaitingParent)
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
    let (transition, syndic_intent, result) = match parent {
        DiscussionParentEligibility::Proven(parent)
            if parent.disposition() == DiscussionParentDisposition::Archived =>
        {
            let release = parent.into_archived_release()?;
            let intent = release.intent().clone();
            command.add(release.contribution())?;
            let evidence = HandoffFailureEvidence::new(HandoffFailureKind::ParentArchived, None)
                .expect("empty failure detail is bounded");
            (
                HandoffJobTransition::ParentArchived(evidence),
                Some(SyndicSettlementIntent::Gate(intent)),
                DiscussionSettlementResult::ParentArchived,
            )
        }
        DiscussionParentEligibility::Proven(parent) if parent_input.is_some() => {
            let Access::Ordinary(store) = access else {
                return Err(DiscussionSettlementError::IdentityMismatch);
            };
            let request = parent_input.expect("matched parent input request");
            let prepared = syndic.prepare_generated_discussion_input(
                store,
                parent,
                syndic_storage::GeneratedDiscussionInput {
                    parent_turn_id: request.turn_id,
                    canonical_item_id: request.item_id,
                    resolution: job.resolution().as_str().to_owned(),
                    admitted_at: request.admitted_at,
                },
            )?;
            let intent = prepared.intent();
            let identity = ParentHandoffIdentity::new(intent.input().id(), request.turn_id);
            command.add(prepared.into_contribution())?;
            (
                HandoffJobTransition::StartParent(identity),
                Some(SyndicSettlementIntent::Input(intent)),
                DiscussionSettlementResult::StartingParent(identity),
            )
        }
        _ => {
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
                        Some(SyndicSettlementIntent::Gate(intent)),
                        DiscussionSettlementResult::ChildInputPending,
                    )
                }
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
            syndic: syndic_intent,
            result,
            disposition: Mutex::new(Disposition::Prepared),
            _flight: flight,
        })),
    )))
}
