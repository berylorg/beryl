use std::{error::Error, fmt};

use beryl_home_store::{
    CursorDirection, CursorRange, CursorReadLimits, DomainCallbackError, DomainCallbackSource,
    DomainHandle, DomainRegistrationError, DomainSchemaVersion, HomeStore, KeyspaceSchemaVersion,
    MutationBuildError, MutationContribution, PointReadLimit, ReadError, RecordFamily,
    StorageDomain,
};
use beryl_model::{DomainRevision, JobId, JobRevision, RevisionError, SyndicThreadId};

use crate::StatePage;

mod codec;
mod mutation;
mod record;
#[cfg(feature = "test-faults")]
mod test_support;
mod validate;
mod value;

use codec::{
    DiscussionAttemptIndexCodec, JobRecordCodec, LatestAttemptIndexCodec, LiveJobIndexCodec,
    RequestIdempotencyIndexCodec, RequestIndexKey,
};
pub use mutation::{
    AdmitBranchHandoffJob, CompleteResolvingTurn, RecordParentCasAcceptance,
    RecordRetryableHandoffFailure, RecordTerminalHandoffFailure, RetryBranchHandoff,
    StartParentHandoff, SucceedBranchHandoff,
};
pub use record::{
    branch_handoff_job_id, BranchHandoffCheckpoint, BranchHandoffJobAdmission,
    BranchHandoffJobLifecycle, BranchHandoffJobRecord, BranchHandoffJobState,
    LatestBranchHandoffAttempt, ResolutionRequestAdmission,
};
pub use value::{
    DiscussionContextDigest, DiscussionContextOwnerId, DurableJobValueError,
    HandoffFailureEvidence, HandoffFailureKind, ParentCasIdentity, ParentHandoffIdentity,
    ParentQueueOrdinal, ResolutionAttemptOrdinal, ResolutionRequestIdentity, ResolutionText,
    HANDOFF_FAILURE_DETAIL_MAX_BYTES, RESOLUTION_TEXT_MAX_BYTES,
};

pub(crate) const BRANCH_HANDOFF_JOB_RECORD_LIMIT: usize = 128 * 1024;
pub(crate) const REQUEST_IDEMPOTENCY_RECORD_LIMIT: usize = 64;

const DURABLE_JOB_FAMILIES: &[RecordFamily<DurableJobDomain>] = &[
    RecordFamily::new::<JobRecordCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<LiveJobIndexCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<RequestIdempotencyIndexCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<DiscussionAttemptIndexCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<LatestAttemptIndexCodec>(KeyspaceSchemaVersion::new(1)),
];

pub(crate) struct DurableJobDomain;

impl StorageDomain for DurableJobDomain {
    const NAME: &'static str = "beryl-durable-job";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const FAMILIES: &'static [RecordFamily<Self>] = DURABLE_JOB_FAMILIES;
    type ValidationError = DurableJobValidationError;

    fn validate(
        reader: &beryl_home_store::DomainReader<'_, Self>,
    ) -> Result<(), Self::ValidationError> {
        validate::validate(reader)
    }
}

/// Opaque typed access to the durable orchestration-job domain.
#[derive(Clone, Copy)]
pub struct DurableJobState {
    handle: DomainHandle<DurableJobDomain>,
}

impl DurableJobState {
    pub(crate) fn register(store: &mut HomeStore) -> Result<Self, DomainRegistrationError> {
        store
            .register_domain::<DurableJobDomain>()
            .map(|handle| Self { handle })
    }

    pub(crate) fn reacquire(
        store: &HomeStore,
    ) -> Result<Self, beryl_home_store::DomainHandleError> {
        store
            .domain_handle::<DurableJobDomain>()
            .map(|handle| Self { handle })
    }

    pub fn revision(&self, store: &HomeStore) -> Result<DomainRevision, ReadError> {
        store.domain_revision(self.handle)
    }

    /// Returns this domain's revision from a still-current successful command.
    pub fn committed_revision(
        &self,
        store: &HomeStore,
        receipt: &beryl_home_store::CommitReceipt,
    ) -> Result<Option<DomainRevision>, beryl_home_store::CommitReceiptError> {
        store.receipt_domain_revision(receipt, self.handle)
    }

    /// Builds a deliberately incompatible persisted failure state for schema tests.
    #[cfg(feature = "test-faults")]
    #[doc(hidden)]
    #[must_use]
    pub fn corrupt_failure_state_for_test(
        &self,
        expected_domain_revision: DomainRevision,
        job_id: JobId,
        expected_job_revision: JobRevision,
        checkpoint: BranchHandoffCheckpoint,
        evidence: HandoffFailureEvidence,
        retryable: bool,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_domain_revision,
            test_support::CorruptFailureState {
                job_id,
                expected_job_revision,
                checkpoint,
                evidence,
                retryable,
            },
        )
    }

    pub fn job(
        &self,
        store: &HomeStore,
        job_id: JobId,
    ) -> Result<Option<BranchHandoffJobRecord>, ReadError> {
        store.read_point::<DurableJobDomain, JobRecordCodec>(
            self.handle,
            &job_id,
            job_point_limit(),
        )
    }

    pub fn request_admission(
        &self,
        store: &HomeStore,
        request: &ResolutionRequestIdentity,
    ) -> Result<Option<ResolutionRequestAdmission>, ReadError> {
        store.read_point::<DurableJobDomain, RequestIdempotencyIndexCodec>(
            self.handle,
            &RequestIndexKey::new(request.clone()),
            request_point_limit(),
        )
    }

    pub fn latest_attempt(
        &self,
        store: &HomeStore,
        discussion_thread_id: SyndicThreadId,
    ) -> Result<Option<LatestBranchHandoffAttempt>, ReadError> {
        store.read_point::<DurableJobDomain, LatestAttemptIndexCodec>(
            self.handle,
            &discussion_thread_id,
            small_point_limit(),
        )
    }

    pub fn list_live(
        &self,
        store: &HomeStore,
        after: Option<JobId>,
        limits: CursorReadLimits,
    ) -> Result<StatePage<BranchHandoffJobRecord>, ReadError> {
        let start = after.unwrap_or_else(|| JobId::from_bytes([0; 16]));
        let end = JobId::from_bytes([u8::MAX; 16]);
        let range = if after.is_some() {
            CursorRange::after(start, end)
        } else {
            CursorRange::closed(start, end)
        };
        let page = store.read_cursor::<DurableJobDomain, LiveJobIndexCodec>(
            self.handle,
            &range,
            CursorDirection::Forward,
            limits,
        )?;
        let stored_bytes = page.stored_bytes();
        let decoded_bytes = page.decoded_bytes();
        let has_more = page.has_more();
        Ok(StatePage {
            records: page
                .into_records()
                .into_iter()
                .map(|entry| entry.into_parts().1)
                .collect(),
            stored_bytes,
            decoded_bytes,
            has_more,
        })
    }

    #[must_use]
    pub fn admit_branch_handoff(
        &self,
        expected_revision: DomainRevision,
        command: AdmitBranchHandoffJob,
    ) -> MutationContribution {
        self.handle.contribution(expected_revision, command)
    }

    #[must_use]
    pub fn complete_resolving_turn(
        &self,
        expected_revision: DomainRevision,
        command: CompleteResolvingTurn,
    ) -> MutationContribution {
        self.handle.contribution(expected_revision, command)
    }

    #[must_use]
    pub fn start_parent_handoff(
        &self,
        expected_revision: DomainRevision,
        command: StartParentHandoff,
    ) -> MutationContribution {
        self.handle.contribution(expected_revision, command)
    }

    #[must_use]
    pub fn record_parent_cas_acceptance(
        &self,
        expected_revision: DomainRevision,
        command: RecordParentCasAcceptance,
    ) -> MutationContribution {
        self.handle.contribution(expected_revision, command)
    }

    #[must_use]
    pub fn record_retryable_failure(
        &self,
        expected_revision: DomainRevision,
        command: RecordRetryableHandoffFailure,
    ) -> MutationContribution {
        self.handle.contribution(expected_revision, command)
    }

    #[must_use]
    pub fn retry_branch_handoff(
        &self,
        expected_revision: DomainRevision,
        command: RetryBranchHandoff,
    ) -> MutationContribution {
        self.handle.contribution(expected_revision, command)
    }

    #[must_use]
    pub fn record_terminal_failure(
        &self,
        expected_revision: DomainRevision,
        command: RecordTerminalHandoffFailure,
    ) -> MutationContribution {
        self.handle.contribution(expected_revision, command)
    }

    #[must_use]
    pub fn succeed_branch_handoff(
        &self,
        expected_revision: DomainRevision,
        command: SucceedBranchHandoff,
    ) -> MutationContribution {
        self.handle.contribution(expected_revision, command)
    }
}

fn job_point_limit() -> PointReadLimit {
    PointReadLimit::new(BRANCH_HANDOFF_JOB_RECORD_LIMIT + 4)
        .expect("durable job point limit is nonzero")
}

fn request_point_limit() -> PointReadLimit {
    PointReadLimit::new(REQUEST_IDEMPOTENCY_RECORD_LIMIT + 4)
        .expect("request-idempotency point limit is nonzero")
}

fn small_point_limit() -> PointReadLimit {
    PointReadLimit::new(128).expect("durable job index point limit is nonzero")
}

/// Why a durable branch-handoff mutation was rejected.
#[derive(Debug)]
pub enum DurableJobMutationError {
    Read(ReadError),
    Build(MutationBuildError),
    Value(DurableJobValueError),
    Revision(RevisionError),
    RequestAlreadyAdmitted {
        existing: ResolutionRequestAdmission,
    },
    JobAlreadyExists {
        job_id: JobId,
    },
    JobMissing {
        job_id: JobId,
    },
    AttemptAlreadyExists {
        discussion_thread_id: SyndicThreadId,
        attempt_ordinal: ResolutionAttemptOrdinal,
    },
    AttemptOrdinalConflict {
        expected: ResolutionAttemptOrdinal,
        actual: ResolutionAttemptOrdinal,
    },
    LiveAttemptExists {
        job_id: JobId,
    },
    SuccessfulAttemptExists {
        job_id: JobId,
    },
    JobRevisionConflict {
        expected: JobRevision,
        current: JobRevision,
    },
    InvalidTransition {
        expected: &'static str,
        current: BranchHandoffJobLifecycle,
    },
    FailureKindMismatch {
        expected: &'static str,
        actual: HandoffFailureKind,
    },
    Invariant(&'static str),
}

impl fmt::Display for DurableJobMutationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(source) => source.fmt(formatter),
            Self::Build(source) => source.fmt(formatter),
            Self::Value(source) => source.fmt(formatter),
            Self::Revision(source) => source.fmt(formatter),
            Self::RequestAlreadyAdmitted { existing } => write!(
                formatter,
                "resolution tool request already admitted job {}",
                existing.job_id()
            ),
            Self::JobAlreadyExists { job_id } => write!(formatter, "job {job_id} already exists"),
            Self::JobMissing { job_id } => write!(formatter, "job {job_id} is missing"),
            Self::AttemptAlreadyExists {
                discussion_thread_id,
                attempt_ordinal,
            } => write!(
                formatter,
                "discussion {discussion_thread_id} already has attempt {}",
                attempt_ordinal.get()
            ),
            Self::AttemptOrdinalConflict { expected, actual } => write!(
                formatter,
                "resolution attempt ordinal conflict: expected {}, got {}",
                expected.get(),
                actual.get()
            ),
            Self::LiveAttemptExists { job_id } => {
                write!(formatter, "discussion already has live job {job_id}")
            }
            Self::SuccessfulAttemptExists { job_id } => write!(
                formatter,
                "discussion already succeeded through handoff job {job_id}"
            ),
            Self::JobRevisionConflict { expected, current } => write!(
                formatter,
                "job revision conflict: expected {}, current {}",
                expected.get(),
                current.get()
            ),
            Self::InvalidTransition { expected, current } => write!(
                formatter,
                "job transition requires {expected}, current state is {}",
                lifecycle_name(*current)
            ),
            Self::FailureKindMismatch { expected, actual } => write!(
                formatter,
                "{actual:?} failure evidence is not valid for a {expected} transition"
            ),
            Self::Invariant(message) => formatter.write_str(message),
        }
    }
}

impl Error for DurableJobMutationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read(source) => Some(source),
            Self::Build(source) => Some(source),
            Self::Value(source) => Some(source),
            Self::Revision(source) => Some(source),
            _ => None,
        }
    }
}

impl DomainCallbackError for DurableJobMutationError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        match self {
            Self::Read(source) => Ok(DomainCallbackSource::Read(source)),
            source => Err(source),
        }
    }
}

impl From<ReadError> for DurableJobMutationError {
    fn from(source: ReadError) -> Self {
        Self::Read(source)
    }
}

impl From<MutationBuildError> for DurableJobMutationError {
    fn from(source: MutationBuildError) -> Self {
        Self::Build(source)
    }
}

impl From<DurableJobValueError> for DurableJobMutationError {
    fn from(source: DurableJobValueError) -> Self {
        Self::Value(source)
    }
}

impl From<RevisionError> for DurableJobMutationError {
    fn from(source: RevisionError) -> Self {
        Self::Revision(source)
    }
}

#[derive(Debug)]
pub(crate) enum DurableJobValidationError {
    Read(ReadError),
    Value(DurableJobValueError),
    Invariant(&'static str),
}

impl fmt::Display for DurableJobValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(source) => source.fmt(formatter),
            Self::Value(source) => source.fmt(formatter),
            Self::Invariant(message) => formatter.write_str(message),
        }
    }
}

impl Error for DurableJobValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read(source) => Some(source),
            Self::Value(source) => Some(source),
            Self::Invariant(_) => None,
        }
    }
}

impl DomainCallbackError for DurableJobValidationError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        match self {
            Self::Read(source) => Ok(DomainCallbackSource::Read(source)),
            source => Err(source),
        }
    }
}

impl From<ReadError> for DurableJobValidationError {
    fn from(source: ReadError) -> Self {
        Self::Read(source)
    }
}

impl From<DurableJobValueError> for DurableJobValidationError {
    fn from(source: DurableJobValueError) -> Self {
        Self::Value(source)
    }
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
