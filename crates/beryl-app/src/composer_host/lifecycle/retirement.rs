use super::*;

#[derive(Debug, Eq, PartialEq)]
pub struct ComposerHostRetiredClose {
    binding: ComposerHostBinding,
    checkpoint: DraftEditorCandidateActivationBindingV1,
    activation_checkpoint: DraftEditorCandidateActivationBindingV1,
    settlement_custody_capacity: std::num::NonZeroUsize,
    selector: DraftEditorCurrentSelectorV1,
    thread_id: beryl_model::SyndicThreadId,
    close: ComposerHostFlushTicket,
}

impl ComposerHostRetiredClose {
    pub fn rebind_candidate(
        self,
        access: &beryl_home_store::HomeCandidateRecoveryAccess<'_>,
        storage: syndic_storage::SyndicStorage,
    ) -> Result<Box<SyndicComposerHost>, (Self, ComposerHostError)> {
        self.reconstruct_candidate(access, storage)
            .map_err(|error| (self, error))
    }

    pub(crate) fn reconstruct_candidate(
        &self,
        access: &beryl_home_store::HomeCandidateRecoveryAccess<'_>,
        storage: syndic_storage::SyndicStorage,
    ) -> Result<Box<SyndicComposerHost>, ComposerHostError> {
        let generation = self
            .binding
            .host_generation()
            .next()
            .ok_or(ComposerHostError::GenerationExhausted)?;
        if !self.saved_checkpoint_matches_candidate(access, &storage)? {
            return Err(ComposerHostError::LifecycleBlocked);
        }
        let binding = ComposerHostBinding::new(
            access.home_id(),
            access.generation(),
            generation,
            self.checkpoint,
            self.binding.presentation_generation(),
        );
        let mut host = Box::new(SyndicComposerHost::with_settlement_custody_capacity(
            storage,
            self.settlement_custody_capacity,
        ));
        host.active = Some(Box::new(super::super::ActiveComposerHost {
            binding,
            storage_candidate: self.checkpoint,
            activation_candidate: self.activation_checkpoint,
            thread_id: self.thread_id,
            initial_responses: Vec::new(),
            unavailable: false,
            durable_selector: self.selector,
            published_candidate_generation: self.checkpoint.candidate_generation(),
            published_pair: syndic_storage::DraftRootHistoryPairV1::new(
                self.selector.root(),
                self.selector.history(),
            ),
            session_disposed: false,
        }));
        host.last_generation = Some(generation);
        host.begin_flush(ComposerHostFlushPurpose::WindowClose)?;
        Ok(host)
    }

    pub fn saved_checkpoint_matches_candidate(
        &self,
        access: &beryl_home_store::HomeCandidateRecoveryAccess<'_>,
        storage: &syndic_storage::SyndicStorage,
    ) -> Result<bool, ComposerHostError> {
        if access.home_id() != self.binding.home_id() {
            return Err(ComposerHostError::ForeignHome {
                expected: self.binding.home_id(),
                actual: access.home_id(),
            });
        }
        if access.generation() == self.binding.home_generation() {
            return Err(ComposerHostError::LifecycleBlocked);
        }
        Ok(storage.draft_editor_candidate_is_saved_candidate(
            access,
            self.checkpoint,
            self.selector,
        )?)
    }

    pub const fn binding(&self) -> ComposerHostBinding {
        self.binding
    }

    pub const fn checkpoint(&self) -> DraftEditorCandidateActivationBindingV1 {
        self.checkpoint
    }

    pub const fn selector(&self) -> DraftEditorCurrentSelectorV1 {
        self.selector
    }

    pub const fn thread_id(&self) -> beryl_model::SyndicThreadId {
        self.thread_id
    }

    pub const fn close_ticket(&self) -> ComposerHostFlushTicket {
        self.close
    }
}

impl SyndicComposerHost {
    pub fn retire_clean_window_close(
        self: Box<Self>,
        ticket: ComposerHostFlushTicket,
    ) -> Result<ComposerHostRetiredClose, Box<Self>> {
        let ready = self.lifecycle.close_ticket == Some(ticket)
            && !self.lifecycle.service_disposed
            && self.lifecycle.autosave.is_none()
            && self.lifecycle.timer.is_none()
            && self.pending.is_empty()
            && self.settlement_custody_in_use() == 0
            && self.publication.lane.is_none()
            && !self.submission_pending()
            && self.flush_state(ticket).ok() == Some(ComposerHostFlushState::CloseReady);
        let Some(active) = self.active.as_ref().filter(|active| {
            ready
                && !active.unavailable
                && !active.session_disposed
                && active.binding.home_id() == ticket.home_id
                && active.binding.home_generation() == ticket.home_generation
                && active.binding.host_generation() == ticket.host_generation
        }) else {
            return Err(self);
        };
        Ok(ComposerHostRetiredClose {
            binding: active.binding,
            checkpoint: active.storage_candidate,
            activation_checkpoint: active.activation_candidate,
            settlement_custody_capacity: std::num::NonZeroUsize::new(
                self.settlement_custody_capacity,
            )
            .unwrap(),
            selector: active.durable_selector,
            thread_id: active.thread_id,
            close: ticket,
        })
    }
}
