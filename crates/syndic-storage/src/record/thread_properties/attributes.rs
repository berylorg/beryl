use beryl_model::{JobId, SyndicPathDigest, SyndicThreadId, SyndicTurnId, ThreadRevision};

use crate::{ContentReference, SyndicTimestamp, SyndicValueError, ThreadAttributesRevision};

/// Maximum UTF-8 bytes retained by one canonical thread title.
pub const THREAD_TITLE_MAX_BYTES: usize = 512;

/// Exact immutable eligibility witness attached to one accepted generated title.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedThreadTitle {
    text: Box<str>,
    source_turn_id: SyndicTurnId,
    source_content: ContentReference,
    source_selected_path_digest: SyndicPathDigest,
    source_thread_revision: ThreadRevision,
    generated_at: SyndicTimestamp,
}

impl GeneratedThreadTitle {
    pub fn new(
        text: impl AsRef<str>,
        source_turn_id: SyndicTurnId,
        source_content: ContentReference,
        source_selected_path_digest: SyndicPathDigest,
        source_thread_revision: ThreadRevision,
        generated_at: SyndicTimestamp,
    ) -> Result<Self, SyndicValueError> {
        Ok(Self {
            text: validate_thread_title(text.as_ref())?,
            source_turn_id,
            source_content,
            source_selected_path_digest,
            source_thread_revision,
            generated_at,
        })
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }
    #[must_use]
    pub const fn source_turn_id(&self) -> SyndicTurnId {
        self.source_turn_id
    }
    #[must_use]
    pub const fn source_content(&self) -> ContentReference {
        self.source_content
    }
    #[must_use]
    pub const fn source_selected_path_digest(&self) -> SyndicPathDigest {
        self.source_selected_path_digest
    }
    #[must_use]
    pub const fn source_thread_revision(&self) -> ThreadRevision {
        self.source_thread_revision
    }
    #[must_use]
    pub const fn generated_at(&self) -> SyndicTimestamp {
        self.generated_at
    }
}

pub(super) fn validate_thread_title(value: &str) -> Result<Box<str>, SyndicValueError> {
    const KIND: &str = "thread title";
    if value.is_empty() {
        return Err(SyndicValueError::EmptyText { kind: KIND });
    }
    if value.len() > THREAD_TITLE_MAX_BYTES {
        return Err(SyndicValueError::TextTooLong {
            kind: KIND,
            maximum: THREAD_TITLE_MAX_BYTES,
            actual: value.len(),
        });
    }
    if value.trim() != value {
        return Err(SyndicValueError::SurroundingWhitespace { kind: KIND });
    }
    if let Some((index, _)) = value.char_indices().find(|(_, value)| value.is_control()) {
        return Err(SyndicValueError::ControlCharacter { kind: KIND, index });
    }
    if !value.chars().any(char::is_alphanumeric) {
        return Err(SyndicValueError::MissingAlphanumeric { kind: KIND });
    }
    Ok(value.into())
}

/// One-way intrinsic archive lifecycle of one named Syndic thread.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThreadArchiveState {
    Ordinary,
    BranchDiscussionOpen,
    BranchDiscussionArchived {
        handoff_job_id: JobId,
        archived_at: SyndicTimestamp,
    },
}

impl ThreadArchiveState {
    #[must_use]
    pub const fn is_archived(self) -> bool {
        matches!(self, Self::BranchDiscussionArchived { .. })
    }
}

/// Independently revisioned generated-title and archive authority for one thread.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThreadAttributesRecord {
    thread_id: SyndicThreadId,
    revision: ThreadAttributesRevision,
    generated_title: Option<GeneratedThreadTitle>,
    archive: ThreadArchiveState,
}

impl ThreadAttributesRecord {
    #[must_use]
    pub const fn ordinary(thread_id: SyndicThreadId) -> Self {
        Self {
            thread_id,
            revision: ThreadAttributesRevision::FIRST,
            generated_title: None,
            archive: ThreadArchiveState::Ordinary,
        }
    }

    pub(crate) const fn branch_discussion_open(thread_id: SyndicThreadId) -> Self {
        Self {
            thread_id,
            revision: ThreadAttributesRevision::FIRST,
            generated_title: None,
            archive: ThreadArchiveState::BranchDiscussionOpen,
        }
    }

    #[must_use]
    pub(crate) const fn from_parts(
        thread_id: SyndicThreadId,
        revision: ThreadAttributesRevision,
        generated_title: Option<GeneratedThreadTitle>,
        archive: ThreadArchiveState,
    ) -> Self {
        Self {
            thread_id,
            revision,
            generated_title,
            archive,
        }
    }

    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }
    #[must_use]
    pub const fn revision(&self) -> ThreadAttributesRevision {
        self.revision
    }
    #[must_use]
    pub const fn generated_title(&self) -> Option<&GeneratedThreadTitle> {
        self.generated_title.as_ref()
    }
    #[must_use]
    pub const fn archive(&self) -> ThreadArchiveState {
        self.archive
    }

    pub(crate) fn accept_generated_title(
        &self,
        title: GeneratedThreadTitle,
    ) -> Result<Self, SyndicValueError> {
        Ok(Self::from_parts(
            self.thread_id,
            self.revision.checked_next()?,
            Some(title),
            self.archive,
        ))
    }

    pub(crate) fn archive_branch_discussion(
        &self,
        handoff_job_id: JobId,
        archived_at: SyndicTimestamp,
    ) -> Result<Self, SyndicValueError> {
        Ok(Self::from_parts(
            self.thread_id,
            self.revision.checked_next()?,
            self.generated_title.clone(),
            ThreadArchiveState::BranchDiscussionArchived {
                handoff_job_id,
                archived_at,
            },
        ))
    }
}
