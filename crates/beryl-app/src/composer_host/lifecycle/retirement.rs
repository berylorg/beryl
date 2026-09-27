use super::*;

#[derive(Debug, Eq, PartialEq)]
pub struct ComposerHostRetiredClose {
    binding: ComposerHostBinding,
    checkpoint: DraftEditorCandidateActivationBindingV1,
    selector: DraftEditorCurrentSelectorV1,
    thread_id: beryl_model::SyndicThreadId,
    close: ComposerHostFlushTicket,
}

impl ComposerHostRetiredClose {
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
            selector: active.durable_selector,
            thread_id: active.thread_id,
            close: ticket,
        })
    }
}
