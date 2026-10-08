use super::*;
use crate::composer_host::{
    ComposerHostFlushAdmission, ComposerHostFlushAdvance, ComposerHostFlushPurpose,
    ComposerHostFlushTicket, ComposerHostSelectionSave,
};

pub(crate) struct MainWindowThreadPredecessorSave {
    service: Arc<MainWindowConversationComposerService>,
    selected: MainWindowComposerSelectionIdentity,
    saved: ComposerHostSelectionSave,
}

impl MainWindowConversationComposerService {
    pub(in crate::main_window) fn begin_thread_predecessor_save(
        &self,
        selected: MainWindowComposerSelectionIdentity,
    ) -> Result<ComposerHostFlushAdmission, String> {
        self.ensure_no_window_close()?;
        let mut slot = self
            .slot
            .lock()
            .map_err(|_| "thread predecessor source lock failed")?;
        if slot.pending_receipt().is_some() {
            return Err("thread predecessor has a pending activation".into());
        }
        slot.begin_selected_flush(selected, ComposerHostFlushPurpose::ThreadSwitch)
            .map_err(|error| error.to_string())
    }
    pub(in crate::main_window) fn advance_thread_predecessor_save(
        &self,
        selected: MainWindowComposerSelectionIdentity,
        flush: ComposerHostFlushTicket,
    ) -> Result<
        (
            ComposerHostFlushAdvance,
            MainWindowComposerSelectionIdentity,
        ),
        String,
    > {
        let mut slot = self
            .slot
            .lock()
            .map_err(|_| "thread predecessor source lock failed")?;
        let advance = slot
            .advance_selected_flush(&self.store, selected, flush)
            .map_err(|error| error.to_string())?;
        let current = slot
            .selected_identity()
            .ok_or("thread predecessor source is missing")?;
        Ok((advance, current))
    }
    #[allow(clippy::too_many_arguments)]
    pub(in crate::main_window) fn capture_thread_predecessor_save(
        &self,
        selected: MainWindowComposerSelectionIdentity,
        flush: ComposerHostFlushTicket,
        assets: beryl_state::AssetState,
        marker_seals: &crate::composer_marker_seal::DraftMarkerSealService,
        operation: syndic_storage::DraftPieceOperationIdV1,
        marker_authority: Option<crate::composer_host::ComposerHostMarkerSealAuthority>,
        timestamp: syndic_storage::SyndicTimestamp,
        cancellation: &CommandCancellation,
    ) -> Result<crate::composer_host::ComposerHostFlushCapture, String> {
        self.slot
            .lock()
            .map_err(|_| "thread predecessor source lock failed")?
            .capture_selected_flush_publication(
                &self.store,
                selected,
                flush,
                assets,
                marker_seals,
                operation,
                marker_authority,
                timestamp,
                cancellation,
            )
            .map_err(|error| error.to_string())
    }
    pub(in crate::main_window) fn qualify_thread_predecessor_save(
        self: &Arc<Self>,
        selected: MainWindowComposerSelectionIdentity,
        flush: ComposerHostFlushTicket,
    ) -> Result<MainWindowThreadPredecessorSave, String> {
        self.ensure_no_window_close()?;
        let saved = self
            .slot
            .lock()
            .map_err(|_| "thread predecessor source lock failed")?
            .qualify_thread_predecessor_save(&self.store, selected, flush)
            .map_err(|error| error.to_string())?;
        Ok(MainWindowThreadPredecessorSave {
            service: self.clone(),
            selected,
            saved,
        })
    }
}

impl MainWindowThreadPredecessorSave {
    pub(crate) fn selected(&self) -> MainWindowComposerSelectionIdentity {
        self.selected
    }
    pub(crate) fn validate(&self) -> Result<(), String> {
        self.service.ensure_no_window_close()?;
        self.service
            .slot
            .lock()
            .map_err(|_| "thread predecessor source lock failed")?
            .validate_thread_predecessor_save(&self.service.store, self.selected, self.saved)
            .map_err(|error| error.to_string())
    }
    pub(crate) fn adopt(
        self,
        receipt: super::super::MainWindowComposerActivationReceipt,
        target: beryl_state::WindowClaimSelection,
    ) -> Result<(), (Self, String)> {
        let result = (|| {
            self.service
                .slot
                .lock()
                .map_err(|_| "thread predecessor source lock failed")?
                .adopt_thread_predecessor_save(
                    &self.service.store,
                    self.selected,
                    self.saved,
                    receipt,
                    target,
                )
                .map_err(|error| error.to_string())
        })();
        result.map_err(|error| (self, error))
    }
    pub(crate) fn release(self, state: &beryl_state::BerylState) -> Result<(), (Self, String)> {
        let result = (|| {
            let source = state
                .session()
                .capture_window_removal(&self.service.store, self.selected.window_id())
                .map_err(|error| error.to_string())?;
            if source.window().selected_thread() != Some(self.selected.claim()) {
                return Err("thread predecessor no longer owns the durable claim".into());
            }
            self.service
                .slot
                .lock()
                .map_err(|_| "thread predecessor source lock failed")?
                .release_thread_predecessor_save(&self.service.store, self.selected, self.saved)
                .map_err(|error| error.to_string())
        })();
        result.map_err(|error| (self, error))
    }
}
