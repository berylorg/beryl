use super::*;

impl SyndicComposerHost {
    pub(crate) fn retire_cancelled_failed_resident_reconstruction(
        &mut self,
        binding: ComposerHostBinding,
        selector: syndic_storage::DraftEditorCurrentSelectorV1,
    ) -> Result<(), String> {
        let active = self
            .active
            .as_ref()
            .ok_or("cancelled resident reconstruction has no host")?;
        if active.binding != binding
            || active.storage_candidate != binding.candidate()
            || active.durable_selector != selector
            || active.published_candidate_generation != binding.candidate().candidate_generation()
            || active.published_pair
                != syndic_storage::DraftRootHistoryPairV1::new(selector.root(), selector.history())
            || active.unavailable
            || active.session_disposed
            || !active.initial_responses.is_empty()
            || !self.pending.is_empty()
            || self.settlement_custody_in_use() != 0
            || self.submission_pending()
            || self.publication.lane.is_some()
            || self.publication.retained.is_some()
            || self.publication.retained_disposal.is_some()
            || self.lifecycle.timer.is_some()
            || self.lifecycle.barrier.is_some()
            || self.lifecycle.close_ticket.is_some()
            || self.lifecycle.autosave.is_some()
            || self.lifecycle.service_disposed
        {
            return Err("cancelled resident reconstruction retains runtime authority".into());
        }
        self.active.take();
        Ok(())
    }
}
