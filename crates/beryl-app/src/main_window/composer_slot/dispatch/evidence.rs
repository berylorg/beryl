use crate::composer_host::{
    ComposerHostMutationEvidenceOutcome, ComposerHostMutationEvidenceRequest,
};

use super::*;

impl MainWindowComposerSlot {
    pub fn dispatch_selected_mutation_evidence(
        &mut self,
        store: &HomeStore,
        selection: MainWindowComposerSelectionIdentity,
        request: ComposerHostMutationEvidenceRequest,
        cancellation: &CommandCancellation,
    ) -> Result<MainWindowComposerDispatchOutcome, MainWindowComposerDispatchError> {
        let request = match request {
            ComposerHostMutationEvidenceRequest::Page {
                pass,
                page,
                metadata,
            } => {
                let widget_request =
                    gpui_text_input::MutationPageRequest::new(page).with_pass(pass);
                let widget_request = match widget_request.page().key().lane() {
                    gpui_text_input::MutationLane::Source => {
                        RangeTextInputRequest::MutationSourcePage(widget_request)
                    }
                    gpui_text_input::MutationLane::Proposal => {
                        RangeTextInputRequest::MutationProposalPage(widget_request)
                    }
                };
                let authenticated = self
                    .marker_authority
                    .authenticate(store, selection, &widget_request, metadata)
                    .map_err(MainWindowComposerDispatchError::MarkerMetadata)?;
                let metadata = authenticated
                    .into_metadata(selection, &widget_request)
                    .map_err(MainWindowComposerDispatchError::MarkerMetadata)?;
                let page = match widget_request {
                    RangeTextInputRequest::MutationSourcePage(request)
                    | RangeTextInputRequest::MutationProposalPage(request) => {
                        request.page().clone()
                    }
                    _ => unreachable!(),
                };
                ComposerHostMutationEvidenceRequest::Page {
                    pass,
                    page,
                    metadata,
                }
            }
            request => request,
        };
        let assets = self.marker_authority.assets();
        let selected = self
            .selected
            .as_mut()
            .filter(|selected| selected.identity == selection)
            .ok_or(MainWindowComposerDispatchError::StaleSelection)?;
        if selected.dispatcher.binding != selection.binding()
            || selected.host.binding() != Some(selection.binding())
        {
            return Err(MainWindowComposerDispatchError::StaleSelection);
        }
        if selected.dispatcher.in_dispatch {
            return Err(MainWindowComposerDispatchError::Busy);
        }
        if let ComposerHostMutationEvidenceRequest::Begin { begin, .. } = &request {
            selected.dispatcher.mutation_begin = Some((
                begin.proposal().key(),
                begin.source_cursor(),
                begin.proposal_cursor(),
            ));
            selected.dispatcher.mutation_finish = None;
            selected.dispatcher.early_terminal = None;
        }
        selected.dispatcher.in_dispatch = true;
        let result = selected.host.dispatch_mutation_evidence(
            store,
            selection.binding(),
            &assets,
            request,
            cancellation,
        );
        selected.dispatcher.in_dispatch = false;
        if matches!(
            result,
            Ok(ComposerHostMutationEvidenceOutcome::Refused { .. })
        ) {
            selected.dispatcher.mutation_begin = None;
            selected.dispatcher.mutation_finish = None;
            selected.dispatcher.early_terminal = None;
        }
        Ok(MainWindowComposerDispatchOutcome::MutationEvidence(result?))
    }
}
