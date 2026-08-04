use beryl_model::{
    ProjectionRevision, SyndicItemId, SyndicPathDigest, SyndicThreadId, SyndicTurnId,
    ThreadRevision,
};

use crate::{
    ItemProjectionGeneration, ProjectionFormatVersion, ProjectionOrdinal, ProjectionTextSource,
    ProjectionTextSourceCursor, SyndicTimestamp, TranscriptGeneration, TurnDepth, TurnItemOrdinal,
    TurnLifecycle, TurnStateRevision,
};

/// Durable phase of one bounded item-projection construction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ItemProjectionBuildPhase {
    Parsing(MarkdownParserCheckpoint),
    Superseded(MarkdownParserCheckpoint),
}

/// Exact delimiter of one open fenced-code block.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MarkdownFenceMarker {
    byte: u8,
    length: u8,
}

impl MarkdownFenceMarker {
    #[must_use]
    pub const fn new(byte: u8, length: u8) -> Self {
        Self { byte, length }
    }

    #[must_use]
    pub const fn byte(self) -> u8 {
        self.byte
    }

    #[must_use]
    pub const fn length(self) -> u8 {
        self.length
    }
}

/// Typed open Markdown block carried across bounded construction steps.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MarkdownOpenBlock {
    Paragraph {
        block_start: u64,
        next_span_ordinal: u64,
        buffered_source: Box<str>,
    },
    FencedCode {
        block_start: u64,
        body_start: u64,
        fence: MarkdownFenceMarker,
        language: Option<Box<str>>,
        preview: Box<str>,
        logical_lines: u64,
        body_bytes: u64,
        resource_digest: [u8; 32],
    },
    Table {
        block_start: u64,
        header_end: u64,
        columns: u64,
        body_rows: u64,
        preview: Box<str>,
        source_bytes: u64,
        resource_digest: [u8; 32],
    },
    Fallback {
        block_start: u64,
        next_span_ordinal: u64,
        buffered_source: Box<str>,
    },
}

/// Bounded parser state needed to resume without replaying an entire canonical item.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarkdownParserCheckpoint {
    consumed_source_bytes: u64,
    closed_source_bytes: u64,
    source_cursor: ProjectionTextSourceCursor,
    line_start: u64,
    line_carry: Box<str>,
    line_continuation: bool,
    open_block: Option<MarkdownOpenBlock>,
}

impl MarkdownParserCheckpoint {
    #[must_use]
    pub fn new(
        consumed_source_bytes: u64,
        closed_source_bytes: u64,
        source_cursor: ProjectionTextSourceCursor,
        line_start: u64,
        line_carry: Box<str>,
        line_continuation: bool,
        open_block: Option<MarkdownOpenBlock>,
    ) -> Self {
        Self {
            consumed_source_bytes,
            closed_source_bytes,
            source_cursor,
            line_start,
            line_carry,
            line_continuation,
            open_block,
        }
    }

    #[must_use]
    pub const fn consumed_source_bytes(&self) -> u64 {
        self.consumed_source_bytes
    }

    #[must_use]
    pub const fn closed_source_bytes(&self) -> u64 {
        self.closed_source_bytes
    }

    #[must_use]
    pub const fn source_cursor(&self) -> ProjectionTextSourceCursor {
        self.source_cursor
    }

    #[must_use]
    pub const fn line_start(&self) -> u64 {
        self.line_start
    }

    #[must_use]
    pub fn line_carry(&self) -> &str {
        &self.line_carry
    }

    #[must_use]
    pub const fn line_continuation(&self) -> bool {
        self.line_continuation
    }

    #[must_use]
    pub const fn open_block(&self) -> Option<&MarkdownOpenBlock> {
        self.open_block.as_ref()
    }
}

/// Mutable bounded frontier for one incomplete item-projection generation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ItemProjectionBuildRecord {
    item_id: SyndicItemId,
    generation: ItemProjectionGeneration,
    revision: ProjectionRevision,
    format: ProjectionFormatVersion,
    source_item_revision: ProjectionRevision,
    source: ProjectionTextSource,
    source_bytes: u64,
    projection_count: u64,
    resource_count: u64,
    output_digest: [u8; 32],
    phase: ItemProjectionBuildPhase,
}

impl ItemProjectionBuildRecord {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        item_id: SyndicItemId,
        generation: ItemProjectionGeneration,
        revision: ProjectionRevision,
        format: ProjectionFormatVersion,
        source_item_revision: ProjectionRevision,
        source: ProjectionTextSource,
        source_bytes: u64,
        projection_count: u64,
        resource_count: u64,
        output_digest: [u8; 32],
        phase: ItemProjectionBuildPhase,
    ) -> Self {
        Self {
            item_id,
            generation,
            revision,
            format,
            source_item_revision,
            source,
            source_bytes,
            projection_count,
            resource_count,
            output_digest,
            phase,
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
    pub const fn revision(&self) -> ProjectionRevision {
        self.revision
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
    pub const fn projection_count(&self) -> u64 {
        self.projection_count
    }
    #[must_use]
    pub const fn resource_count(&self) -> u64 {
        self.resource_count
    }
    #[must_use]
    pub const fn output_digest(&self) -> [u8; 32] {
        self.output_digest
    }
    #[must_use]
    pub const fn phase(&self) -> &ItemProjectionBuildPhase {
        &self.phase
    }
}

/// Durable phase and cursor of one generation-owned transcript rebuild.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TranscriptBuildPhase {
    Collecting {
        next_turn: Option<SyndicTurnId>,
    },
    Publishing {
        next_depth: TurnDepth,
        next_item: TurnItemOrdinal,
        next_projection: ProjectionOrdinal,
    },
    Complete,
    Superseded,
}

/// Bounded mutable state and completed manifest of one transcript generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TranscriptBuildRecord {
    thread_id: SyndicThreadId,
    generation: TranscriptGeneration,
    revision: ProjectionRevision,
    source_thread_revision: ThreadRevision,
    committed_tail: Option<SyndicTurnId>,
    selected_path_digest: SyndicPathDigest,
    path_turn_count: u64,
    entry_count: u64,
    entry_digest: [u8; 32],
    history_complete: bool,
    phase: TranscriptBuildPhase,
}

impl TranscriptBuildRecord {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        generation: TranscriptGeneration,
        revision: ProjectionRevision,
        source_thread_revision: ThreadRevision,
        committed_tail: Option<SyndicTurnId>,
        selected_path_digest: SyndicPathDigest,
        path_turn_count: u64,
        entry_count: u64,
        entry_digest: [u8; 32],
        history_complete: bool,
        phase: TranscriptBuildPhase,
    ) -> Self {
        Self {
            thread_id,
            generation,
            revision,
            source_thread_revision,
            committed_tail,
            selected_path_digest,
            path_turn_count,
            entry_count,
            entry_digest,
            history_complete,
            phase,
        }
    }

    #[must_use]
    pub const fn thread_id(self) -> SyndicThreadId {
        self.thread_id
    }
    #[must_use]
    pub const fn generation(self) -> TranscriptGeneration {
        self.generation
    }
    #[must_use]
    pub const fn revision(self) -> ProjectionRevision {
        self.revision
    }
    #[must_use]
    pub const fn source_thread_revision(self) -> ThreadRevision {
        self.source_thread_revision
    }
    #[must_use]
    pub const fn committed_tail(self) -> Option<SyndicTurnId> {
        self.committed_tail
    }
    #[must_use]
    pub const fn selected_path_digest(self) -> SyndicPathDigest {
        self.selected_path_digest
    }
    #[must_use]
    pub const fn path_turn_count(self) -> u64 {
        self.path_turn_count
    }
    #[must_use]
    pub const fn entry_count(self) -> u64 {
        self.entry_count
    }
    #[must_use]
    pub const fn entry_digest(self) -> [u8; 32] {
        self.entry_digest
    }
    #[must_use]
    pub const fn history_complete(self) -> bool {
        self.history_complete
    }
    #[must_use]
    pub const fn phase(self) -> TranscriptBuildPhase {
        self.phase
    }
}

/// One exact turn in a transcript generation's selected path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TranscriptPathTurnRecord {
    thread_id: SyndicThreadId,
    generation: TranscriptGeneration,
    depth: TurnDepth,
    turn_id: SyndicTurnId,
    turn_path_digest: SyndicPathDigest,
    state_revision: TurnStateRevision,
    lifecycle: TurnLifecycle,
    source_event_count: u64,
    item_count: u64,
    finalized_item_count: u64,
    updated_at: SyndicTimestamp,
}

impl TranscriptPathTurnRecord {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        generation: TranscriptGeneration,
        depth: TurnDepth,
        turn_id: SyndicTurnId,
        turn_path_digest: SyndicPathDigest,
        state_revision: TurnStateRevision,
        lifecycle: TurnLifecycle,
        source_event_count: u64,
        item_count: u64,
        finalized_item_count: u64,
        updated_at: SyndicTimestamp,
    ) -> Self {
        Self {
            thread_id,
            generation,
            depth,
            turn_id,
            turn_path_digest,
            state_revision,
            lifecycle,
            source_event_count,
            item_count,
            finalized_item_count,
            updated_at,
        }
    }

    #[must_use]
    pub const fn thread_id(self) -> SyndicThreadId {
        self.thread_id
    }
    #[must_use]
    pub const fn generation(self) -> TranscriptGeneration {
        self.generation
    }
    #[must_use]
    pub const fn depth(self) -> TurnDepth {
        self.depth
    }
    #[must_use]
    pub const fn turn_id(self) -> SyndicTurnId {
        self.turn_id
    }
    #[must_use]
    pub const fn turn_path_digest(self) -> SyndicPathDigest {
        self.turn_path_digest
    }

    #[must_use]
    pub const fn state_revision(self) -> TurnStateRevision {
        self.state_revision
    }

    #[must_use]
    pub const fn lifecycle(self) -> TurnLifecycle {
        self.lifecycle
    }

    #[must_use]
    pub const fn source_event_count(self) -> u64 {
        self.source_event_count
    }

    #[must_use]
    pub const fn item_count(self) -> u64 {
        self.item_count
    }

    #[must_use]
    pub const fn finalized_item_count(self) -> u64 {
        self.finalized_item_count
    }

    #[must_use]
    pub const fn updated_at(self) -> SyndicTimestamp {
        self.updated_at
    }
}
