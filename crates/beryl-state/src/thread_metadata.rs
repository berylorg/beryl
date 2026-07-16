use std::{error::Error, fmt};

use beryl_home_store::{
    CursorDirection, CursorRange, CursorReadLimits, DomainCallbackError, DomainCallbackSource,
    DomainHandle, DomainRegistrationError, DomainSchemaVersion, HomeStore, KeyspaceSchemaVersion,
    MutationBuildError, MutationContribution, PointReadLimit, ReadError, RecordFamily,
    StorageDomain,
};
use beryl_model::{ExecutionBinding, SyndicThreadId};

use crate::{
    GeneratedTitle, RecordRevision, StatePage, ThreadActivitySummary, ThreadArchiveState,
    TokenUsageSnapshot, ValueError,
};

mod codec;
mod mutation;
mod validate;

use codec::ThreadMetadataRecordCodec;
pub use mutation::{
    ArchiveBranchDiscussion, CreateThreadMetadata, SetGeneratedTitle, UpdateThreadActivity,
    UpdateTokenUsage,
};

const THREAD_METADATA_RECORD_LIMIT: usize = 64 * 1024;
const THREAD_METADATA_FAMILIES: &[RecordFamily<ThreadMetadataDomain>] =
    &[RecordFamily::new::<ThreadMetadataRecordCodec>(
        KeyspaceSchemaVersion::new(1),
    )];

pub(crate) struct ThreadMetadataDomain;

impl StorageDomain for ThreadMetadataDomain {
    const NAME: &'static str = "beryl-thread-metadata";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const FAMILIES: &'static [RecordFamily<Self>] = THREAD_METADATA_FAMILIES;
    type ValidationError = ThreadMetadataValidationError;

    fn validate(
        reader: &beryl_home_store::DomainReader<'_, Self>,
    ) -> Result<(), Self::ValidationError> {
        validate::validate(reader)
    }
}

/// Initial presentation kind for a newly created thread-metadata record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThreadMetadataKind {
    Ordinary,
    BranchDiscussion,
}

/// Durable Beryl-owned presentation metadata for one Syndic thread.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThreadMetadataRecord {
    thread_id: SyndicThreadId,
    binding: ExecutionBinding,
    generated_title: Option<GeneratedTitle>,
    archive_state: ThreadArchiveState,
    activity: Option<ThreadActivitySummary>,
    token_usage: Option<TokenUsageSnapshot>,
    revision: RecordRevision,
}

impl ThreadMetadataRecord {
    fn initial(
        thread_id: SyndicThreadId,
        binding: ExecutionBinding,
        kind: ThreadMetadataKind,
    ) -> Self {
        Self {
            thread_id,
            binding,
            generated_title: None,
            archive_state: match kind {
                ThreadMetadataKind::Ordinary => ThreadArchiveState::Ordinary,
                ThreadMetadataKind::BranchDiscussion => ThreadArchiveState::BranchDiscussionOpen,
            },
            activity: None,
            token_usage: None,
            revision: RecordRevision::INITIAL,
        }
    }

    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }

    #[must_use]
    pub const fn binding(&self) -> &ExecutionBinding {
        &self.binding
    }

    #[must_use]
    pub const fn generated_title(&self) -> Option<&GeneratedTitle> {
        self.generated_title.as_ref()
    }

    #[must_use]
    pub const fn archive_state(&self) -> ThreadArchiveState {
        self.archive_state
    }

    #[must_use]
    pub const fn activity(&self) -> Option<ThreadActivitySummary> {
        self.activity
    }

    #[must_use]
    pub const fn token_usage(&self) -> Option<TokenUsageSnapshot> {
        self.token_usage
    }

    #[must_use]
    pub const fn revision(&self) -> RecordRevision {
        self.revision
    }
}

/// Opaque typed access to the thread-presentation metadata domain.
#[derive(Clone, Copy)]
pub struct ThreadMetadataState {
    handle: DomainHandle<ThreadMetadataDomain>,
}

impl ThreadMetadataState {
    pub(crate) fn register(store: &mut HomeStore) -> Result<Self, DomainRegistrationError> {
        store
            .register_domain::<ThreadMetadataDomain>()
            .map(|handle| Self { handle })
    }

    pub(crate) fn reacquire(
        store: &HomeStore,
    ) -> Result<Self, beryl_home_store::DomainHandleError> {
        store
            .domain_handle::<ThreadMetadataDomain>()
            .map(|handle| Self { handle })
    }

    pub fn revision(&self, store: &HomeStore) -> Result<beryl_model::DomainRevision, ReadError> {
        store.domain_revision(self.handle)
    }

    /// Returns this domain's revision from a still-current successful command.
    pub fn committed_revision(
        &self,
        store: &HomeStore,
        receipt: &beryl_home_store::CommitReceipt,
    ) -> Result<Option<beryl_model::DomainRevision>, beryl_home_store::CommitReceiptError> {
        store.receipt_domain_revision(receipt, self.handle)
    }

    pub fn metadata(
        &self,
        store: &HomeStore,
        thread_id: SyndicThreadId,
    ) -> Result<Option<ThreadMetadataRecord>, ReadError> {
        store.read_point::<ThreadMetadataDomain, ThreadMetadataRecordCodec>(
            self.handle,
            &thread_id,
            point_limit(),
        )
    }

    pub fn list(
        &self,
        store: &HomeStore,
        after: Option<SyndicThreadId>,
        limits: CursorReadLimits,
    ) -> Result<StatePage<ThreadMetadataRecord>, ReadError> {
        let start = after.unwrap_or_else(|| SyndicThreadId::from_bytes([0; 16]));
        let end = SyndicThreadId::from_bytes([u8::MAX; 16]);
        let range = if after.is_some() {
            CursorRange::after(start, end)
        } else {
            CursorRange::closed(start, end)
        };
        let page = store.read_cursor::<ThreadMetadataDomain, ThreadMetadataRecordCodec>(
            self.handle,
            &range,
            CursorDirection::Forward,
            limits,
        )?;
        let stored_bytes = page.stored_bytes();
        let has_more = page.has_more();
        Ok(StatePage {
            records: page
                .into_records()
                .into_iter()
                .map(|record| record.into_parts().1)
                .collect(),
            stored_bytes,
            has_more,
        })
    }

    #[must_use]
    pub fn create(
        &self,
        expected_revision: beryl_model::DomainRevision,
        command: CreateThreadMetadata,
    ) -> MutationContribution {
        self.handle.contribution(expected_revision, command)
    }

    #[must_use]
    pub fn set_generated_title(
        &self,
        expected_revision: beryl_model::DomainRevision,
        command: SetGeneratedTitle,
    ) -> MutationContribution {
        self.handle.contribution(expected_revision, command)
    }

    #[must_use]
    pub fn update_activity(
        &self,
        expected_revision: beryl_model::DomainRevision,
        command: UpdateThreadActivity,
    ) -> MutationContribution {
        self.handle.contribution(expected_revision, command)
    }

    #[must_use]
    pub fn update_token_usage(
        &self,
        expected_revision: beryl_model::DomainRevision,
        command: UpdateTokenUsage,
    ) -> MutationContribution {
        self.handle.contribution(expected_revision, command)
    }

    #[must_use]
    pub fn archive_branch_discussion(
        &self,
        expected_revision: beryl_model::DomainRevision,
        command: ArchiveBranchDiscussion,
    ) -> MutationContribution {
        self.handle.contribution(expected_revision, command)
    }
}

fn point_limit() -> PointReadLimit {
    PointReadLimit::new(THREAD_METADATA_RECORD_LIMIT + 4)
        .expect("thread metadata point limit is nonzero")
}

/// Why an automatic thread-metadata mutation was rejected.
#[derive(Debug)]
pub enum ThreadMetadataMutationError {
    Read(ReadError),
    Build(MutationBuildError),
    Value(ValueError),
    MetadataExists {
        thread_id: SyndicThreadId,
    },
    MetadataMissing {
        thread_id: SyndicThreadId,
    },
    ImmutableBindingMismatch {
        thread_id: SyndicThreadId,
    },
    RecordRevisionConflict {
        expected: RecordRevision,
        current: RecordRevision,
    },
    GeneratedTitleAlreadySet,
    SourceRevisionNotLater {
        kind: &'static str,
    },
    ActivityTimeRegressed,
    NotOpenBranchDiscussion,
}

impl fmt::Display for ThreadMetadataMutationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(source) => source.fmt(formatter),
            Self::Build(source) => source.fmt(formatter),
            Self::Value(source) => source.fmt(formatter),
            Self::MetadataExists { thread_id } => {
                write!(formatter, "metadata already exists for {thread_id}")
            }
            Self::MetadataMissing { thread_id } => {
                write!(formatter, "metadata is missing for {thread_id}")
            }
            Self::ImmutableBindingMismatch { thread_id } => write!(
                formatter,
                "metadata creation attempted to change immutable binding for {thread_id}"
            ),
            Self::RecordRevisionConflict { expected, current } => write!(
                formatter,
                "thread metadata revision conflict: expected {}, current {}",
                expected.get(),
                current.get()
            ),
            Self::GeneratedTitleAlreadySet => {
                formatter.write_str("generated title is already accepted and immutable")
            }
            Self::SourceRevisionNotLater { kind } => {
                write!(
                    formatter,
                    "{kind} source thread revision must strictly advance"
                )
            }
            Self::ActivityTimeRegressed => {
                formatter.write_str("thread activity time cannot move backward")
            }
            Self::NotOpenBranchDiscussion => {
                formatter.write_str("only an open branch discussion can be archived")
            }
        }
    }
}

impl Error for ThreadMetadataMutationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read(source) => Some(source),
            Self::Build(source) => Some(source),
            Self::Value(source) => Some(source),
            _ => None,
        }
    }
}

impl DomainCallbackError for ThreadMetadataMutationError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        match self {
            Self::Read(source) => Ok(DomainCallbackSource::Read(source)),
            source => Err(source),
        }
    }
}

impl From<ReadError> for ThreadMetadataMutationError {
    fn from(source: ReadError) -> Self {
        Self::Read(source)
    }
}

impl From<MutationBuildError> for ThreadMetadataMutationError {
    fn from(source: MutationBuildError) -> Self {
        Self::Build(source)
    }
}

impl From<ValueError> for ThreadMetadataMutationError {
    fn from(source: ValueError) -> Self {
        Self::Value(source)
    }
}

#[derive(Debug)]
pub(crate) enum ThreadMetadataValidationError {
    Read(ReadError),
    Invariant(&'static str),
}

impl fmt::Display for ThreadMetadataValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(source) => source.fmt(formatter),
            Self::Invariant(message) => formatter.write_str(message),
        }
    }
}

impl Error for ThreadMetadataValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read(source) => Some(source),
            Self::Invariant(_) => None,
        }
    }
}

impl DomainCallbackError for ThreadMetadataValidationError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        match self {
            Self::Read(source) => Ok(DomainCallbackSource::Read(source)),
            source => Err(source),
        }
    }
}

impl From<ReadError> for ThreadMetadataValidationError {
    fn from(source: ReadError) -> Self {
        Self::Read(source)
    }
}
