use super::*;
use crate::composer_host::{
    ComposerHostHistoricalAdvance, ComposerHostHistoricalCompletion, ComposerHostHistoricalTarget,
    ComposerHostHistoricalTicket,
};
use syndic_storage::{DraftSubmissionIntent, SyndicTimestamp};

impl MainWindowComposerSlot {
    pub(in crate::main_window) fn begin_historical_replacement_flush(
        &mut self,
        selection: MainWindowComposerSelectionIdentity,
    ) -> Result<crate::composer_host::ComposerHostFlushAdmission, String> {
        self.selected_mut(selection)
            .map_err(|e| e.to_string())?
            .host
            .begin_historical_replacement_flush(selection.binding())
            .map_err(|e| e.to_string())
    }

    pub(in crate::main_window) fn clear_historical_preflush(
        &mut self,
        selection: MainWindowComposerSelectionIdentity,
    ) {
        if let Ok(selected) = self.selected_mut(selection) {
            selected.host.clear_historical_preflush(selection.binding());
        }
    }
    pub(in crate::main_window) fn selected_submission_intent(
        &self,
        selection: MainWindowComposerSelectionIdentity,
    ) -> Option<DraftSubmissionIntent> {
        self.selected
            .as_ref()
            .filter(|s| s.identity == selection)
            .and_then(|s| s.host.submission_intent())
            .or_else(|| {
                self.pending
                    .as_ref()
                    .filter(|s| s.host.binding() == Some(selection.binding()))
                    .and_then(|s| s.host.submission_intent())
            })
    }

    pub(in crate::main_window) fn begin_historical_replacement(
        &mut self,
        store: &HomeStore,
        selection: MainWindowComposerSelectionIdentity,
        target: Option<ComposerHostHistoricalTarget>,
        operation: DraftPieceOperationIdV1,
        at: SyndicTimestamp,
    ) -> Result<ComposerHostHistoricalTicket, String> {
        if !self.claim_publication_is_current(selection) {
            return Err("Historical replacement selected owner is no longer current".into());
        }
        let selected = self.selected_mut(selection).map_err(|e| e.to_string())?;
        if !selected.dispatcher.is_drained() {
            return Err("Historical replacement is waiting for admitted editor work".into());
        }
        match target {
            Some(target) => selected.host.begin_historical_replacement(
                store,
                selection.binding(),
                target,
                operation,
            ),
            None => selected.host.begin_historical_replacement_cancellation(
                store,
                selection.binding(),
                operation,
                at,
            ),
        }
        .map_err(|e| e.to_string())
    }

    pub(in crate::main_window) fn advance_historical_replacement(
        &mut self,
        store: &HomeStore,
        selection: MainWindowComposerSelectionIdentity,
        ticket: ComposerHostHistoricalTicket,
        at: SyndicTimestamp,
        cancellation: &CommandCancellation,
    ) -> Result<
        (
            MainWindowComposerSelectionIdentity,
            ComposerHostHistoricalAdvance,
        ),
        String,
    > {
        let assets = self.assets();
        let selected = self.selected_mut(selection).map_err(|e| e.to_string())?;
        let advance = selected
            .host
            .advance_historical_replacement(store, ticket, &assets, at, cancellation)
            .map_err(|e| e.to_string())?;
        if let ComposerHostHistoricalAdvance::Ready(completion) = advance {
            if completion.requires_rebind() {
                let (generation, pair) = selected
                    .host
                    .published_draft()
                    .ok_or("Imported composer checkpoint is unavailable")?;
                selected.draft_state = crate::main_window::MainWindowComposerDraftState::new(
                    completion.binding(),
                    generation,
                    pair,
                )
                .map_err(|_| "Imported composer checkpoint is inconsistent")?;
                selected.identity.binding = completion.binding();
                selected.dispatcher.replace_binding(completion.binding());
            } else {
                selected
                    .draft_state
                    .adopt(completion.previous_binding(), completion.binding())
                    .map_err(|_| "Replacement metadata adoption changed the editor candidate")?;
                selected.identity.binding = completion.binding();
                selected.dispatcher.replace_binding(completion.binding());
            }
        }
        Ok((selected.identity, advance))
    }

    pub(in crate::main_window) fn acknowledge_historical_replacement(
        &mut self,
        selection: MainWindowComposerSelectionIdentity,
        ticket: ComposerHostHistoricalTicket,
        completion: ComposerHostHistoricalCompletion,
    ) -> Result<(), String> {
        self.selected_mut(selection)
            .map_err(|e| e.to_string())?
            .host
            .acknowledge_historical_replacement(ticket, completion.binding())
            .map_err(|e| e.to_string())
    }
}
