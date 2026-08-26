#[cfg(feature = "test-faults")]
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use beryl_home_store::HomeStore;
use gpui_text_input::RangeTextInputRequest;

use super::{MainWindowComposerSelectionIdentity, MainWindowComposerWidgetRelease};
use crate::main_window::MainWindowComposerSlot;

pub struct MainWindowConversationComposerService {
    pub(super) store: Arc<HomeStore>,
    pub(super) slot: Mutex<MainWindowComposerSlot>,
    #[cfg(feature = "test-faults")]
    test_cancel_next_mutation_commit: AtomicBool,
}

impl MainWindowConversationComposerService {
    pub fn new(store: Arc<HomeStore>, slot: MainWindowComposerSlot) -> Self {
        Self {
            store,
            slot: Mutex::new(slot),
            #[cfg(feature = "test-faults")]
            test_cancel_next_mutation_commit: AtomicBool::new(false),
        }
    }

    pub fn selected_identity(&self) -> Option<MainWindowComposerSelectionIdentity> {
        self.slot.lock().ok()?.selected_identity()
    }

    #[cfg(feature = "test-faults")]
    pub fn test_cancel_next_mutation_commit(&self) {
        assert!(
            !self
                .test_cancel_next_mutation_commit
                .swap(true, Ordering::SeqCst)
        );
    }

    #[cfg(feature = "test-faults")]
    pub(super) fn take_test_mutation_commit_cancellation(&self) -> bool {
        self.test_cancel_next_mutation_commit
            .swap(false, Ordering::SeqCst)
    }

    pub(in crate::main_window) fn assets(&self) -> Result<beryl_state::AssetState, String> {
        Ok(self
            .slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .assets())
    }

    pub(in crate::main_window) fn selected_autosave_timer(
        &self,
        selection: MainWindowComposerSelectionIdentity,
    ) -> Result<Option<crate::composer_host::ComposerHostAutosaveTimer>, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .selected_autosave_timer(selection)
            .map_err(|error| error.to_string())
    }

    pub(in crate::main_window) fn selected_autosave_publication(
        &self,
        selection: MainWindowComposerSelectionIdentity,
    ) -> Result<Option<crate::composer_host::ComposerHostPublicationTicket>, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .selected_autosave_publication(selection)
            .map_err(|error| error.to_string())
    }

    pub(in crate::main_window) fn autosave_capture_requirement(
        &self,
        selection: MainWindowComposerSelectionIdentity,
    ) -> Result<super::MainWindowComposerAutosaveCaptureRequirement, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .selected_autosave_capture_requirement(selection)
            .map_err(|error| error.to_string())
    }

    pub(in crate::main_window) fn publish_autosave_interval(
        &self,
        selection: MainWindowComposerSelectionIdentity,
        settings_generation: u64,
        interval: crate::composer_host::ComposerHostAutosaveInterval,
    ) -> Result<crate::composer_host::ComposerHostAutosaveSettingsCompletion, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .publish_selected_autosave_interval(selection, settings_generation, interval)
            .map_err(|error| error.to_string())
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::main_window) fn fire_autosave(
        &self,
        selection: MainWindowComposerSelectionIdentity,
        timer: crate::composer_host::ComposerHostAutosaveTimer,
        assets: beryl_state::AssetState,
        marker_seals: &crate::composer_marker_seal::DraftMarkerSealService,
        operation_id: syndic_storage::DraftPieceOperationIdV1,
        marker_authority: Option<crate::composer_host::ComposerHostMarkerSealAuthority>,
        published_at: syndic_storage::SyndicTimestamp,
        cancellation: &beryl_home_store::CommandCancellation,
    ) -> Result<crate::composer_host::ComposerHostAutosaveCapture, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .fire_selected_autosave(
                &self.store,
                selection,
                timer,
                assets,
                marker_seals,
                operation_id,
                marker_authority,
                published_at,
                cancellation,
            )
            .map_err(|error| error.to_string())
    }

    pub(in crate::main_window) fn advance_autosave(
        &self,
        selection: MainWindowComposerSelectionIdentity,
        ticket: crate::composer_host::ComposerHostPublicationTicket,
    ) -> Result<crate::composer_host::ComposerHostAutosaveAdvance, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .advance_selected_autosave(&self.store, selection, ticket)
            .map_err(|error| error.to_string())
    }

    pub(in crate::main_window) fn autosave_publication_ready(
        &self,
        selection: MainWindowComposerSelectionIdentity,
        ticket: crate::composer_host::ComposerHostPublicationTicket,
    ) -> Result<bool, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .selected_autosave_publication_ready(selection, ticket)
            .map_err(|error| error.to_string())
    }

    pub(in crate::main_window) fn begin_activation(
        &self,
        claim: beryl_state::WindowClaimSelection,
        request: crate::composer_host::ComposerHostActivationRequest,
        retirement_operation_id: syndic_storage::DraftPieceOperationIdV1,
        cancellation: &beryl_home_store::CommandCancellation,
    ) -> Result<super::MainWindowComposerActivationAdvance, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .begin_activation(
                &self.store,
                claim,
                request,
                retirement_operation_id,
                cancellation,
            )
            .map_err(|error| error.to_string())
    }

    pub(in crate::main_window) fn retire_pending(
        &self,
        receipt: super::MainWindowComposerActivationReceipt,
    ) -> Result<super::MainWindowComposerRetirementAdvance, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .retire_pending(&self.store, receipt)
            .map_err(|error| error.to_string())
    }

    pub(in crate::main_window) fn begin_publish(
        &self,
        receipt: super::MainWindowComposerActivationReceipt,
    ) -> Result<crate::composer_host::ComposerHostFlushAdmission, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .begin_publish(&self.store, receipt)
            .map_err(|error| error.to_string())
    }

    pub(in crate::main_window) fn publish_preflight(
        &self,
        receipt: super::MainWindowComposerActivationReceipt,
    ) -> Result<MainWindowComposerSelectionIdentity, String> {
        let slot = self
            .slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?;
        if slot.pending_receipt() != Some(receipt)
            || slot.pending_status() != Some(super::MainWindowComposerPendingStatus::Ready)
            || slot.selected_identity() != Some(receipt.expected_prior())
        {
            return Err("conversation composer publication preflight is stale".to_owned());
        }
        Ok(receipt.expected_prior())
    }

    pub(in crate::main_window) fn advance_publish(
        &self,
        receipt: super::MainWindowComposerActivationReceipt,
    ) -> Result<super::MainWindowComposerPublishAdvance, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .advance_publish(&self.store, receipt)
            .map_err(|error| error.to_string())
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::main_window) fn capture_flush_publication(
        &self,
        selection: MainWindowComposerSelectionIdentity,
        flush: crate::composer_host::ComposerHostFlushTicket,
        assets: beryl_state::AssetState,
        marker_seals: &crate::composer_marker_seal::DraftMarkerSealService,
        operation_id: syndic_storage::DraftPieceOperationIdV1,
        marker_authority: Option<crate::composer_host::ComposerHostMarkerSealAuthority>,
        published_at: syndic_storage::SyndicTimestamp,
        cancellation: &beryl_home_store::CommandCancellation,
    ) -> Result<crate::composer_host::ComposerHostFlushCapture, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .capture_selected_flush_publication(
                &self.store,
                selection,
                flush,
                assets,
                marker_seals,
                operation_id,
                marker_authority,
                published_at,
                cancellation,
            )
            .map_err(|error| error.to_string())
    }

    pub(in crate::main_window) fn capture_flush_disposal(
        &self,
        selection: MainWindowComposerSelectionIdentity,
        flush: crate::composer_host::ComposerHostFlushTicket,
        operation_id: syndic_storage::DraftPieceOperationIdV1,
        cancellation: &beryl_home_store::CommandCancellation,
    ) -> Result<crate::composer_host::ComposerHostFlushCapture, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .capture_selected_flush_disposal(
                &self.store,
                selection,
                flush,
                operation_id,
                cancellation,
            )
            .map_err(|error| error.to_string())
    }

    pub(in crate::main_window) fn complete_publish_after_widget_release(
        &self,
        receipt: super::MainWindowComposerActivationReceipt,
        release: &MainWindowComposerWidgetRelease,
    ) -> Result<super::MainWindowComposerPublishAdvance, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .complete_publish_after_widget_release(&self.store, receipt, release)
            .map_err(|error| error.to_string())
    }

    pub(in crate::main_window) fn begin_disposal(
        &self,
    ) -> Result<crate::composer_host::ComposerHostFlushAdmission, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .begin_disposal(&self.store)
            .map_err(|error| error.to_string())
    }

    pub(in crate::main_window) fn disposal_preflight(
        &self,
    ) -> Result<MainWindowComposerSelectionIdentity, String> {
        let mut slot = self
            .slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?;
        if let Some(receipt) = slot.pending_receipt() {
            match slot
                .retire_pending(&self.store, receipt)
                .map_err(|error| error.to_string())?
            {
                super::MainWindowComposerRetirementAdvance::Retired => {}
                super::MainWindowComposerRetirementAdvance::Pending => {
                    return Err("composer disposal is waiting for pending retirement".to_owned());
                }
                super::MainWindowComposerRetirementAdvance::DepartedFreshBoundary => {
                    return Err("composer disposal pending target departed fresh state".to_owned());
                }
            }
        }
        slot.selected_identity()
            .ok_or_else(|| "conversation composer disposal has no selected slot".to_owned())
    }

    pub(in crate::main_window) fn advance_disposal(
        &self,
    ) -> Result<super::MainWindowComposerDisposalAdvance, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .advance_disposal(&self.store)
            .map_err(|error| error.to_string())
    }

    pub(in crate::main_window) fn complete_disposal_after_widget_release(
        &self,
        release: &MainWindowComposerWidgetRelease,
    ) -> Result<super::MainWindowComposerDisposalAdvance, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .complete_disposal_after_widget_release(&self.store, release)
            .map_err(|error| error.to_string())
    }

    pub(super) fn take_initial_presentation(
        &self,
        selection: MainWindowComposerSelectionIdentity,
    ) -> Result<Box<[crate::composer_host::ComposerHostResponse]>, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .take_selected_initial_presentation(selection)
            .map(|presentation| presentation.into_responses())
            .map_err(|error| error.to_string())
    }

    pub(super) fn release_widget_work(
        &self,
        selection: MainWindowComposerSelectionIdentity,
        requests: Vec<RangeTextInputRequest>,
    ) -> Result<MainWindowComposerWidgetRelease, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .release_selected_widget_work(selection, requests)
            .map_err(|error| error.to_string())
    }
}
