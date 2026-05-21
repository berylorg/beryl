use super::*;

impl ThreadedDecisionState {
    pub fn records(&self) -> &[ThreadedDecisionRecord] {
        &self.records
    }

    pub fn record(&self, record_id: &ThreadedDecisionRecordId) -> Option<&ThreadedDecisionRecord> {
        self.records
            .iter()
            .find(|record| record.record_id() == record_id)
    }

    pub fn active_record_for_item(
        &self,
        checklist_item_id: &SemanticNodeId,
    ) -> Option<&ThreadedDecisionRecord> {
        self.records.iter().find(|record| {
            record.checklist_item_id() == checklist_item_id && record.blocks_new_branch()
        })
    }

    pub fn active_record_for_child_thread(
        &self,
        child_thread_id: &ConversationThreadId,
    ) -> Option<&ThreadedDecisionRecord> {
        self.records.iter().find(|record| {
            record.status() == ThreadedDecisionStatus::ActiveBranch
                && record.child_thread_id() == Some(child_thread_id)
        })
    }

    pub fn record_for_child_thread(
        &self,
        child_thread_id: &ConversationThreadId,
    ) -> Option<&ThreadedDecisionRecord> {
        self.records
            .iter()
            .find(|record| record.child_thread_id() == Some(child_thread_id))
    }

    pub fn insert_record(
        &mut self,
        record: ThreadedDecisionRecord,
    ) -> Result<bool, ThreadedDecisionStateError> {
        if self
            .records
            .iter()
            .any(|existing| existing.record_id() == record.record_id())
        {
            return Err(ThreadedDecisionStateError::DuplicateRecordId {
                record_id: record.record_id().clone(),
            });
        }

        if record.blocks_new_branch()
            && let Some(existing) = self.active_record_for_item(record.checklist_item_id())
        {
            return Err(ThreadedDecisionStateError::ActiveBranchExists {
                checklist_item_id: record.checklist_item_id().clone(),
                existing_record_id: existing.record_id().clone(),
            });
        }

        self.records.push(record);
        Ok(true)
    }

    pub fn activate_branch(
        &mut self,
        record_id: &ThreadedDecisionRecordId,
        child_thread_id: ConversationThreadId,
        branch_point_turn_id: Option<ConversationTurnId>,
        provenance: MutationProvenance,
    ) -> Result<bool, ThreadedDecisionStateError> {
        self.activate_branch_with_bootstrap_turn(
            record_id,
            child_thread_id,
            None,
            branch_point_turn_id,
            provenance,
        )
    }

    pub fn activate_branch_with_bootstrap_turn(
        &mut self,
        record_id: &ThreadedDecisionRecordId,
        child_thread_id: ConversationThreadId,
        bootstrap_turn_id: Option<ConversationTurnId>,
        branch_point_turn_id: Option<ConversationTurnId>,
        provenance: MutationProvenance,
    ) -> Result<bool, ThreadedDecisionStateError> {
        let record = self.record_mut(record_id)?;
        let mut changed = record.transition(ThreadedDecisionStatus::ActiveBranch, provenance)?;
        if record.child_thread_id.as_ref() != Some(&child_thread_id) {
            record.child_thread_id = Some(child_thread_id);
            changed = true;
        }
        if let Some(bootstrap_turn_id) = bootstrap_turn_id
            && record.bootstrap_turn_id.as_ref() != Some(&bootstrap_turn_id)
        {
            record.bootstrap_turn_id = Some(bootstrap_turn_id);
            changed = true;
        }
        if let Some(branch_point_turn_id) = branch_point_turn_id
            && record.branch_point_turn_id.as_ref() != Some(&branch_point_turn_id)
        {
            record.branch_point_turn_id = Some(branch_point_turn_id);
            changed = true;
        }
        Ok(changed)
    }

    pub fn remove_record(&mut self, record_id: &ThreadedDecisionRecordId) -> bool {
        let Some(index) = self
            .records
            .iter()
            .position(|record| record.record_id() == record_id)
        else {
            return false;
        };
        self.records.remove(index);
        true
    }

    pub fn mark_pending_resolution(
        &mut self,
        record_id: &ThreadedDecisionRecordId,
        outcome: ThreadedDecisionOutcome,
        resolution_summary: impl Into<String>,
        handoff_message: impl Into<String>,
        resolution_operation_id: ThreadedDecisionOperationId,
        provenance: MutationProvenance,
    ) -> Result<bool, ThreadedDecisionStateError> {
        let record = self.record_mut(record_id)?;
        let mut changed =
            record.transition(ThreadedDecisionStatus::PendingResolution, provenance)?;
        if record.outcome != Some(outcome) {
            record.outcome = Some(outcome);
            changed = true;
        }
        let resolution_summary = normalize_optional_text(resolution_summary.into());
        if record.resolution_summary != resolution_summary {
            record.resolution_summary = resolution_summary;
            changed = true;
        }
        let handoff_message = normalize_optional_text(handoff_message.into());
        if record.handoff_message != handoff_message {
            record.handoff_message = handoff_message;
            changed = true;
        }
        if record.resolution_operation_id.as_ref() != Some(&resolution_operation_id) {
            record.resolution_operation_id = Some(resolution_operation_id);
            changed = true;
        }
        if record.resolved_at_millis.is_none() {
            record.resolved_at_millis = Some(record.updated_at_millis);
            changed = true;
        }
        Ok(changed)
    }

    pub fn mark_handoff_started(
        &mut self,
        record_id: &ThreadedDecisionRecordId,
        handoff_turn_id: Option<ConversationTurnId>,
        provenance: MutationProvenance,
    ) -> Result<bool, ThreadedDecisionStateError> {
        let record = self.record_mut(record_id)?;
        let mut changed = record.transition(ThreadedDecisionStatus::HandoffStarted, provenance)?;
        if let Some(handoff_turn_id) = handoff_turn_id
            && record.handoff_turn_id.as_ref() != Some(&handoff_turn_id)
        {
            record.handoff_turn_id = Some(handoff_turn_id);
            changed = true;
        }
        Ok(changed)
    }

    pub fn mark_checklist_updated(
        &mut self,
        record_id: &ThreadedDecisionRecordId,
        handoff_turn_id: ConversationTurnId,
        provenance: MutationProvenance,
    ) -> Result<bool, ThreadedDecisionStateError> {
        let record = self.record_mut(record_id)?;
        let mut changed =
            record.transition(ThreadedDecisionStatus::ChecklistUpdated, provenance)?;
        if record.handoff_turn_id.as_ref() != Some(&handoff_turn_id) {
            record.handoff_turn_id = Some(handoff_turn_id);
            changed = true;
        }
        Ok(changed)
    }

    pub fn mark_archive_pending(
        &mut self,
        record_id: &ThreadedDecisionRecordId,
        archive_operation_id: ThreadedDecisionOperationId,
        provenance: MutationProvenance,
    ) -> Result<bool, ThreadedDecisionStateError> {
        let recorded_at_millis = provenance.recorded_at_millis();
        let record = self.record_mut(record_id)?;
        let mut changed = record.transition(ThreadedDecisionStatus::ArchivePending, provenance)?;
        if record.archive_operation_id.as_ref() != Some(&archive_operation_id) {
            record.archive_operation_id = Some(archive_operation_id.clone());
            changed = true;
        }
        let archive_status =
            ThreadedDecisionArchiveStatus::pending(archive_operation_id, recorded_at_millis);
        if record.archive_status != archive_status {
            record.archive_status = archive_status;
            changed = true;
        }
        Ok(changed)
    }

    pub fn mark_closed(
        &mut self,
        record_id: &ThreadedDecisionRecordId,
        provenance: MutationProvenance,
    ) -> Result<bool, ThreadedDecisionStateError> {
        let recorded_at_millis = provenance.recorded_at_millis();
        let record = self.record_mut(record_id)?;
        let mut changed = record.transition(ThreadedDecisionStatus::Closed, provenance)?;
        let archive_status = ThreadedDecisionArchiveStatus::archived(
            record.archive_operation_id.clone(),
            recorded_at_millis,
        );
        if record.archive_status != archive_status {
            record.archive_status = archive_status;
            changed = true;
        }
        Ok(changed)
    }

    pub fn mark_archive_failed(
        &mut self,
        record_id: &ThreadedDecisionRecordId,
        failure_message: impl Into<String>,
        provenance: MutationProvenance,
    ) -> Result<bool, ThreadedDecisionStateError> {
        let recorded_at_millis = provenance.recorded_at_millis();
        let record = self.record_mut(record_id)?;
        let mut changed = record.transition(ThreadedDecisionStatus::ArchiveFailed, provenance)?;
        let archive_status = ThreadedDecisionArchiveStatus::failed(
            record.archive_operation_id.clone(),
            normalize_optional_text(failure_message.into()),
            recorded_at_millis,
        );
        if record.archive_status != archive_status {
            record.archive_status = archive_status;
            changed = true;
        }
        Ok(changed)
    }

    pub fn supersede_closed_records_for_item(
        &mut self,
        checklist_item_id: &SemanticNodeId,
        superseded_by_record_id: ThreadedDecisionRecordId,
        provenance: MutationProvenance,
    ) -> Result<bool, ThreadedDecisionStateError> {
        let mut changed = false;
        for record in &mut self.records {
            if record.checklist_item_id() != checklist_item_id
                || record.record_id() == &superseded_by_record_id
                || record.status() != ThreadedDecisionStatus::Closed
            {
                continue;
            }
            let supersession = ThreadedDecisionSupersession {
                superseded_by_record_id: superseded_by_record_id.clone(),
                superseded_at_millis: provenance.recorded_at_millis(),
                provenance: provenance.clone(),
            };
            if record.supersession.as_ref() != Some(&supersession) {
                record.supersession = Some(supersession);
                changed = true;
            }
            changed |= record.transition(ThreadedDecisionStatus::Superseded, provenance.clone())?;
        }
        Ok(changed)
    }

    pub fn reconcile_references(
        &mut self,
        graph: &SemanticGraph,
        workspace_state: &WorkspaceConversationState,
        provenance: MutationProvenance,
    ) -> bool {
        let mut changed = false;
        for record in &mut self.records {
            if record.status() == ThreadedDecisionStatus::Invalidated {
                continue;
            }
            let reason = if graph.node(record.checklist_item_id()).is_none() {
                Some(ThreadedDecisionInvalidationReason::MissingChecklistItem)
            } else if workspace_state
                .thread_registration(record.parent_thread_id())
                .is_none()
            {
                Some(ThreadedDecisionInvalidationReason::MissingParentThread)
            } else if record.child_thread_id().is_some_and(|child_thread_id| {
                workspace_state
                    .thread_registration(child_thread_id)
                    .is_none()
            }) {
                Some(ThreadedDecisionInvalidationReason::MissingChildThread)
            } else {
                None
            };

            if let Some(reason) = reason {
                changed |= record.invalidate(reason, provenance.clone());
            }
        }
        changed
    }

    pub fn invalidate_record(
        &mut self,
        record_id: &ThreadedDecisionRecordId,
        reason: ThreadedDecisionInvalidationReason,
        provenance: MutationProvenance,
    ) -> Result<bool, ThreadedDecisionStateError> {
        let record = self.record_mut(record_id)?;
        Ok(record.invalidate(reason, provenance))
    }

    pub fn protected_resolved_checklist_item_ids(&self) -> impl Iterator<Item = &SemanticNodeId> {
        self.records
            .iter()
            .filter(|record| record.protects_resolved_item())
            .map(ThreadedDecisionRecord::checklist_item_id)
    }

    fn record_mut(
        &mut self,
        record_id: &ThreadedDecisionRecordId,
    ) -> Result<&mut ThreadedDecisionRecord, ThreadedDecisionStateError> {
        self.records
            .iter_mut()
            .find(|record| record.record_id() == record_id)
            .ok_or_else(|| ThreadedDecisionStateError::MissingRecord {
                record_id: record_id.clone(),
            })
    }
}
