use std::sync::atomic::AtomicBool;

use beryl_model::SyndicThreadId;

use super::PublishedRunningThreadsReader;
use crate::cas_projection::{
    SelectedTranscriptConstructionError, SelectedTranscriptConstructionProgress,
};

impl PublishedRunningThreadsReader {
    pub(crate) fn step_selected_transcript(
        &self,
        thread: SyndicThreadId,
        cancelled: &AtomicBool,
    ) -> Result<SelectedTranscriptConstructionProgress, SelectedTranscriptConstructionError> {
        self.selected_transcript_construction.step_guarded(
            thread,
            cancelled,
            || self.current(),
            || (),
        )
    }
}
