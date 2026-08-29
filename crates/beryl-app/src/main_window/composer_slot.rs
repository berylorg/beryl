use beryl_home_store::{CommandCancellation, HomeStore};
use beryl_model::WindowId;
use beryl_state::{AssetState, WindowClaimSelection};
use syndic_storage::{
    DraftEditorCurrentSelectorV1, DraftPieceOperationIdV1, SyndicPointReadLimit, SyndicStorage,
};

use crate::composer_host::{
    ComposerHostActivationOutcome, ComposerHostActivationRequest, ComposerHostError,
    ComposerHostFlushAdmission, ComposerHostFlushAdvance, ComposerHostFlushPurpose,
    ComposerHostFlushState, SyndicComposerHost,
};
use crate::main_window::MainWindowComposerMarkerMetadataAuthority;

mod dispatch;
mod lifecycle;
mod model;
mod retirement;
mod state;

pub use dispatch::*;
pub(in crate::main_window) use dispatch::{
    MainWindowComposerSuccessorProof, MainWindowComposerSuccessorProofLimits,
    translate_initial_composer_response,
};
pub use lifecycle::MainWindowComposerAutosaveCaptureRequirement;
pub use model::*;
use state::{DisposalStage, PendingComposer, PendingStage, SelectedComposer};

pub struct MainWindowComposerSlot {
    window_id: WindowId,
    storage: SyndicStorage,
    marker_authority: MainWindowComposerMarkerMetadataAuthority,
    selected: Option<SelectedComposer>,
    pending: Option<PendingComposer>,
    last_activation_generation: u64,
    disposed: bool,
    disposal_stage: Option<DisposalStage>,
    #[cfg(feature = "test-faults")]
    activation_after_open_fault:
        Option<Box<dyn FnOnce(&HomeStore, SyndicStorage) + Send + 'static>>,
    #[cfg(feature = "test-faults")]
    abandonment_before_execute_fault:
        Option<Box<dyn FnOnce(&HomeStore, SyndicStorage) + Send + 'static>>,
}

impl MainWindowComposerSlot {
    pub(in crate::main_window) const fn assets(&self) -> AssetState {
        self.marker_authority.assets()
    }

    pub fn new(
        window_id: WindowId,
        claim: WindowClaimSelection,
        host: SyndicComposerHost,
        storage: SyndicStorage,
        marker_authority: MainWindowComposerMarkerMetadataAuthority,
    ) -> Result<Self, MainWindowComposerSlotError> {
        let binding = host
            .binding()
            .ok_or(MainWindowComposerSlotError::IdentityMismatch)?;
        if host.active_thread_id() != Some(claim.thread_id()) {
            return Err(MainWindowComposerSlotError::IdentityMismatch);
        }
        let dispatcher = MainWindowComposerDispatcher::new(binding, &host);
        let draft_state = draft_state_for_host(&host, binding)?;
        Ok(Self {
            window_id,
            storage,
            marker_authority,
            selected: Some(SelectedComposer {
                identity: MainWindowComposerSelectionIdentity {
                    window_id,
                    claim,
                    binding,
                },
                dispatcher,
                draft_state,
                host,
            }),
            pending: None,
            last_activation_generation: 0,
            disposed: false,
            disposal_stage: None,
            #[cfg(feature = "test-faults")]
            activation_after_open_fault: None,
            #[cfg(feature = "test-faults")]
            abandonment_before_execute_fault: None,
        })
    }

    pub fn selected_identity(&self) -> Option<MainWindowComposerSelectionIdentity> {
        if matches!(
            self.pending.as_ref().map(|pending| pending.stage),
            Some(PendingStage::AwaitingWidgetRelease | PendingStage::Finalizing)
        ) || matches!(
            self.disposal_stage,
            Some(DisposalStage::AwaitingWidgetRelease)
        ) {
            return self.selected.as_ref().map(|selected| selected.identity);
        }
        self.selected.as_ref().and_then(|selected| {
            selected
                .host
                .binding()
                .map(|binding| MainWindowComposerSelectionIdentity {
                    binding,
                    ..selected.identity
                })
        })
    }

    pub(in crate::main_window) fn release_selected_widget_work(
        &self,
        selection: MainWindowComposerSelectionIdentity,
        requests: impl IntoIterator<Item = gpui_text_input::RangeTextInputRequest>,
    ) -> Result<MainWindowComposerWidgetRelease, MainWindowComposerSlotError> {
        let awaiting_release = matches!(
            self.pending.as_ref().map(|pending| pending.stage),
            Some(PendingStage::Finalizing)
        ) || matches!(
            self.disposal_stage,
            Some(DisposalStage::AwaitingWidgetRelease)
        );
        if !awaiting_release || self.selected_identity() != Some(selection) {
            return Err(MainWindowComposerSlotError::StaleActivationReceipt);
        }
        if requests.into_iter().any(|request| {
            !matches!(
                request,
                gpui_text_input::RangeTextInputRequest::CancelPage(_)
                    | gpui_text_input::RangeTextInputRequest::ReleasePage(_)
                    | gpui_text_input::RangeTextInputRequest::CancelObjectPage(_)
                    | gpui_text_input::RangeTextInputRequest::ReleaseObjectPage(_)
                    | gpui_text_input::RangeTextInputRequest::CancelClipboardProvenancePage(_)
                    | gpui_text_input::RangeTextInputRequest::CancelClipboardWrite(_)
            )
        }) {
            return Err(MainWindowComposerSlotError::WidgetReleaseIncomplete);
        }
        Ok(MainWindowComposerWidgetRelease::new(selection))
    }

    pub fn selected_host(&self) -> Option<&SyndicComposerHost> {
        self.selected.as_ref().map(|selected| &selected.host)
    }

    #[cfg(feature = "test-faults")]
    pub fn test_selected_host_mut(&mut self) -> Option<&mut SyndicComposerHost> {
        self.selected.as_mut().map(|selected| &mut selected.host)
    }

    pub fn pending_receipt(&self) -> Option<MainWindowComposerActivationReceipt> {
        self.pending.as_ref().map(|pending| pending.receipt)
    }

    pub fn pending_identity(
        &self,
        receipt: MainWindowComposerActivationReceipt,
    ) -> Option<MainWindowComposerSelectionIdentity> {
        let pending = self
            .pending
            .as_ref()
            .filter(|pending| pending.receipt == receipt)?;
        let binding = pending.host.binding()?;
        (pending.dispatcher.binding == binding
            && pending.host.active_thread_id() == Some(pending.claim.thread_id()))
        .then_some(MainWindowComposerSelectionIdentity {
            window_id: self.window_id,
            claim: pending.claim,
            binding,
        })
    }

    #[cfg(feature = "test-faults")]
    pub fn test_pending_host_request_id(
        &self,
        receipt: MainWindowComposerActivationReceipt,
    ) -> Option<u64> {
        self.pending
            .as_ref()
            .filter(|pending| pending.receipt == receipt)
            .map(|pending| pending.dispatcher.last_host_request_id)
    }

    pub fn pending_status(&self) -> Option<MainWindowComposerPendingStatus> {
        self.pending.as_ref().map(|pending| match pending.stage {
            PendingStage::Ready => MainWindowComposerPendingStatus::Ready,
            PendingStage::Publishing(ticket) => {
                let state = self
                    .selected
                    .as_ref()
                    .and_then(|selected| selected.host.flush_state(ticket).ok())
                    .unwrap_or(ComposerHostFlushState::DisposalRequired);
                MainWindowComposerPendingStatus::Publishing(state)
            }
            PendingStage::AwaitingWidgetRelease | PendingStage::Finalizing => {
                MainWindowComposerPendingStatus::WidgetReleaseRequired
            }
            PendingStage::Retiring => MainWindowComposerPendingStatus::RetirementPending,
            PendingStage::Reconciliation => MainWindowComposerPendingStatus::ReconciliationPending,
            PendingStage::Departed => MainWindowComposerPendingStatus::DepartedFreshBoundary,
        })
    }

    #[cfg(feature = "test-faults")]
    pub fn test_pending_binding(&self) -> Option<crate::composer_host::ComposerHostBinding> {
        self.pending
            .as_ref()
            .and_then(|pending| pending.host.binding())
    }

    #[cfg(feature = "test-faults")]
    pub fn test_arm_activation_after_open_fault(
        &mut self,
        fault: impl FnOnce(&HomeStore, SyndicStorage) + Send + 'static,
    ) {
        assert!(self.activation_after_open_fault.is_none());
        self.activation_after_open_fault = Some(Box::new(fault));
    }

    #[cfg(feature = "test-faults")]
    pub fn test_arm_abandonment_before_execute_fault(
        &mut self,
        fault: impl FnOnce(&HomeStore, SyndicStorage) + Send + 'static,
    ) {
        assert!(self.abandonment_before_execute_fault.is_none());
        self.abandonment_before_execute_fault = Some(Box::new(fault));
    }

    pub fn begin_activation(
        &mut self,
        store: &HomeStore,
        claim: WindowClaimSelection,
        request: ComposerHostActivationRequest,
        retirement_operation_id: DraftPieceOperationIdV1,
        cancellation: &CommandCancellation,
    ) -> Result<MainWindowComposerActivationAdvance, MainWindowComposerSlotError> {
        self.ensure_live()?;
        if self.pending.is_some() {
            return Err(MainWindowComposerSlotError::ActivationPending);
        }
        if request.thread_id() != claim.thread_id() {
            return Err(MainWindowComposerSlotError::IdentityMismatch);
        }
        let expected_prior = self
            .selected_identity()
            .ok_or(MainWindowComposerSlotError::Disposed)?;
        let source_selector = current_selector(self.storage, store, request.thread_id())?;
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
            expected_prior,
        };
        let mut host = SyndicComposerHost::new(self.storage);
        #[cfg(feature = "test-faults")]
        if let Some(fault) = self.activation_after_open_fault.take() {
            host.test_arm_activation_after_open_fault(fault);
        }
        let activation = host.activate_unpublished(store, request, cancellation);
        self.last_activation_generation = generation;
        match activation {
            Ok(ComposerHostActivationOutcome::Activated { .. }) => {
                let binding = host
                    .binding()
                    .ok_or(MainWindowComposerSlotError::IdentityMismatch)?;
                let dispatcher = MainWindowComposerDispatcher::new(binding, &host);
                self.pending = Some(PendingComposer {
                    receipt,
                    claim,
                    retirement_operation_id,
                    host,
                    dispatcher,
                    source_selector: Some(source_selector),
                    stage: PendingStage::Ready,
                    abandonment: None,
                });
                Ok(MainWindowComposerActivationAdvance::Ready(receipt))
            }
            Ok(ComposerHostActivationOutcome::Cancelled) if host.binding().is_none() => {
                Ok(MainWindowComposerActivationAdvance::Cancelled)
            }
            Ok(ComposerHostActivationOutcome::Cancelled) => {
                self.install_retiring(receipt, claim, retirement_operation_id, host);
                match self.drive_retirement(store, receipt)? {
                    MainWindowComposerRetirementAdvance::Retired => {
                        Ok(MainWindowComposerActivationAdvance::Cancelled)
                    }
                    _ => Ok(MainWindowComposerActivationAdvance::RetirementPending(
                        receipt,
                    )),
                }
            }
            Ok(outcome) => Ok(MainWindowComposerActivationAdvance::Rejected(outcome)),
            Err(error) if host.binding().is_none() => Err(error.into()),
            Err(error) => {
                self.install_retiring(receipt, claim, retirement_operation_id, host);
                match self.drive_retirement(store, receipt)? {
                    MainWindowComposerRetirementAdvance::Retired => {
                        Ok(MainWindowComposerActivationAdvance::FailureRetired(error))
                    }
                    _ => Ok(
                        MainWindowComposerActivationAdvance::FailureRetirementPending {
                            receipt,
                            error,
                        },
                    ),
                }
            }
        }
    }

    pub fn begin_publish(
        &mut self,
        store: &HomeStore,
        receipt: MainWindowComposerActivationReceipt,
    ) -> Result<ComposerHostFlushAdmission, MainWindowComposerSlotError> {
        self.ensure_receipt(receipt)?;
        if !matches!(self.pending.as_ref().unwrap().stage, PendingStage::Ready) {
            return Err(MainWindowComposerSlotError::TargetNotReady);
        }
        if !same_selected_host(self.selected_identity(), receipt.expected_prior) {
            return Err(MainWindowComposerSlotError::StaleActivationReceipt);
        }
        if !self.pending_source_is_current(store, receipt)? {
            return Err(MainWindowComposerSlotError::StaleActivationReceipt);
        }
        if self
            .selected
            .as_ref()
            .unwrap()
            .host
            .fresh_abandonment_request(receipt.open_operation_id)
            .is_some()
        {
            self.pending.as_mut().unwrap().stage = PendingStage::AwaitingWidgetRelease;
            return Ok(ComposerHostFlushAdmission::Satisfied(
                ComposerHostFlushPurpose::ThreadSwitch,
            ));
        }
        let admission = self
            .selected
            .as_mut()
            .unwrap()
            .host
            .begin_flush(ComposerHostFlushPurpose::ThreadSwitch)?;
        match admission {
            ComposerHostFlushAdmission::Started { ticket, .. }
            | ComposerHostFlushAdmission::Joined { ticket, .. } => {
                self.pending.as_mut().unwrap().stage = PendingStage::Publishing(ticket);
            }
            ComposerHostFlushAdmission::Satisfied(ComposerHostFlushPurpose::ThreadSwitch) => {
                self.pending.as_mut().unwrap().stage = PendingStage::AwaitingWidgetRelease;
            }
            ComposerHostFlushAdmission::Satisfied(_) => {
                return Err(MainWindowComposerSlotError::StaleActivationReceipt);
            }
        }
        Ok(admission)
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::main_window) fn capture_activation_flush_publication(
        &mut self,
        store: &HomeStore,
        selection: MainWindowComposerSelectionIdentity,
        flush: crate::composer_host::ComposerHostFlushTicket,
        assets: AssetState,
        marker_seals: &crate::composer_marker_seal::DraftMarkerSealService,
        operation_id: DraftPieceOperationIdV1,
        marker_authority: Option<crate::composer_host::ComposerHostMarkerSealAuthority>,
        published_at: syndic_storage::SyndicTimestamp,
        cancellation: &CommandCancellation,
    ) -> Result<crate::composer_host::ComposerHostFlushCapture, MainWindowComposerDispatchError>
    {
        let drifted_pending = self
            .pending
            .as_ref()
            .filter(|pending| matches!(pending.stage, PendingStage::Publishing(ticket) if ticket == flush))
            .map(|pending| pending.receipt)
            .is_some_and(|receipt| {
                !self
                    .pending_source_is_current(store, receipt)
                    .unwrap_or(false)
            });
        let cancelled;
        let cancellation = if drifted_pending {
            cancelled = CommandCancellation::new();
            cancelled.cancel();
            &cancelled
        } else {
            cancellation
        };
        self.capture_selected_flush_publication(
            store,
            selection,
            flush,
            assets,
            marker_seals,
            operation_id,
            marker_authority,
            published_at,
            cancellation,
        )
    }

    pub fn publish_preflight(
        &self,
        store: &HomeStore,
        receipt: MainWindowComposerActivationReceipt,
    ) -> Result<MainWindowComposerSelectionIdentity, MainWindowComposerSlotError> {
        self.ensure_receipt(receipt)?;
        if !matches!(self.pending.as_ref().unwrap().stage, PendingStage::Ready) {
            return Err(MainWindowComposerSlotError::TargetNotReady);
        }
        let current = self
            .selected_identity()
            .ok_or(MainWindowComposerSlotError::StaleActivationReceipt)?;
        if !same_selected_host(Some(current), receipt.expected_prior) {
            return Err(MainWindowComposerSlotError::StaleActivationReceipt);
        }
        if !self.pending_source_is_current(store, receipt)? {
            return Err(MainWindowComposerSlotError::StaleActivationReceipt);
        }
        Ok(current)
    }

    pub fn advance_publish(
        &mut self,
        store: &HomeStore,
        receipt: MainWindowComposerActivationReceipt,
    ) -> Result<MainWindowComposerPublishAdvance, MainWindowComposerSlotError> {
        self.ensure_receipt(receipt)?;
        let ticket = match self.pending.as_ref().unwrap().stage {
            PendingStage::Publishing(ticket) => ticket,
            PendingStage::AwaitingWidgetRelease | PendingStage::Finalizing => {
                return Ok(MainWindowComposerPublishAdvance::WidgetReleaseRequired(
                    self.selected_identity().unwrap(),
                ));
            }
            _ => return Err(MainWindowComposerSlotError::TargetNotReady),
        };
        if !self.pending_source_is_current(store, receipt)? {
            return Err(MainWindowComposerSlotError::StaleActivationReceipt);
        }
        match self.advance_slot_flush(store, ticket)? {
            ComposerHostFlushAdvance::Progress(state) => {
                Ok(MainWindowComposerPublishAdvance::Progress(state))
            }
            ComposerHostFlushAdvance::ReconciliationPending => {
                Ok(MainWindowComposerPublishAdvance::ReconciliationPending)
            }
            ComposerHostFlushAdvance::Satisfied(ComposerHostFlushPurpose::ThreadSwitch) => {
                self.pending.as_mut().unwrap().stage = PendingStage::AwaitingWidgetRelease;
                Ok(MainWindowComposerPublishAdvance::WidgetReleaseRequired(
                    self.selected_identity().unwrap(),
                ))
            }
            ComposerHostFlushAdvance::Unsatisfied(_) => {
                self.pending.as_mut().unwrap().stage = PendingStage::Retiring;
                Ok(MainWindowComposerPublishAdvance::PriorFlushFailed)
            }
            ComposerHostFlushAdvance::Stale | ComposerHostFlushAdvance::Satisfied(_) => {
                Err(MainWindowComposerSlotError::StaleActivationReceipt)
            }
        }
    }

    pub fn complete_publish_after_widget_release(
        &mut self,
        store: &HomeStore,
        receipt: MainWindowComposerActivationReceipt,
        release: &MainWindowComposerWidgetRelease,
    ) -> Result<MainWindowComposerPublishAdvance, MainWindowComposerSlotError> {
        self.ensure_receipt(receipt)?;
        if !matches!(
            self.pending.as_ref().unwrap().stage,
            PendingStage::Finalizing
        ) || self.selected_identity() != Some(release.selection())
        {
            return Err(MainWindowComposerSlotError::StaleActivationReceipt);
        }
        let pending = self.pending.as_ref().unwrap();
        let binding = pending
            .host
            .binding()
            .ok_or(MainWindowComposerSlotError::IdentityMismatch)?;
        if pending.host.active_thread_id() != Some(pending.claim.thread_id()) {
            return Err(MainWindowComposerSlotError::IdentityMismatch);
        }
        let target_identity = MainWindowComposerSelectionIdentity {
            window_id: self.window_id,
            claim: pending.claim,
            binding,
        };
        if receipt != pending.receipt {
            return Err(MainWindowComposerSlotError::StaleActivationReceipt);
        }
        let draft_state = draft_state_for_host(&pending.host, binding)?;
        match self
            .selected
            .as_mut()
            .unwrap()
            .host
            .dispose_composer_service(store)?
        {
            crate::composer_host::ComposerHostServiceDisposalCompletion::Pending => {
                return Ok(MainWindowComposerPublishAdvance::ReconciliationPending);
            }
            crate::composer_host::ComposerHostServiceDisposalCompletion::Disposed => {}
        }
        let pending = self.pending.take().unwrap();
        self.selected = Some(SelectedComposer {
            identity: target_identity,
            dispatcher: pending.dispatcher,
            draft_state,
            host: pending.host,
        });
        Ok(MainWindowComposerPublishAdvance::Published(target_identity))
    }

    pub fn begin_final_publish(
        &mut self,
        store: &HomeStore,
        receipt: MainWindowComposerActivationReceipt,
        expected: MainWindowComposerSelectionIdentity,
    ) -> Result<(), MainWindowComposerSlotError> {
        self.ensure_receipt(receipt)?;
        if matches!(
            self.pending.as_ref().unwrap().stage,
            PendingStage::Finalizing
        ) && self.selected_identity() == Some(expected)
            && same_selected_host(Some(expected), receipt.expected_prior)
        {
            return Ok(());
        }
        if !matches!(
            self.pending.as_ref().unwrap().stage,
            PendingStage::AwaitingWidgetRelease
        ) || self.selected_identity() != Some(expected)
            || !same_selected_host(Some(expected), receipt.expected_prior)
            || !self.pending_source_is_current(store, receipt)?
        {
            return Err(MainWindowComposerSlotError::StaleActivationReceipt);
        }
        self.pending.as_mut().unwrap().stage = PendingStage::Finalizing;
        Ok(())
    }

    fn ensure_live(&self) -> Result<(), MainWindowComposerSlotError> {
        if self.disposed {
            Err(MainWindowComposerSlotError::Disposed)
        } else {
            Ok(())
        }
    }

    fn ensure_receipt(
        &self,
        receipt: MainWindowComposerActivationReceipt,
    ) -> Result<(), MainWindowComposerSlotError> {
        self.ensure_live()?;
        if receipt.window_id != self.window_id
            || self
                .pending
                .as_ref()
                .is_none_or(|pending| pending.receipt != receipt)
        {
            Err(MainWindowComposerSlotError::StaleActivationReceipt)
        } else {
            Ok(())
        }
    }

    fn pending_source_is_current(
        &self,
        store: &HomeStore,
        receipt: MainWindowComposerActivationReceipt,
    ) -> Result<bool, MainWindowComposerSlotError> {
        self.ensure_receipt(receipt)?;
        let pending = self.pending.as_ref().unwrap();
        let Some(expected) = pending.source_selector else {
            return Ok(false);
        };
        if current_selector(self.storage, store, pending.claim.thread_id())? != expected {
            return Ok(false);
        }
        let Some(binding) = pending.host.binding() else {
            return Ok(false);
        };
        let head = self
            .storage
            .draft_editor_candidate_session(
                store,
                binding.candidate().draft_id(),
                binding.candidate().session_id(),
            )
            .map_err(ComposerHostError::from)?;
        Ok(matches!(
            head,
            syndic_storage::DraftEditorCandidateSessionReadOutcomeV1::Active(ref head)
                if syndic_storage::DraftEditorCandidateActivationBindingV1::from_head(head)
                    == binding.candidate()
        ))
    }
}

fn same_selected_host(
    current: Option<MainWindowComposerSelectionIdentity>,
    expected: MainWindowComposerSelectionIdentity,
) -> bool {
    current.is_some_and(|current| {
        current.window_id == expected.window_id
            && current.claim == expected.claim
            && current.binding.home_id() == expected.binding.home_id()
            && current.binding.home_generation() == expected.binding.home_generation()
            && current.binding.host_generation() == expected.binding.host_generation()
            && current.binding.presentation_generation()
                == expected.binding.presentation_generation()
    })
}

fn draft_state_for_host(
    host: &SyndicComposerHost,
    binding: crate::composer_host::ComposerHostBinding,
) -> Result<crate::main_window::MainWindowComposerDraftState, MainWindowComposerSlotError> {
    let (published_candidate_generation, published_pair) = host
        .published_draft()
        .ok_or(MainWindowComposerSlotError::IdentityMismatch)?;
    crate::main_window::MainWindowComposerDraftState::new(
        binding,
        published_candidate_generation,
        published_pair,
    )
    .map_err(|_| MainWindowComposerSlotError::IdentityMismatch)
}

fn current_selector(
    storage: SyndicStorage,
    store: &HomeStore,
    thread: beryl_model::SyndicThreadId,
) -> Result<DraftEditorCurrentSelectorV1, MainWindowComposerSlotError> {
    let current = storage
        .current_draft(store, thread, SyndicPointReadLimit::new(65_536).unwrap())
        .map_err(ComposerHostError::from)?
        .ok_or(ComposerHostError::MissingCurrentDraft)?;
    Ok(DraftEditorCurrentSelectorV1::new(
        current.thread().id(),
        current.thread().revision(),
        current.draft().id(),
        current.draft().revision(),
        current.draft().piece_root(),
        current.draft().history(),
    ))
}
