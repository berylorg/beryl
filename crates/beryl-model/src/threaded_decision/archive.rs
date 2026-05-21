use super::*;

impl ThreadedDecisionArchiveStatus {
    pub fn state(&self) -> ThreadedDecisionArchiveState {
        self.state
    }

    pub fn operation_id(&self) -> Option<&ThreadedDecisionOperationId> {
        self.operation_id.as_ref()
    }

    pub fn failure_message(&self) -> Option<&str> {
        self.failure_message.as_deref()
    }

    pub fn updated_at_millis(&self) -> Option<u64> {
        self.updated_at_millis
    }

    pub(super) fn pending(
        operation_id: ThreadedDecisionOperationId,
        updated_at_millis: u64,
    ) -> Self {
        Self {
            state: ThreadedDecisionArchiveState::Pending,
            operation_id: Some(operation_id),
            failure_message: None,
            updated_at_millis: Some(updated_at_millis),
        }
    }

    pub(super) fn archived(
        operation_id: Option<ThreadedDecisionOperationId>,
        updated_at_millis: u64,
    ) -> Self {
        Self {
            state: ThreadedDecisionArchiveState::Archived,
            operation_id,
            failure_message: None,
            updated_at_millis: Some(updated_at_millis),
        }
    }

    pub(super) fn failed(
        operation_id: Option<ThreadedDecisionOperationId>,
        failure_message: Option<String>,
        updated_at_millis: u64,
    ) -> Self {
        Self {
            state: ThreadedDecisionArchiveState::Failed,
            operation_id,
            failure_message,
            updated_at_millis: Some(updated_at_millis),
        }
    }
}

impl ThreadedDecisionInvalidation {
    pub fn reason(&self) -> ThreadedDecisionInvalidationReason {
        self.reason
    }

    pub fn invalidated_at_millis(&self) -> u64 {
        self.invalidated_at_millis
    }

    pub fn provenance(&self) -> &MutationProvenance {
        &self.provenance
    }
}

impl ThreadedDecisionSupersession {
    pub fn superseded_by_record_id(&self) -> &ThreadedDecisionRecordId {
        &self.superseded_by_record_id
    }

    pub fn superseded_at_millis(&self) -> u64 {
        self.superseded_at_millis
    }

    pub fn provenance(&self) -> &MutationProvenance {
        &self.provenance
    }
}
