use beryl_home_store::{DomainMutation, DomainReader, MutationBuilder};
use beryl_model::{JobId, JobRevision};

use super::super::{
    BranchHandoffJobLifecycle, BranchHandoffJobState, DurableJobDomain, DurableJobMutationError,
    HandoffFailureEvidence, ParentCasIdentity, ParentHandoffIdentity,
};
use super::{advance, ensure_revision, put_live_transition, put_terminal_transition, required_job};

/// Advance an admitted job after the resolving child turn is durably complete.
pub struct CompleteResolvingTurn {
    job_id: JobId,
    expected_job_revision: JobRevision,
}

impl CompleteResolvingTurn {
    #[must_use]
    pub const fn new(job_id: JobId, expected_job_revision: JobRevision) -> Self {
        Self {
            job_id,
            expected_job_revision,
        }
    }
}

/// Atomically correlate the exact parent input and turn admitted by Syndic.
pub struct StartParentHandoff {
    job_id: JobId,
    expected_job_revision: JobRevision,
    parent: ParentHandoffIdentity,
}

impl StartParentHandoff {
    #[must_use]
    pub const fn new(
        job_id: JobId,
        expected_job_revision: JobRevision,
        parent: ParentHandoffIdentity,
    ) -> Self {
        Self {
            job_id,
            expected_job_revision,
            parent,
        }
    }
}

/// Record exact CAS acceptance of the already admitted parent turn.
pub struct RecordParentCasAcceptance {
    job_id: JobId,
    expected_job_revision: JobRevision,
    cas: ParentCasIdentity,
}

impl RecordParentCasAcceptance {
    #[must_use]
    pub const fn new(
        job_id: JobId,
        expected_job_revision: JobRevision,
        cas: ParentCasIdentity,
    ) -> Self {
        Self {
            job_id,
            expected_job_revision,
            cas,
        }
    }
}

/// Suspend one exact live checkpoint with bounded retryable evidence.
pub struct RecordRetryableHandoffFailure {
    job_id: JobId,
    expected_job_revision: JobRevision,
    evidence: HandoffFailureEvidence,
}

impl RecordRetryableHandoffFailure {
    #[must_use]
    pub const fn new(
        job_id: JobId,
        expected_job_revision: JobRevision,
        evidence: HandoffFailureEvidence,
    ) -> Self {
        Self {
            job_id,
            expected_job_revision,
            evidence,
        }
    }
}

/// Resume the checkpoint retained by this same immutable retryable job.
pub struct RetryBranchHandoff {
    job_id: JobId,
    expected_job_revision: JobRevision,
}

impl RetryBranchHandoff {
    #[must_use]
    pub const fn new(job_id: JobId, expected_job_revision: JobRevision) -> Self {
        Self {
            job_id,
            expected_job_revision,
        }
    }
}

/// Permanently fail one live job while retaining its exact last checkpoint.
pub struct RecordTerminalHandoffFailure {
    job_id: JobId,
    expected_job_revision: JobRevision,
    evidence: HandoffFailureEvidence,
}

impl RecordTerminalHandoffFailure {
    #[must_use]
    pub const fn new(
        job_id: JobId,
        expected_job_revision: JobRevision,
        evidence: HandoffFailureEvidence,
    ) -> Self {
        Self {
            job_id,
            expected_job_revision,
            evidence,
        }
    }
}

/// Mark an exact active parent CAS turn successful for atomic archive composition.
pub struct SucceedBranchHandoff {
    job_id: JobId,
    expected_job_revision: JobRevision,
}

impl SucceedBranchHandoff {
    #[must_use]
    pub const fn new(job_id: JobId, expected_job_revision: JobRevision) -> Self {
        Self {
            job_id,
            expected_job_revision,
        }
    }
}

impl DomainMutation<DurableJobDomain> for CompleteResolvingTurn {
    type Error = DurableJobMutationError;

    fn validate(&self, reader: &DomainReader<'_, DurableJobDomain>) -> Result<(), Self::Error> {
        validate_lifecycle(
            reader,
            self.job_id,
            self.expected_job_revision,
            BranchHandoffJobLifecycle::WaitingResolvingTurn,
        )
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, DurableJobDomain>,
        mutations: &mut MutationBuilder<'_, DurableJobDomain>,
    ) -> Result<(), Self::Error> {
        let mut job = required_job(reader, self.job_id)?;
        job.state = BranchHandoffJobState::WaitingParent;
        advance(&mut job)?;
        put_live_transition(mutations, &job)
    }
}

impl DomainMutation<DurableJobDomain> for StartParentHandoff {
    type Error = DurableJobMutationError;

    fn validate(&self, reader: &DomainReader<'_, DurableJobDomain>) -> Result<(), Self::Error> {
        validate_lifecycle(
            reader,
            self.job_id,
            self.expected_job_revision,
            BranchHandoffJobLifecycle::WaitingParent,
        )
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, DurableJobDomain>,
        mutations: &mut MutationBuilder<'_, DurableJobDomain>,
    ) -> Result<(), Self::Error> {
        let mut job = required_job(reader, self.job_id)?;
        job.state = BranchHandoffJobState::StartingParent {
            parent: self.parent,
        };
        advance(&mut job)?;
        put_live_transition(mutations, &job)
    }
}

impl DomainMutation<DurableJobDomain> for RecordParentCasAcceptance {
    type Error = DurableJobMutationError;

    fn validate(&self, reader: &DomainReader<'_, DurableJobDomain>) -> Result<(), Self::Error> {
        validate_lifecycle(
            reader,
            self.job_id,
            self.expected_job_revision,
            BranchHandoffJobLifecycle::StartingParent,
        )
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, DurableJobDomain>,
        mutations: &mut MutationBuilder<'_, DurableJobDomain>,
    ) -> Result<(), Self::Error> {
        let mut job = required_job(reader, self.job_id)?;
        let parent = match job.state {
            BranchHandoffJobState::StartingParent { parent } => parent,
            _ => return Err(invariant_transition()),
        };
        job.state = BranchHandoffJobState::ParentActive {
            parent,
            cas: self.cas.clone(),
        };
        advance(&mut job)?;
        put_live_transition(mutations, &job)
    }
}

impl DomainMutation<DurableJobDomain> for RecordRetryableHandoffFailure {
    type Error = DurableJobMutationError;

    fn validate(&self, reader: &DomainReader<'_, DurableJobDomain>) -> Result<(), Self::Error> {
        if !self.evidence.kind().is_retryable() {
            return Err(DurableJobMutationError::FailureKindMismatch {
                expected: "retryable",
                actual: self.evidence.kind(),
            });
        }
        let job = validate_current(reader, self.job_id, self.expected_job_revision)?;
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
        validate_failure_checkpoint(&job, &self.evidence)?;
        Ok(())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, DurableJobDomain>,
        mutations: &mut MutationBuilder<'_, DurableJobDomain>,
    ) -> Result<(), Self::Error> {
        let mut job = required_job(reader, self.job_id)?;
        let resume = job.state.checkpoint().ok_or_else(invariant_transition)?;
        job.state = BranchHandoffJobState::RetryableFailed {
            resume,
            evidence: self.evidence.clone(),
        };
        advance(&mut job)?;
        put_live_transition(mutations, &job)
    }
}

impl DomainMutation<DurableJobDomain> for RetryBranchHandoff {
    type Error = DurableJobMutationError;

    fn validate(&self, reader: &DomainReader<'_, DurableJobDomain>) -> Result<(), Self::Error> {
        validate_lifecycle(
            reader,
            self.job_id,
            self.expected_job_revision,
            BranchHandoffJobLifecycle::RetryableFailed,
        )
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, DurableJobDomain>,
        mutations: &mut MutationBuilder<'_, DurableJobDomain>,
    ) -> Result<(), Self::Error> {
        let mut job = required_job(reader, self.job_id)?;
        let resume = match job.state {
            BranchHandoffJobState::RetryableFailed { resume, .. } => resume,
            _ => return Err(invariant_transition()),
        };
        job.state = resume.into_state();
        advance(&mut job)?;
        put_live_transition(mutations, &job)
    }
}

impl DomainMutation<DurableJobDomain> for RecordTerminalHandoffFailure {
    type Error = DurableJobMutationError;

    fn validate(&self, reader: &DomainReader<'_, DurableJobDomain>) -> Result<(), Self::Error> {
        if !self.evidence.kind().is_terminal() {
            return Err(DurableJobMutationError::FailureKindMismatch {
                expected: "terminal",
                actual: self.evidence.kind(),
            });
        }
        let job = validate_current(reader, self.job_id, self.expected_job_revision)?;
        if !job.lifecycle().is_live() {
            return Err(invalid_transition("a live job", job.lifecycle()));
        }
        validate_failure_checkpoint(&job, &self.evidence)?;
        Ok(())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, DurableJobDomain>,
        mutations: &mut MutationBuilder<'_, DurableJobDomain>,
    ) -> Result<(), Self::Error> {
        let mut job = required_job(reader, self.job_id)?;
        let stopped_at = job.state.checkpoint().ok_or_else(invariant_transition)?;
        job.state = BranchHandoffJobState::TerminalFailed {
            stopped_at,
            evidence: self.evidence.clone(),
        };
        advance(&mut job)?;
        put_terminal_transition(mutations, &job)
    }
}

impl DomainMutation<DurableJobDomain> for SucceedBranchHandoff {
    type Error = DurableJobMutationError;

    fn validate(&self, reader: &DomainReader<'_, DurableJobDomain>) -> Result<(), Self::Error> {
        validate_lifecycle(
            reader,
            self.job_id,
            self.expected_job_revision,
            BranchHandoffJobLifecycle::ParentActive,
        )
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, DurableJobDomain>,
        mutations: &mut MutationBuilder<'_, DurableJobDomain>,
    ) -> Result<(), Self::Error> {
        let mut job = required_job(reader, self.job_id)?;
        let (parent, cas) = match job.state {
            BranchHandoffJobState::ParentActive { parent, cas } => (parent, cas),
            _ => return Err(invariant_transition()),
        };
        job.state = BranchHandoffJobState::Succeeded { parent, cas };
        advance(&mut job)?;
        put_terminal_transition(mutations, &job)
    }
}

fn validate_lifecycle(
    reader: &DomainReader<'_, DurableJobDomain>,
    job_id: JobId,
    expected_job_revision: JobRevision,
    expected_lifecycle: BranchHandoffJobLifecycle,
) -> Result<(), DurableJobMutationError> {
    let job = validate_current(reader, job_id, expected_job_revision)?;
    if job.lifecycle() != expected_lifecycle {
        return Err(invalid_transition(
            lifecycle_name(expected_lifecycle),
            job.lifecycle(),
        ));
    }
    Ok(())
}

fn validate_current(
    reader: &DomainReader<'_, DurableJobDomain>,
    job_id: JobId,
    expected_job_revision: JobRevision,
) -> Result<super::super::BranchHandoffJobRecord, DurableJobMutationError> {
    let job = required_job(reader, job_id)?;
    ensure_revision(expected_job_revision, job.revision)?;
    Ok(job)
}

fn validate_failure_checkpoint(
    job: &super::super::BranchHandoffJobRecord,
    evidence: &HandoffFailureEvidence,
) -> Result<(), DurableJobMutationError> {
    let checkpoint = job.state.checkpoint().ok_or_else(invariant_transition)?;
    let lifecycle = checkpoint.lifecycle();
    let valid = match evidence.kind() {
        super::super::HandoffFailureKind::CasRejectedBeforeAcceptance => {
            lifecycle == BranchHandoffJobLifecycle::StartingParent
        }
        super::super::HandoffFailureKind::UnrecoverablePostAppend => matches!(
            lifecycle,
            BranchHandoffJobLifecycle::StartingParent | BranchHandoffJobLifecycle::ParentActive
        ),
        super::super::HandoffFailureKind::ParentInterrupted
        | super::super::HandoffFailureKind::ParentIncomplete
        | super::super::HandoffFailureKind::ParentTerminalFailure => {
            lifecycle == BranchHandoffJobLifecycle::ParentActive
        }
        _ => true,
    };
    if valid {
        Ok(())
    } else {
        Err(DurableJobMutationError::FailureKindMismatch {
            expected: "failure kind valid for the retained checkpoint",
            actual: evidence.kind(),
        })
    }
}

fn invalid_transition(
    expected: &'static str,
    current: BranchHandoffJobLifecycle,
) -> DurableJobMutationError {
    DurableJobMutationError::InvalidTransition { expected, current }
}

fn invariant_transition() -> DurableJobMutationError {
    DurableJobMutationError::Invariant(
        "job state changed between transition validation and contribution",
    )
}

const fn lifecycle_name(lifecycle: BranchHandoffJobLifecycle) -> &'static str {
    match lifecycle {
        BranchHandoffJobLifecycle::WaitingResolvingTurn => "waiting_resolving_turn",
        BranchHandoffJobLifecycle::WaitingParent => "waiting_parent",
        BranchHandoffJobLifecycle::StartingParent => "starting_parent",
        BranchHandoffJobLifecycle::ParentActive => "parent_active",
        BranchHandoffJobLifecycle::RetryableFailed => "retryable_failed",
        BranchHandoffJobLifecycle::TerminalFailed => "terminal_failed",
        BranchHandoffJobLifecycle::Succeeded => "succeeded",
    }
}
