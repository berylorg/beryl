use super::{
    ConversationSurfaceState,
    status_line::{self, CancellableActiveTurn, SelectedTurnHardStopTargets, StatusLineTurnView},
    syndic_transcript::ResidentTranscriptStatusFacts,
};

impl ConversationSurfaceState {
    pub(super) fn status_line_projection(&self) -> status_line::StatusLineProjection {
        self.status_line_projection_with_turn_view(StatusLineTurnView::unknown())
    }

    pub(super) fn status_line_projection_with_transcript_facts(
        &self,
        transcript_status_facts: &ResidentTranscriptStatusFacts,
    ) -> status_line::StatusLineProjection {
        self.status_line_projection_with_turn_view(
            self.status_line_turn_view(transcript_status_facts),
        )
    }

    fn status_line_projection_with_turn_view(
        &self,
        turn_view: StatusLineTurnView,
    ) -> status_line::StatusLineProjection {
        let cancellable_active_turn = self.status_line_turn_operation_target();
        let hard_stop_targets =
            self.status_line_hard_stop_targets_for(cancellable_active_turn.as_ref());
        self.status_line
            .projection_with_turn_operations(
                self.selected_thread_id(),
                self.status_line_model_reasoning_available(),
                self.status_line_context_operation_available(),
                self.active_turn_state.last_turn_state().label(),
                cancellable_active_turn,
                hard_stop_targets,
            )
            .with_turn_view(turn_view)
    }

    fn status_line_turn_view(
        &self,
        transcript_status_facts: &ResidentTranscriptStatusFacts,
    ) -> StatusLineTurnView {
        StatusLineTurnView::new(
            transcript_status_facts.turn_view.current,
            transcript_status_facts.turn_view.total,
        )
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
}
