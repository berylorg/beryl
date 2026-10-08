use super::*;

#[derive(Clone, Copy)]
pub(crate) struct ComposerHostSelectionSave {
    binding: ComposerHostBinding,
    candidate: DraftEditorCandidateActivationBindingV1,
    selector: DraftEditorCurrentSelectorV1,
    flush: ComposerHostFlushTicket,
}

impl ComposerHostSelectionSave {
    pub(crate) fn binding(self) -> ComposerHostBinding { self.binding }
    pub(crate) fn checkpoint(self) -> DraftEditorCandidateActivationBindingV1 { self.candidate }
    pub(crate) fn selector(self) -> DraftEditorCurrentSelectorV1 { self.selector }
    pub(crate) fn flush_ticket(self) -> ComposerHostFlushTicket {
        self.flush
    }
}

impl SyndicComposerHost {
    pub(crate) fn validate_failed_selection_save(
        &self,
        saved: ComposerHostSelectionSave,
    ) -> Result<(), ComposerHostError> {
        let active = self.active.as_ref().ok_or(ComposerHostError::OldBinding)?;
        if active.binding != saved.binding
            || active.storage_candidate != saved.candidate
            || active.durable_selector != saved.selector
            || self.lifecycle.barrier_generation != saved.flush.barrier_generation
            || !self.lifecycle.barrier_matches(saved.flush)
            || self.publication.lane.is_some()
            || self.is_dirty()
            || self.live_operation_pending()
            || active.session_disposed
        {
            return Err(ComposerHostError::LifecycleBlocked);
        }
        Ok(())
    }
    pub(crate) fn qualify_selection_save(
        &mut self,
        store: &HomeStore,
        flush: ComposerHostFlushTicket,
    ) -> Result<ComposerHostSelectionSave, ComposerHostError> {
        let barrier = self
            .lifecycle
            .barrier
            .as_ref()
            .ok_or(ComposerHostError::LifecycleBlocked)?;
        let active = self.active.as_ref().ok_or(ComposerHostError::OldBinding)?;
        if barrier.ticket != flush
            || barrier.purpose != ComposerHostFlushPurpose::ThreadSwitch
            || barrier.publication.is_some()
            || barrier.disposal.is_some()
            || barrier.saved_checkpoint != Some((active.storage_candidate, active.durable_selector))
            || self.publication.lane.is_some()
            || self.is_dirty()
            || self.live_operation_pending()
            || active.session_disposed
            || active.unavailable
        {
            return Err(ComposerHostError::LifecycleBlocked);
        }
        let saved = ComposerHostSelectionSave {
            binding: active.binding,
            candidate: active.storage_candidate,
            selector: active.durable_selector,
            flush,
        };
        self.validate_selection_save(store, saved)?;
        Ok(saved)
    }

    pub(crate) fn validate_selection_save(
        &self,
        store: &HomeStore,
        saved: ComposerHostSelectionSave,
    ) -> Result<(), ComposerHostError> {
        let active = self.active.as_ref().ok_or(ComposerHostError::OldBinding)?;
        super::super::request::validate_store(saved.binding, store)?;
        if active.binding != saved.binding
            || active.storage_candidate != saved.candidate
            || active.durable_selector != saved.selector
            || self.lifecycle.barrier_generation != saved.flush.barrier_generation
            || !self.lifecycle.barrier_matches(saved.flush)
            || saved.flush.host_generation != active.binding.host_generation()
            || self.publication.lane.is_some()
            || self.is_dirty()
            || self.live_operation_pending()
            || active.session_disposed
            || active.unavailable
            || !self.storage.draft_editor_candidate_is_saved(
                store,
                saved.candidate,
                saved.selector,
            )?
        {
            return Err(ComposerHostError::LifecycleBlocked);
        }
        Ok(())
    }

    pub(crate) fn release_selection_save(
        &mut self,
        store: &HomeStore,
        saved: ComposerHostSelectionSave,
    ) -> Result<(), ComposerHostError> {
        self.validate_selection_save(store, saved)?;
        if !self.lifecycle.barrier_matches(saved.flush) {
            return Err(ComposerHostError::LifecycleBlocked);
        }
        self.lifecycle.barrier = None;
        Ok(())
    }
}

impl ComposerHostSelectionSave {
    pub(crate) fn flush(self) -> ComposerHostFlushTicket {
        self.flush
    }
}
