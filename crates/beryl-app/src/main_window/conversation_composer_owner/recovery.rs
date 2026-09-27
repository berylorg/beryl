use super::*;
use crate::{
    composer_host::ComposerHostFlushTicket, main_window::MainWindowConversationComposerCloseTicket,
};

pub struct MainWindowComposerRecoverySnapshot {
    selection: MainWindowComposerSelectionIdentity,
    close: MainWindowConversationComposerCloseTicket,
    flush: ComposerHostFlushTicket,
    restoration: RangeRestorationSeed,
}

impl MainWindowComposerRecoverySnapshot {
    pub const fn selection(&self) -> MainWindowComposerSelectionIdentity {
        self.selection
    }

    pub const fn close_ticket(&self) -> MainWindowConversationComposerCloseTicket {
        self.close
    }

    pub const fn flush_ticket(&self) -> ComposerHostFlushTicket {
        self.flush
    }

    pub const fn restoration(&self) -> &RangeRestorationSeed {
        &self.restoration
    }
}

impl MainWindowConversationComposer {
    pub(super) fn bound_service(
        &self,
    ) -> Result<Arc<MainWindowConversationComposerService>, String> {
        self.service
            .clone()
            .ok_or_else(|| "resident composer service is detached for recovery".to_owned())
    }

    pub fn detach_recovery_service(
        &mut self,
        close: MainWindowConversationComposerCloseTicket,
        cx: &mut Context<Self>,
    ) -> Result<Option<Arc<MainWindowConversationComposerService>>, String> {
        if !matches!(
            self.phase,
            MainWindowConversationComposerPhase::RecoveryFenced
        ) || !self.shutdown_interaction_gated
            || !self
                .recovery_snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.close == close)
            || self.window_close != Some(close)
        {
            return Err("resident service detachment requires its exact recovery fence".to_owned());
        }
        if self.active_flight.is_some()
            || self.pending_dispatch.is_some()
            || self.pending_realizer.is_some()
            || !self.activation_seeds.is_empty()
            || self.propagated_clipboard.is_some()
            || self.propagated_cut.is_some()
            || self.pending_marker_metadata.is_some()
            || self.mutation_evidence.is_some()
            || self.pending_marker_removal.is_some()
            || self.image_surface_attachment.is_some()
            || self.startup_release_completion.is_some()
            || !self.input.read(cx).is_quiescent()
        {
            return Err("resident service detachment is waiting for editor work".to_owned());
        }
        Ok(self.service.take())
    }

    pub fn recovery_snapshot(&self) -> Option<&MainWindowComposerRecoverySnapshot> {
        self.recovery_snapshot.as_ref()
    }

    pub(in crate::main_window) fn fence_clean_recovery(
        &mut self,
        close: MainWindowConversationComposerCloseTicket,
        flush: ComposerHostFlushTicket,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        if let Some(snapshot) = &self.recovery_snapshot {
            return if snapshot.close == close && snapshot.flush == flush {
                Ok(true)
            } else {
                Err("resident recovery snapshot belongs to another close".to_owned())
            };
        }
        if !self.shutdown_interaction_gated || self.is_pending_target() {
            return Err("resident recovery requires the shutdown interaction gate".to_owned());
        }
        if !self.window_close_flush_ready(close, cx)?
            || self.pending_realizer.is_some()
            || !self.activation_seeds.is_empty()
            || self.propagated_cut.is_some()
            || self.pending_marker_metadata.is_some()
            || self.mutation_evidence.is_some()
            || self.pending_marker_removal.is_some()
            || self.image_surface_attachment.is_some()
            || !self.input.read(cx).is_quiescent()
        {
            return Ok(false);
        }
        let restoration = self
            .input
            .update(cx, |input, _| {
                input.export_restoration(Some(self.selection.binding().range_history_frontier()))
            })
            .map_err(|error| format!("resident recovery export was rejected: {error:?}"))?;
        self.recovery_snapshot = Some(MainWindowComposerRecoverySnapshot {
            selection: self.selection,
            close,
            flush,
            restoration,
        });
        self.phase = MainWindowConversationComposerPhase::RecoveryFenced;
        self.admitted_positions = None;
        self.scheduled = false;
        self.input
            .update(cx, |input, cx| input.set_enabled(false, cx));
        cx.notify();
        Ok(true)
    }
}
