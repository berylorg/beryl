use beryl_home_store::{DomainMutation, DomainReader, MutationBuilder, ReconciliationReservation};
use beryl_model::{JobId, JobRevision};

use super::super::codec::{JobRecordCodec, LiveJobIndexCodec};
use super::super::{
    BranchHandoffJobLifecycle, BranchHandoffJobRecord, BranchHandoffJobState, DurableJobDomain,
    DurableJobMutationError, HandoffFailureEvidence, ParentCasIdentity, ParentHandoffIdentity,
    record::failure_state_is_compatible,
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

/// Mark an exact active parent CAS turn successful for composition with Syndic archive authority.
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
    type Prepared = BranchHandoffJobRecord;

    fn prepare(
        self,
        reader: &DomainReader<'_, DurableJobDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let mut job = validate_lifecycle(
            reader,
            self.job_id,
            self.expected_job_revision,
            BranchHandoffJobLifecycle::WaitingResolvingTurn,
        )?;
        job.state = BranchHandoffJobState::WaitingParent;
        advance(&mut job)?;
        Ok(job)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, DurableJobDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<JobRecordCodec>(1)?;
        reservation.reserve_records::<LiveJobIndexCodec>(1)?;
        Ok(())
    }

    fn contribute(
        job: Self::Prepared,
        mutations: &mut MutationBuilder<'_, DurableJobDomain>,
    ) -> Result<(), Self::Error> {
        put_live_transition(mutations, &job)
    }
}

impl DomainMutation<DurableJobDomain> for StartParentHandoff {
    type Error = DurableJobMutationError;
    type Prepared = BranchHandoffJobRecord;

    fn prepare(
        self,
        reader: &DomainReader<'_, DurableJobDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let mut job = validate_lifecycle(
            reader,
            self.job_id,
            self.expected_job_revision,
            BranchHandoffJobLifecycle::WaitingParent,
        )?;
        job.state = BranchHandoffJobState::StartingParent {
            parent: self.parent,
        };
        advance(&mut job)?;
        Ok(job)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, DurableJobDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<JobRecordCodec>(1)?;
        reservation.reserve_records::<LiveJobIndexCodec>(1)?;
        Ok(())
    }

    fn contribute(
        job: Self::Prepared,
        mutations: &mut MutationBuilder<'_, DurableJobDomain>,
    ) -> Result<(), Self::Error> {
        put_live_transition(mutations, &job)
    }
}

impl DomainMutation<DurableJobDomain> for RecordParentCasAcceptance {
    type Error = DurableJobMutationError;
    type Prepared = BranchHandoffJobRecord;

    fn prepare(
        self,
        reader: &DomainReader<'_, DurableJobDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let mut job = validate_lifecycle(
            reader,
            self.job_id,
            self.expected_job_revision,
            BranchHandoffJobLifecycle::StartingParent,
        )?;
        let parent = match job.state {
            BranchHandoffJobState::StartingParent { parent } => parent,
            _ => return Err(invariant_transition()),
        };
        job.state = BranchHandoffJobState::ParentActive {
            parent,
            cas: self.cas,
        };
        advance(&mut job)?;
        Ok(job)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, DurableJobDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<JobRecordCodec>(1)?;
        reservation.reserve_records::<LiveJobIndexCodec>(1)?;
        Ok(())
    }

    fn contribute(
        job: Self::Prepared,
        mutations: &mut MutationBuilder<'_, DurableJobDomain>,
    ) -> Result<(), Self::Error> {
        put_live_transition(mutations, &job)
    }
}

impl DomainMutation<DurableJobDomain> for RecordRetryableHandoffFailure {
    type Error = DurableJobMutationError;
    type Prepared = BranchHandoffJobRecord;

    fn prepare(
        self,
        reader: &DomainReader<'_, DurableJobDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let mut job = validate_current(reader, self.job_id, self.expected_job_revision)?;
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
        validate_failure_checkpoint(
            &job,
            &self.evidence,
            BranchHandoffJobLifecycle::RetryableFailed,
        )?;
        let resume = job.state.checkpoint().ok_or_else(invariant_transition)?;
        job.state = BranchHandoffJobState::RetryableFailed {
            resume,
            evidence: self.evidence,
        };
        advance(&mut job)?;
        Ok(job)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, DurableJobDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<JobRecordCodec>(1)?;
        reservation.reserve_records::<LiveJobIndexCodec>(1)?;
        Ok(())
    }

    fn contribute(
        job: Self::Prepared,
        mutations: &mut MutationBuilder<'_, DurableJobDomain>,
    ) -> Result<(), Self::Error> {
        put_live_transition(mutations, &job)
    }
}

impl DomainMutation<DurableJobDomain> for RetryBranchHandoff {
    type Error = DurableJobMutationError;
    type Prepared = BranchHandoffJobRecord;

    fn prepare(
        self,
        reader: &DomainReader<'_, DurableJobDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let mut job = validate_lifecycle(
            reader,
            self.job_id,
            self.expected_job_revision,
            BranchHandoffJobLifecycle::RetryableFailed,
        )?;
        let resume = match job.state {
            BranchHandoffJobState::RetryableFailed { resume, .. } => resume,
            _ => return Err(invariant_transition()),
        };
        job.state = resume.into_state();
        advance(&mut job)?;
        Ok(job)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, DurableJobDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<JobRecordCodec>(1)?;
        reservation.reserve_records::<LiveJobIndexCodec>(1)?;
        Ok(())
    }

    fn contribute(
        job: Self::Prepared,
        mutations: &mut MutationBuilder<'_, DurableJobDomain>,
    ) -> Result<(), Self::Error> {
        put_live_transition(mutations, &job)
    }
}

impl DomainMutation<DurableJobDomain> for RecordTerminalHandoffFailure {
    type Error = DurableJobMutationError;
    type Prepared = BranchHandoffJobRecord;

    fn prepare(
        self,
        reader: &DomainReader<'_, DurableJobDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let mut job = validate_current(reader, self.job_id, self.expected_job_revision)?;
        if !job.lifecycle().is_live() {
            return Err(invalid_transition("a live job", job.lifecycle()));
        }
        validate_failure_checkpoint(
            &job,
            &self.evidence,
            BranchHandoffJobLifecycle::TerminalFailed,
        )?;
        let stopped_at = job.state.checkpoint().ok_or_else(invariant_transition)?;
        job.state = BranchHandoffJobState::TerminalFailed {
            stopped_at,
            evidence: self.evidence,
        };
        advance(&mut job)?;
        Ok(job)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, DurableJobDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<JobRecordCodec>(1)?;
        reservation.reserve_records::<LiveJobIndexCodec>(1)?;
        Ok(())
    }

    fn contribute(
        job: Self::Prepared,
        mutations: &mut MutationBuilder<'_, DurableJobDomain>,
    ) -> Result<(), Self::Error> {
        put_terminal_transition(mutations, &job)
    }
}

impl DomainMutation<DurableJobDomain> for SucceedBranchHandoff {
    type Error = DurableJobMutationError;
    type Prepared = BranchHandoffJobRecord;

    fn prepare(
        self,
        reader: &DomainReader<'_, DurableJobDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let mut job = validate_lifecycle(
            reader,
            self.job_id,
            self.expected_job_revision,
            BranchHandoffJobLifecycle::ParentActive,
        )?;
        let (parent, cas) = match job.state {
            BranchHandoffJobState::ParentActive { parent, cas } => (parent, cas),
            _ => return Err(invariant_transition()),
        };
        job.state = BranchHandoffJobState::Succeeded { parent, cas };
        advance(&mut job)?;
        Ok(job)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, DurableJobDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<JobRecordCodec>(1)?;
        reservation.reserve_records::<LiveJobIndexCodec>(1)?;
        Ok(())
    }

    fn contribute(
        job: Self::Prepared,
        mutations: &mut MutationBuilder<'_, DurableJobDomain>,
    ) -> Result<(), Self::Error> {
        put_terminal_transition(mutations, &job)
    }
}

fn validate_lifecycle(
    reader: &DomainReader<'_, DurableJobDomain>,
    job_id: JobId,
    expected_job_revision: JobRevision,
    expected_lifecycle: BranchHandoffJobLifecycle,
) -> Result<BranchHandoffJobRecord, DurableJobMutationError> {
    let job = validate_current(reader, job_id, expected_job_revision)?;
    if job.lifecycle() != expected_lifecycle {
        return Err(invalid_transition(
            lifecycle_name(expected_lifecycle),
            job.lifecycle(),
        ));
    }
    Ok(job)
}

fn validate_current(
    reader: &DomainReader<'_, DurableJobDomain>,
    job_id: JobId,
    expected_job_revision: JobRevision,
) -> Result<BranchHandoffJobRecord, DurableJobMutationError> {
    let job = required_job(reader, job_id)?;
    ensure_revision(expected_job_revision, job.revision)?;
    Ok(job)
}

fn validate_failure_checkpoint(
    job: &super::super::BranchHandoffJobRecord,
    evidence: &HandoffFailureEvidence,
    failure_lifecycle: BranchHandoffJobLifecycle,
) -> Result<(), DurableJobMutationError> {
    let checkpoint = job.state.checkpoint().ok_or_else(invariant_transition)?;
    if failure_state_is_compatible(failure_lifecycle, &checkpoint, evidence.kind()) {
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
