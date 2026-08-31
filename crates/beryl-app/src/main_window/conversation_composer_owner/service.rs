#[cfg(feature = "test-faults")]
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
#[cfg(feature = "test-faults")]
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
};

use beryl_home_store::HomeStore;
use gpui_text_input::RangeTextInputRequest;

use super::{MainWindowComposerSelectionIdentity, MainWindowComposerWidgetRelease};
use crate::main_window::MainWindowComposerSlot;

#[cfg(feature = "test-faults")]
#[derive(Clone)]
pub struct MainWindowComposerCutPreparationTestRelease(Arc<Mutex<CutPreparationTestGateState>>);

#[cfg(feature = "test-faults")]
#[derive(Clone)]
pub struct MainWindowComposerPendingCompletionTestRelease(
    Arc<Mutex<PendingCompletionTestGateState>>,
);

#[cfg(feature = "test-faults")]
#[derive(Clone)]
pub struct MainWindowComposerPendingDispatchTestRelease(Arc<Mutex<PendingCompletionTestGateState>>);

#[cfg(feature = "test-faults")]
#[derive(Clone)]
pub(super) struct CutPreparationTestGate(Arc<Mutex<CutPreparationTestGateState>>);

#[cfg(feature = "test-faults")]
#[derive(Clone)]
pub(super) struct PendingCompletionTestGate(Arc<Mutex<PendingCompletionTestGateState>>);

#[cfg(feature = "test-faults")]
struct CutPreparationTestGateState {
    released: bool,
    waker: Option<Waker>,
}

#[cfg(feature = "test-faults")]
struct PendingCompletionTestGateState {
    entered: bool,
    released: bool,
    waker: Option<Waker>,
}

#[cfg(feature = "test-faults")]
impl MainWindowComposerCutPreparationTestRelease {
    pub fn release(self) {
        let mut state = self.0.lock().unwrap();
        state.released = true;
        if let Some(waker) = state.waker.take() {
            waker.wake();
        }
    }
}

#[cfg(feature = "test-faults")]
impl MainWindowComposerPendingCompletionTestRelease {
    pub fn is_blocked(&self) -> bool {
        self.0.lock().unwrap().entered
    }

    pub fn release(self) {
        let mut state = self.0.lock().unwrap();
        state.released = true;
        if let Some(waker) = state.waker.take() {
            waker.wake();
        }
    }
}

#[cfg(feature = "test-faults")]
impl MainWindowComposerPendingDispatchTestRelease {
    pub fn is_blocked(&self) -> bool {
        self.0.lock().unwrap().entered
    }

    pub fn release(self) {
        let mut state = self.0.lock().unwrap();
        state.released = true;
        if let Some(waker) = state.waker.take() {
            waker.wake();
        }
    }
}

#[cfg(feature = "test-faults")]
impl Future for CutPreparationTestGate {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = self.0.lock().unwrap();
        if state.released {
            Poll::Ready(())
        } else {
            state.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

#[cfg(feature = "test-faults")]
impl Future for PendingCompletionTestGate {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = self.0.lock().unwrap();
        state.entered = true;
        if state.released {
            Poll::Ready(())
        } else {
            state.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

pub struct MainWindowConversationComposerService {
    pub(super) store: Arc<HomeStore>,
    pub(super) slot: Mutex<MainWindowComposerSlot>,
    #[cfg(feature = "test-faults")]
    test_cancel_next_mutation_commit: AtomicBool,
    #[cfg(feature = "test-faults")]
    test_cut_preparation_gate: Mutex<Option<CutPreparationTestGate>>,
    #[cfg(feature = "test-faults")]
    test_pending_completion_gate: Mutex<Option<PendingCompletionTestGate>>,
    #[cfg(feature = "test-faults")]
    test_pending_dispatch_gate: Mutex<Option<PendingCompletionTestGate>>,
    #[cfg(feature = "test-faults")]
    test_append_impossible_pending_initial_response: AtomicBool,
}

impl MainWindowConversationComposerService {
    #[cfg(feature = "test-faults")]
    pub fn test_submission_diagnostics(
        &self,
    ) -> Result<crate::main_window::MainWindowComposerSlotSubmissionTestDiagnostics, String> {
        Ok(self
            .slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .test_submission_diagnostics())
    }

    pub fn new(store: Arc<HomeStore>, slot: MainWindowComposerSlot) -> Self {
        Self {
            store,
            slot: Mutex::new(slot),
            #[cfg(feature = "test-faults")]
            test_cancel_next_mutation_commit: AtomicBool::new(false),
            #[cfg(feature = "test-faults")]
            test_cut_preparation_gate: Mutex::new(None),
            #[cfg(feature = "test-faults")]
            test_pending_completion_gate: Mutex::new(None),
            #[cfg(feature = "test-faults")]
            test_pending_dispatch_gate: Mutex::new(None),
            #[cfg(feature = "test-faults")]
            test_append_impossible_pending_initial_response: AtomicBool::new(false),
        }
    }

    pub fn selected_identity(&self) -> Option<MainWindowComposerSelectionIdentity> {
        self.slot.lock().ok()?.selected_identity()
    }

    pub fn pending_identity(
        &self,
        receipt: super::MainWindowComposerActivationReceipt,
    ) -> Option<MainWindowComposerSelectionIdentity> {
        self.slot.lock().ok()?.pending_identity(receipt)
    }

    pub(in crate::main_window) fn pending_request_is_admitted(
        &self,
        receipt: super::MainWindowComposerActivationReceipt,
        selection: MainWindowComposerSelectionIdentity,
    ) -> bool {
        self.slot
            .lock()
            .is_ok_and(|slot| slot.pending_request_is_admitted(receipt, selection))
    }

    pub fn pending_receipt(&self) -> Option<super::MainWindowComposerActivationReceipt> {
        self.slot.lock().ok()?.pending_receipt()
    }

    #[cfg(feature = "test-faults")]
    pub fn test_pending_host_request_id(
        &self,
        receipt: super::MainWindowComposerActivationReceipt,
    ) -> Option<u64> {
        self.slot.lock().ok()?.test_pending_host_request_id(receipt)
    }

    #[cfg(feature = "test-faults")]
    pub fn test_arm_activation_after_open_fault(
        &self,
        fault: impl FnOnce(&HomeStore, syndic_storage::SyndicStorage) + Send + 'static,
    ) {
        self.slot
            .lock()
            .unwrap()
            .test_arm_activation_after_open_fault(fault);
    }

    #[cfg(feature = "test-faults")]
    pub fn test_append_impossible_pending_initial_response(&self) {
        assert!(
            !self
                .test_append_impossible_pending_initial_response
                .swap(true, Ordering::SeqCst)
        );
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
    pub fn test_block_next_cut_preparation(&self) -> MainWindowComposerCutPreparationTestRelease {
        let state = Arc::new(Mutex::new(CutPreparationTestGateState {
            released: false,
            waker: None,
        }));
        let mut gate = self.test_cut_preparation_gate.lock().unwrap();
        assert!(
            gate.replace(CutPreparationTestGate(state.clone()))
                .is_none()
        );
        MainWindowComposerCutPreparationTestRelease(state)
    }

    #[cfg(feature = "test-faults")]
    pub(super) fn take_test_cut_preparation_gate(&self) -> Option<CutPreparationTestGate> {
        self.test_cut_preparation_gate.lock().ok()?.take()
    }

    #[cfg(feature = "test-faults")]
    pub(super) fn take_test_mutation_commit_cancellation(&self) -> bool {
        self.test_cancel_next_mutation_commit
            .swap(false, Ordering::SeqCst)
    }

    #[cfg(feature = "test-faults")]
    pub fn test_block_next_pending_completion(
        &self,
    ) -> MainWindowComposerPendingCompletionTestRelease {
        let state = Arc::new(Mutex::new(PendingCompletionTestGateState {
            entered: false,
            released: false,
            waker: None,
        }));
        let mut gate = self.test_pending_completion_gate.lock().unwrap();
        assert!(
            gate.replace(PendingCompletionTestGate(state.clone()))
                .is_none()
        );
        MainWindowComposerPendingCompletionTestRelease(state)
    }

    #[cfg(feature = "test-faults")]
    pub(super) fn take_test_pending_completion_gate(&self) -> Option<PendingCompletionTestGate> {
        self.test_pending_completion_gate.lock().ok()?.take()
    }

    #[cfg(feature = "test-faults")]
    pub fn test_block_next_pending_dispatch(&self) -> MainWindowComposerPendingDispatchTestRelease {
        let state = Arc::new(Mutex::new(PendingCompletionTestGateState {
            entered: false,
            released: false,
            waker: None,
        }));
        let mut gate = self.test_pending_dispatch_gate.lock().unwrap();
        assert!(
            gate.replace(PendingCompletionTestGate(state.clone()))
                .is_none()
        );
        MainWindowComposerPendingDispatchTestRelease(state)
    }

    #[cfg(feature = "test-faults")]
    pub(super) fn take_test_pending_dispatch_gate(&self) -> Option<PendingCompletionTestGate> {
        self.test_pending_dispatch_gate.lock().ok()?.take()
    }

    #[cfg(feature = "test-faults")]
    pub fn test_advance_publish(
        &self,
        receipt: super::MainWindowComposerActivationReceipt,
    ) -> Result<super::MainWindowComposerPublishAdvance, String> {
        self.advance_publish(receipt)
    }

    pub(in crate::main_window) fn assets(&self) -> Result<beryl_state::AssetState, String> {
        Ok(self
            .slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .assets())
    }

    pub(in crate::main_window) fn begin_submission(
        &self,
        selection: MainWindowComposerSelectionIdentity,
        request: crate::composer_host::ComposerHostSubmissionRequest,
    ) -> Result<crate::composer_host::ComposerHostSubmissionTicket, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .begin_selected_submission(selection, request)
            .map_err(|error| error.to_string())
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::main_window) fn advance_submission(
        &self,
        selection: MainWindowComposerSelectionIdentity,
        ticket: crate::composer_host::ComposerHostSubmissionTicket,
        assets: beryl_state::AssetState,
        marker_seals: &crate::composer_marker_seal::DraftMarkerSealService,
        publication_operation_id: syndic_storage::DraftPieceOperationIdV1,
        marker_authority: Option<crate::composer_host::ComposerHostMarkerSealAuthority>,
        published_at: syndic_storage::SyndicTimestamp,
        successor_request: &crate::composer_host::ComposerHostActivationRequest,
        successor_retirement_operation_id: syndic_storage::DraftPieceOperationIdV1,
        expected_next_draft: beryl_model::SyndicDraftId,
        cancellation: &beryl_home_store::CommandCancellation,
    ) -> Result<crate::main_window::MainWindowComposerSubmissionAdvance, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .advance_selected_submission(
                &self.store,
                selection,
                ticket,
                assets,
                marker_seals,
                publication_operation_id,
                marker_authority,
                published_at,
                successor_request,
                successor_retirement_operation_id,
                expected_next_draft,
                cancellation,
            )
            .map_err(|error| error.to_string())
    }

    pub(in crate::main_window) fn complete_submission_successor_after_widget_release(
        &self,
        receipt: super::MainWindowComposerActivationReceipt,
        release: &MainWindowComposerWidgetRelease,
    ) -> Result<MainWindowComposerSelectionIdentity, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .complete_submission_successor_after_widget_release(receipt, release)
            .map_err(|error| error.to_string())
    }

    pub(in crate::main_window) fn selected_autosave_timer(
        &self,
        selection: MainWindowComposerSelectionIdentity,
    ) -> Result<Option<crate::composer_host::ComposerHostAutosaveTimer>, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .selected_autosave_timer(selection)
            .map_err(|_| "conversation composer service operation failed".to_owned())
    }

    pub(in crate::main_window) fn selected_autosave_publication(
        &self,
        selection: MainWindowComposerSelectionIdentity,
    ) -> Result<Option<crate::composer_host::ComposerHostPublicationTicket>, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .selected_autosave_publication(selection)
            .map_err(|_| "conversation composer service operation failed".to_owned())
    }

    pub(in crate::main_window) fn autosave_capture_requirement(
        &self,
        selection: MainWindowComposerSelectionIdentity,
    ) -> Result<super::MainWindowComposerAutosaveCaptureRequirement, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .selected_autosave_capture_requirement(selection)
            .map_err(|_| "conversation composer service operation failed".to_owned())
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
            .map_err(|_| "conversation composer service operation failed".to_owned())
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
            .map_err(|_| "conversation composer service operation failed".to_owned())
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
            .map_err(|_| "conversation composer service operation failed".to_owned())
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
            .map_err(|_| "conversation composer service operation failed".to_owned())
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
            .map_err(|_| "conversation composer service operation failed".to_owned())
    }

    pub(in crate::main_window) fn retire_pending(
        &self,
        receipt: super::MainWindowComposerActivationReceipt,
    ) -> Result<super::MainWindowComposerRetirementAdvance, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .retire_pending(&self.store, receipt)
            .map_err(|_| "conversation composer service operation failed".to_owned())
    }

    pub(in crate::main_window) fn release_failed_pending(
        &self,
        receipt: super::MainWindowComposerActivationReceipt,
    ) -> Result<super::MainWindowComposerRetirementAdvance, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .release_failed_pending(&self.store, receipt)
            .map_err(|_| "conversation composer service operation failed".to_owned())
    }

    pub(in crate::main_window) fn begin_publish(
        &self,
        receipt: super::MainWindowComposerActivationReceipt,
    ) -> Result<crate::composer_host::ComposerHostFlushAdmission, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .begin_publish(&self.store, receipt)
            .map_err(|_| "conversation composer service operation failed".to_owned())
    }

    pub(in crate::main_window) fn publish_preflight(
        &self,
        receipt: super::MainWindowComposerActivationReceipt,
    ) -> Result<MainWindowComposerSelectionIdentity, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .publish_preflight(&self.store, receipt)
            .map_err(|_| "conversation composer publication preflight is stale".to_owned())
    }

    pub(in crate::main_window) fn advance_publish(
        &self,
        receipt: super::MainWindowComposerActivationReceipt,
    ) -> Result<super::MainWindowComposerPublishAdvance, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .advance_publish(&self.store, receipt)
            .map_err(|_| "conversation composer service operation failed".to_owned())
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
            .capture_activation_flush_publication(
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
            .map_err(|_| "conversation composer service operation failed".to_owned())
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
            .map_err(|_| "conversation composer service operation failed".to_owned())
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
            .map_err(|_| "conversation composer service operation failed".to_owned())
    }

    pub(in crate::main_window) fn begin_final_publish(
        &self,
        receipt: super::MainWindowComposerActivationReceipt,
        expected: MainWindowComposerSelectionIdentity,
    ) -> Result<(), String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .begin_final_publish(&self.store, receipt, expected)
            .map_err(|_| "conversation composer final publication fence is stale".to_owned())
    }

    pub(in crate::main_window) fn begin_disposal(
        &self,
    ) -> Result<crate::composer_host::ComposerHostFlushAdmission, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .begin_disposal(&self.store)
            .map_err(|_| "conversation composer service operation failed".to_owned())
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
                .map_err(|_| "conversation composer service operation failed".to_owned())?
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
            .map_err(|_| "conversation composer service operation failed".to_owned())
    }

    pub(in crate::main_window) fn complete_disposal_after_widget_release(
        &self,
        release: &MainWindowComposerWidgetRelease,
    ) -> Result<super::MainWindowComposerDisposalAdvance, String> {
        self.slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .complete_disposal_after_widget_release(&self.store, release)
            .map_err(|_| "conversation composer service operation failed".to_owned())
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
            .map_err(|_| "conversation composer service operation failed".to_owned())
    }

    pub(super) fn take_pending_initial_presentation(
        &self,
        receipt: super::MainWindowComposerActivationReceipt,
    ) -> Result<
        (
            MainWindowComposerSelectionIdentity,
            Box<[crate::composer_host::ComposerHostResponse]>,
        ),
        String,
    > {
        let presentation = self
            .slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?
            .take_pending_initial_presentation(receipt)
            .map_err(|_| "conversation composer service operation failed".to_owned())?;
        let selection = presentation.selection();
        let responses = presentation.into_responses().into_vec();
        #[cfg(feature = "test-faults")]
        let mut responses = responses;
        #[cfg(feature = "test-faults")]
        if self
            .test_append_impossible_pending_initial_response
            .swap(false, Ordering::SeqCst)
        {
            let (key, value) = responses
                .iter()
                .find_map(|response| match response.value() {
                    crate::composer_host::ComposerHostResponseValue::CandidateText(candidate) => {
                        Some((response.key(), candidate.value().clone()))
                    }
                    _ => None,
                })
                .expect("test activation includes candidate text");
            responses.push(crate::composer_host::ComposerHostResponse::new(
                key,
                crate::composer_host::ComposerHostResponseValue::HistoricalText(value),
            ));
        }
        Ok((selection, responses.into_boxed_slice()))
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
            .map_err(|_| "conversation composer service operation failed".to_owned())
    }
}
