use crate::{
    CanonicalItemRecord, CasProjectionBindingRecord, ConversationRecord, CursorRecord,
    ProjectionRecord, RecoveryMarkerId, RecoveryMarkerRecord, ResourceRecord, SourceEventRecord,
    ThreadViewId, TranscriptViewPosition, TranscriptViewRecord, TranscriptViewRecordId, TurnRecord,
};

#[derive(Clone, Debug, Default)]
pub struct SyndicWriteBatch {
    pub(crate) operations: Vec<SyndicWriteOperation>,
}

impl SyndicWriteBatch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }

    pub fn len(&self) -> usize {
        self.operations.len()
    }

    pub fn put_conversation(mut self, record: ConversationRecord) -> Self {
        self.operations
            .push(SyndicWriteOperation::PutConversation(record));
        self
    }

    pub fn put_turn(mut self, record: TurnRecord) -> Self {
        self.operations.push(SyndicWriteOperation::PutTurn(record));
        self
    }

    pub fn put_source_event(mut self, record: SourceEventRecord) -> Self {
        self.operations
            .push(SyndicWriteOperation::PutSourceEvent(record));
        self
    }

    pub fn put_item(mut self, record: CanonicalItemRecord) -> Self {
        self.operations.push(SyndicWriteOperation::PutItem(record));
        self
    }

    pub fn put_projection(mut self, record: ProjectionRecord) -> Self {
        self.operations
            .push(SyndicWriteOperation::PutProjection(record));
        self
    }

    pub fn put_view_record(mut self, record: TranscriptViewRecord) -> Self {
        self.operations
            .push(SyndicWriteOperation::PutViewRecord(record));
        self
    }

    pub fn remove_view_record(
        mut self,
        view_id: ThreadViewId,
        position: TranscriptViewPosition,
        id: TranscriptViewRecordId,
    ) -> Self {
        self.operations
            .push(SyndicWriteOperation::RemoveViewRecord {
                view_id,
                position,
                id,
            });
        self
    }

    pub fn put_resource(mut self, record: ResourceRecord) -> Self {
        self.operations
            .push(SyndicWriteOperation::PutResource(record));
        self
    }

    pub fn put_cursor(mut self, record: CursorRecord) -> Self {
        self.operations
            .push(SyndicWriteOperation::PutCursor(record));
        self
    }

    pub fn put_recovery_marker(mut self, record: RecoveryMarkerRecord) -> Self {
        self.operations
            .push(SyndicWriteOperation::PutRecoveryMarker(record));
        self
    }

    pub fn clear_recovery_marker(mut self, id: RecoveryMarkerId) -> Self {
        self.operations
            .push(SyndicWriteOperation::ClearRecoveryMarker(id));
        self
    }

    pub fn put_cas_projection_binding(mut self, record: CasProjectionBindingRecord) -> Self {
        self.operations
            .push(SyndicWriteOperation::PutCasProjectionBinding(record));
        self
    }
}

#[derive(Clone, Debug)]
pub enum SyndicWriteOperation {
    PutConversation(ConversationRecord),
    PutTurn(TurnRecord),
    PutSourceEvent(SourceEventRecord),
    PutItem(CanonicalItemRecord),
    PutProjection(ProjectionRecord),
    PutViewRecord(TranscriptViewRecord),
    RemoveViewRecord {
        view_id: ThreadViewId,
        position: TranscriptViewPosition,
        id: TranscriptViewRecordId,
    },
    PutResource(ResourceRecord),
    PutCursor(CursorRecord),
    PutRecoveryMarker(RecoveryMarkerRecord),
    ClearRecoveryMarker(RecoveryMarkerId),
    PutCasProjectionBinding(CasProjectionBindingRecord),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CommitSummary {
    pub operations: usize,
    pub idempotent_source_events: usize,
}
