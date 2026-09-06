use super::*;
use crate::composer_host::{
    ComposerHostFlushCapture, ComposerHostFlushState, ComposerHostFlushTicket,
};
use crate::composer_marker_seal::DraftMarkerSealService;
use crate::main_window::conversation_composer_mount::autosave;
use crate::main_window::{
    MainWindowComposerAutosaveCaptureRequirement, MainWindowComposerDisposalAdvance,
    MainWindowConversationComposerMount,
};

impl MainWindowConversationComposerService {
    pub(in crate::main_window) fn advance_native_lineage_disposal(
        &self,
        selection: MainWindowComposerSelectionIdentity,
        flush: ComposerHostFlushTicket,
        release: MainWindowComposerWidgetRelease,
        assets: beryl_state::AssetState,
        seals: &DraftMarkerSealService,
        cancellation: &CommandCancellation,
    ) -> Result<
        (
            MainWindowComposerDisposalAdvance,
            Option<ComposerHostFlushCapture>,
            Option<MainWindowComposerSelectionIdentity>,
        ),
        String,
    > {
        #[cfg(feature = "test-faults")]
        if self
            .test_fail_next_native_lineage_disposal_advance
            .swap(false, Ordering::AcqRel)
        {
            return Err("conversation composer disposal advance failed for test".to_owned());
        }
        let mut slot = self
            .slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?;
        if !slot.disposal_flush_is_current(selection, flush)
            || !MainWindowConversationComposerMount::native_lineage_successor_is_exact(
                release.selection(),
                selection,
            )
        {
            return Err("native lineage disposal attempt is stale".to_owned());
        }
        let mut advance = slot
            .advance_disposal(&self.store)
            .map_err(|error| format!("native lineage disposal advance failed: {error}"))?;
        let current = slot
            .selected_identity()
            .ok_or_else(|| "native lineage disposal selection is unavailable".to_owned())?;
        if !MainWindowConversationComposerMount::native_lineage_successor_is_exact(
            selection, current,
        ) || !slot.disposal_flush_is_current(current, flush)
        {
            return Err("native lineage disposal selection left its exact lineage".to_owned());
        }
        let capture = match advance {
            MainWindowComposerDisposalAdvance::Progress(
                ComposerHostFlushState::CaptureRequired,
            ) => {
                if cancellation.is_cancelled() {
                    return Ok((
                        MainWindowComposerDisposalAdvance::Failed,
                        None,
                        Some(current),
                    ));
                }
                let marker_authority = match slot
                    .selected_autosave_capture_requirement(current)
                    .map_err(|error| {
                        format!("native lineage capture requirement failed: {error}")
                    })? {
                    MainWindowComposerAutosaveCaptureRequirement::ChangedMarkers => {
                        Some(autosave::fresh_marker_authority()?)
                    }
                    MainWindowComposerAutosaveCaptureRequirement::Clean
                    | MainWindowComposerAutosaveCaptureRequirement::UnchangedMarkers => None,
                };
                Some(
                    slot.capture_selected_flush_publication(
                        &self.store,
                        current,
                        flush,
                        assets,
                        seals,
                        autosave::fresh_piece_operation_id()?,
                        marker_authority,
                        autosave::current_timestamp()?,
                        cancellation,
                    )
                    .map_err(|error| {
                        format!("native lineage publication capture failed: {error}")
                    })?,
                )
            }
            MainWindowComposerDisposalAdvance::Progress(
                ComposerHostFlushState::DisposalRequired,
            ) => Some(
                slot.capture_selected_flush_disposal(
                    &self.store,
                    current,
                    flush,
                    autosave::fresh_piece_operation_id()?,
                    cancellation,
                )
                .map_err(|error| format!("native lineage disposal capture failed: {error}"))?,
            ),
            MainWindowComposerDisposalAdvance::WidgetReleaseRequired(expected) => {
                if expected != current {
                    return Err("native lineage widget release selection is stale".to_owned());
                }
                let release = MainWindowComposerWidgetRelease::new(current);
                advance = slot
                    .complete_disposal_after_widget_release(&self.store, &release)
                    .map_err(|error| {
                        format!("native lineage widget release completion failed: {error}")
                    })?;
                None
            }
            _ => None,
        };
        match capture {
            Some(ComposerHostFlushCapture::Captured(_)) => {
                advance = MainWindowComposerDisposalAdvance::Progress(
                    ComposerHostFlushState::PublicationPending,
                )
            }
            Some(ComposerHostFlushCapture::State(state)) => {
                advance = MainWindowComposerDisposalAdvance::Progress(state)
            }
            Some(ComposerHostFlushCapture::Unsatisfied(_) | ComposerHostFlushCapture::Stale) => {
                advance = MainWindowComposerDisposalAdvance::Failed
            }
            _ => {}
        }
        Ok((advance, capture, slot.selected_identity()))
    }
}
