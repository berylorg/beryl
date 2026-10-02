use super::*;
use crate::main_window::MainWindowConversationComposerCloseTicket;

impl MainWindowComposerSlot {
    pub(in crate::main_window) fn rebind_surviving_native_close(
        &mut self,
        home: &HomeStore,
        state: &beryl_state::BerylState,
        evidence: &beryl_state::SessionWindowRemovalEvidence,
        close: MainWindowConversationComposerCloseTicket,
    ) -> Result<
        (
            MainWindowConversationComposerCloseTicket,
            beryl_state::SessionWindowRecord,
        ),
        String,
    > {
        let previous = close.selection();
        if !self.window_close_is_current(close)
            || self.pending.is_some()
            || self.disposed
            || evidence.window().window_id() != previous.window_id()
            || evidence.window().selected_thread() != Some(previous.claim())
            || home.home_id() != previous.binding().home_id()
            || home.health().generation() != Some(previous.binding().home_generation())
        {
            return Err("surviving native close lost its original resident correspondence".into());
        }
        let session = state.session();
        let classified = session
            .classify_window_removal(home, evidence)
            .map_err(|e| e.to_string())?;
        if !matches!(
            classified,
            beryl_state::SessionWindowRemovalState::Original
                | beryl_state::SessionWindowRemovalState::Recovered
        ) {
            return Err("surviving native close membership is not restored".into());
        }
        let bootstrap = session
            .minimal_bootstrap(home)
            .map_err(|e| e.to_string())?
            .ok_or("surviving native close session is unavailable")?;
        let record = bootstrap
            .windows()
            .iter()
            .find(|window| window.window_id() == previous.window_id())
            .cloned()
            .ok_or("surviving native close window is unavailable")?;
        let claim = record
            .selected_thread()
            .ok_or("surviving native close selected claim is unavailable")?;
        let paired = session
            .window_claim_catalog_source(home, previous.window_id())
            .map_err(|e| e.to_string())?;
        if claim.thread_id() != previous.claim().thread_id()
            || claim.generation() != previous.claim().generation()
            || !paired.claim().is_some_and(|paired| {
                paired.thread_id() == claim.thread_id()
                    && paired.generation() == claim.generation()
                    && paired.revision() == claim.revision()
                    && paired.state() == beryl_state::ThreadClaimState::Active
            })
        {
            return Err("surviving native close restored claim conflicts".into());
        }
        let selected = self.selected.as_mut().unwrap();
        selected.identity.claim = claim;
        let fresh = close.with_recovered_selection(selected.identity);
        self.window_close = Some(fresh);
        Ok((fresh, record))
    }
}
