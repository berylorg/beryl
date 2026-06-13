use super::{
    ConversationSurfaceState,
    status_line::{self, CancellableActiveTurn, SelectedTurnHardStopTargets, StatusLineTurnView},
    turn_view::transcript_turn_numbering_snapshot,
};

impl ConversationSurfaceState {
    pub(super) fn status_line_projection(&self) -> status_line::StatusLineProjection {
        let cancellable_active_turn = self.status_line_turn_operation_target();
        let hard_stop_targets =
            self.status_line_hard_stop_targets_for(cancellable_active_turn.as_ref());
        self.status_line
            .projection_with_turn_operations(
                self.selected_thread_id(),
                self.status_line_model_reasoning_available(),
                self.status_line_context_operation_available(),
                self.execution_details.last_turn_state().label(),
                cancellable_active_turn,
                hard_stop_targets,
            )
            .with_turn_view(self.status_line_turn_view())
    }

    fn status_line_turn_view(&self) -> StatusLineTurnView {
        let snapshot = self.transcript_turn_numbering_snapshot();
        StatusLineTurnView::new(snapshot.current(), snapshot.total())
    }

    pub(super) fn status_line_turn_operation_target(&self) -> Option<CancellableActiveTurn> {
        self.selected_cancellable_active_turn()
    }

    pub(super) fn status_line_turn_hard_stop_targets(&self) -> Option<SelectedTurnHardStopTargets> {
        let target = self.status_line_turn_operation_target();
        self.status_line_hard_stop_targets_for(target.as_ref())
    }

    pub(super) fn status_line_hard_stop_targets_for(
        &self,
        target: Option<&CancellableActiveTurn>,
    ) -> Option<SelectedTurnHardStopTargets> {
        self.hard_stop_targets.selected_turn_targets(target)
    }

    fn status_line_model_reasoning_available(&self) -> bool {
        status_line::status_line_model_reasoning_available(
            self.selected_thread_id(),
            self.selected_thread_status.as_ref(),
        )
    }

    fn status_line_context_operation_available(&self) -> bool {
        status_line::status_line_context_operation_available(
            self.selected_thread_id(),
            self.selected_thread_status.as_ref(),
        )
    }

    pub(super) fn transcript_turn_numbering_snapshot(
        &self,
    ) -> super::turn_view::TranscriptTurnNumberingSnapshot {
        transcript_turn_numbering_snapshot(
            self.selected_thread_id(),
            &self.execution_details,
            &self.transcript_history_window,
            &self.transcript_presentation,
            &self.transcript_viewport,
            &self.transcript_list_state,
        )
    }
}
