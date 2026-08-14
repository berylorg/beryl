use super::super::execution_detail::TurnExecutionRecord;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct TranscriptPresentationRowMetrics {
    pub(super) item_count: usize,
    pub(super) text_chars: usize,
}

impl TranscriptPresentationRowMetrics {
    pub(super) fn from_turn(turn: &TurnExecutionRecord) -> Self {
        Self {
            item_count: turn.item_count(),
            text_chars: turn.text_char_count(),
        }
    }
}
