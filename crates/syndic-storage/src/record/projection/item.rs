use beryl_model::{
    ProjectionRevision, SyndicItemId, SyndicProjectionId, SyndicResourceId, SyndicTurnId,
};

use crate::{
    ComposerAtomOrdinal, ComposerImageMarker, InputMarkerOrdinal, ItemProjectionGeneration,
    MarkdownParserCheckpoint, ProjectionLifecycle, ProjectionOrdinal, ProjectionTextSource,
    SyndicRecordError,
};

use super::super::{MAX_INLINE_TEXT_BYTES, validate_text};

mod payload;

pub use payload::ProjectionPayload;

pub const MARKDOWN_PARAGRAPH_INLINE_MAX_BYTES: usize = 16_384;
pub const MARKDOWN_SPAN_MAX_BYTES: usize = 8_192;
pub const MARKDOWN_CODE_INLINE_MAX_BYTES: usize = 4_096;
pub const MARKDOWN_CODE_INLINE_MAX_LINES: u64 = 64;
pub const MARKDOWN_CODE_PREVIEW_MAX_BYTES: usize = 2_048;
pub const MARKDOWN_CODE_PREVIEW_MAX_LINES: u64 = 8;
pub const MARKDOWN_TABLE_INLINE_MAX_BYTES: usize = 8_192;
pub const MARKDOWN_TABLE_INLINE_MAX_BODY_ROWS: u64 = 32;
pub const MARKDOWN_TABLE_INLINE_MAX_COLUMNS: u64 = 12;
pub const MARKDOWN_TABLE_PREVIEW_MAX_BYTES: usize = 4_096;
pub const MARKDOWN_TABLE_PREVIEW_MAX_BODY_ROWS: u64 = 4;
pub const TRANSCRIPT_PAGE_MAX_BYTES: usize = 65_536;

/// Exact durable format of one parsed Markdown projection generation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ProjectionFormatVersion {
    V1,
}

/// Stable digest identity shared by all source-preserving spans of one Markdown block.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MarkdownBlockId([u8; 32]);

impl MarkdownBlockId {
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub const fn as_bytes(self) -> [u8; 32] {
        self.0
    }
}

/// Exact half-open UTF-8 byte range in the owning canonical item.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProjectionSourceRange {
    start: u64,
    end: u64,
}

impl ProjectionSourceRange {
    pub fn new(start: u64, end: u64) -> Result<Self, SyndicRecordError> {
        if start >= end {
            return Err(SyndicRecordError::InvalidByteRange {
                kind: "projection source range",
                start,
                end,
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

/// Bounded block-level structure retained after Syndic Markdown recognition.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MarkdownBlockKind {
    Paragraph,
    Heading(u8),
    BlockQuote,
    List,
    ThematicBreak,
    FencedCode,
    Table,
    Fallback,
}

impl MarkdownBlockKind {
    pub fn validate(self) -> Result<Self, SyndicRecordError> {
        if let Self::Heading(level) = self
            && !(1..=6).contains(&level)
        {
            return Err(SyndicRecordError::InvalidMarkdownHeadingLevel { level });
        }
        Ok(self)
    }
}

/// One immutable completed projection record inside an item generation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectionRecord {
    id: SyndicProjectionId,
    revision: ProjectionRevision,
    item_id: SyndicItemId,
    turn_id: SyndicTurnId,
    ordinal: ProjectionOrdinal,
    payload: ProjectionPayload,
}

impl ProjectionRecord {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        id: SyndicProjectionId,
        revision: ProjectionRevision,
        item_id: SyndicItemId,
        turn_id: SyndicTurnId,
        ordinal: ProjectionOrdinal,
        payload: ProjectionPayload,
    ) -> Self {
        Self {
            id,
            revision,
            item_id,
            turn_id,
            ordinal,
            payload,
        }
    }

    #[must_use]
    pub const fn id(&self) -> SyndicProjectionId {
        self.id
    }
    #[must_use]
    pub const fn revision(&self) -> ProjectionRevision {
        self.revision
    }
    #[must_use]
    pub const fn item_id(&self) -> SyndicItemId {
        self.item_id
    }
    #[must_use]
    pub const fn turn_id(&self) -> SyndicTurnId {
        self.turn_id
    }
    #[must_use]
    pub const fn ordinal(&self) -> ProjectionOrdinal {
        self.ordinal
    }
    #[must_use]
    pub const fn payload(&self) -> &ProjectionPayload {
        &self.payload
    }
}

/// Mutable selector for one item's coherent projection generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ItemProjectionHeadRecord {
    item_id: SyndicItemId,
    revision: ProjectionRevision,
    source_item_revision: ProjectionRevision,
    generation: ItemProjectionGeneration,
    lifecycle: ProjectionLifecycle,
}

impl ItemProjectionHeadRecord {
    #[must_use]
    pub const fn new(
        item_id: SyndicItemId,
        revision: ProjectionRevision,
        source_item_revision: ProjectionRevision,
        generation: ItemProjectionGeneration,
        lifecycle: ProjectionLifecycle,
    ) -> Self {
        Self {
            item_id,
            revision,
            source_item_revision,
            generation,
            lifecycle,
        }
    }

    #[must_use]
    pub const fn item_id(&self) -> SyndicItemId {
        self.item_id
    }
    #[must_use]
    pub const fn revision(self) -> ProjectionRevision {
        self.revision
    }
    #[must_use]
    pub const fn source_item_revision(self) -> ProjectionRevision {
        self.source_item_revision
    }
    #[must_use]
    pub const fn generation(&self) -> ItemProjectionGeneration {
        self.generation
    }
    #[must_use]
    pub const fn lifecycle(self) -> ProjectionLifecycle {
        self.lifecycle
    }
}

/// Immutable summary of one completely constructed item-projection generation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ItemProjectionSetRecord {
    item_id: SyndicItemId,
    generation: ItemProjectionGeneration,
    format: ProjectionFormatVersion,
    source_item_revision: ProjectionRevision,
    source: ProjectionTextSource,
    source_bytes: u64,
    stable_projection_count: u64,
    stable_resource_count: u64,
    stable_digest: [u8; 32],
    projection_count: u64,
    resource_count: u64,
    digest: [u8; 32],
    resume_checkpoint: MarkdownParserCheckpoint,
    stable_eof_resolved: bool,
}

impl ItemProjectionSetRecord {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        item_id: SyndicItemId,
        generation: ItemProjectionGeneration,
        format: ProjectionFormatVersion,
        source_item_revision: ProjectionRevision,
        source: ProjectionTextSource,
        source_bytes: u64,
        stable_projection_count: u64,
        stable_resource_count: u64,
        stable_digest: [u8; 32],
        projection_count: u64,
        resource_count: u64,
        digest: [u8; 32],
        resume_checkpoint: MarkdownParserCheckpoint,
        stable_eof_resolved: bool,
    ) -> Self {
        Self {
            item_id,
            generation,
            format,
            source_item_revision,
            source,
            source_bytes,
            stable_projection_count,
            stable_resource_count,
            stable_digest,
            projection_count,
            resource_count,
            digest,
            resume_checkpoint,
            stable_eof_resolved,
        }
    }

    #[must_use]
    pub const fn item_id(&self) -> SyndicItemId {
        self.item_id
    }
    #[must_use]
    pub const fn generation(&self) -> ItemProjectionGeneration {
        self.generation
    }
    #[must_use]
    pub const fn format(&self) -> ProjectionFormatVersion {
        self.format
    }
    #[must_use]
    pub const fn source_item_revision(&self) -> ProjectionRevision {
        self.source_item_revision
    }
    #[must_use]
    pub const fn source(&self) -> ProjectionTextSource {
        self.source
    }
    #[must_use]
    pub const fn source_bytes(&self) -> u64 {
        self.source_bytes
    }
    #[must_use]
    pub const fn stable_projection_count(&self) -> u64 {
        self.stable_projection_count
    }
    #[must_use]
    pub const fn stable_resource_count(&self) -> u64 {
        self.stable_resource_count
    }
    #[must_use]
    pub const fn stable_digest(&self) -> [u8; 32] {
        self.stable_digest
    }
    #[must_use]
    pub const fn projection_count(&self) -> u64 {
        self.projection_count
    }
    #[must_use]
    pub const fn resource_count(&self) -> u64 {
        self.resource_count
    }
    #[must_use]
    pub const fn digest(&self) -> [u8; 32] {
        self.digest
    }

    #[must_use]
    pub const fn resume_checkpoint(&self) -> &MarkdownParserCheckpoint {
        &self.resume_checkpoint
    }

    #[must_use]
    pub const fn stable_eof_resolved(&self) -> bool {
        self.stable_eof_resolved
    }
}
