use super::*;

impl SyndicComposerHost {
    pub fn window_close_ticket(&self) -> Option<ComposerHostFlushTicket> {
        self.lifecycle.close_ticket
    }

    pub fn authorize_window_close_disposal(
        &mut self,
        store: &HomeStore,
        ticket: ComposerHostFlushTicket,
    ) -> Result<ComposerHostFlushAdvance, ComposerHostError> {
        if self.lifecycle.close_ticket != Some(ticket)
            || !self.lifecycle.barrier_matches(ticket)
            || !self.current_flush_callback(store, ticket)
        {
            return Ok(ComposerHostFlushAdvance::Stale);
        }
        if self
            .lifecycle
            .barrier
            .as_ref()
            .unwrap()
            .close_disposal_authorized
        {
            return Ok(ComposerHostFlushAdvance::Progress(
                ComposerHostFlushState::DisposalRequired,
            ));
        }
        if self.flush_state(ticket)? != ComposerHostFlushState::CloseReady {
            return Err(ComposerHostError::LifecycleBlocked);
        }
        let active = self.active.as_ref().ok_or(ComposerHostError::OldBinding)?;
        if active.unavailable || active.session_disposed || self.publication.lane.is_some() {
            return Err(ComposerHostError::PublicationUnavailable);
        }
        self.lifecycle
            .barrier
            .as_mut()
            .unwrap()
            .close_disposal_authorized = true;
        Ok(ComposerHostFlushAdvance::Progress(
            ComposerHostFlushState::DisposalRequired,
        ))
    }

    pub fn release_window_close(
        &mut self,
        ticket: ComposerHostFlushTicket,
    ) -> Result<bool, ComposerHostError> {
        if self.lifecycle.close_ticket != Some(ticket) {
            return Ok(false);
        }
        if self
            .lifecycle
            .barrier
            .as_ref()
            .is_some_and(|barrier| barrier.ticket != ticket || barrier.close_disposal_authorized)
        {
            return Err(ComposerHostError::LifecycleBlocked);
        }
        if let Some(mut barrier) = self.lifecycle.barrier.take() {
            self.lifecycle.autosave = barrier.publication.take();
        }
        self.lifecycle.close_ticket = None;
        if self.lifecycle.autosave.is_none()
            && self.publication_unavailable().is_none()
            && self.lifecycle.dirty_adoption_seen
            && self.is_dirty()
            && let Some(active) = self.active.as_ref()
            && !active.unavailable
            && !active.session_disposed
        {
            let _ = self.lifecycle.arm(active.binding, Instant::now());
        }
        Ok(true)
    }
}
