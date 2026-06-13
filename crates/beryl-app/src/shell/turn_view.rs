use super::{
    execution_detail::ExecutionDetailState,
    transcript_history::TranscriptHistoryWindow,
    transcript_presentation::TranscriptPresentationState,
    transcript_viewport::{TranscriptViewportMode, TranscriptViewportState},
    virtual_list::ListState,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TranscriptTurnNumberingSnapshot {
    current: Option<usize>,
    total: Option<usize>,
}

impl TranscriptTurnNumberingSnapshot {
    pub(crate) fn current(self) -> Option<usize> {
        self.current
    }

    pub(crate) fn total(self) -> Option<usize> {
        self.total
    }
}

pub(crate) fn transcript_turn_numbering_snapshot(
    selected_thread_id: Option<&str>,
    execution_details: &ExecutionDetailState,
    history_window: &TranscriptHistoryWindow,
    presentation: &TranscriptPresentationState,
    viewport: &TranscriptViewportState,
    list_state: &ListState,
) -> TranscriptTurnNumberingSnapshot {
    let Some(thread_id) = selected_thread_id else {
        return TranscriptTurnNumberingSnapshot::default();
    };
    let loaded_backend_turn_count = execution_details.backend_turn_count_for_thread(thread_id);
    let total = (history_window.selected_thread_turn_total_is_exact()
        && loaded_backend_turn_count > 0)
        .then_some(loaded_backend_turn_count);
    let current = transcript_view_current_turn_number(
        history_window,
        presentation,
        viewport,
        list_state,
        loaded_backend_turn_count,
        total,
    );
    TranscriptTurnNumberingSnapshot { current, total }
}

fn transcript_view_current_turn_number(
    history_window: &TranscriptHistoryWindow,
    presentation: &TranscriptPresentationState,
    viewport: &TranscriptViewportState,
    list_state: &ListState,
    loaded_backend_turn_count: usize,
    total: Option<usize>,
) -> Option<usize> {
    if list_state.viewport_ends_in_virtual_trailing_space() {
        return total;
    }

    if loaded_backend_turn_count == 0 || !history_window.oldest_source_position_known() {
        return None;
    }
    if let Some(source_turn_index) = streamed_view_current_source_turn_index(viewport, presentation)
    {
        return (source_turn_index < loaded_backend_turn_count).then_some(source_turn_index + 1);
    }
    let visible_range = list_state.visible_range();
    if visible_range.is_empty() {
        return None;
    }
    let viewport_bottom_row_index = visible_range.end.checked_sub(1)?;
    let source_turn_index = presentation.source_turn_index_at(viewport_bottom_row_index)?;
    if source_turn_index >= loaded_backend_turn_count {
        return None;
    }
    Some(source_turn_index + 1)
}

fn streamed_view_current_source_turn_index(
    viewport: &TranscriptViewportState,
    presentation: &TranscriptPresentationState,
) -> Option<usize> {
    let TranscriptViewportMode::Streamed(anchor) = viewport.mode() else {
        return None;
    };
    let row_index = anchor
        .turn
        .row_identity
        .as_deref()
        .and_then(|identity| presentation.row_index_for_identity(identity))
        .unwrap_or(anchor.turn.turn_index);
    presentation.source_turn_index_at(row_index)
}
