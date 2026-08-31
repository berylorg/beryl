use beryl_home_store::{CommandCancellation, HomeStore};
use beryl_state::AssetState;
use syndic_storage::{DraftPieceOperationIdV1, SyndicTimestamp};

use super::*;
#[cfg(feature = "test-faults")]
use crate::composer_host::ComposerHostSubmissionDiagnostics;
use crate::composer_host::{
    ComposerHostActivationOutcome, ComposerHostActivationRequest, ComposerHostMarkerSealAuthority,
    ComposerHostSubmissionAdvance, ComposerHostSubmissionRequest, ComposerHostSubmissionStage,
    ComposerHostSubmissionTicket, SyndicComposerHost,
};
use crate::composer_marker_seal::DraftMarkerSealService;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MainWindowComposerSubmissionAdvance {
    Progress(ComposerHostSubmissionStage),
    ReconciliationPending,
    DirectAdmissionDenied(beryl_home_store::FreeSpaceOutcome),
    NotCommitted,
    Cancelled,
    Collision,
    Stale,
    SuccessorReady {
        receipt: MainWindowComposerActivationReceipt,
        predecessor: MainWindowComposerSelectionIdentity,
        successor: MainWindowComposerSelectionIdentity,
    },
    SuccessorUnavailable,
    #[cfg(feature = "test-faults")]
    TestReconciliationPending,
}

#[cfg(feature = "test-faults")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MainWindowComposerSlotSubmissionTestDiagnostics {
    selected_submission: Option<ComposerHostSubmissionDiagnostics>,
    submission_successor_reserved: bool,
    pending_activation_reserved: bool,
}

#[cfg(feature = "test-faults")]
impl MainWindowComposerSlotSubmissionTestDiagnostics {
    pub const fn selected_submission(self) -> Option<ComposerHostSubmissionDiagnostics> {
        self.selected_submission
    }

    pub const fn submission_successor_reserved(self) -> bool {
        self.submission_successor_reserved
    }

    pub const fn pending_activation_reserved(self) -> bool {
        self.pending_activation_reserved
    }
}

impl MainWindowComposerSlot {
    #[cfg(feature = "test-faults")]
    pub fn test_submission_diagnostics(&self) -> MainWindowComposerSlotSubmissionTestDiagnostics {
        MainWindowComposerSlotSubmissionTestDiagnostics {
            selected_submission: self
                .selected
                .as_ref()
                .map(|selected| selected.host.submission_diagnostics()),
            submission_successor_reserved: self.submission_successor.is_some(),
            pending_activation_reserved: self.pending.is_some(),
        }
    }

    pub(in crate::main_window) fn begin_selected_submission(
        &mut self,
        selection: MainWindowComposerSelectionIdentity,
        request: ComposerHostSubmissionRequest,
    ) -> Result<ComposerHostSubmissionTicket, MainWindowComposerSlotError> {
        self.ensure_live()?;
        if self.pending.is_some() || self.disposal_stage.is_some() {
            return Err(MainWindowComposerSlotError::ActivationPending);
        }
        let selected = self
            .selected
            .as_mut()
            .filter(|selected| selected.identity == selection)
            .ok_or(MainWindowComposerSlotError::StaleActivationReceipt)?;
        selected.host.begin_submission(request).map_err(Into::into)
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::main_window) fn advance_selected_submission(
        &mut self,
        store: &HomeStore,
        selection: MainWindowComposerSelectionIdentity,
        ticket: ComposerHostSubmissionTicket,
        assets: AssetState,
        marker_seals: &DraftMarkerSealService,
        publication_operation_id: DraftPieceOperationIdV1,
        marker_authority: Option<ComposerHostMarkerSealAuthority>,
        published_at: SyndicTimestamp,
        successor_request: &ComposerHostActivationRequest,
        successor_retirement_operation_id: DraftPieceOperationIdV1,
        expected_next_draft: beryl_model::SyndicDraftId,
        cancellation: &CommandCancellation,
    ) -> Result<MainWindowComposerSubmissionAdvance, MainWindowComposerSlotError> {
        self.ensure_live()?;
        if self.submission_successor.is_some() {
            return Err(MainWindowComposerSlotError::ActivationPending);
        }
        let selected = self
            .selected
            .as_mut()
            .filter(|selected| selected.identity == selection)
            .ok_or(MainWindowComposerSlotError::StaleActivationReceipt)?;
        let outcome = selected.host.advance_submission(
            store,
            ticket,
            assets,
            marker_seals,
            publication_operation_id,
            marker_authority,
            published_at,
            cancellation,
        )?;
        if let Some(binding) = selected.host.binding()
            && binding != selected.identity.binding
        {
            selected.identity.binding = binding;
            selected.dispatcher.replace_binding(binding);
            selected.draft_state = draft_state_for_host(&selected.host, binding)?;
        }
        match outcome {
            ComposerHostSubmissionAdvance::Progress(stage) => {
                Ok(MainWindowComposerSubmissionAdvance::Progress(stage))
            }
            ComposerHostSubmissionAdvance::ReconciliationPending => {
                Ok(MainWindowComposerSubmissionAdvance::ReconciliationPending)
            }
            ComposerHostSubmissionAdvance::DirectAdmissionDenied(outcome) => Ok(
                MainWindowComposerSubmissionAdvance::DirectAdmissionDenied(outcome),
            ),
            ComposerHostSubmissionAdvance::NotCommitted => {
                Ok(MainWindowComposerSubmissionAdvance::NotCommitted)
            }
            ComposerHostSubmissionAdvance::Collision => {
                Ok(MainWindowComposerSubmissionAdvance::Collision)
            }
            ComposerHostSubmissionAdvance::Cancelled => {
                Ok(MainWindowComposerSubmissionAdvance::Cancelled)
            }
            ComposerHostSubmissionAdvance::Stale => Ok(MainWindowComposerSubmissionAdvance::Stale),
            ComposerHostSubmissionAdvance::ExactSuccess(_) => self.open_submission_successor(
                store,
                selection,
                successor_request,
                successor_retirement_operation_id,
                expected_next_draft,
            ),
        }
    }

    pub(in crate::main_window) fn complete_submission_successor_after_widget_release(
        &mut self,
        receipt: MainWindowComposerActivationReceipt,
        release: &MainWindowComposerWidgetRelease,
    ) -> Result<MainWindowComposerSelectionIdentity, MainWindowComposerSlotError> {
        if self.submission_successor != Some(receipt)
            || release.selection() != receipt.expected_prior
        {
            return Err(MainWindowComposerSlotError::StaleActivationReceipt);
        }
        let pending = self
            .pending
            .take()
            .filter(|pending| pending.receipt == receipt)
            .ok_or(MainWindowComposerSlotError::StaleActivationReceipt)?;
        let binding = pending
            .host
            .binding()
            .ok_or(MainWindowComposerSlotError::IdentityMismatch)?;
        let selection = MainWindowComposerSelectionIdentity {
            window_id: self.window_id,
            claim: pending.claim,
            binding,
        };
        self.selected = Some(SelectedComposer {
            identity: selection,
            dispatcher: pending.dispatcher,
            draft_state: draft_state_for_host(&pending.host, binding)?,
            host: pending.host,
        });
        self.submission_successor = None;
        Ok(selection)
    }

    fn open_submission_successor(
        &mut self,
        store: &HomeStore,
        predecessor: MainWindowComposerSelectionIdentity,
        request: &ComposerHostActivationRequest,
        retirement_operation_id: DraftPieceOperationIdV1,
        expected_next_draft: beryl_model::SyndicDraftId,
    ) -> Result<MainWindowComposerSubmissionAdvance, MainWindowComposerSlotError> {
        if request.thread_id() != predecessor.claim().thread_id() {
            return Ok(MainWindowComposerSubmissionAdvance::SuccessorUnavailable);
        }
        let source_selector = current_selector(&self.storage, store, request.thread_id())?;
        if source_selector.draft_id() != expected_next_draft {
            return Ok(MainWindowComposerSubmissionAdvance::SuccessorUnavailable);
        }
        let generation = self
            .last_activation_generation
            .checked_add(1)
            .ok_or(MainWindowComposerSlotError::GenerationExhausted)?;
        let receipt = MainWindowComposerActivationReceipt {
            window_id: self.window_id,
            generation,
            target_thread: request.thread_id(),
            session_id: request.session_id(),
            open_operation_id: request.operation_id(),
            presentation_generation: request.presentation_generation(),
            expected_prior: predecessor,
        };
        let mut host = SyndicComposerHost::new(self.storage.clone());
        let outcome =
            host.activate_unpublished(store, request.clone(), &CommandCancellation::new());
        self.last_activation_generation = generation;
        let Ok(ComposerHostActivationOutcome::Activated { binding, .. }) = outcome else {
            let _ = host.dispose_composer_service(store);
            return Ok(MainWindowComposerSubmissionAdvance::SuccessorUnavailable);
        };
        if binding.candidate().draft_id() != expected_next_draft {
            let _ = host.dispose_composer_service(store);
            return Ok(MainWindowComposerSubmissionAdvance::SuccessorUnavailable);
        }
        let dispatcher = MainWindowComposerDispatcher::new(binding, &host);
        let successor = MainWindowComposerSelectionIdentity {
            window_id: self.window_id,
            claim: predecessor.claim(),
            binding,
        };
        self.pending = Some(PendingComposer {
            receipt,
            claim: predecessor.claim(),
            retirement_operation_id,
            host,
            dispatcher,
            source_selector: Some(source_selector),
            stage: PendingStage::Ready,
            abandonment: None,
        });
        self.submission_successor = Some(receipt);
        Ok(MainWindowComposerSubmissionAdvance::SuccessorReady {
            receipt,
            predecessor,
            successor,
        })
    }
}
