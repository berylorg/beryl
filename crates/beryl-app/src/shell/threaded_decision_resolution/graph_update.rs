use beryl_model::{
    provenance::{MutationProvenance, MutationSource},
    threaded_decision::ThreadedDecisionRecord,
};
use gpui::{Context, Window};
use tracing::warn;

use crate::{
    GraphPatchWriteRequest, threaded_decision_resolution_core::decision_resolution_checklist_patch,
};

use super::super::{
    ShellView, SurfaceNotice,
    graph::GraphMutationUpdate,
    graph_worker::{GraphUpdate, spawn_threaded_decision_graph_patch_worker},
    token_usage_snapshot,
};
use super::{DecisionResolutionGraphTask, QueuedDecisionResolutionJob};

impl ShellView {
    pub(in crate::shell) fn poll_decision_resolution_graph_updates(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(task) = self.decision_resolution_graph_receiver.as_ref() else {
            return false;
        };

        let update = match task.try_recv() {
            Ok(GraphUpdate::MutationFinished(update)) => update,
            Ok(GraphUpdate::ReloadFinished(_)) => return false,
            Err(std::sync::mpsc::TryRecvError::Empty) => return false,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => task.disconnected_update(),
        };
        let record_id = task.record_id.clone();
        let handoff_turn_id = task.handoff_turn_id.clone();
        let provenance = task.provenance.clone();
        self.decision_resolution_graph_receiver = None;

        let failed = matches!(update, GraphMutationUpdate::Failure(_));
        let mut updated = self.finish_graph_mutation_update(update);
        if failed {
            if let Some(surface) = self.conversation_surface_mut() {
                surface.set_notice(SurfaceNotice::new(
                    "Decision checklist update failed",
                    "The parent handoff turn exists, but Beryl could not mark the decision checklist item done.",
                ));
                updated = true;
            }
            self.remove_queued_decision_resolution_job(&record_id);
            return updated;
        }

        let changed = self.workspace_shell_state_mut().is_some_and(|loaded| {
            loaded
                .threaded_decision_state
                .mark_checklist_updated(&record_id, handoff_turn_id.clone(), provenance)
                .unwrap_or_else(|error| {
                    warn!(error = %error, "failed to mark threaded-decision checklist update");
                    false
                })
        });
        if changed {
            self.persist_current_threaded_decision_state();
        }
        if let Some(workspace_id) = self
            .loaded_workspace()
            .map(|loaded| loaded.workspace.id().clone())
        {
            self.queue_decision_archive_job(workspace_id, record_id.clone());
        }
        self.remove_queued_decision_resolution_job(&record_id);
        if let Some(surface) = self.conversation_surface_mut() {
            surface.set_notice(SurfaceNotice::new(
                "Decision resolved",
                "Beryl created the parent handoff turn and marked the decision item done.",
            ));
            updated = true;
        }
        updated |= self.begin_next_ready_decision_archive_cleanup(window, cx);
        updated |= self.begin_next_ready_decision_resolution_handoff(window, cx);
        updated
    }

    pub(super) fn begin_known_handoff_checklist_update(
        &mut self,
        job: QueuedDecisionResolutionJob,
        record: ThreadedDecisionRecord,
        _: &mut Context<Self>,
    ) -> bool {
        if self.graph_receiver.is_some() || self.graph_thread_start_receiver.is_some() {
            return false;
        }
        let Some(handoff_turn_id) = record.handoff_turn_id().cloned() else {
            self.remove_queued_decision_resolution_job(&job.record_id);
            return true;
        };
        let parent_thread_id = record.parent_thread_id().clone();
        let provenance = MutationProvenance::new(
            "beryl",
            token_usage_snapshot::current_unix_millis(),
            MutationSource::conversation_turn(parent_thread_id, handoff_turn_id.clone()),
            Some(100),
        )
        .expect("conversation-turn provenance is valid");

        let Some((workspace_id, patch, no_op_message)) =
            self.loaded_workspace().and_then(|loaded| {
                let workspace_id = loaded.workspace.id().clone();
                let graph = self.conversation_surface()?.graph_overlay().graph();
                let resolution = decision_resolution_checklist_patch(graph, &record, &provenance)?;
                Some((
                    workspace_id,
                    resolution.patch,
                    format!(
                        "Decision branch {} was already marked done.",
                        record.record_id().as_str()
                    ),
                ))
            })
        else {
            self.remove_queued_decision_resolution_job(&job.record_id);
            return true;
        };
        let Some(persistence) = self.workspace_persistence_for_worker() else {
            return false;
        };
        let graph_task = spawn_threaded_decision_graph_patch_worker(
            persistence,
            workspace_id.clone(),
            GraphPatchWriteRequest {
                workspace_id,
                patch,
                expected_base_revision: None,
            },
            None,
            no_op_message,
        );
        self.decision_resolution_graph_receiver = Some(DecisionResolutionGraphTask::new(
            record.record_id().clone(),
            handoff_turn_id,
            provenance,
            graph_task,
        ));
        true
    }
}
