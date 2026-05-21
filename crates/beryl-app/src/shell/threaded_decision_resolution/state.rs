use beryl_model::{
    threaded_decision::{ThreadedDecisionRecord, ThreadedDecisionRecordId, ThreadedDecisionStatus},
    workspace::BerylWorkspaceId,
};

use crate::{
    member_thread_inventory::resolved_thread_title,
    threaded_decision_resolution_core::{DecisionHandoffMessageInput, decision_handoff_message},
};

use super::super::ShellView;
use super::QueuedDecisionResolutionJob;

impl ShellView {
    pub(in crate::shell) fn hydrate_ready_decision_resolution_jobs(&mut self) {
        let Some(loaded) = self.loaded_workspace() else {
            return;
        };
        let workspace_id = loaded.workspace.id().clone();
        let record_ids = loaded
            .threaded_decision_state
            .records()
            .iter()
            .filter(|record| {
                record.status() == ThreadedDecisionStatus::PendingResolution
                    || (record.status() == ThreadedDecisionStatus::HandoffStarted
                        && record.handoff_turn_id().is_some())
            })
            .map(|record| record.record_id().clone())
            .collect::<Vec<_>>();
        for record_id in record_ids {
            self.queue_decision_resolution_job(workspace_id.clone(), record_id);
        }
    }

    pub(in crate::shell) fn queue_decision_resolution_job(
        &mut self,
        workspace_id: BerylWorkspaceId,
        record_id: ThreadedDecisionRecordId,
    ) {
        if self
            .pending_decision_resolution_jobs
            .iter()
            .any(|job| job.record_id == record_id)
        {
            return;
        }
        if self
            .decision_resolution_graph_receiver
            .as_ref()
            .is_some_and(|task| task.record_id == record_id)
        {
            return;
        }
        self.pending_decision_resolution_jobs
            .push_back(QueuedDecisionResolutionJob::new(workspace_id, record_id));
    }

    pub(in crate::shell) fn remove_queued_decision_resolution_job(
        &mut self,
        record_id: &ThreadedDecisionRecordId,
    ) {
        self.pending_decision_resolution_jobs
            .retain(|job| &job.record_id != record_id);
    }

    pub(in crate::shell) fn decision_resolution_record(
        &self,
        record_id: &ThreadedDecisionRecordId,
    ) -> Option<ThreadedDecisionRecord> {
        self.loaded_workspace()?
            .threaded_decision_state
            .record(record_id)
            .cloned()
    }

    pub(in crate::shell) fn parent_thread_activation_details(
        &self,
        record: &ThreadedDecisionRecord,
    ) -> Option<(beryl_model::workspace::WorkspaceId, String, bool)> {
        let loaded = self.loaded_workspace()?;
        let registration = loaded
            .workspace_state
            .thread_registration(record.parent_thread_id())?;
        let execution_target = registration.execution_target().clone();
        let title = resolved_thread_title(
            &loaded.workspace_state,
            record.parent_thread_id(),
            &execution_target,
            registration.preview(),
            registration.backend_name(),
            registration.created_at_millis(),
            registration.updated_at_millis(),
        );
        let automatic_title_generation_allowed = loaded
            .workspace_state
            .thread_automatic_title_generation_eligible(record.parent_thread_id());
        Some((execution_target, title, automatic_title_generation_allowed))
    }

    pub(in crate::shell) fn normalized_parent_handoff_message(
        &self,
        record: &ThreadedDecisionRecord,
    ) -> Option<String> {
        let graph = self.conversation_surface()?.graph_overlay().graph();
        let node = graph.node(record.checklist_item_id())?;
        Some(decision_handoff_message(DecisionHandoffMessageInput {
            checklist_item_id: record.checklist_item_id(),
            checklist_item_title: node.title(),
            child_thread_id: record.child_thread_id()?,
            parent_thread_id: record.parent_thread_id(),
            branch_point_turn_id: record.branch_point_turn_id(),
            outcome: record.outcome()?,
            summary: record.resolution_summary().unwrap_or_default(),
            handoff_message: record.handoff_message().unwrap_or_default(),
        }))
    }
}
