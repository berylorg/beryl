use super::access::Access;
use super::*;
use beryl_state::{
    BranchHandoffJobLifecycle, HandoffFailureEvidence, HandoffFailureKind, HandoffJobTransition,
};
use syndic_storage::{
    DiscussionChildSettlement, DiscussionChildSettlementDisposition, DiscussionHandoffGateState,
    DiscussionParentDisposition, DiscussionParentEligibility,
};

mod execution;

#[derive(Clone)]
pub(super) enum Selection {
    Retry(beryl_model::JobRevision),
    Child,
    ParentInput(DiscussionParentInputRequest),
    ParentExecution(SyndicTimestamp),
    ParentNondispatch(
        DiscussionParentNondispatch,
        Option<beryl_model::JobRevision>,
    ),
}

type Transition = (
    HandoffJobTransition,
    Option<SyndicSettlementIntent>,
    DiscussionSettlementResult,
);

impl DiscussionSettlementService {
    pub fn prepare_parent_nondispatch(
        &self,
        job_id: JobId,
        evidence: DiscussionParentNondispatch,
        cancellation: CommandCancellation,
    ) -> Result<PreparedDiscussionSettlement<'static>, DiscussionSettlementError> {
        self.prepare_selected(
            job_id,
            Selection::ParentNondispatch(evidence, None),
            cancellation,
        )?
        .ok_or(DiscussionSettlementError::IdentityMismatch)
    }
    pub fn prepare_retry(
        &self,
        job_id: JobId,
        expected_revision: beryl_model::JobRevision,
        cancellation: CommandCancellation,
    ) -> Result<PreparedDiscussionSettlement<'static>, DiscussionSettlementError> {
        self.prepare_selected(job_id, Selection::Retry(expected_revision), cancellation)?
            .ok_or(DiscussionSettlementError::IdentityMismatch)
    }
    pub fn prepare_parent_execution(
        &self,
        job_id: JobId,
        at: SyndicTimestamp,
        cancellation: CommandCancellation,
    ) -> Result<Option<PreparedDiscussionSettlement<'static>>, DiscussionSettlementError> {
        self.prepare_selected(job_id, Selection::ParentExecution(at), cancellation)
    }
    pub fn prepare(
        &self,
        job_id: JobId,
        cancellation: CommandCancellation,
    ) -> Result<Option<PreparedDiscussionSettlement<'static>>, DiscussionSettlementError> {
        self.prepare_selected(job_id, Selection::Child, cancellation)
    }

    pub fn prepare_parent_input(
        &self,
        job_id: JobId,
        request: DiscussionParentInputRequest,
        cancellation: CommandCancellation,
    ) -> Result<Option<PreparedDiscussionSettlement<'static>>, DiscussionSettlementError> {
        self.prepare_selected(job_id, Selection::ParentInput(request), cancellation)
    }

    fn prepare_selected(
        &self,
        job_id: JobId,
        selection: Selection,
        cancellation: CommandCancellation,
    ) -> Result<Option<PreparedDiscussionSettlement<'static>>, DiscussionSettlementError> {
        let permit = self.operations.permit();
        let flight = permit.commit(|| self.operations.acquire(job_id))??;
        let Some((command, audit)) = prepare(
            Access::Ordinary(&self.store),
            &self.state,
            &self.syndic,
            job_id,
            selection,
            cancellation,
            Arc::new(flight),
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
    pub fn prepare_parent_execution_candidate<'a>(
        &self,
        access: &'a HomeCandidateRecoveryAccess<'a>,
        state: &BerylState,
        syndic: &SyndicStorage,
        job_id: JobId,
        at: SyndicTimestamp,
        cancellation: CommandCancellation,
    ) -> Result<Option<PreparedDiscussionSettlement<'a>>, DiscussionSettlementError> {
        self.prepare_selected_candidate(
            access,
            state,
            syndic,
            job_id,
            Selection::ParentExecution(at),
            cancellation,
        )
    }

    pub fn prepare_candidate<'a>(
        &self,
        access: &'a HomeCandidateRecoveryAccess<'a>,
        state: &BerylState,
        syndic: &SyndicStorage,
        job_id: JobId,
        cancellation: CommandCancellation,
    ) -> Result<Option<PreparedDiscussionSettlement<'a>>, DiscussionSettlementError> {
        self.prepare_selected_candidate(
            access,
            state,
            syndic,
            job_id,
            Selection::Child,
            cancellation,
        )
    }

    fn prepare_selected_candidate<'a>(
        &self,
        access: &'a HomeCandidateRecoveryAccess<'a>,
        state: &BerylState,
        syndic: &SyndicStorage,
        job_id: JobId,
        selection: Selection,
        cancellation: CommandCancellation,
    ) -> Result<Option<PreparedDiscussionSettlement<'a>>, DiscussionSettlementError> {
        let flight = self.acquire(job_id)?;
        let Some((command, audit)) = prepare(
            Access::Candidate(access),
            state,
            syndic,
            job_id,
            selection,
            cancellation,
            Arc::new(flight),
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

pub(super) fn prepare(
    access: Access<'_>,
    state: &BerylState,
    syndic: &SyndicStorage,
    job_id: JobId,
    selection: Selection,
    cancellation: CommandCancellation,
    flight: Arc<Flight>,
) -> Result<Option<(HomeCommand, DiscussionSettlementAudit)>, DiscussionSettlementError> {
    if cancellation.is_cancelled() {
        return Err(DiscussionSettlementError::Cancelled);
    }
    let before = access.revision()?;
    let job = access.job(state, job_id)?;
    let parent_input = match &selection {
        Selection::ParentInput(request) => Some(*request),
        _ => None,
    };
    let allowed = match &selection {
        Selection::ParentNondispatch(_, expected) => {
            job.lifecycle() == BranchHandoffJobLifecycle::StartingParent
                && expected.is_none_or(|revision| job.revision() == revision)
        }
        Selection::Retry(expected) => {
            job.lifecycle() == BranchHandoffJobLifecycle::RetryableFailed
                && job.revision() == *expected
        }
        Selection::ParentExecution(_) => matches!(
            job.lifecycle(),
            BranchHandoffJobLifecycle::StartingParent | BranchHandoffJobLifecycle::ParentActive
        ),
        _ => matches!(
            job.lifecycle(),
            BranchHandoffJobLifecycle::WaitingResolvingTurn
                | BranchHandoffJobLifecycle::WaitingParent
        ),
    };
    if job.job_id() != job_id
        || (parent_input.is_some() && job.lifecycle() != BranchHandoffJobLifecycle::WaitingParent)
        || !allowed
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
    let mut command = HomeCommand::new(before).with_cancellation(cancellation);
    let (transition, syndic_intent, result) = if let Selection::Retry(_) = selection {
        super::candidate::validate_job_sources(access, syndic, &job)?;
        (
            HandoffJobTransition::Retry,
            None,
            DiscussionSettlementResult::RetryResumed,
        )
    } else if let Selection::ParentNondispatch(evidence, _) = selection {
        super::nondispatch::prepare(access, syndic, &job, evidence, &mut command)?
    } else if let Selection::ParentExecution(at) = selection {
        let prepared = execution::prepare(access, syndic, &job, gate, at, &mut command)?;
        if access.revision()? != before {
            return Err(DiscussionSettlementError::ConcurrentChange);
        }
        let Some(prepared) = prepared else {
            return Ok(None);
        };
        prepared
    } else {
        let parent = access.parent(syndic, &job, gate)?;
        match parent {
            DiscussionParentEligibility::Proven(parent)
                if parent.disposition() == DiscussionParentDisposition::Archived =>
            {
                let release = parent.into_archived_release()?;
                let intent = release.intent().clone();
                command.add(release.contribution())?;
                let evidence =
                    HandoffFailureEvidence::new(HandoffFailureKind::ParentArchived, None)
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
                        let evidence = HandoffFailureEvidence::new(
                            HandoffFailureKind::ChildInputPending,
                            None,
                        )
                        .expect("empty failure detail is bounded");
                        (
                            HandoffJobTransition::ChildInputPending(evidence),
                            Some(SyndicSettlementIntent::Gate(intent)),
                            DiscussionSettlementResult::ChildInputPending,
                        )
                    }
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
            job: JobWitness::Transition(witness),
            syndic: syndic_intent,
            result,
            disposition: Mutex::new(Disposition::Prepared),
            _flight: flight,
        })),
    )))
}

pub(super) fn prepare_reserved<'a>(
    service: &DiscussionSettlementService,
    job_id: JobId,
    expected: beryl_model::JobRevision,
    evidence: DiscussionParentNondispatch,
    cancellation: CommandCancellation,
    flight: Arc<Flight>,
    permit: ProcessExecutionPermit,
) -> Result<PreparedDiscussionSettlement<'a>, DiscussionSettlementError> {
    permit.commit(|| ())?;
    let (command, audit) = prepare(
        Access::Ordinary(&service.store),
        &service.state,
        &service.syndic,
        job_id,
        Selection::ParentNondispatch(evidence, Some(expected)),
        cancellation,
        flight,
    )?
    .ok_or(DiscussionSettlementError::IdentityMismatch)?;
    Ok(PreparedDiscussionSettlement {
        command: Some(command),
        execution: Execution::Ordinary(service.store.clone()),
        permit: Some(permit),
        audit,
    })
}
