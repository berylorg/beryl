use beryl_home_store::{CommandCancellation, HomeStore};
use beryl_model::WindowId;
use beryl_state::WindowClaimSelection;
use syndic_storage::{DraftPieceOperationIdV1, SyndicStorage};

use crate::composer_host::{
    ComposerHostActivationOutcome, ComposerHostActivationRequest, ComposerHostFlushAdmission,
    ComposerHostFlushAdvance, ComposerHostFlushPurpose, ComposerHostFlushState,
    ComposerHostFlushTicket, SyndicComposerHost,
};

mod model;
mod retirement;
mod state;

pub use model::*;
use state::{PendingComposer, PendingStage, SelectedComposer};

pub struct MainWindowComposerSlot {
    window_id: WindowId,
    storage: SyndicStorage,
    selected: Option<SelectedComposer>,
    pending: Option<PendingComposer>,
    last_activation_generation: u64,
    disposed: bool,
    disposal_flush: Option<ComposerHostFlushTicket>,
    #[cfg(feature = "test-faults")]
    activation_after_open_fault:
        Option<Box<dyn FnOnce(&HomeStore, SyndicStorage) + Send + 'static>>,
    #[cfg(feature = "test-faults")]
    abandonment_before_execute_fault:
        Option<Box<dyn FnOnce(&HomeStore, SyndicStorage) + Send + 'static>>,
}

impl MainWindowComposerSlot {
    pub fn new(
        window_id: WindowId,
        claim: WindowClaimSelection,
        host: SyndicComposerHost,
        storage: SyndicStorage,
    ) -> Result<Self, MainWindowComposerSlotError> {
        let binding = host
            .binding()
            .ok_or(MainWindowComposerSlotError::IdentityMismatch)?;
        if host.active_thread_id() != Some(claim.thread_id()) {
            return Err(MainWindowComposerSlotError::IdentityMismatch);
        }
        Ok(Self {
            window_id,
            storage,
            selected: Some(SelectedComposer {
                identity: MainWindowComposerSelectionIdentity {
                    window_id,
                    claim,
                    binding,
                },
                host,
            }),
            pending: None,
            last_activation_generation: 0,
            disposed: false,
            disposal_flush: None,
            #[cfg(feature = "test-faults")]
            activation_after_open_fault: None,
            #[cfg(feature = "test-faults")]
            abandonment_before_execute_fault: None,
        })
    }

    pub fn selected_identity(&self) -> Option<MainWindowComposerSelectionIdentity> {
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
                self.pending = Some(PendingComposer {
                    receipt,
                    claim,
                    retirement_operation_id,
                    host,
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
                self.publish_target(store, receipt)?;
            }
            ComposerHostFlushAdmission::Satisfied(_) => {
                return Err(MainWindowComposerSlotError::StaleActivationReceipt);
            }
        }
        Ok(admission)
    }

    pub fn advance_publish(
        &mut self,
        store: &HomeStore,
        receipt: MainWindowComposerActivationReceipt,
    ) -> Result<MainWindowComposerPublishAdvance, MainWindowComposerSlotError> {
        self.ensure_receipt(receipt)?;
        let PendingStage::Publishing(ticket) = self.pending.as_ref().unwrap().stage else {
            return Err(MainWindowComposerSlotError::TargetNotReady);
        };
        match self
            .selected
            .as_mut()
            .unwrap()
            .host
            .advance_flush(store, ticket)?
        {
            ComposerHostFlushAdvance::Progress(state) => {
                Ok(MainWindowComposerPublishAdvance::Progress(state))
            }
            ComposerHostFlushAdvance::ReconciliationPending => {
                Ok(MainWindowComposerPublishAdvance::ReconciliationPending)
            }
            ComposerHostFlushAdvance::Satisfied(ComposerHostFlushPurpose::ThreadSwitch) => {
                self.publish_target(store, receipt)
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

    fn publish_target(
        &mut self,
        store: &HomeStore,
        receipt: MainWindowComposerActivationReceipt,
    ) -> Result<MainWindowComposerPublishAdvance, MainWindowComposerSlotError> {
        let pending = self.pending.take().unwrap();
        let binding = pending
            .host
            .binding()
            .ok_or(MainWindowComposerSlotError::IdentityMismatch)?;
        if pending.host.active_thread_id() != Some(pending.claim.thread_id()) {
            return Err(MainWindowComposerSlotError::IdentityMismatch);
        }
        let identity = MainWindowComposerSelectionIdentity {
            window_id: self.window_id,
            claim: pending.claim,
            binding,
        };
        let mut prior = self.selected.replace(SelectedComposer {
            identity,
            host: pending.host,
        });
        if let Some(prior) = prior.as_mut() {
            let _ = prior.host.dispose_composer_service(store);
        }
        if receipt != pending.receipt {
            return Err(MainWindowComposerSlotError::StaleActivationReceipt);
        }
        Ok(MainWindowComposerPublishAdvance::Published(identity))
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
