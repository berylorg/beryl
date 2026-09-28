use super::*;
use crate::{
    composer_host::ComposerHostFlushTicket, main_window::MainWindowConversationComposerCloseTicket,
};

pub struct MainWindowComposerRecoverySnapshot {
    selection: MainWindowComposerSelectionIdentity,
    close: MainWindowConversationComposerCloseTicket,
    flush: ComposerHostFlushTicket,
    restoration: RangeRestorationSeed,
    retired: Option<crate::main_window::MainWindowComposerRetiredClose>,
}

pub struct MainWindowComposerRecoveryResources {
    pub service: Option<Arc<MainWindowConversationComposerService>>,
    pub clipboard_writer: Option<ComposerClipboardWriter>,
    pub mutation_failure: Option<Arc<crate::composer_host::ComposerHostMutationAdmissionFailure>>,
}

impl MainWindowComposerRecoverySnapshot {
    pub fn retired_close(&self) -> Option<&crate::main_window::MainWindowComposerRetiredClose> {
        self.retired.as_ref()
    }

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
    pub(in crate::main_window) fn take_recovery_retirement(
        &mut self,
        close: MainWindowConversationComposerCloseTicket,
        cx: &Context<Self>,
    ) -> Result<Option<crate::main_window::MainWindowComposerRetiredClose>, String> {
        self.validate_recovery_retirement(close, cx)?;
        Ok(self.recovery_snapshot.as_mut().unwrap().retired.take())
    }

    pub(in crate::main_window) fn recovery_retirement_ready(
        &self,
        close: MainWindowConversationComposerCloseTicket,
        cx: &App,
    ) -> Result<bool, String> {
        self.validate_recovery_retirement(close, cx)?;
        Ok(self.recovery_snapshot.as_ref().unwrap().retired.is_some())
    }

    fn validate_recovery_retirement(
        &self,
        close: MainWindowConversationComposerCloseTicket,
        cx: &App,
    ) -> Result<(), String> {
        self.validate_recovery_detachment(close, cx)?;
        if self.service.is_some()
            || self.clipboard_writer.is_some()
            || self.last_mutation_admission_failure.is_some()
        {
            return Err("resident recovery resources are still attached".to_owned());
        }
        Ok(())
    }

    pub(in crate::main_window) fn accept_recovery_retirement(
        &mut self,
        close: MainWindowConversationComposerCloseTicket,
        retired: crate::main_window::MainWindowComposerRetiredClose,
        cx: &Context<Self>,
    ) -> Result<(), crate::main_window::MainWindowComposerRetiredClose> {
        if self.validate_recovery_retirement(close, cx).is_err() {
            return Err(retired);
        }
        let snapshot = self.recovery_snapshot.as_mut().unwrap();
        if snapshot.retired.is_some()
            || retired.close_ticket() != snapshot.close
            || retired.selection() != snapshot.selection
            || retired.host().close_ticket() != snapshot.flush
            || retired.host().binding() != snapshot.selection.binding()
        {
            return Err(retired);
        }
        snapshot.retired = Some(retired);
        Ok(())
    }

    pub(in crate::main_window) fn detach_recovery_resources(
        &mut self,
        close: MainWindowConversationComposerCloseTicket,
        cx: &mut Context<Self>,
    ) -> Result<MainWindowComposerRecoveryResources, String> {
        self.validate_recovery_detachment(close, cx)?;
        Ok(MainWindowComposerRecoveryResources {
            service: self.service.take(),
            clipboard_writer: self.clipboard_writer.take(),
            mutation_failure: self.last_mutation_admission_failure.take(),
        })
    }

    pub(super) fn write_clipboard(&mut self, text: &str, cx: &mut App) -> ClipboardWriteOutcome {
        match self.clipboard_writer.as_mut() {
            Some(writer) => writer(text, cx),
            None => ClipboardWriteOutcome::Failed,
        }
    }

    #[cfg(feature = "test-faults")]
    pub fn test_set_clipboard_writer(&mut self, writer: ComposerClipboardWriter) {
        assert!(matches!(
            self.phase,
            MainWindowConversationComposerPhase::Live
        ));
        self.clipboard_writer = Some(writer);
    }

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
        self.validate_recovery_detachment(close, cx)?;
        Ok(self.service.take())
    }

    pub fn detach_recovery_clipboard_writer(
        &mut self,
        close: MainWindowConversationComposerCloseTicket,
        cx: &mut Context<Self>,
    ) -> Result<Option<ComposerClipboardWriter>, String> {
        self.validate_recovery_detachment(close, cx)?;
        Ok(self.clipboard_writer.take())
    }

    pub fn detach_recovery_mutation_failure(
        &mut self,
        close: MainWindowConversationComposerCloseTicket,
        cx: &mut Context<Self>,
    ) -> Result<Option<Arc<crate::composer_host::ComposerHostMutationAdmissionFailure>>, String>
    {
        self.validate_recovery_detachment(close, cx)?;
        Ok(self.last_mutation_admission_failure.take())
    }

    #[cfg(feature = "test-faults")]
    pub fn test_set_mutation_admission_failure(
        &mut self,
        failure: Arc<crate::composer_host::ComposerHostMutationAdmissionFailure>,
    ) {
        assert!(matches!(
            self.phase,
            MainWindowConversationComposerPhase::Live
        ));
        self.last_mutation_admission_failure = Some(failure);
    }

    fn validate_recovery_detachment(
        &self,
        close: MainWindowConversationComposerCloseTicket,
        cx: &App,
    ) -> Result<(), String> {
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
            return Err("resident detachment requires its exact recovery fence".to_owned());
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
            return Err("resident detachment is waiting for editor work".to_owned());
        }
        Ok(())
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
            retired: None,
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
