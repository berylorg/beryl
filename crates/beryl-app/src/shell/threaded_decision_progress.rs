use beryl_model::{
    conversation::{ConversationThreadId, ConversationTurnId},
    provenance::{MutationProvenance, MutationSource},
};
use gpui::Context;

use crate::{GraphPatchWriteRequest, threaded_decision_branch_core::decision_child_progress_patch};

use super::{
    ShellView, SurfaceNotice,
    graph::GraphMutationUpdate,
    graph_worker::{GraphUpdate, spawn_graph_patch_worker},
    token_usage_snapshot,
};

impl ShellView {
    pub(super) fn note_decision_child_turn_started(
        &mut self,
        thread_id: &str,
        turn_id: &str,
        _: &mut Context<Self>,
    ) -> bool {
        if self.decision_child_progress_receiver.is_some() {
            return false;
        }

        let child_thread_id = ConversationThreadId::new(thread_id.to_string());
        let child_turn_id = ConversationTurnId::new(turn_id.to_string());
        let provenance = MutationProvenance::new(
            "beryl",
            token_usage_snapshot::current_unix_millis(),
            MutationSource::conversation_turn(child_thread_id.clone(), child_turn_id.clone()),
            Some(100),
        )
        .expect("conversation-turn provenance is valid");

        let Some((workspace_id, patch, no_op_message)) =
            self.loaded_workspace().and_then(|loaded| {
                let workspace_id = loaded.workspace.id().clone();
                let decisions = &loaded.threaded_decision_state;
                let graph = self.conversation_surface()?.graph_overlay().graph();
                let progress = decision_child_progress_patch(
                    graph,
                    decisions,
                    &child_thread_id,
                    &child_turn_id,
                    &provenance,
                )?;
                Some((
                    workspace_id,
                    progress.patch,
                    format!(
                        "Decision branch {} was already marked in progress.",
                        progress.record_id.as_str()
                    ),
                ))
            })
        else {
            return false;
        };

        let Some(persistence) = self.workspace_persistence_for_worker() else {
            if let Some(surface) = self.conversation_surface_mut() {
                surface.set_notice(SurfaceNotice::new(
                    "Decision progress update failed",
                    "Beryl could not open workspace persistence to mark the decision item in progress.",
                ));
            }
            return true;
        };

        self.decision_child_progress_receiver = Some(spawn_graph_patch_worker(
            persistence,
            workspace_id.clone(),
            GraphPatchWriteRequest {
                workspace_id,
                patch,
                expected_base_revision: None,
            },
            None,
            no_op_message,
        ));
        true
    }

    pub(super) fn poll_decision_child_progress_updates(&mut self) -> bool {
        let Some(receiver) = self.decision_child_progress_receiver.as_ref() else {
            return false;
        };

        let update = match receiver.try_recv() {
            Ok(GraphUpdate::MutationFinished(update)) => update,
            Ok(GraphUpdate::ReloadFinished(_)) => return false,
            Err(std::sync::mpsc::TryRecvError::Empty) => return false,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => receiver.disconnected_update(
                "Beryl lost the background task that was marking the decision item in progress.",
            ),
        };
        self.decision_child_progress_receiver = None;

        let failure_message = match &update {
            GraphMutationUpdate::Failure(failure) => Some(failure.message.clone()),
            GraphMutationUpdate::Commit(_) => None,
        };
        let mut updated = self.finish_graph_mutation_update(update);
        if let Some(message) = failure_message
            && let Some(surface) = self.conversation_surface_mut()
        {
            surface.set_notice(SurfaceNotice::new(
                "Decision progress update failed",
                message,
            ));
            updated = true;
        }
        updated
    }
}
