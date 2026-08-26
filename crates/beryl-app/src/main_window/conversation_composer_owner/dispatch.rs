use beryl_home_store::CommandCancellation;
use gpui::{Context, Entity, Window};
use gpui_text_input::{MutationOutcome, RangeTextInput, RangeTextInputRequest};

use crate::composer_host::ComposerHostMutationOutcome;

use super::{MainWindowComposerDispatchOutcome, MainWindowConversationComposer};

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

impl MainWindowConversationComposer {
    pub(super) fn pump_one(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_pump() || self.active_flight.is_some() || self.last_error.is_some() {
            return;
        }
        let request = self.input.update(cx, |input, _| input.take_request());
        let Some(request) = request else {
            self.pump_edit_proof(window, cx);
            return;
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
            self.last_error = Some(format!("composer history admission was rejected: {error}"));
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
        let proof_limits = self.proof_limits;
        let request_diagnostic = format!("{request:?}");
        let marker_metadata = self.marker_metadata_for_request(&request);
        let cancellation = CommandCancellation::new();
        #[cfg(feature = "test-faults")]
        if matches!(request, RangeTextInputRequest::MutationCommit(_))
            && self.service.take_test_mutation_commit_cancellation()
        {
            cancellation.cancel();
        }
        let task = cx.background_executor().spawn(async move {
            let mut slot = service
                .slot
                .lock()
                .map_err(|_| "conversation composer service lock failed".to_owned())?;
            let outcome = slot
                .dispatch_selected_request(
                    &service.store,
                    selection,
                    request,
                    marker_metadata,
                    &cancellation,
                )
                .map_err(|error| {
                    format!("composer dispatch failed for {request_diagnostic}: {error}")
                })?;
            let proof = match &outcome {
                MainWindowComposerDispatchOutcome::Mutation {
                    key,
                    outcome: ComposerHostMutationOutcome::Committed { positions, .. },
                } => {
                    let successor = slot
                        .selected_identity()
                        .ok_or_else(|| "committed composer selection disappeared".to_owned())?;
                    Some((
                        *key,
                        slot.build_selected_successor_proof(
                            &service.store,
                            successor,
                            *positions,
                            proof_limits,
                        )
                        .map_err(|error| format!("composer successor proof failed: {error}"))?,
                    ))
                }
                _ => None,
            };
            let settled_selection = slot
                .selected_identity()
                .ok_or_else(|| "composer selection disappeared after dispatch".to_owned())?;
            drop(slot);
            let cut_page = cut_page_request
                .map(|request| {
                    super::clipboard::prepare_next_cut_page(&service, selection, request)
                })
                .transpose()?;
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
                if let Err(error) = this.finish(result, window, cx) {
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
        if !self.is_live() {
            return;
        }
        let Some(positions) = self.input.update(cx, |input, _| {
            input.surface().map(|surface| {
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
        let selection = self.selection;
        let limits = self.proof_limits;
        let task = cx.background_executor().spawn(async move {
            let mut slot = service
                .slot
                .lock()
                .map_err(|_| "conversation composer service lock failed".to_owned())?;
            let proof = slot
                .build_selected_successor_proof(&service.store, selection, positions, limits)
                .map_err(|error| error.to_string())?;
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
                if let Err(error) = this.finish(result, window, cx) {
                    this.last_error = Some(error);
                }
                this.schedule_pump(window, cx);
            });
        })
        .detach();
    }

    fn finish(
        &mut self,
        result: Result<Box<MainWindowConversationComposerDispatch>, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let result = match result {
            Ok(result) => result,
            Err(_) if self.service.selected_identity() != Some(self.selection) => return Ok(()),
            Err(error) => return Err(error),
        };
        if result.initiating_selection != self.selection
            || self.service.selected_identity() != Some(result.settled_selection)
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
                .map_err(|error| format!("composer edit-position proof was rejected: {error}"))?;
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
                .map_err(|error| error.to_string())?;
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
            MainWindowComposerDispatchOutcome::Page(page) => input
                .update(cx, |input, cx| input.deliver_page(page, window, cx))
                .map_err(|error| format!("composer text page was rejected: {error}")),
            MainWindowComposerDispatchOutcome::ObjectPage(page) => input
                .update(cx, |input, cx| {
                    input.deliver_object_page_in_window(page, window, cx)
                })
                .map_err(|error| format!("composer marker page was rejected: {error}")),
            MainWindowComposerDispatchOutcome::MutationBegan(key) => {
                input
                    .update(cx, |input, cx| input.accept_mutation_preflight(key, cx))
                    .map_err(|error| {
                        format!("composer mutation preflight was rejected: {error}")
                    })?;
                self.submit_propagated_cut_page(key, cx)
            }
            MainWindowComposerDispatchOutcome::MutationPage { key, .. } => {
                self.submit_propagated_cut_page(key, cx)
            }
            MainWindowComposerDispatchOutcome::Released => Ok(()),
            MainWindowComposerDispatchOutcome::MutationInputFinished(key) => input
                .update(cx, |input, cx| input.accept_mutation_finish(key, cx))
                .map_err(|error| format!("composer mutation finish was rejected: {error}")),
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
                    .map_err(|error| {
                        format!("composer history settlement was rejected: {error}")
                    })?;
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
                    .map_err(|error| format!("composer clipboard settlement was rejected: {error}"))
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
            return Err(error.to_string());
        }
        self.input.update(cx, |input, _| input.focus(window));
        Ok(())
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
        .map_err(|error| error.to_string())
}
