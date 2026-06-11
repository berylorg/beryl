use super::row_model::TranscriptRowPresentationModel;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct TranscriptPresentationRowMetrics {
    pub(super) item_count: usize,
    pub(super) text_chars: usize,
}

impl TranscriptPresentationRowMetrics {
    pub(super) fn from_model(model: &TranscriptRowPresentationModel) -> Self {
        Self {
            item_count: model.item_count(),
            text_chars: model.text_chars(),
        }
    }
}
