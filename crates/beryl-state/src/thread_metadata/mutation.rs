use beryl_home_store::{DomainMutation, DomainReader, MutationBuilder, PointReadLimit};
use beryl_model::{ExecutionBinding, JobId, SyndicThreadId};

use crate::{
    GeneratedTitle, RecordRevision, ThreadActivitySummary, ThreadArchiveState, TokenUsageSnapshot,
    UnixMillis,
};

use super::{
    THREAD_METADATA_RECORD_LIMIT, ThreadMetadataDomain, ThreadMetadataKind,
    ThreadMetadataMutationError, ThreadMetadataRecord, codec::ThreadMetadataRecordCodec,
};

/// Create the sole metadata record for one exact Syndic thread and binding.
pub struct CreateThreadMetadata {
    thread_id: SyndicThreadId,
    binding: ExecutionBinding,
    kind: ThreadMetadataKind,
}

impl CreateThreadMetadata {
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        binding: ExecutionBinding,
        kind: ThreadMetadataKind,
    ) -> Self {
        Self {
            thread_id,
            binding,
            kind,
        }
    }
}

impl DomainMutation<ThreadMetadataDomain> for CreateThreadMetadata {
    type Error = ThreadMetadataMutationError;

    fn validate(&self, reader: &DomainReader<'_, ThreadMetadataDomain>) -> Result<(), Self::Error> {
        if let Some(existing) = read(reader, self.thread_id)? {
            if existing.binding != self.binding {
                return Err(ThreadMetadataMutationError::ImmutableBindingMismatch {
                    thread_id: self.thread_id,
                });
            }
            return Err(ThreadMetadataMutationError::MetadataExists {
                thread_id: self.thread_id,
            });
        }
        Ok(())
    }

    fn contribute(
        &self,
        _reader: &DomainReader<'_, ThreadMetadataDomain>,
        mutations: &mut MutationBuilder<'_, ThreadMetadataDomain>,
    ) -> Result<(), Self::Error> {
        mutations.put::<ThreadMetadataRecordCodec>(
            &self.thread_id,
            &ThreadMetadataRecord::initial(self.thread_id, self.binding.clone(), self.kind),
        )?;
        Ok(())
    }
}

/// Accept the one immutable automatically generated title.
pub struct SetGeneratedTitle {
    thread_id: SyndicThreadId,
    expected_record_revision: RecordRevision,
    title: GeneratedTitle,
}

impl SetGeneratedTitle {
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        expected_record_revision: RecordRevision,
        title: GeneratedTitle,
    ) -> Self {
        Self {
            thread_id,
            expected_record_revision,
            title,
        }
    }
}

impl DomainMutation<ThreadMetadataDomain> for SetGeneratedTitle {
    type Error = ThreadMetadataMutationError;

    fn validate(&self, reader: &DomainReader<'_, ThreadMetadataDomain>) -> Result<(), Self::Error> {
        let record = required(reader, self.thread_id)?;
        ensure_record_revision(self.expected_record_revision, record.revision)?;
        if record.generated_title.is_some() {
            return Err(ThreadMetadataMutationError::GeneratedTitleAlreadySet);
        }
        Ok(())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, ThreadMetadataDomain>,
        mutations: &mut MutationBuilder<'_, ThreadMetadataDomain>,
    ) -> Result<(), Self::Error> {
        let mut record = required(reader, self.thread_id)?;
        record.generated_title = Some(self.title.clone());
        advance(&mut record)?;
        mutations.put::<ThreadMetadataRecordCodec>(&self.thread_id, &record)?;
        Ok(())
    }
}

/// Advance the exact recent-activity summary.
pub struct UpdateThreadActivity {
    thread_id: SyndicThreadId,
    expected_record_revision: RecordRevision,
    activity: ThreadActivitySummary,
}

impl UpdateThreadActivity {
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        expected_record_revision: RecordRevision,
        activity: ThreadActivitySummary,
    ) -> Self {
        Self {
            thread_id,
            expected_record_revision,
            activity,
        }
    }
}

impl DomainMutation<ThreadMetadataDomain> for UpdateThreadActivity {
    type Error = ThreadMetadataMutationError;

    fn validate(&self, reader: &DomainReader<'_, ThreadMetadataDomain>) -> Result<(), Self::Error> {
        let record = required(reader, self.thread_id)?;
        ensure_record_revision(self.expected_record_revision, record.revision)?;
        if let Some(current) = record.activity {
            if self.activity.source_thread_revision() <= current.source_thread_revision() {
                return Err(ThreadMetadataMutationError::SourceRevisionNotLater {
                    kind: "activity",
                });
            }
            if self.activity.last_activity_at() < current.last_activity_at() {
                return Err(ThreadMetadataMutationError::ActivityTimeRegressed);
            }
        }
        Ok(())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, ThreadMetadataDomain>,
        mutations: &mut MutationBuilder<'_, ThreadMetadataDomain>,
    ) -> Result<(), Self::Error> {
        let mut record = required(reader, self.thread_id)?;
        record.activity = Some(self.activity);
        advance(&mut record)?;
        mutations.put::<ThreadMetadataRecordCodec>(&self.thread_id, &record)?;
        Ok(())
    }
}

/// Advance the exact durable token-usage presentation snapshot.
pub struct UpdateTokenUsage {
    thread_id: SyndicThreadId,
    expected_record_revision: RecordRevision,
    token_usage: TokenUsageSnapshot,
}

impl UpdateTokenUsage {
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        expected_record_revision: RecordRevision,
        token_usage: TokenUsageSnapshot,
    ) -> Self {
        Self {
            thread_id,
            expected_record_revision,
            token_usage,
        }
    }
}

impl DomainMutation<ThreadMetadataDomain> for UpdateTokenUsage {
    type Error = ThreadMetadataMutationError;

    fn validate(&self, reader: &DomainReader<'_, ThreadMetadataDomain>) -> Result<(), Self::Error> {
        let record = required(reader, self.thread_id)?;
        ensure_record_revision(self.expected_record_revision, record.revision)?;
        if record.token_usage.is_some_and(|current| {
            self.token_usage.source_thread_revision() <= current.source_thread_revision()
        }) {
            return Err(ThreadMetadataMutationError::SourceRevisionNotLater {
                kind: "token usage",
            });
        }
        Ok(())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, ThreadMetadataDomain>,
        mutations: &mut MutationBuilder<'_, ThreadMetadataDomain>,
    ) -> Result<(), Self::Error> {
        let mut record = required(reader, self.thread_id)?;
        record.token_usage = Some(self.token_usage);
        advance(&mut record)?;
        mutations.put::<ThreadMetadataRecordCodec>(&self.thread_id, &record)?;
        Ok(())
    }
}

/// One-way automatic publication after the exact parent handoff succeeds.
pub struct ArchiveBranchDiscussion {
    thread_id: SyndicThreadId,
    expected_record_revision: RecordRevision,
    handoff_job_id: JobId,
    archived_at: UnixMillis,
}

impl ArchiveBranchDiscussion {
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        expected_record_revision: RecordRevision,
        handoff_job_id: JobId,
        archived_at: UnixMillis,
    ) -> Self {
        Self {
            thread_id,
            expected_record_revision,
            handoff_job_id,
            archived_at,
        }
    }
}

impl DomainMutation<ThreadMetadataDomain> for ArchiveBranchDiscussion {
    type Error = ThreadMetadataMutationError;

    fn validate(&self, reader: &DomainReader<'_, ThreadMetadataDomain>) -> Result<(), Self::Error> {
        let record = required(reader, self.thread_id)?;
        ensure_record_revision(self.expected_record_revision, record.revision)?;
        if record.archive_state != ThreadArchiveState::BranchDiscussionOpen {
            return Err(ThreadMetadataMutationError::NotOpenBranchDiscussion);
        }
        Ok(())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, ThreadMetadataDomain>,
        mutations: &mut MutationBuilder<'_, ThreadMetadataDomain>,
    ) -> Result<(), Self::Error> {
        let mut record = required(reader, self.thread_id)?;
        record.archive_state = ThreadArchiveState::BranchDiscussionArchived {
            handoff_job_id: self.handoff_job_id,
            archived_at: self.archived_at,
        };
        advance(&mut record)?;
        mutations.put::<ThreadMetadataRecordCodec>(&self.thread_id, &record)?;
        Ok(())
    }
}

fn read(
    reader: &DomainReader<'_, ThreadMetadataDomain>,
    thread_id: SyndicThreadId,
) -> Result<Option<ThreadMetadataRecord>, ThreadMetadataMutationError> {
    reader
        .point::<ThreadMetadataRecordCodec>(&thread_id, point_limit())
        .map_err(Into::into)
}

fn required(
    reader: &DomainReader<'_, ThreadMetadataDomain>,
    thread_id: SyndicThreadId,
) -> Result<ThreadMetadataRecord, ThreadMetadataMutationError> {
    read(reader, thread_id)?.ok_or(ThreadMetadataMutationError::MetadataMissing { thread_id })
}

fn ensure_record_revision(
    expected: RecordRevision,
    current: RecordRevision,
) -> Result<(), ThreadMetadataMutationError> {
    if expected == current {
        Ok(())
    } else {
        Err(ThreadMetadataMutationError::RecordRevisionConflict { expected, current })
    }
}

fn advance(record: &mut ThreadMetadataRecord) -> Result<(), ThreadMetadataMutationError> {
    record.revision = record.revision.checked_next()?;
    Ok(())
}

fn point_limit() -> PointReadLimit {
    PointReadLimit::new(THREAD_METADATA_RECORD_LIMIT + 4)
        .expect("thread metadata point limit is nonzero")
}
