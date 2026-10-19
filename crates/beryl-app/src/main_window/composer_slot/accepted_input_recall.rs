use super::*;
use crate::composer_host::ComposerAcceptedInputHistoryOwner;
use crate::composer_host::{
    ComposerHostRecallAdvance, ComposerHostRecallCompletion, ComposerHostRecallTicket,
};
use gpui_text_input::MutationPositions;
use std::sync::Arc;
use syndic_storage::{ComposerAcceptedInputReferenceV1, DraftComposerBrowseCheckpointV1};

impl MainWindowComposerSlot {
    pub(in crate::main_window) fn begin_accepted_input_recall(
        &mut self,
        store: &HomeStore,
        selection: MainWindowComposerSelectionIdentity,
        source: Option<ComposerAcceptedInputReferenceV1>,
        checkpoint: Option<DraftComposerBrowseCheckpointV1>,
        positions: MutationPositions,
        operation: DraftPieceOperationIdV1,
        capture_checkpoint: bool,
    ) -> Result<ComposerHostRecallTicket, String> {
        if !self.claim_publication_is_current(selection)
            || self.pending.is_some()
            || self.disposal_stage.is_some()
        {
            return Err("Composer history selected owner is no longer available".into());
        }
        let selected = self
            .selected_mut(selection)
            .map_err(|error| error.to_string())?;
        if !selected.dispatcher.is_drained() {
            return Err("Composer history is waiting for admitted editor work".into());
        }
        match (source, checkpoint) {
            (Some(source), None) => selected.host.begin_accepted_input_recall(
                store,
                selection.binding(),
                source,
                positions,
                operation,
                capture_checkpoint,
            ),
            (None, Some(checkpoint)) => selected.host.begin_composer_checkpoint_restore(
                store,
                selection.binding(),
                checkpoint,
                positions,
                operation,
            ),
            _ => return Err("Composer history request has no exact source".into()),
        }
        .map_err(|error| error.to_string())
    }

    pub(in crate::main_window) fn advance_accepted_input_recall(
        &mut self,
        store: &HomeStore,
        selection: MainWindowComposerSelectionIdentity,
        ticket: ComposerHostRecallTicket,
        cancellation: &CommandCancellation,
    ) -> Result<
        (
            MainWindowComposerSelectionIdentity,
            ComposerHostRecallAdvance,
        ),
        String,
    > {
        let assets = self.assets();
        let selected = self
            .selected_mut(selection)
            .map_err(|error| error.to_string())?;
        let advance = selected
            .host
            .advance_accepted_input_recall(store, ticket, &assets, cancellation)
            .map_err(|error| error.to_string())?;
        if let ComposerHostRecallAdvance::Ready(completion) = &advance {
            if selected.draft_state.adopted() != completion.binding() {
                selected
                    .draft_state
                    .adopt(completion.previous_binding(), completion.binding())
                    .map_err(
                        |_| "Composer history adoption no longer matches its exact candidate",
                    )?;
            }
            selected.identity.binding = completion.binding();
            selected.dispatcher.replace_binding(completion.binding());
        }
        Ok((selected.identity, advance))
    }

    pub(in crate::main_window) fn acknowledge_accepted_input_recall(
        &mut self,
        selection: MainWindowComposerSelectionIdentity,
        ticket: ComposerHostRecallTicket,
        completion: &ComposerHostRecallCompletion,
    ) -> Result<(), String> {
        self.selected_mut(selection)
            .map_err(|error| error.to_string())?
            .host
            .acknowledge_accepted_input_recall(ticket, completion.binding())
            .map_err(|error| error.to_string())
    }

    pub(in crate::main_window) fn accepted_input_history(
        &self,
    ) -> Arc<ComposerAcceptedInputHistoryOwner> {
        self.accepted_input_history.clone()
    }

    pub(in crate::main_window) fn bind_accepted_input_history(
        &mut self,
        owner: Arc<ComposerAcceptedInputHistoryOwner>,
    ) -> Result<(), String> {
        owner.bind_window(self.window_id)?;
        if let Some(selected) = self.selected.as_mut() {
            owner.select(
                selected.identity.binding().home_id(),
                selected.identity.claim().thread_id(),
            );
            selected.host.bind_accepted_input_history(owner.clone());
        }
        if let Some(pending) = self.pending.as_mut() {
            pending.host.bind_accepted_input_history(owner.clone());
        }
        self.accepted_input_history = owner;
        Ok(())
    }
}
