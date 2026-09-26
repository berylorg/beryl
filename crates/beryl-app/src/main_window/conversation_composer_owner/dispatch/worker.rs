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
    pub(super) request: RangeTextInputRequest,
    pub(super) marker_metadata: Box<[ComposerHostImageMarkerMetadata]>,
    pub(super) cancellation: CommandCancellation,
    pub(super) proof_limits: MainWindowComposerSuccessorProofLimits,
    pub(super) settlement: Option<MainWindowConversationComposerFailureSettlement>,
    pub(super) mutation_key: Option<gpui_text_input::MutationKey>,
}

impl DispatchWork {
    pub(super) fn run(
        self,
        service: &MainWindowConversationComposerService,
    ) -> MainWindowConversationComposerTaskResult {
        let Self {
            selection,
            route,
            request,
            marker_metadata,
            cancellation,
            proof_limits,
            settlement,
            mutation_key,
        } = self;
        let mut slot = service.slot.lock().map_err(|_| {
            MainWindowConversationComposerTaskError::exact(
                "conversation composer service lock failed".to_owned(),
                settlement,
            )
        })?;
        let admitted = match route {
            MainWindowConversationComposerRoute::Selected => {
                slot.selected_identity() == Some(selection)
            }
            MainWindowConversationComposerRoute::Pending(receipt) => {
                slot.pending_request_is_admitted(receipt, selection)
            }
        };
        if !admitted {
            return Err(
                MainWindowConversationComposerTaskError::CustodyNotDispatched { settlement },
            );
        }
        #[cfg(feature = "test-faults")]
        if matches!(request, RangeTextInputRequest::MutationCommit(_))
            && let Some(error) = service.take_test_mutation_dispatch_error()
        {
            return Err(MainWindowConversationComposerTaskError::Dispatch {
                error: Box::new(crate::main_window::MainWindowComposerDispatchError::Host(
                    error,
                )),
                selection,
                mutation_key,
                settlement,
            });
        }
        let mut completed = Box::new(MainWindowConversationComposerDispatch {
            initiating_selection: selection,
            settled_selection: selection,
            outcome: MainWindowComposerDispatchOutcome::Released,
            proof: None,
            edit_proof: None,
            cut_page: None,
            cut_page_expected: false,
        });
        completed.outcome = match route {
            MainWindowConversationComposerRoute::Selected => slot.dispatch_selected_request(
                &service.store,
                selection,
                request,
                marker_metadata,
                &cancellation,
            ),
            MainWindowConversationComposerRoute::Pending(receipt) => slot.dispatch_pending_request(
                &service.store,
                receipt,
                selection,
                request,
                &cancellation,
            ),
        }
        .map_err(|error| MainWindowConversationComposerTaskError::Dispatch {
            error: Box::new(error),
            selection,
            mutation_key,
            settlement,
        })?;
        populate_successor_proof(service, &mut slot, &mut completed, proof_limits, settlement)?;
        completed.settled_selection = match route {
            MainWindowConversationComposerRoute::Selected => slot.selected_identity(),
            MainWindowConversationComposerRoute::Pending(receipt) => slot.pending_identity(receipt),
        }
        .ok_or_else(|| {
            MainWindowConversationComposerTaskError::exact(
                "composer selection disappeared after dispatch".to_owned(),
                settlement,
            )
        })?;
        Ok(completed)
    }
}

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
