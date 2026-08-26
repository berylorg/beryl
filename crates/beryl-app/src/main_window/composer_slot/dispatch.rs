use beryl_home_store::{CommandCancellation, HomeStore};
use gpui_text_input::{
    ClipboardWriteRequest, MutationCommitRequest, MutationIdentity, MutationKey,
    MutationPageAcceptance, RangeHistoryOutcome, RangePage, RangeTextInputRequest,
};

use crate::composer_host::{
    ComposerHostError, ComposerHostImageMarkerMetadata, ComposerHostMutationOutcome,
    ComposerHostResponse, SyndicComposerHost,
};

use super::{MainWindowComposerSelectionIdentity, MainWindowComposerSlot};

mod proof;
mod terminal;
mod translate;

use terminal::{
    MainWindowComposerEarlyTerminal, accept_early_terminal_page, capture_early_terminal,
    finish_for, validate_early_terminal_finish,
};

pub(in crate::main_window) use proof::{
    MainWindowComposerSuccessorProof, MainWindowComposerSuccessorProofLimits,
};

pub enum MainWindowComposerDispatchOutcome {
    Page(RangePage),
    ObjectPage(gpui_text_input::ObjectPage),
    MutationBegan(MutationKey),
    MutationPage {
        key: MutationKey,
        acceptance: MutationPageAcceptance,
    },
    MutationInputFinished(MutationKey),
    Mutation {
        key: MutationKey,
        outcome: ComposerHostMutationOutcome,
    },
    History {
        intent: gpui_text_input::RangeHistoryIntent,
        outcome: RangeHistoryOutcome,
    },
    ClipboardWrite(ClipboardWriteRequest),
    Released,
}

pub struct MainWindowComposerInitialPresentation {
    selection: MainWindowComposerSelectionIdentity,
    responses: Box<[ComposerHostResponse]>,
}

pub(in crate::main_window) fn translate_initial_composer_response(
    selection: MainWindowComposerSelectionIdentity,
    request: RangeTextInputRequest,
    response: &ComposerHostResponse,
) -> Result<MainWindowComposerDispatchOutcome, MainWindowComposerDispatchError> {
    translate::initial_response(selection.binding(), request, response)
}

impl MainWindowComposerInitialPresentation {
    pub const fn selection(&self) -> MainWindowComposerSelectionIdentity {
        self.selection
    }

    pub fn responses(&self) -> &[ComposerHostResponse] {
        &self.responses
    }

    pub(in crate::main_window) fn into_responses(self) -> Box<[ComposerHostResponse]> {
        self.responses
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MainWindowComposerDispatchError {
    #[error("composer request belongs to a stale or wrong selection")]
    StaleSelection,
    #[error("another composer request is being dispatched")]
    Busy,
    #[error("composer request cannot be represented by the bounded Syndic protocol")]
    Malformed,
    #[error("composer successor proof failed at {0}")]
    SuccessorProof(&'static str),
    #[error("composer response translation failed at {0}")]
    ResponseTranslation(&'static str),
    #[error("composer object page violated its request contract: {0}")]
    ObjectPage(String),
    #[error("composer marker metadata authentication failed: {0}")]
    MarkerMetadata(String),
    #[error("composer host failed: {0}")]
    Host(#[from] ComposerHostError),
}

pub(super) struct MainWindowComposerDispatcher {
    pub(super) binding: crate::composer_host::ComposerHostBinding,
    last_host_request_id: u64,
    in_dispatch: bool,
    mutation_begin: Option<(
        MutationKey,
        gpui_text_input::MutationCursor,
        gpui_text_input::MutationCursor,
    )>,
    mutation_finish: Option<(MutationKey, MutationIdentity)>,
    early_terminal: Option<MainWindowComposerEarlyTerminal>,
    pub(super) publication_capture: Option<crate::composer_host::ComposerHostBinding>,
}

impl MainWindowComposerDispatcher {
    pub(super) fn new(
        binding: crate::composer_host::ComposerHostBinding,
        host: &SyndicComposerHost,
    ) -> Self {
        let last_host_request_id = host
            .initial_responses()
            .iter()
            .map(|response| response.key().request_id().get())
            .max()
            .unwrap_or(0);
        Self {
            binding,
            last_host_request_id,
            in_dispatch: false,
            mutation_begin: None,
            mutation_finish: None,
            early_terminal: None,
            publication_capture: None,
        }
    }

    pub(super) fn replace_binding(&mut self, binding: crate::composer_host::ComposerHostBinding) {
        self.binding = binding;
        self.last_host_request_id = 0;
    }

    fn allocate_host_request_id(&mut self) -> Result<u64, MainWindowComposerDispatchError> {
        let request_id = self
            .last_host_request_id
            .checked_add(1)
            .ok_or(MainWindowComposerDispatchError::Malformed)?;
        self.last_host_request_id = request_id;
        Ok(request_id)
    }
}

impl MainWindowComposerSlot {
    pub fn take_selected_initial_presentation(
        &mut self,
        selection: MainWindowComposerSelectionIdentity,
    ) -> Result<MainWindowComposerInitialPresentation, MainWindowComposerDispatchError> {
        let selected = self
            .selected
            .as_mut()
            .filter(|selected| selected.identity == selection)
            .ok_or(MainWindowComposerDispatchError::StaleSelection)?;
        Ok(MainWindowComposerInitialPresentation {
            selection,
            responses: selected.host.take_initial_responses().into_boxed_slice(),
        })
    }

    pub fn selected_draft_state(
        &self,
        selection: MainWindowComposerSelectionIdentity,
    ) -> Result<crate::main_window::MainWindowComposerDraftState, MainWindowComposerDispatchError>
    {
        let selected = self
            .selected
            .as_ref()
            .filter(|selected| selected.identity == selection)
            .ok_or(MainWindowComposerDispatchError::StaleSelection)?;
        Ok(selected.draft_state)
    }

    pub fn dispatch_selected_request(
        &mut self,
        store: &HomeStore,
        selection: MainWindowComposerSelectionIdentity,
        request: RangeTextInputRequest,
        marker_metadata: Box<[ComposerHostImageMarkerMetadata]>,
        cancellation: &CommandCancellation,
    ) -> Result<MainWindowComposerDispatchOutcome, MainWindowComposerDispatchError> {
        let authenticated = self
            .marker_authority
            .authenticate(store, selection, &request, marker_metadata)
            .map_err(MainWindowComposerDispatchError::MarkerMetadata)?;
        let marker_metadata = authenticated
            .into_metadata(selection, &request)
            .map_err(MainWindowComposerDispatchError::MarkerMetadata)?;
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
        selected.dispatcher.in_dispatch = true;
        let result = dispatch(
            &mut selected.host,
            &mut selected.dispatcher,
            store,
            request,
            marker_metadata,
            cancellation,
        );
        selected.dispatcher.in_dispatch = false;
        if result.is_ok() {
            if let Some(binding) = selected.host.binding()
                && binding != selected.identity.binding
            {
                let predecessor = selected.identity.binding;
                selected
                    .draft_state
                    .adopt(predecessor, binding)
                    .map_err(|_| MainWindowComposerDispatchError::StaleSelection)?;
                selected.identity.binding = binding;
                selected.dispatcher.replace_binding(binding);
            }
        }
        result
    }

    pub(in crate::main_window) fn read_selected_predecessor_object_page(
        &mut self,
        store: &HomeStore,
        selection: MainWindowComposerSelectionIdentity,
        request: gpui_text_input::ObjectRequest,
    ) -> Result<gpui_text_input::ObjectPage, MainWindowComposerDispatchError> {
        let selected = self
            .selected
            .as_mut()
            .filter(|selected| selected.identity == selection)
            .ok_or(MainWindowComposerDispatchError::StaleSelection)?;
        if selected.dispatcher.in_dispatch
            || selected.dispatcher.binding != selection.binding()
            || selected.host.binding() != Some(selection.binding())
        {
            return Err(MainWindowComposerDispatchError::Busy);
        }
        selected.dispatcher.in_dispatch = true;
        let request_id = selected.dispatcher.allocate_host_request_id();
        let result = request_id.and_then(|request_id| {
            translate::historical_object_page(
                &mut selected.host,
                store,
                selection.binding(),
                request_id,
                request,
            )
        });
        selected.dispatcher.in_dispatch = false;
        result
    }
}

fn dispatch(
    host: &mut SyndicComposerHost,
    dispatcher: &mut MainWindowComposerDispatcher,
    store: &HomeStore,
    request: RangeTextInputRequest,
    marker_metadata: Box<[ComposerHostImageMarkerMetadata]>,
    cancellation: &CommandCancellation,
) -> Result<MainWindowComposerDispatchOutcome, MainWindowComposerDispatchError> {
    Ok(match request {
        RangeTextInputRequest::Page(request) => {
            MainWindowComposerDispatchOutcome::Page(translate::text_page(
                host,
                store,
                dispatcher.binding,
                dispatcher.allocate_host_request_id()?,
                request,
            )?)
        }
        RangeTextInputRequest::ObjectPage(request) => {
            MainWindowComposerDispatchOutcome::ObjectPage(translate::object_page(
                host,
                store,
                dispatcher.binding,
                dispatcher.allocate_host_request_id()?,
                request,
            )?)
        }
        RangeTextInputRequest::CancelPage(_)
        | RangeTextInputRequest::ReleasePage(_)
        | RangeTextInputRequest::CancelObjectPage(_)
        | RangeTextInputRequest::ReleaseObjectPage(_)
        | RangeTextInputRequest::CancelClipboardWrite(_) => {
            MainWindowComposerDispatchOutcome::Released
        }
        RangeTextInputRequest::MutationBegin(request) => {
            let key = request.proposal().key();
            dispatcher.mutation_begin =
                Some((key, request.source_cursor(), request.proposal_cursor()));
            dispatcher.mutation_finish = None;
            dispatcher.early_terminal = None;
            host.begin_mutation(store, dispatcher.binding, request)?;
            MainWindowComposerDispatchOutcome::MutationBegan(key)
        }
        RangeTextInputRequest::MutationSourcePage(request)
        | RangeTextInputRequest::MutationProposalPage(request) => {
            let key = request.page().key().key();
            let acceptance = if dispatcher.early_terminal.is_some() {
                accept_early_terminal_page(dispatcher, request.page())?
            } else {
                match host.stage_mutation_page(store, request.clone(), marker_metadata) {
                    Ok(acceptance) => acceptance,
                    Err(
                        ComposerHostError::MutationNotPending
                        | ComposerHostError::MutationUnavailable,
                    ) => {
                        capture_early_terminal(host, dispatcher, store, key)?;
                        accept_early_terminal_page(dispatcher, request.page())?
                    }
                    Err(error) => return Err(error.into()),
                }
            };
            MainWindowComposerDispatchOutcome::MutationPage { key, acceptance }
        }
        RangeTextInputRequest::MutationFinishInput(finish) => {
            if dispatcher.early_terminal.is_some() {
                validate_early_terminal_finish(dispatcher, finish)?;
            } else if let Err(error) = host.finish_mutation_input(store, finish) {
                if !matches!(
                    error,
                    ComposerHostError::MutationNotPending | ComposerHostError::MutationUnavailable
                ) {
                    return Err(error.into());
                }
                capture_early_terminal(host, dispatcher, store, finish.key())?;
                validate_early_terminal_finish(dispatcher, finish)?;
            }
            dispatcher.mutation_finish =
                Some((finish.key(), finish.proposal().cumulative_identity));
            MainWindowComposerDispatchOutcome::MutationInputFinished(finish.key())
        }
        RangeTextInputRequest::MutationCommit(request) => {
            let outcome = if let Some(terminal) = dispatcher.early_terminal.as_ref() {
                let finish = finish_for(dispatcher, request.key())?;
                if terminal.key != request.key() || finish != request.finish_identity() {
                    return Err(MainWindowComposerDispatchError::Malformed);
                }
                terminal.outcome.clone()
            } else {
                host.execute_mutation(store, request, cancellation)?
            };
            dispatcher.early_terminal = None;
            dispatcher.mutation_begin = None;
            dispatcher.mutation_finish = None;
            MainWindowComposerDispatchOutcome::Mutation {
                key: request.key(),
                outcome,
            }
        }
        RangeTextInputRequest::CancelMutation(request) => {
            let finish = dispatcher
                .mutation_finish
                .filter(|(retained_key, _)| *retained_key == request.key())
                .map_or(MutationIdentity::ROOT, |(_, finish)| finish);
            let cancelled = CommandCancellation::new();
            cancelled.cancel();
            let outcome = if let Some(terminal) = dispatcher.early_terminal.as_ref() {
                if terminal.key != request.key() {
                    return Err(MainWindowComposerDispatchError::Malformed);
                }
                terminal.outcome.clone()
            } else {
                match host.execute_mutation(
                    store,
                    MutationCommitRequest::new(request.key(), finish),
                    &cancelled,
                ) {
                    Ok(outcome) => outcome,
                    Err(ComposerHostError::MutationNotPending) => {
                        return Ok(MainWindowComposerDispatchOutcome::Released);
                    }
                    Err(error) => return Err(error.into()),
                }
            };
            dispatcher.early_terminal = None;
            dispatcher.mutation_begin = None;
            dispatcher.mutation_finish = None;
            MainWindowComposerDispatchOutcome::Mutation {
                key: request.key(),
                outcome,
            }
        }
        RangeTextInputRequest::DetachedMutation(key) => {
            let finish = finish_for(dispatcher, key)?;
            let outcome = if let Some(terminal) = dispatcher.early_terminal.as_ref() {
                if terminal.key != key {
                    return Err(MainWindowComposerDispatchError::Malformed);
                }
                terminal.outcome.clone()
            } else {
                host.execute_mutation(store, MutationCommitRequest::new(key, finish), cancellation)?
            };
            dispatcher.early_terminal = None;
            dispatcher.mutation_begin = None;
            dispatcher.mutation_finish = None;
            MainWindowComposerDispatchOutcome::Mutation { key, outcome }
        }
        RangeTextInputRequest::HistoryIntent(intent) => {
            host.begin_history_selection(store, dispatcher.binding, intent)?;
            MainWindowComposerDispatchOutcome::History {
                intent,
                outcome: host.execute_history_selection(store, intent.key(), cancellation)?,
            }
        }
        RangeTextInputRequest::CancelHistoryIntent(intent) => {
            let cancelled = CommandCancellation::new();
            cancelled.cancel();
            match host.execute_history_selection(store, intent.key(), &cancelled) {
                Ok(outcome) => MainWindowComposerDispatchOutcome::History { intent, outcome },
                Err(ComposerHostError::HistoryNotPending) => {
                    MainWindowComposerDispatchOutcome::Released
                }
                Err(error) => return Err(error.into()),
            }
        }
        RangeTextInputRequest::ClipboardWrite(request) => {
            MainWindowComposerDispatchOutcome::ClipboardWrite(request)
        }
    })
}
