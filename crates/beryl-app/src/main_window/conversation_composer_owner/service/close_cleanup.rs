use super::*;
use crate::composer_host::{
    ComposerHostFlushCapture, ComposerHostFlushState, ComposerHostFlushTicket,
};
use crate::main_window::{
    MainWindowComposerDisposalAdvance, MainWindowConversationComposerCloseTicket,
};

enum CleanupStep {
    Pending,
    Finished,
}

impl MainWindowConversationComposerService {
    pub(in crate::main_window) fn cleanup_unmounted_window_close(
        self: Arc<Self>,
        ticket: MainWindowConversationComposerCloseTicket,
        flush: Option<ComposerHostFlushTicket>,
        disposing: bool,
        mut disposal_captured: bool,
        executor: BackgroundExecutor,
    ) {
        let timer = executor.clone();
        executor
            .spawn(async move {
                let mut delay = Duration::from_millis(1);
                for _ in 0..32 {
                    match self.cleanup_unmounted_window_close_step(
                        ticket,
                        flush,
                        disposing,
                        &mut disposal_captured,
                    ) {
                        Ok(CleanupStep::Pending) => {}
                        Ok(CleanupStep::Finished) | Err(_) => return,
                    }
                    timer.timer(delay).await;
                    delay = delay.saturating_mul(2).min(Duration::from_millis(100));
                }
            })
            .detach();
    }

    fn cleanup_unmounted_window_close_step(
        &self,
        ticket: MainWindowConversationComposerCloseTicket,
        flush: Option<ComposerHostFlushTicket>,
        disposing: bool,
        disposal_captured: &mut bool,
    ) -> Result<CleanupStep, String> {
        let mut slot = self
            .slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?;
        if !self.window_close_is_current(ticket) {
            return Ok(CleanupStep::Finished);
        }
        if !disposing {
            if (flush.is_some() || slot.window_close_is_current(ticket))
                && !slot
                    .release_window_close_gate(ticket, flush)
                    .map_err(|error| format!("unmounted close release failed: {error}"))?
            {
                return Ok(CleanupStep::Finished);
            }
            self.finish_window_close_gate(ticket);
            return Ok(CleanupStep::Finished);
        }
        if !slot.window_close_is_current(ticket) {
            return Ok(CleanupStep::Finished);
        }
        let flush = flush.ok_or_else(|| "unmounted final close has no flush ticket".to_owned())?;
        match slot.advance_disposal(&self.store) {
            Ok(MainWindowComposerDisposalAdvance::Progress(
                ComposerHostFlushState::DisposalRequired,
            )) if !*disposal_captured => {
                let mut bytes = [0_u8; 16];
                getrandom::fill(&mut bytes)
                    .map_err(|_| "unmounted close disposal identity failed".to_owned())?;
                let selection = slot.selected_identity().unwrap();
                let capture = slot
                    .capture_selected_flush_disposal(
                        &self.store,
                        selection,
                        flush,
                        syndic_storage::DraftPieceOperationIdV1::from_bytes(bytes),
                        &CommandCancellation::new(),
                    )
                    .map_err(|error| format!("unmounted close disposal capture failed: {error}"))?;
                *disposal_captured = true;
                match capture {
                    ComposerHostFlushCapture::Unsatisfied(_) | ComposerHostFlushCapture::Stale => {
                        if slot
                            .release_window_close_gate(ticket, Some(flush))
                            .map_err(|error| format!("unmounted close release failed: {error}"))?
                        {
                            self.finish_window_close_gate(ticket);
                        }
                        Ok(CleanupStep::Finished)
                    }
                    _ => Ok(CleanupStep::Pending),
                }
            }
            Ok(MainWindowComposerDisposalAdvance::WidgetReleaseRequired(_)) => {
                self.finish_window_close_gate(ticket);
                Ok(CleanupStep::Finished)
            }
            Ok(MainWindowComposerDisposalAdvance::Failed) | Err(_) => {
                if slot
                    .release_window_close_gate(ticket, Some(flush))
                    .map_err(|error| format!("unmounted close release failed: {error}"))?
                {
                    self.finish_window_close_gate(ticket);
                }
                Ok(CleanupStep::Finished)
            }
            Ok(MainWindowComposerDisposalAdvance::Disposed) => {
                self.finish_window_close_gate(ticket);
                Ok(CleanupStep::Finished)
            }
            Ok(MainWindowComposerDisposalAdvance::Progress(_))
            | Ok(MainWindowComposerDisposalAdvance::ReconciliationPending) => {
                Ok(CleanupStep::Pending)
            }
        }
    }
}
