use super::{
    CommandCancellation, MainWindowComposerDispatchOutcome, MainWindowConversationComposerDispatch,
    MainWindowConversationComposerFailureSettlement, MainWindowConversationComposerRoute,
    MainWindowConversationComposerService, MainWindowConversationComposerTaskError,
    MainWindowConversationComposerTaskResult, RangeTextInputRequest,
};
use crate::{
    composer_host::{ComposerHostImageMarkerMetadata, ComposerHostMutationOutcome},
    main_window::{
        MainWindowComposerSelectionIdentity, MainWindowComposerSlot,
        MainWindowComposerSuccessorProofLimits,
    },
};

pub(super) struct DispatchWork {
    pub(super) selection: MainWindowComposerSelectionIdentity,
    pub(super) route: MainWindowConversationComposerRoute,
    pub(super) request: Option<RangeTextInputRequest>,
    pub(super) marker_metadata: Box<[ComposerHostImageMarkerMetadata]>,
    pub(super) cancellation: CommandCancellation,
    pub(super) proof_limits: MainWindowComposerSuccessorProofLimits,
    pub(super) settlement: Option<MainWindowConversationComposerFailureSettlement>,
    pub(super) mutation_key: Option<gpui_text_input::MutationKey>,
    #[cfg(test)]
    pub(super) test_flight: std::sync::Arc<super::DispatchFlightObservation>,
}

impl DispatchWork {
    #[inline(never)]
    pub(super) fn run(
        mut self: Box<Self>,
        service: &MainWindowConversationComposerService,
    ) -> MainWindowConversationComposerTaskResult {
        #[cfg(test)]
        let _test_work_return = {
            self.test_flight.advance(1);
            super::DispatchFlightWorkerObservation(self.test_flight.clone())
        };
        let mut slot = service.slot.lock().map_err(|_| {
            MainWindowConversationComposerTaskError::exact(
                "conversation composer service lock failed".to_owned(),
                self.settlement,
            )
        })?;
        #[cfg(test)]
        self.test_flight.advance(2);
        let admitted = match self.route {
            MainWindowConversationComposerRoute::Selected => {
                slot.selected_identity() == Some(self.selection)
            }
            MainWindowConversationComposerRoute::Pending(receipt) => {
                slot.pending_request_is_admitted(receipt, self.selection)
            }
        };
        if !admitted {
            return Err(
                MainWindowConversationComposerTaskError::CustodyNotDispatched {
                    settlement: self.settlement,
                },
            );
        }
        #[cfg(feature = "test-faults")]
        if matches!(
            self.request.as_ref(),
            Some(RangeTextInputRequest::MutationCommit(_))
        ) && let Some(error) = service.take_test_mutation_dispatch_error()
        {
            return Err(MainWindowConversationComposerTaskError::Dispatch {
                error: Box::new(crate::main_window::MainWindowComposerDispatchError::Host(
                    error,
                )),
                selection: self.selection,
                mutation_key: self.mutation_key,
                settlement: self.settlement,
            });
        }
        let request = self
            .request
            .take()
            .expect("composer dispatch owns its request");
        let outcome = match self.route {
            MainWindowConversationComposerRoute::Selected => slot.dispatch_selected_request(
                &service.store,
                self.selection,
                request,
                std::mem::take(&mut self.marker_metadata),
                &self.cancellation,
            ),
            MainWindowConversationComposerRoute::Pending(receipt) => slot.dispatch_pending_request(
                &service.store,
                receipt,
                self.selection,
                request,
                &self.cancellation,
            ),
        }
        .map_err(|error| MainWindowConversationComposerTaskError::Dispatch {
            error: Box::new(error),
            selection: self.selection,
            mutation_key: self.mutation_key,
            settlement: self.settlement,
        })?;
        let mut completed = dispatch_completion(self.selection, outcome);
        #[cfg(test)]
        self.test_flight.observe_response(&completed.outcome);
        populate_successor_proof(
            service,
            &mut slot,
            &mut completed,
            self.proof_limits,
            self.settlement,
        )?;
        completed.settled_selection = match self.route {
            MainWindowConversationComposerRoute::Selected => slot.selected_identity(),
            MainWindowConversationComposerRoute::Pending(receipt) => slot.pending_identity(receipt),
        }
        .ok_or_else(|| {
            MainWindowConversationComposerTaskError::exact(
                "composer selection disappeared after dispatch".to_owned(),
                self.settlement,
            )
        })?;
        Ok(completed)
    }
}

#[inline(never)]
fn dispatch_completion(
    selection: MainWindowComposerSelectionIdentity,
    outcome: MainWindowComposerDispatchOutcome,
) -> Box<MainWindowConversationComposerDispatch> {
    Box::new(MainWindowConversationComposerDispatch {
        initiating_selection: selection,
        settled_selection: selection,
        outcome,
        proof: None,
        edit_proof: None,
        cut_page: None,
        cut_page_expected: false,
    })
}

#[inline(never)]
fn populate_successor_proof(
    service: &MainWindowConversationComposerService,
    slot: &mut MainWindowComposerSlot,
    completed: &mut MainWindowConversationComposerDispatch,
    proof_limits: MainWindowComposerSuccessorProofLimits,
    settlement: Option<MainWindowConversationComposerFailureSettlement>,
) -> Result<(), MainWindowConversationComposerTaskError> {
    let MainWindowComposerDispatchOutcome::Mutation {
        key,
        outcome: ComposerHostMutationOutcome::Committed { positions, .. },
    } = &completed.outcome
    else {
        return Ok(());
    };
    let successor = slot.selected_identity().ok_or_else(|| {
        MainWindowConversationComposerTaskError::exact(
            "committed composer selection disappeared".to_owned(),
            settlement,
        )
    })?;
    #[cfg(feature = "test-faults")]
    service.run_test_successor_proof_fault();
    completed.proof = Some((
        *key,
        slot.build_selected_successor_proof(&service.store, successor, *positions, proof_limits)
            .map_err(
                |error| MainWindowConversationComposerTaskError::CommittedPresentation {
                    error: Box::new(error),
                    initiating_selection: completed.initiating_selection,
                    successor,
                    key: *key,
                },
            )?,
    ));
    Ok(())
}
