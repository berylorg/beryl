use beryl_model::{
    DiscussionContextDigest, ProjectionRevision, SyndicItemId, SyndicProjectionId, SyndicThreadId,
    SyndicTurnId,
};
use sha2::{Digest, Sha256};

use super::{SyndicTimestamp, SyndicValueError};

/// Exact maximum UTF-8 bytes accepted for one discussion selection.
pub const DISCUSSION_CONTEXT_MAX_BYTES: usize = 65_536;

/// Exact durable envelope shape used for one branch-discussion selection.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DiscussionContextVersion {
    V1,
}

/// Exact selected UTF-8 text retained without trimming or role conversion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscussionContextText(Box<str>);

impl DiscussionContextText {
    /// Validates complete selected text while preserving its exact bytes.
    pub fn new(value: impl AsRef<str>) -> Result<Self, SyndicValueError> {
        let value = value.as_ref();
        if value.is_empty() {
            return Err(SyndicValueError::EmptyText {
                kind: "discussion context",
            });
        }
        if value.len() > DISCUSSION_CONTEXT_MAX_BYTES {
            return Err(SyndicValueError::TextTooLong {
                kind: "discussion context",
                maximum: DISCUSSION_CONTEXT_MAX_BYTES,
                actual: value.len(),
            });
        }
        if let Some(index) = value.as_bytes().iter().position(|byte| *byte == 0) {
            return Err(SyndicValueError::NulByte {
                kind: "discussion context",
                index,
            });
        }
        Ok(Self(value.into()))
    }

    /// Returns the exact admitted text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns the exact UTF-8 byte length.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns whether this value is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        false
    }
}

/// End-exclusive source range for one accepted discussion selection.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DiscussionContextRange {
    start: u64,
    end: u64,
}

impl DiscussionContextRange {
    /// Constructs a non-empty bounded source range.
    pub fn new(start: u64, end: u64) -> Result<Self, SyndicValueError> {
        if end <= start {
            return Err(SyndicValueError::InvalidRange { start, end });
        }
        let length = end - start;
        if length > DISCUSSION_CONTEXT_MAX_BYTES as u64 {
            return Err(SyndicValueError::RangeTooLong {
                maximum: DISCUSSION_CONTEXT_MAX_BYTES as u64,
                actual: length,
            });
        }
        Ok(Self { start, end })
    }

    #[must_use]
    pub const fn start(self) -> u64 {
        self.start
    }

    #[must_use]
    pub const fn end(self) -> u64 {
        self.end
    }

    #[must_use]
    pub const fn len(self) -> u64 {
        self.end - self.start
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        false
    }
}

/// Exact immutable source provenance for one assistant discussion selection.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DiscussionContextSource {
    thread_id: SyndicThreadId,
    turn_id: SyndicTurnId,
    item_id: SyndicItemId,
    projection_id: SyndicProjectionId,
    projection_revision: ProjectionRevision,
    range: DiscussionContextRange,
}

impl DiscussionContextSource {
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        turn_id: SyndicTurnId,
        item_id: SyndicItemId,
        projection_id: SyndicProjectionId,
        projection_revision: ProjectionRevision,
        range: DiscussionContextRange,
    ) -> Self {
        Self {
            thread_id,
            turn_id,
            item_id,
            projection_id,
            projection_revision,
            range,
        }
    }

    #[must_use]
    pub const fn thread_id(self) -> SyndicThreadId {
        self.thread_id
    }

    #[must_use]
    pub const fn turn_id(self) -> SyndicTurnId {
        self.turn_id
    }

    #[must_use]
    pub const fn item_id(self) -> SyndicItemId {
        self.item_id
    }

    #[must_use]
    pub const fn projection_id(self) -> SyndicProjectionId {
        self.projection_id
    }

    #[must_use]
    pub const fn projection_revision(self) -> ProjectionRevision {
        self.projection_revision
    }

    #[must_use]
    pub const fn range(self) -> DiscussionContextRange {
        self.range
    }
}

/// Immutable source and integrity facts for one assistant discussion selection.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DiscussionContextDescriptor {
    version: DiscussionContextVersion,
    source: DiscussionContextSource,
    digest: DiscussionContextDigest,
    created_at: SyndicTimestamp,
}

impl DiscussionContextDescriptor {
    fn new(
        source: DiscussionContextSource,
        selected_text: &DiscussionContextText,
        created_at: SyndicTimestamp,
    ) -> Result<Self, SyndicValueError> {
        let text_bytes = selected_text.len() as u64;
        if source.range().len() != text_bytes {
            return Err(SyndicValueError::ContextLengthMismatch {
                text_bytes,
                range_bytes: source.range().len(),
            });
        }
        let digest =
            DiscussionContextDigest::from_bytes(Sha256::digest(selected_text.as_str()).into());
        Ok(Self {
            version: DiscussionContextVersion::V1,
            source,
            digest,
            created_at,
        })
    }

    #[must_use]
    pub const fn version(self) -> DiscussionContextVersion {
        self.version
    }

    #[must_use]
    pub const fn source(self) -> DiscussionContextSource {
        self.source
    }

    #[must_use]
    pub const fn source_thread_id(self) -> SyndicThreadId {
        self.source.thread_id()
    }

    #[must_use]
    pub const fn source_turn_id(self) -> SyndicTurnId {
        self.source.turn_id()
    }

    #[must_use]
    pub const fn source_item_id(self) -> SyndicItemId {
        self.source.item_id()
    }

    #[must_use]
    pub const fn source_projection_id(self) -> SyndicProjectionId {
        self.source.projection_id()
    }

    #[must_use]
    pub const fn source_projection_revision(self) -> ProjectionRevision {
        self.source.projection_revision()
    }

    #[must_use]
    pub const fn source_range(self) -> DiscussionContextRange {
        self.source.range()
    }

    #[must_use]
    pub const fn digest(self) -> DiscussionContextDigest {
        self.digest
    }

    #[must_use]
    pub const fn created_at(self) -> SyndicTimestamp {
        self.created_at
    }
}

/// Complete immutable selected-context value stored under a typed context owner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscussionContextEnvelope {
    descriptor: DiscussionContextDescriptor,
    text: DiscussionContextText,
}

impl DiscussionContextEnvelope {
    /// Constructs one complete envelope after validating source-range agreement.
    pub fn new(
        source: DiscussionContextSource,
        text: DiscussionContextText,
        created_at: SyndicTimestamp,
    ) -> Result<Self, SyndicValueError> {
        let descriptor = DiscussionContextDescriptor::new(source, &text, created_at)?;
        Ok(Self { descriptor, text })
    }

    #[must_use]
    pub const fn descriptor(&self) -> DiscussionContextDescriptor {
        self.descriptor
    }

    #[must_use]
    pub fn text(&self) -> &DiscussionContextText {
        &self.text
    }
}
