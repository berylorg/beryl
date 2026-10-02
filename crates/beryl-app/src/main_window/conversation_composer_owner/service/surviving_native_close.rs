use super::*;

impl MainWindowConversationComposerService {
    pub(crate) fn rebind_surviving_native_close(
        &self,
        state: &beryl_state::BerylState,
        evidence: &beryl_state::SessionWindowRemovalEvidence,
        close: crate::main_window::MainWindowConversationComposerCloseTicket,
    ) -> Result<
        (
            crate::main_window::MainWindowConversationComposerCloseTicket,
            beryl_state::SessionWindowRecord,
        ),
        String,
    > {
        if !self.window_close_is_current(close) {
            return Err("surviving native close service is not quiescent".into());
        }
        let mut gate = self
            .window_close
            .lock()
            .map_err(|_| "surviving native close gate lock failed")?;
        if *gate != Some(close) {
            return Err("surviving native close gate changed".into());
        }
        let mut slot = self
            .slot
            .lock()
            .map_err(|_| "surviving native close slot lock failed")?;
        let result = slot.rebind_surviving_native_close(&self.store, state, evidence, close)?;
        *gate = Some(result.0);
        Ok(result)
    }
}
