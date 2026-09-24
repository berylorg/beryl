use super::*;
use beryl_state::{BranchHandoffJobRecord, ParentCasIdentity};
use syndic_storage::{
    DiscussionHandoffGateRecord, DiscussionParentExecution, DiscussionParentExecutionDisposition,
    DiscussionParentExecutionRequest, TurnTerminalOutcome,
};

pub(super) fn prepare(
    access: Access<'_>,
    syndic: &SyndicStorage,
    job: &BranchHandoffJobRecord,
    gate: DiscussionHandoffGateRecord,
    at: SyndicTimestamp,
    command: &mut HomeCommand,
) -> Result<Option<Transition>, DiscussionSettlementError> {
    let parent = job
        .state()
        .parent()
        .ok_or(DiscussionSettlementError::IdentityMismatch)?;
    let request = DiscussionParentExecutionRequest {
        child_gate: gate,
        parent_thread_id: job.parent_thread_id(),
        input_id: parent.accepted_input_id(),
        turn_id: parent.turn_id(),
        context_owner: job.context_owner_id(),
        context_digest: job.context_digest(),
        resolution: job.resolution().as_str().to_owned(),
    };
    let observation = match access {
        Access::Ordinary(store) => syndic.prepare_discussion_parent_execution(store, request)?,
        Access::Candidate(access) => {
            syndic.prepare_discussion_parent_execution_candidate(access, request)?
        }
    };
    let DiscussionParentExecution::Proven(proof) = observation else {
        return Ok(None);
    };
    let cas = match proof.disposition() {
        DiscussionParentExecutionDisposition::Accepted(cas) => Some(cas),
        DiscussionParentExecutionDisposition::Terminal { cas, .. } => cas.as_ref(),
    }
    .map(|cas| ParentCasIdentity::new(cas.thread_id().clone(), cas.turn_id().clone()));
    if job.lifecycle() == BranchHandoffJobLifecycle::StartingParent {
        if let Some(cas) = cas {
            command.add_validation(proof.into_validation())?;
            return Ok(Some((
                HandoffJobTransition::ParentAccepted(cas),
                None,
                DiscussionSettlementResult::ParentActive(parent),
            )));
        }
    } else if job.state().parent_cas() != cas.as_ref() || cas.is_none() {
        return Err(DiscussionSettlementError::IdentityMismatch);
    }
    let DiscussionParentExecutionDisposition::Terminal { status, .. } = proof.disposition() else {
        return Ok(None);
    };
    let failure = if job.lifecycle() == BranchHandoffJobLifecycle::StartingParent {
        if status.outcome() != TurnTerminalOutcome::Incomplete {
            return Err(DiscussionSettlementError::IdentityMismatch);
        }
        Some(HandoffFailureKind::UnrecoverablePostAppend)
    } else {
        match status.outcome() {
            TurnTerminalOutcome::Complete => None,
            TurnTerminalOutcome::Interrupted => Some(HandoffFailureKind::ParentInterrupted),
            TurnTerminalOutcome::Incomplete => Some(HandoffFailureKind::ParentIncomplete),
            TurnTerminalOutcome::Failed => Some(HandoffFailureKind::ParentTerminalFailure),
            _ => return Err(DiscussionSettlementError::IdentityMismatch),
        }
    };
    let release = proof.into_terminal_settlement(at)?;
    let intent = release.intent().clone();
    command.add(release.contribution())?;
    let (transition, result) = match failure {
        None => (
            HandoffJobTransition::Succeed,
            DiscussionSettlementResult::ParentSucceeded(parent),
        ),
        Some(kind) => (
            HandoffJobTransition::TerminalFailure(
                HandoffFailureEvidence::new(kind, None).expect("empty failure detail is bounded"),
            ),
            DiscussionSettlementResult::ParentFailed { parent, kind },
        ),
    };
    Ok(Some((
        transition,
        Some(SyndicSettlementIntent::Gate(intent)),
        result,
    )))
}
