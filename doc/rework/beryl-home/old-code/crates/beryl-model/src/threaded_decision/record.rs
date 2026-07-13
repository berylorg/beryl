use super::*;

impl ThreadedDecisionRecord {
    pub fn queued_branch(
        record_id: ThreadedDecisionRecordId,
        checklist_item_id: SemanticNodeId,
        parent_thread_id: ConversationThreadId,
        branch_point_turn_id: Option<ConversationTurnId>,
        branch_operation_id: ThreadedDecisionOperationId,
        created_at_millis: u64,
        provenance: MutationProvenance,
    ) -> Self {
        Self {
            record_id,
            checklist_item_id,
            parent_thread_id,
            child_thread_id: None,
            bootstrap_turn_id: None,
            branch_point_turn_id,
            handoff_turn_id: None,
            status: ThreadedDecisionStatus::QueuedBranch,
            outcome: None,
            resolution_summary: None,
            handoff_message: None,
            archive_status: ThreadedDecisionArchiveStatus::default(),
            branch_operation_id,
            resolution_operation_id: None,
            archive_operation_id: None,
            created_at_millis,
            updated_at_millis: created_at_millis,
            resolved_at_millis: None,
            supersession: None,
            invalidation: None,
            provenance: ElementProvenance::new(provenance),
        }
    }

    pub fn active_branch(
        record_id: ThreadedDecisionRecordId,
        checklist_item_id: SemanticNodeId,
        parent_thread_id: ConversationThreadId,
        child_thread_id: ConversationThreadId,
        branch_point_turn_id: Option<ConversationTurnId>,
        branch_operation_id: ThreadedDecisionOperationId,
        created_at_millis: u64,
        provenance: MutationProvenance,
    ) -> Self {
        let mut record = Self::queued_branch(
            record_id,
            checklist_item_id,
            parent_thread_id,
            branch_point_turn_id,
            branch_operation_id,
            created_at_millis,
            provenance,
        );
        record.child_thread_id = Some(child_thread_id);
        record.status = ThreadedDecisionStatus::ActiveBranch;
        record
    }

    pub fn record_id(&self) -> &ThreadedDecisionRecordId {
        &self.record_id
    }

    pub fn checklist_item_id(&self) -> &SemanticNodeId {
        &self.checklist_item_id
    }

    pub fn parent_thread_id(&self) -> &ConversationThreadId {
        &self.parent_thread_id
    }

    pub fn child_thread_id(&self) -> Option<&ConversationThreadId> {
        self.child_thread_id.as_ref()
    }

    pub fn bootstrap_turn_id(&self) -> Option<&ConversationTurnId> {
        self.bootstrap_turn_id.as_ref()
    }

    pub fn branch_point_turn_id(&self) -> Option<&ConversationTurnId> {
        self.branch_point_turn_id.as_ref()
    }

    pub fn handoff_turn_id(&self) -> Option<&ConversationTurnId> {
        self.handoff_turn_id.as_ref()
    }

    pub fn status(&self) -> ThreadedDecisionStatus {
        self.status
    }

    pub fn outcome(&self) -> Option<ThreadedDecisionOutcome> {
        self.outcome
    }

    pub fn resolution_summary(&self) -> Option<&str> {
        self.resolution_summary.as_deref()
    }

    pub fn handoff_message(&self) -> Option<&str> {
        self.handoff_message.as_deref()
    }

    pub fn archive_status(&self) -> &ThreadedDecisionArchiveStatus {
        &self.archive_status
    }

    pub fn branch_operation_id(&self) -> &ThreadedDecisionOperationId {
        &self.branch_operation_id
    }

    pub fn resolution_operation_id(&self) -> Option<&ThreadedDecisionOperationId> {
        self.resolution_operation_id.as_ref()
    }

    pub fn archive_operation_id(&self) -> Option<&ThreadedDecisionOperationId> {
        self.archive_operation_id.as_ref()
    }

    pub fn created_at_millis(&self) -> u64 {
        self.created_at_millis
    }

    pub fn updated_at_millis(&self) -> u64 {
        self.updated_at_millis
    }

    pub fn resolved_at_millis(&self) -> Option<u64> {
        self.resolved_at_millis
    }

    pub fn supersession(&self) -> Option<&ThreadedDecisionSupersession> {
        self.supersession.as_ref()
    }

    pub fn invalidation(&self) -> Option<&ThreadedDecisionInvalidation> {
        self.invalidation.as_ref()
    }

    pub fn provenance(&self) -> &ElementProvenance {
        &self.provenance
    }

    pub fn blocks_new_branch(&self) -> bool {
        matches!(
            self.status,
            ThreadedDecisionStatus::QueuedBranch
                | ThreadedDecisionStatus::ActiveBranch
                | ThreadedDecisionStatus::PendingResolution
                | ThreadedDecisionStatus::HandoffStarted
                | ThreadedDecisionStatus::ChecklistUpdated
                | ThreadedDecisionStatus::ArchivePending
                | ThreadedDecisionStatus::ArchiveFailed
        )
    }

    pub fn protects_resolved_item(&self) -> bool {
        self.outcome.is_some()
            && matches!(
                self.status,
                ThreadedDecisionStatus::ChecklistUpdated
                    | ThreadedDecisionStatus::ArchivePending
                    | ThreadedDecisionStatus::ArchiveFailed
                    | ThreadedDecisionStatus::Closed
                    | ThreadedDecisionStatus::Superseded
            )
    }

    pub(super) fn transition(
        &mut self,
        to: ThreadedDecisionStatus,
        provenance: MutationProvenance,
    ) -> Result<bool, ThreadedDecisionStateError> {
        if self.status == to {
            return Ok(false);
        }

        let valid = matches!(
            (self.status, to),
            (
                ThreadedDecisionStatus::QueuedBranch,
                ThreadedDecisionStatus::ActiveBranch
            ) | (
                ThreadedDecisionStatus::ActiveBranch,
                ThreadedDecisionStatus::PendingResolution
            ) | (
                ThreadedDecisionStatus::PendingResolution,
                ThreadedDecisionStatus::HandoffStarted
            ) | (
                ThreadedDecisionStatus::PendingResolution,
                ThreadedDecisionStatus::ChecklistUpdated
            ) | (
                ThreadedDecisionStatus::HandoffStarted,
                ThreadedDecisionStatus::ChecklistUpdated
            ) | (
                ThreadedDecisionStatus::ChecklistUpdated,
                ThreadedDecisionStatus::ArchivePending
            ) | (
                ThreadedDecisionStatus::ArchivePending,
                ThreadedDecisionStatus::Closed
            ) | (
                ThreadedDecisionStatus::ArchivePending,
                ThreadedDecisionStatus::ArchiveFailed
            ) | (
                ThreadedDecisionStatus::ArchiveFailed,
                ThreadedDecisionStatus::ArchivePending
            ) | (
                ThreadedDecisionStatus::ArchiveFailed,
                ThreadedDecisionStatus::Closed
            ) | (
                ThreadedDecisionStatus::Closed,
                ThreadedDecisionStatus::Superseded
            )
        );
        if !valid {
            return Err(ThreadedDecisionStateError::InvalidTransition {
                record_id: self.record_id.clone(),
                from: self.status,
                to,
            });
        }

        self.status = to;
        self.updated_at_millis = provenance.recorded_at_millis();
        self.provenance.touch(provenance);
        Ok(true)
    }

    pub(super) fn invalidate(
        &mut self,
        reason: ThreadedDecisionInvalidationReason,
        provenance: MutationProvenance,
    ) -> bool {
        if self.status == ThreadedDecisionStatus::Invalidated
            && self
                .invalidation
                .as_ref()
                .is_some_and(|invalidation| invalidation.reason == reason)
        {
            return false;
        }

        self.status = ThreadedDecisionStatus::Invalidated;
        self.updated_at_millis = provenance.recorded_at_millis();
        self.invalidation = Some(ThreadedDecisionInvalidation {
            reason,
            invalidated_at_millis: provenance.recorded_at_millis(),
            provenance: provenance.clone(),
        });
        self.provenance.touch(provenance);
        true
    }
}
