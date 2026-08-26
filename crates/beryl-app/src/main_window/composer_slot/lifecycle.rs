use beryl_home_store::{CommandCancellation, HomeStore};
use beryl_state::AssetState;
use syndic_storage::{DraftPieceOperationIdV1, SyndicTimestamp};

use crate::composer_host::{
    ComposerHostAutosaveAdvance, ComposerHostAutosaveCapture, ComposerHostAutosaveInterval,
    ComposerHostAutosaveSettingsCompletion, ComposerHostAutosaveTimer, ComposerHostFlushAdmission,
    ComposerHostFlushAdvance, ComposerHostFlushCapture, ComposerHostFlushPurpose,
    ComposerHostFlushTicket, ComposerHostMarkerSealAuthority, ComposerHostPublicationTicket,
};
use crate::composer_marker_seal::DraftMarkerSealService;

use super::{
    MainWindowComposerDispatchError, MainWindowComposerSelectionIdentity, MainWindowComposerSlot,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MainWindowComposerAutosaveCaptureRequirement {
    Clean,
    UnchangedMarkers,
    ChangedMarkers,
}

impl MainWindowComposerSlot {
    pub fn selected_autosave_timer(
        &self,
        selection: MainWindowComposerSelectionIdentity,
    ) -> Result<Option<ComposerHostAutosaveTimer>, MainWindowComposerDispatchError> {
        let selected = self.selected_ref(selection)?;
        Ok(selected.host.autosave_timer())
    }

    pub fn selected_autosave_publication(
        &self,
        selection: MainWindowComposerSelectionIdentity,
    ) -> Result<Option<ComposerHostPublicationTicket>, MainWindowComposerDispatchError> {
        let selected = self.selected_ref(selection)?;
        Ok(selected
            .host
            .lifecycle_diagnostics()
            .joined_publication_ticket())
    }

    pub fn selected_autosave_capture_requirement(
        &self,
        selection: MainWindowComposerSelectionIdentity,
    ) -> Result<MainWindowComposerAutosaveCaptureRequirement, MainWindowComposerDispatchError> {
        let selected = self.selected_ref(selection)?;
        if !selected.draft_state.is_dirty() {
            return Ok(MainWindowComposerAutosaveCaptureRequirement::Clean);
        }
        let adopted = selected.draft_state.adopted().root().marker_commitment();
        let published = selected.draft_state.published().root().marker_commitment();
        Ok(if adopted == published {
            MainWindowComposerAutosaveCaptureRequirement::UnchangedMarkers
        } else {
            MainWindowComposerAutosaveCaptureRequirement::ChangedMarkers
        })
    }

    pub fn publish_selected_autosave_interval(
        &mut self,
        selection: MainWindowComposerSelectionIdentity,
        settings_generation: u64,
        interval: ComposerHostAutosaveInterval,
    ) -> Result<ComposerHostAutosaveSettingsCompletion, MainWindowComposerDispatchError> {
        let selected = self.selected_mut(selection)?;
        Ok(selected.host.publish_autosave_interval(
            selection.binding(),
            settings_generation,
            interval,
        )?)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn fire_selected_autosave(
        &mut self,
        store: &HomeStore,
        selection: MainWindowComposerSelectionIdentity,
        timer: ComposerHostAutosaveTimer,
        assets: AssetState,
        marker_seals: &DraftMarkerSealService,
        operation_id: DraftPieceOperationIdV1,
        marker_authority: Option<ComposerHostMarkerSealAuthority>,
        published_at: SyndicTimestamp,
        cancellation: &CommandCancellation,
    ) -> Result<ComposerHostAutosaveCapture, MainWindowComposerDispatchError> {
        let selected = self.selected_mut(selection)?;
        let outcome = selected.host.fire_autosave(
            store,
            timer,
            assets,
            marker_seals,
            operation_id,
            marker_authority,
            published_at,
            cancellation,
        )?;
        if matches!(outcome, ComposerHostAutosaveCapture::Captured(_)) {
            selected.dispatcher.publication_capture = Some(selected.draft_state.adopted());
        }
        Ok(outcome)
    }

    pub fn advance_selected_autosave(
        &mut self,
        store: &HomeStore,
        selection: MainWindowComposerSelectionIdentity,
        ticket: ComposerHostPublicationTicket,
    ) -> Result<ComposerHostAutosaveAdvance, MainWindowComposerDispatchError> {
        let selected = self.selected_mut(selection)?;
        let outcome = selected.host.advance_autosave(store, ticket)?;
        if matches!(outcome, ComposerHostAutosaveAdvance::Saved { .. }) {
            publish_capture(selected)?;
        } else if matches!(
            outcome,
            ComposerHostAutosaveAdvance::Unsatisfied(_) | ComposerHostAutosaveAdvance::Stale
        ) {
            selected.dispatcher.publication_capture = None;
        }
        Ok(outcome)
    }

    pub fn selected_autosave_publication_ready(
        &mut self,
        selection: MainWindowComposerSelectionIdentity,
        ticket: ComposerHostPublicationTicket,
    ) -> Result<bool, MainWindowComposerDispatchError> {
        Ok(self
            .selected_mut(selection)?
            .host
            .publication_execution_ready(ticket)?)
    }

    pub fn begin_selected_flush(
        &mut self,
        selection: MainWindowComposerSelectionIdentity,
        purpose: ComposerHostFlushPurpose,
    ) -> Result<ComposerHostFlushAdmission, MainWindowComposerDispatchError> {
        let selected = self.selected_mut(selection)?;
        Ok(selected.host.begin_flush(purpose)?)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn capture_selected_flush_publication(
        &mut self,
        store: &HomeStore,
        selection: MainWindowComposerSelectionIdentity,
        flush: ComposerHostFlushTicket,
        assets: AssetState,
        marker_seals: &DraftMarkerSealService,
        operation_id: DraftPieceOperationIdV1,
        marker_authority: Option<ComposerHostMarkerSealAuthority>,
        published_at: SyndicTimestamp,
        cancellation: &CommandCancellation,
    ) -> Result<ComposerHostFlushCapture, MainWindowComposerDispatchError> {
        let selected = self.selected_mut(selection)?;
        let outcome = selected.host.capture_flush_publication(
            store,
            flush,
            assets,
            marker_seals,
            operation_id,
            marker_authority,
            published_at,
            cancellation,
        )?;
        if matches!(outcome, ComposerHostFlushCapture::Captured(_)) {
            selected.dispatcher.publication_capture = Some(selected.draft_state.adopted());
        }
        Ok(outcome)
    }

    pub fn capture_selected_flush_disposal(
        &mut self,
        store: &HomeStore,
        selection: MainWindowComposerSelectionIdentity,
        flush: ComposerHostFlushTicket,
        operation_id: DraftPieceOperationIdV1,
        cancellation: &CommandCancellation,
    ) -> Result<ComposerHostFlushCapture, MainWindowComposerDispatchError> {
        let selected = self.selected_mut(selection)?;
        Ok(selected
            .host
            .capture_flush_disposal(store, flush, operation_id, cancellation)?)
    }

    pub fn advance_selected_flush(
        &mut self,
        store: &HomeStore,
        selection: MainWindowComposerSelectionIdentity,
        flush: ComposerHostFlushTicket,
    ) -> Result<ComposerHostFlushAdvance, MainWindowComposerDispatchError> {
        let selected = self.selected_mut(selection)?;
        advance_flush(selected, store, flush)
    }

    pub(super) fn advance_slot_flush(
        &mut self,
        store: &HomeStore,
        flush: ComposerHostFlushTicket,
    ) -> Result<ComposerHostFlushAdvance, super::MainWindowComposerSlotError> {
        let selected = self
            .selected
            .as_mut()
            .ok_or(super::MainWindowComposerSlotError::Disposed)?;
        advance_flush(selected, store, flush).map_err(|error| match error {
            MainWindowComposerDispatchError::Host(error) => {
                super::MainWindowComposerSlotError::Host(error)
            }
            _ => super::MainWindowComposerSlotError::IdentityMismatch,
        })
    }

    fn selected_mut(
        &mut self,
        selection: MainWindowComposerSelectionIdentity,
    ) -> Result<&mut super::state::SelectedComposer, MainWindowComposerDispatchError> {
        self.selected
            .as_mut()
            .filter(|selected| {
                selected.identity == selection
                    && selected.host.binding() == Some(selection.binding())
                    && selected.dispatcher.binding == selection.binding()
            })
            .ok_or(MainWindowComposerDispatchError::StaleSelection)
    }

    fn selected_ref(
        &self,
        selection: MainWindowComposerSelectionIdentity,
    ) -> Result<&super::state::SelectedComposer, MainWindowComposerDispatchError> {
        self.selected
            .as_ref()
            .filter(|selected| {
                selected.identity == selection
                    && selected.host.binding() == Some(selection.binding())
                    && selected.dispatcher.binding == selection.binding()
            })
            .ok_or(MainWindowComposerDispatchError::StaleSelection)
    }
}

fn advance_flush(
    selected: &mut super::state::SelectedComposer,
    store: &HomeStore,
    flush: ComposerHostFlushTicket,
) -> Result<ComposerHostFlushAdvance, MainWindowComposerDispatchError> {
    let outcome = selected.host.advance_flush(store, flush)?;
    if matches!(
        outcome,
        ComposerHostFlushAdvance::Progress(
            crate::composer_host::ComposerHostFlushState::CaptureRequired
                | crate::composer_host::ComposerHostFlushState::DisposalRequired
        ) | ComposerHostFlushAdvance::Satisfied(_)
    ) {
        if selected.dispatcher.publication_capture.is_some() {
            publish_capture(selected)?;
        }
    } else if matches!(
        outcome,
        ComposerHostFlushAdvance::Unsatisfied(_) | ComposerHostFlushAdvance::Stale
    ) {
        selected.dispatcher.publication_capture = None;
    }
    Ok(outcome)
}

fn publish_capture(
    selected: &mut super::state::SelectedComposer,
) -> Result<(), MainWindowComposerDispatchError> {
    let captured = selected
        .dispatcher
        .publication_capture
        .take()
        .ok_or(MainWindowComposerDispatchError::Malformed)?;
    let adopted = selected
        .host
        .binding()
        .ok_or(MainWindowComposerDispatchError::StaleSelection)?;
    let (published_candidate_generation, published_pair) = selected
        .host
        .published_draft()
        .ok_or(MainWindowComposerDispatchError::StaleSelection)?;
    selected
        .draft_state
        .publish(
            captured,
            adopted,
            published_candidate_generation,
            published_pair,
        )
        .map_err(|_| MainWindowComposerDispatchError::StaleSelection)?;
    selected.identity.binding = adopted;
    selected.dispatcher.replace_binding(adopted);
    Ok(())
}
