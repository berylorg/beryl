use super::*;

pub(in crate::durable_job::mutation) fn parent_accepted_job(
    mut job: BranchHandoffJobRecord,
    cas: ParentCasIdentity,
) -> Result<BranchHandoffJobRecord, DurableJobMutationError> {
    if job.lifecycle() != BranchHandoffJobLifecycle::StartingParent {
        return Err(invalid_transition("starting_parent", job.lifecycle()));
    }
    let BranchHandoffJobState::StartingParent { parent } = job.state else {
        return Err(invariant_transition());
    };
    job.state = BranchHandoffJobState::ParentActive { parent, cas };
    advance(&mut job)?;
    Ok(job)
}

pub(in crate::durable_job::mutation) fn retryable_failed_job(
    mut job: BranchHandoffJobRecord,
    evidence: HandoffFailureEvidence,
) -> Result<BranchHandoffJobRecord, DurableJobMutationError> {
    if !matches!(
        job.lifecycle(),
        BranchHandoffJobLifecycle::WaitingResolvingTurn
            | BranchHandoffJobLifecycle::WaitingParent
            | BranchHandoffJobLifecycle::StartingParent
            | BranchHandoffJobLifecycle::ParentActive
    ) {
        return Err(invalid_transition(
            "a non-failed live checkpoint",
            job.lifecycle(),
        ));
    }
    validate_failure_checkpoint(&job, &evidence, BranchHandoffJobLifecycle::RetryableFailed)?;
    let resume = job.state.checkpoint().ok_or_else(invariant_transition)?;
    job.state = BranchHandoffJobState::RetryableFailed { resume, evidence };
    advance(&mut job)?;
    Ok(job)
}

pub(in crate::durable_job::mutation) fn succeeded_job(
    mut job: BranchHandoffJobRecord,
) -> Result<BranchHandoffJobRecord, DurableJobMutationError> {
    if job.lifecycle() != BranchHandoffJobLifecycle::ParentActive {
        return Err(invalid_transition("parent_active", job.lifecycle()));
    }
    let BranchHandoffJobState::ParentActive { parent, cas } = job.state else {
        return Err(invariant_transition());
    };
    job.state = BranchHandoffJobState::Succeeded { parent, cas };
    advance(&mut job)?;
    Ok(job)
}

pub(in crate::durable_job::mutation) fn start_parent_job(
    mut job: BranchHandoffJobRecord,
    parent: ParentHandoffIdentity,
) -> Result<BranchHandoffJobRecord, DurableJobMutationError> {
    if job.lifecycle() != BranchHandoffJobLifecycle::WaitingParent {
        return Err(invalid_transition("waiting parent", job.lifecycle()));
    }
    job.state = BranchHandoffJobState::StartingParent { parent };
    advance(&mut job)?;
    Ok(job)
}

pub(in crate::durable_job::mutation) fn complete_resolving_job(
    mut job: BranchHandoffJobRecord,
) -> Result<BranchHandoffJobRecord, DurableJobMutationError> {
    if job.lifecycle() != BranchHandoffJobLifecycle::WaitingResolvingTurn {
        return Err(invalid_transition(
            "waiting resolving turn",
            job.lifecycle(),
        ));
    }
    job.state = BranchHandoffJobState::WaitingParent;
    advance(&mut job)?;
    Ok(job)
}

pub(in crate::durable_job::mutation) fn terminal_failed_job(
    mut job: BranchHandoffJobRecord,
    evidence: HandoffFailureEvidence,
) -> Result<BranchHandoffJobRecord, DurableJobMutationError> {
    if !job.lifecycle().is_live() {
        return Err(invalid_transition("a live job", job.lifecycle()));
    }
    validate_failure_checkpoint(&job, &evidence, BranchHandoffJobLifecycle::TerminalFailed)?;
    let stopped_at = job.state.checkpoint().ok_or_else(invariant_transition)?;
    job.state = BranchHandoffJobState::TerminalFailed {
        stopped_at,
        evidence,
    };
    advance(&mut job)?;
    Ok(job)
}
