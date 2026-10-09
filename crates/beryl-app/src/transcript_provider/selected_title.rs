use super::*;

impl TranscriptProviderReader {
    pub(super) fn prepare_selected_title(
        &self,
        request: &TranscriptAttachmentRequest,
        cancelled: &AtomicBool,
    ) -> Result<beryl_state::CatalogResolvedTitle, TranscriptAttachmentError> {
        self.check_request(request, cancelled)?;
        let prepared = self
            .syndic
            .prepare_thread_catalog_summary(&self.home, request.thread_id)
            .map_err(|_| TranscriptAttachmentError::Unavailable)?
            .ok_or(TranscriptAttachmentError::Unavailable)?;
        let summary = match &prepared {
            syndic_storage::ThreadCatalogSummaryPreparation::ExactCurrent(exact) => exact.summary(),
            syndic_storage::ThreadCatalogSummaryPreparation::PreparedReplacement(replacement) => {
                replacement.replacement()
            }
        };
        if summary.thread_id() != request.thread_id {
            return Err(TranscriptAttachmentError::Identity);
        }
        let title = match summary.title() {
            None => beryl_state::CatalogResolvedTitle::absent(),
            Some(title) => match title.source() {
                syndic_storage::ThreadCatalogTitleSource::Generated => {
                    beryl_state::CatalogResolvedTitle::generated(title.text())
                }
                syndic_storage::ThreadCatalogTitleSource::HistoryDerived => {
                    beryl_state::CatalogResolvedTitle::history_derived(title.text())
                }
            }
            .map_err(|_| TranscriptAttachmentError::Capacity)?,
        };
        self.check_request(request, cancelled)?;
        Ok(title)
    }
}
