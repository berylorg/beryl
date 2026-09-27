use super::*;

impl MainWindowConversationComposerMount {
    pub fn detach_interrupted_exit_publication_adapters(
        &mut self,
        ticket: MainWindowConversationComposerCloseTicket,
        cx: &mut Context<Self>,
    ) -> Result<Option<(beryl_state::AssetState, DraftMarkerSealService)>, String> {
        let close = self
            .window_close
            .filter(|close| {
                close.ticket == ticket && ticket.owner == cx.entity_id() && close.recovery_fenced
            })
            .ok_or_else(|| {
                "publication adapter detachment requires the exact recovery fence".to_owned()
            })?;
        if close.state != MainWindowConversationComposerCloseAdvance::Ready
            || close.disposing
            || close.release_requested
            || self.window_close_task.is_some()
            || self.submission.is_active()
            || self.pending_presentation.is_some()
            || self.native_lineage_snapshot.is_some()
            || self.native_lineage_disposal_active
            || self.native_lineage_disposal_task.is_some()
            || self.native_lineage_disposal_flush.is_some()
        {
            return Err("publication adapter work is not drained".to_owned());
        }
        let resident = self
            .contribution
            .as_ref()
            .ok_or_else(|| "publication adapter recovery lost its editor".to_owned())?;
        if !resident
            .read(cx)
            .recovery_snapshot()
            .is_some_and(|snapshot| {
                snapshot.close_ticket() == ticket && Some(snapshot.flush_ticket()) == close.flush
            })
        {
            return Err("publication adapter recovery lost its resident proof".to_owned());
        }
        self.autosave.detach_recovery_adapters()
    }

    pub fn fence_interrupted_exit_resident(
        &mut self,
        ticket: MainWindowConversationComposerCloseTicket,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        let close = self
            .window_close
            .filter(|close| close.ticket == ticket && ticket.owner == cx.entity_id())
            .ok_or_else(|| "resident recovery close ticket is stale".to_owned())?;
        if close.state != MainWindowConversationComposerCloseAdvance::Ready
            || close.disposing
            || close.release_requested
            || self.window_close_task.is_some()
            || self.submission.is_active()
            || self.pending_presentation.is_some()
            || self.native_lineage_snapshot.is_some()
            || self.native_lineage_disposal_active
        {
            return Ok(false);
        }
        let flush = close
            .flush
            .ok_or_else(|| "resident recovery lost its flush ticket".to_owned())?;
        let resident = self
            .contribution
            .clone()
            .ok_or_else(|| "resident recovery lost its editor".to_owned())?;
        if !resident.update(cx, |resident, cx| {
            resident.fence_clean_recovery(ticket, flush, cx)
        })? {
            return Ok(false);
        }
        self.window_close.as_mut().unwrap().recovery_fenced = true;
        cx.notify();
        Ok(true)
    }
}
