use beryl_home_store::CommandCancellation;
use gpui::{Context, Entity, Window};
use gpui_text_input::{
    MutationOutcome, ObjectPageFailure, ObjectRequestKey, PageFailure, PageRequestKey,
    RangeTextInput, RangeTextInputError, RangeTextInputRequest,
};

use crate::composer_host::ComposerHostMutationOutcome;

use super::{
    MainWindowComposerDispatchOutcome, MainWindowConversationComposer,
    MainWindowConversationComposerPhase, MainWindowConversationComposerRoute,
    MainWindowConversationComposerService,
};

struct MainWindowConversationComposerDispatch {
    initiating_selection: crate::main_window::MainWindowComposerSelectionIdentity,
    settled_selection: crate::main_window::MainWindowComposerSelectionIdentity,
    outcome: MainWindowComposerDispatchOutcome,
    proof: Option<(
        gpui_text_input::MutationKey,
        crate::main_window::MainWindowComposerSuccessorProof,
    )>,
    edit_proof: Option<crate::main_window::MainWindowComposerSuccessorProof>,
    cut_page: Option<super::clipboard::PreparedPropagatedCutPage>,
    cut_page_expected: bool,
}

#[derive(Clone, Copy)]
enum MainWindowConversationComposerFailureSettlement {
    Page(PageRequestKey),
    ObjectPage(ObjectRequestKey),
}

enum MainWindowConversationComposerTaskError {
    RouteNotAdmitted,
    CustodyNotDispatched {
        settlement: Option<MainWindowConversationComposerFailureSettlement>,
    },
    Exact {
        error: String,
        settlement: Option<MainWindowConversationComposerFailureSettlement>,
    },
}

type MainWindowConversationComposerTaskResult =
    Result<Box<MainWindowConversationComposerDispatch>, MainWindowConversationComposerTaskError>;

impl MainWindowConversationComposerTaskError {
    fn exact(
        error: String,
        settlement: Option<MainWindowConversationComposerFailureSettlement>,
    ) -> Self {
        Self::Exact { error, settlement }
    }
}

impl MainWindowConversationComposer {
    pub(super) fn pump_one(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_pump() || self.active_flight.is_some() || self.last_error.is_some() {
            return;
        }
        if matches!(self.phase, MainWindowConversationComposerPhase::Fencing)
            && self
                .input
                .update(cx, |input, _| input.is_semantically_quiescent())
        {
            return;
        }
        if let MainWindowConversationComposerRoute::Pending(receipt) = self.route
            && !self
                .service
                .pending_request_is_admitted(receipt, self.selection)
        {
            return;
        }
        let request = self.input.update(cx, |input, _| input.take_request());
        let Some(request) = request else {
            self.pump_edit_proof(window, cx);
            return;
        };
        let request = match self.deliver_next_initial_response(request, window, cx) {
            Ok(Some(request)) => request,
            Ok(None) => {
                self.schedule_pump(window, cx);
                return;
            }
            Err(error) => {
                self.last_error = Some(error);
                return;
            }
        };
        if let Err(error) = self.observe_operation(&request) {
            self.last_error = Some(error);
            return;
        }
        if let RangeTextInputRequest::HistoryIntent(intent) = request
            && let Err(error) = self.input.update(cx, |input, _| {
                input.submit_history_session(gpui_text_input::RangeHistorySession::new(intent))
            })
        {
            self.last_error = Some("composer history admission was rejected".into());
            return;
        }
        let cut_page_request = matches!(request, RangeTextInputRequest::MutationProposalPage(_))
            .then(|| {
                self.propagated_cut
                    .as_ref()
                    .and_then(super::clipboard::ActivePropagatedCut::next_page_request)
            })
            .flatten();
        let cut_page_expected = cut_page_request.is_some();
        let flight = match self.begin_flight() {
            Ok(flight) => flight,
            Err(error) => {
                self.last_error = Some(error);
                return;
            }
        };
        let service = self.service.clone();
        let selection = self.selection;
        let route = self.route;
        let settlement = match &request {
            RangeTextInputRequest::Page(request) => Some(
                MainWindowConversationComposerFailureSettlement::Page(request.key()),
            ),
            RangeTextInputRequest::ObjectPage(request) => Some(
                MainWindowConversationComposerFailureSettlement::ObjectPage(request.key()),
            ),
            _ => None,
        };
        let proof_limits = self.proof_limits;
        let marker_metadata = self.marker_metadata_for_request(&request);
        let cancellation = CommandCancellation::new();
        #[cfg(feature = "test-faults")]
        if matches!(request, RangeTextInputRequest::MutationCommit(_))
            && self.service.take_test_mutation_commit_cancellation()
        {
            cancellation.cancel();
        }
        let task = cx.background_executor().spawn(async move {
            #[cfg(feature = "test-faults")]
            if matches!(route, MainWindowConversationComposerRoute::Pending(_))
                && let Some(gate) = service.take_test_pending_dispatch_gate()
            {
                gate.await;
            }
            let (outcome, proof, settled_selection) = {
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
                        MainWindowConversationComposerTaskError::CustodyNotDispatched {
                            settlement,
                        },
                    );
                }
                let outcome = match route {
                    MainWindowConversationComposerRoute::Selected => slot
                        .dispatch_selected_request(
                            &service.store,
                            selection,
                            request,
                            marker_metadata,
                            &cancellation,
                        )
                        .map_err(|error| {
                            MainWindowConversationComposerTaskError::exact(
                                format!("composer dispatch failed: {error}"),
                                settlement,
                            )
                        })?,
                    MainWindowConversationComposerRoute::Pending(receipt) => slot
                        .dispatch_pending_request(
                            &service.store,
                            receipt,
                            selection,
                            request,
                            &cancellation,
                        )
                        .map_err(|error| {
                            MainWindowConversationComposerTaskError::exact(
                                format!("pending composer dispatch failed: {error}"),
                                settlement,
                            )
                        })?,
                };
                let proof = match &outcome {
                    MainWindowComposerDispatchOutcome::Mutation {
                        key,
                        outcome: ComposerHostMutationOutcome::Committed { positions, .. },
                    } => {
                        let successor = slot.selected_identity().ok_or_else(|| {
                            MainWindowConversationComposerTaskError::exact(
                                "committed composer selection disappeared".to_owned(),
                                settlement,
                            )
                        })?;
                        Some((
                            *key,
                            slot.build_selected_successor_proof(
                                &service.store,
                                successor,
                                *positions,
                                proof_limits,
                            )
                            .map_err(|_| {
                                MainWindowConversationComposerTaskError::exact(
                                    "composer successor proof failed".to_owned(),
                                    settlement,
                                )
                            })?,
                        ))
                    }
                    _ => None,
                };
                let settled_selection = match route {
                    MainWindowConversationComposerRoute::Selected => slot.selected_identity(),
                    MainWindowConversationComposerRoute::Pending(receipt) => {
                        slot.pending_identity(receipt)
                    }
                }
                .ok_or_else(|| {
                    MainWindowConversationComposerTaskError::exact(
                        "composer selection disappeared after dispatch".to_owned(),
                        settlement,
                    )
                })?;
                (outcome, proof, settled_selection)
            };
            #[cfg(feature = "test-faults")]
            if matches!(route, MainWindowConversationComposerRoute::Pending(_))
                && let Some(gate) = service.take_test_pending_completion_gate()
            {
                gate.await;
                return Err(MainWindowConversationComposerTaskError::exact(
                    "pending composer dispatched completion failed".to_owned(),
                    settlement,
                ));
            }
            let cut_page = cut_page_request
                .map(|request| {
                    super::clipboard::prepare_next_cut_page(&service, selection, request)
                })
                .transpose()
                .map_err(|error| {
                    MainWindowConversationComposerTaskError::exact(error, settlement)
                })?;
            Ok(Box::new(MainWindowConversationComposerDispatch {
                initiating_selection: selection,
                settled_selection,
                outcome,
                proof,
                edit_proof: None,
                cut_page,
                cut_page_expected,
            }))
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if !this.settle_flight(flight) {
                    return;
                }
                if let Err(error) = this.finish(route, result, window, cx) {
                    this.last_error = Some(error);
                }
                this.schedule_pump(window, cx);
            });
        })
        .detach();
    }

    fn observe_operation(&mut self, request: &RangeTextInputRequest) -> Result<(), String> {
        let operation = match request {
            RangeTextInputRequest::MutationBegin(begin) => Some(begin.proposal().key().operation()),
            RangeTextInputRequest::HistoryIntent(intent) => Some(intent.key().operation()),
            _ => None,
        };
        let Some(operation) = operation else {
            return Ok(());
        };
        if operation.get() != self.next_operation {
            return Err("composer widget operation sequence became stale".into());
        }
        self.next_operation = self
            .next_operation
            .checked_add(1)
            .ok_or_else(|| "composer widget operation identity exhausted".to_owned())?;
        Ok(())
    }

    fn marker_metadata_for_request(
        &mut self,
        request: &RangeTextInputRequest,
    ) -> Box<[crate::composer_host::ComposerHostImageMarkerMetadata]> {
        let key = match request {
            RangeTextInputRequest::MutationProposalPage(request) => Some(request.page().key()),
            _ => None,
        };
        let Some((expected, _)) = self.pending_marker_metadata.as_ref() else {
            return Vec::new().into_boxed_slice();
        };
        if key.is_none_or(|key| key.key() != *expected) {
            return Vec::new().into_boxed_slice();
        }
        self.pending_marker_metadata
            .take()
            .map_or_else(|| Vec::new().into_boxed_slice(), |(_, metadata)| metadata)
    }

    fn pump_edit_proof(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_live() || !matches!(self.route, MainWindowConversationComposerRoute::Selected) {
            return;
        }
        let selection = self.selection;
        let Some(positions) = self.input.update(cx, |input, _| {
            input
                .surface()
                .filter(|surface| {
                    input.is_surface_current_and_interactive()
                        && surface.binding() == selection.binding().range_binding()
                })
                .map(|surface| {
                    let selection = surface.selection();
                    gpui_text_input::MutationPositions::new(
                        surface.caret(),
                        selection.anchor,
                        selection.head,
                    )
                })
        }) else {
            return;
        };
        if self.admitted_positions == Some(positions) {
            return;
        }
        self.admitted_positions = Some(positions);
        let flight = match self.begin_flight() {
            Ok(flight) => flight,
            Err(error) => {
                self.last_error = Some(error);
                return;
            }
        };
        let service = self.service.clone();
        let limits = self.proof_limits;
        let task = cx.background_executor().spawn(async move {
            let mut slot = service.slot.lock().map_err(|_| {
                MainWindowConversationComposerTaskError::exact(
                    "conversation composer service lock failed".to_owned(),
                    None,
                )
            })?;
            if slot.selected_identity() != Some(selection) {
                return Err(MainWindowConversationComposerTaskError::RouteNotAdmitted);
            }
            let proof = slot
                .build_selected_successor_proof(&service.store, selection, positions, limits)
                .map_err(|_| {
                    MainWindowConversationComposerTaskError::exact(
                        "composer successor proof failed".to_owned(),
                        None,
                    )
                })?;
            Ok(Box::new(MainWindowConversationComposerDispatch {
                initiating_selection: selection,
                settled_selection: selection,
                outcome: MainWindowComposerDispatchOutcome::Released,
                proof: None,
                edit_proof: Some(proof),
                cut_page: None,
                cut_page_expected: false,
            }))
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if !this.settle_flight(flight) {
                    return;
                }
                if let Err(error) = this.finish(
                    MainWindowConversationComposerRoute::Selected,
                    result,
                    window,
                    cx,
                ) {
                    this.last_error = Some(error);
                }
                this.schedule_pump(window, cx);
            });
        })
        .detach();
    }

    fn finish(
        &mut self,
        route: MainWindowConversationComposerRoute,
        result: MainWindowConversationComposerTaskResult,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let route_is_current = |service: &MainWindowConversationComposerService,
                                initiated: MainWindowConversationComposerRoute,
                                current: MainWindowConversationComposerRoute,
                                selection| match (initiated, current)
        {
            (
                MainWindowConversationComposerRoute::Selected,
                MainWindowConversationComposerRoute::Selected,
            ) => service.selected_identity() == Some(selection),
            (
                MainWindowConversationComposerRoute::Pending(receipt),
                MainWindowConversationComposerRoute::Pending(current_receipt),
            ) if receipt == current_receipt => service.pending_identity(receipt) == Some(selection),
            (
                MainWindowConversationComposerRoute::Pending(_),
                MainWindowConversationComposerRoute::Selected,
            ) => service.selected_identity() == Some(selection),
            _ => false,
        };
        let result = match result {
            Ok(result) => result,
            Err(MainWindowConversationComposerTaskError::RouteNotAdmitted) => return Ok(()),
            Err(MainWindowConversationComposerTaskError::CustodyNotDispatched { settlement }) => {
                self.settle_exact_dispatch_failure(settlement, cx)?;
                return Ok(());
            }
            Err(MainWindowConversationComposerTaskError::Exact { error, settlement }) => {
                self.settle_exact_dispatch_failure(settlement, cx)?;
                if !route_is_current(&self.service, route, self.route, self.selection) {
                    return Ok(());
                }
                return Err(error);
            }
        };
        if result.initiating_selection != self.selection
            || !route_is_current(&self.service, route, self.route, result.settled_selection)
        {
            return Ok(());
        }
        if result.cut_page_expected && result.cut_page.is_none() {
            return Err("composer cut lookahead request returned no page".to_owned());
        }
        if let Some(page) = result.cut_page {
            let cut = self
                .propagated_cut
                .as_mut()
                .ok_or_else(|| "composer cut page returned without an active cut".to_owned())?;
            cut.admit_prepared_page(page)?;
        }
        if let Some(proof) = result.edit_proof {
            let positions = proof.positions;
            let mut unique = Vec::with_capacity(3);
            for position in [
                positions.caret(),
                positions.selection_anchor(),
                positions.selection_head(),
            ] {
                if !unique.contains(&position) {
                    unique.push(position);
                }
            }
            self.input
                .update(cx, |input, _| {
                    input.admit_edit_positions(&unique, &proof.text, &proof.objects)
                })
                .map_err(|_| "composer edit-position proof was rejected".to_owned())?;
            self.admitted_positions = Some(positions);
            return Ok(());
        }
        if let Some((key, proof)) = result.proof {
            if self
                .propagated_cut
                .as_ref()
                .is_some_and(|cut| cut.key() == key)
            {
                self.propagated_cut = None;
            }
            let history_frontier = proof.selection.binding().range_history_frontier();
            let previous = self.selection;
            self.selection = proof.selection;
            self.image_surfaces.selection_changed(self.selection);
            self.admitted_positions = Some(proof.positions);
            self.input
                .update(cx, |input, cx| {
                    input.settle_committed_mutation(
                        key,
                        proof.binding,
                        proof.positions,
                        &proof.text,
                        &proof.objects,
                        window,
                        cx,
                    )?;
                    input.set_history_frontier(input.history_frontier(), history_frontier)
                })
                .map_err(|_| "composer history frontier was rejected".to_owned())?;
            if self.pending_marker_removal == Some(key) {
                self.pending_marker_removal = None;
                self.image_surface_attachment = None;
            }
            cx.emit(
                super::MainWindowConversationComposerEvent::SelectionAdvanced {
                    previous,
                    current: self.selection,
                },
            );
            return Ok(());
        }
        let outcome = result.outcome;
        let input = self.input.clone();
        match outcome {
            MainWindowComposerDispatchOutcome::Page(page) => {
                match input.update(cx, |input, cx| input.deliver_page(page, window, cx)) {
                    Ok(()) | Err(RangeTextInputError::PageResponseRejected(_)) => Ok(()),
                    Err(_) => Err("composer text page was rejected".to_owned()),
                }
            }
            MainWindowComposerDispatchOutcome::ObjectPage(page) => {
                match input.update(cx, |input, cx| {
                    input.deliver_object_page_in_window(page, window, cx)
                }) {
                    Ok(()) | Err(RangeTextInputError::ObjectResponseRejected(_)) => Ok(()),
                    Err(_) => Err("composer marker page was rejected".to_owned()),
                }
            }
            MainWindowComposerDispatchOutcome::MutationBegan(key) => {
                input
                    .update(cx, |input, cx| input.accept_mutation_preflight(key, cx))
                    .map_err(|_| "composer mutation preflight was rejected".to_owned())?;
                self.submit_propagated_cut_page(key, cx)
            }
            MainWindowComposerDispatchOutcome::MutationPage { key, .. } => {
                self.submit_propagated_cut_page(key, cx)
            }
            MainWindowComposerDispatchOutcome::Released => Ok(()),
            MainWindowComposerDispatchOutcome::MutationInputFinished(key) => input
                .update(cx, |input, cx| input.accept_mutation_finish(key, cx))
                .map_err(|_| "composer mutation finish was rejected".to_owned()),
            MainWindowComposerDispatchOutcome::Mutation { key, outcome } => match outcome {
                ComposerHostMutationOutcome::Committed { .. } => {
                    Err("committed composer mutation omitted successor proof".to_owned())
                }
                ComposerHostMutationOutcome::Rejected => {
                    self.clear_propagated_cut(key);
                    settle_mutation(&input, key, MutationOutcome::Rejected, window, cx)?;
                    self.finish_marker_removal_noncommit(key, window, cx)
                }
                ComposerHostMutationOutcome::Conflict => {
                    self.clear_propagated_cut(key);
                    settle_mutation(&input, key, MutationOutcome::Conflict, window, cx)?;
                    self.finish_marker_removal_noncommit(key, window, cx)
                }
                ComposerHostMutationOutcome::Cancelled => {
                    self.clear_propagated_cut(key);
                    settle_mutation(&input, key, MutationOutcome::Cancelled, window, cx)?;
                    self.finish_marker_removal_noncommit(key, window, cx)
                }
                ComposerHostMutationOutcome::Error => {
                    self.clear_propagated_cut(key);
                    settle_mutation(&input, key, MutationOutcome::Error, window, cx)?;
                    self.finish_marker_removal_noncommit(key, window, cx)
                }
            },
            MainWindowComposerDispatchOutcome::History { intent, outcome } => {
                input
                    .update(cx, |input, cx| {
                        input.settle_history(intent, outcome, window, cx)
                    })
                    .map_err(|_| "composer history settlement was rejected".to_owned())?;
                let previous = self.selection;
                self.selection = result.settled_selection;
                self.image_surfaces.selection_changed(self.selection);
                if previous != self.selection {
                    cx.emit(
                        super::MainWindowConversationComposerEvent::SelectionAdvanced {
                            previous,
                            current: self.selection,
                        },
                    );
                }
                Ok(())
            }
            MainWindowComposerDispatchOutcome::ClipboardWrite(write) => {
                let key = write.key();
                let outcome = (self.clipboard_writer)(write.text(), cx);
                input
                    .update(cx, |input, cx| {
                        input.settle_clipboard_write(key, outcome, cx)
                    })
                    .map(|_| ())
                    .map_err(|_| "composer clipboard settlement was rejected".to_owned())
            }
        }
    }

    fn submit_propagated_cut_page(
        &mut self,
        key: gpui_text_input::MutationKey,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(cut) = self.propagated_cut.as_mut().filter(|cut| cut.key() == key) else {
            return Ok(());
        };
        self.input
            .update(cx, |input, input_cx| cut.submit_next(input, input_cx))
    }

    fn clear_propagated_cut(&mut self, key: gpui_text_input::MutationKey) {
        if self
            .propagated_cut
            .as_ref()
            .is_some_and(|cut| cut.key() == key)
        {
            self.propagated_cut = None;
        }
    }

    fn finish_marker_removal_noncommit(
        &mut self,
        key: gpui_text_input::MutationKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.pending_marker_removal != Some(key) {
            return Ok(());
        }
        self.pending_marker_removal = None;
        if let Some(attachment) = self.image_surface_attachment.take()
            && let Err(error) = self.input.update(cx, |input, input_cx| {
                input.dismiss_active_inline_object_surface(
                    attachment,
                    gpui_text_input::InlineObjectSurfaceDismissal::ClearObject,
                    window,
                    input_cx,
                )
            })
            && !matches!(error, gpui_text_input::RangeTextInputError::Stale)
        {
            return Err("composer marker surface dismissal was rejected".into());
        }
        self.input.update(cx, |input, _| input.focus(window));
        Ok(())
    }

    fn settle_exact_dispatch_failure(
        &mut self,
        settlement: Option<MainWindowConversationComposerFailureSettlement>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(settlement) = settlement else {
            return Ok(());
        };
        self.input
            .update(cx, |input, input_cx| match settlement {
                MainWindowConversationComposerFailureSettlement::Page(key) => {
                    input.fail_page(key, PageFailure::Unavailable, input_cx)
                }
                MainWindowConversationComposerFailureSettlement::ObjectPage(key) => {
                    input.fail_object_page(key, ObjectPageFailure::Unavailable, input_cx)
                }
            })
            .map_err(|_| "composer failed request settlement was rejected".to_owned())
    }
}

fn settle_mutation(
    input: &Entity<RangeTextInput>,
    key: gpui_text_input::MutationKey,
    outcome: MutationOutcome,
    window: &mut Window,
    cx: &mut Context<MainWindowConversationComposer>,
) -> Result<(), String> {
    input
        .update(cx, |input, cx| {
            input.settle_mutation(key, outcome, window, cx)
        })
        .map(|_| ())
        .map_err(|_| "composer mutation settlement was rejected".to_owned())
}
