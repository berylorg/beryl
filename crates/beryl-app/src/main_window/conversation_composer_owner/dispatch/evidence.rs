use gpui_text_input::{MutationEvidenceAcknowledgement, MutationPass};

use crate::composer_host::{
    ComposerHostMutationEvidenceOutcome, ComposerHostMutationEvidenceRequest,
};

use super::*;

pub(in crate::main_window::conversation_composer_owner) struct ActiveComposerMutationEvidence {
    pass: MutationPass,
    awaiting_host: bool,
    local_page_submitted: bool,
    acknowledgement: Option<MutationEvidenceAcknowledgement>,
    cancelled: bool,
    terminally_unavailable: bool,
}

impl MainWindowConversationComposer {
    pub(super) fn intercept_mutation_evidence(
        &mut self,
        request: &RangeTextInputRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        match request {
            RangeTextInputRequest::MutationBegin(begin) => {
                if self.route != MainWindowConversationComposerRoute::Selected
                    || self.mutation_evidence.is_some()
                {
                    return Err("composer evidence belongs to an inactive mutation lane".to_owned());
                }
                let pass = self
                    .input
                    .update(cx, |input, _| {
                        input.request_mutation_evidence(begin.proposal().key())
                    })
                    .map_err(|error| {
                        format!("composer immutable edit evidence is unavailable: {error}")
                    })?;
                self.last_mutation_admission_failure = None;
                self.mutation_feedback = None;
                cx.notify();
                self.mutation_evidence = Some(ActiveComposerMutationEvidence {
                    pass,
                    awaiting_host: true,
                    local_page_submitted: false,
                    acknowledgement: None,
                    cancelled: false,
                    terminally_unavailable: false,
                });
                self.dispatch_mutation_evidence(
                    ComposerHostMutationEvidenceRequest::Begin {
                        begin: *begin,
                        pass,
                    },
                    window,
                    cx,
                )?;
                Ok(true)
            }
            RangeTextInputRequest::CancelMutation(cancel)
                if self
                    .mutation_evidence
                    .as_ref()
                    .is_some_and(|evidence| evidence.pass.key() == cancel.key()) =>
            {
                if let Some(evidence) = self.mutation_evidence.as_mut() {
                    evidence.cancelled = true;
                    evidence.awaiting_host = true;
                }
                self.dispatch_mutation_evidence(
                    ComposerHostMutationEvidenceRequest::Cancel(cancel.key()),
                    window,
                    cx,
                )?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    pub(super) fn pump_mutation_producer(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        if let Some(evidence) = self.mutation_evidence.as_ref() {
            if evidence.terminally_unavailable {
                return Ok(true);
            }
            let pass = evidence.pass;
            if evidence.awaiting_host {
                self.dispatch_mutation_evidence(
                    ComposerHostMutationEvidenceRequest::Advance(pass.key()),
                    window,
                    cx,
                )?;
                return Ok(true);
            }
            let input = if let Some(cut) = self
                .propagated_cut
                .as_mut()
                .filter(|cut| cut.key() == pass.key())
            {
                if let Some(request) = cut.next_page_request() {
                    self.dispatch_cut_evidence_read(request, window, cx)?;
                    return Ok(true);
                }
                cut.next_input()?.ok_or_else(|| {
                    "composer cut immutable producer returned no evidence".to_owned()
                })?
            } else {
                let submitted = evidence.local_page_submitted;
                self.input
                    .update(cx, |input, _| {
                        let (page, finish) = input.local_mutation_evidence(pass)?;
                        Ok::<_, RangeTextInputError>(if !submitted && page.is_some() {
                            super::super::clipboard::CutMutationInput::Page(page.unwrap().clone())
                        } else {
                            super::super::clipboard::CutMutationInput::Finish(finish)
                        })
                    })
                    .map_err(|error| format!("composer local evidence was rejected: {error}"))?
            };
            let request = match input {
                super::super::clipboard::CutMutationInput::Page(page) => {
                    let acknowledgement = self
                        .input
                        .update(cx, |input, cx| {
                            input.submit_mutation_evidence_page(pass, page.clone(), cx)
                        })
                        .map_err(|error| format!("composer evidence page was rejected: {error}"))?;
                    let metadata = self.pending_marker_metadata.as_ref()
                        .filter(|(key, _)| *key == pass.key())
                        .map_or_else(|| Box::new([]) as Box<[_]>, |(_, metadata)| {
                            metadata.iter().filter(|metadata| page.items().iter().any(|item| matches!(
                                item, gpui_text_input::MutationPageItem::Object(
                                    gpui_text_input::ObjectChange::Insert { object } | gpui_text_input::ObjectChange::Replace { object, .. }
                                ) if object.id() == metadata.object_id()
                            ))).cloned().collect::<Vec<_>>().into_boxed_slice()
                        });
                    let evidence = self
                        .mutation_evidence
                        .as_mut()
                        .ok_or_else(|| "composer evidence owner disappeared".to_owned())?;
                    evidence.local_page_submitted = true;
                    evidence.acknowledgement = Some(acknowledgement);
                    ComposerHostMutationEvidenceRequest::Page {
                        pass,
                        page,
                        metadata,
                    }
                }
                super::super::clipboard::CutMutationInput::Finish(finish) => {
                    self.input
                        .update(cx, |input, _| {
                            input.submit_mutation_evidence_finish(pass, finish)
                        })
                        .map_err(|error| format!("composer evidence EOF was rejected: {error}"))?;
                    ComposerHostMutationEvidenceRequest::Finish { pass, finish }
                }
            };
            self.mutation_evidence.as_mut().unwrap().awaiting_host = true;
            self.dispatch_mutation_evidence(request, window, cx)?;
            return Ok(true);
        }
        if let Some(cut) = self.propagated_cut.as_mut().filter(|cut| cut.is_staging()) {
            if let Some(request) = cut.next_page_request() {
                self.dispatch_cut_evidence_read(request, window, cx)?;
            } else {
                let key = cut.key();
                self.submit_propagated_cut_page(key, cx)?;
                self.schedule_pump(window, cx);
            }
            return Ok(true);
        }
        Ok(false)
    }

    fn dispatch_mutation_evidence(
        &mut self,
        request: ComposerHostMutationEvidenceRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let flight = self.begin_flight()?;
        let service = self.service.clone();
        let selection = self.selection;
        let route = self.route;
        let task = cx.background_executor().spawn(async move {
            let mut slot = service.slot.lock().map_err(|_| {
                MainWindowConversationComposerTaskError::exact(
                    "conversation composer service lock failed".to_owned(),
                    None,
                )
            })?;
            if slot.selected_identity() != Some(selection) {
                return Err(
                    MainWindowConversationComposerTaskError::CustodyNotDispatched {
                        settlement: None,
                    },
                );
            }
            let outcome = slot
                .dispatch_selected_mutation_evidence(
                    &service.store,
                    selection,
                    request,
                    &CommandCancellation::new(),
                )
                .map_err(|error| {
                    MainWindowConversationComposerTaskError::exact(
                        format!("composer evidence dispatch failed: {error}"),
                        None,
                    )
                })?;
            Ok(Box::new(MainWindowConversationComposerDispatch {
                initiating_selection: selection,
                settled_selection: selection,
                outcome,
                proof: None,
                edit_proof: None,
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
                if let Err(error) = this.finish(route, result, window, cx) {
                    this.last_error = Some(error);
                }
                this.schedule_pump(window, cx);
            });
        })
        .detach();
        Ok(())
    }

    fn dispatch_cut_evidence_read(
        &mut self,
        request: super::super::clipboard::PropagatedCutPageRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let flight = self.begin_flight()?;
        let service = self.service.clone();
        let selection = self.selection;
        let route = self.route;
        let task = cx.background_executor().spawn(async move {
            let page = super::super::clipboard::prepare_next_cut_page(&service, selection, request)
                .map_err(|error| MainWindowConversationComposerTaskError::exact(error, None))?;
            Ok(Box::new(MainWindowConversationComposerDispatch {
                initiating_selection: selection,
                settled_selection: selection,
                outcome: MainWindowComposerDispatchOutcome::Released,
                proof: None,
                edit_proof: None,
                cut_page: Some(page),
                cut_page_expected: true,
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
        Ok(())
    }

    pub(super) fn finish_mutation_evidence(
        &mut self,
        outcome: ComposerHostMutationEvidenceOutcome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        match outcome {
            ComposerHostMutationEvidenceOutcome::Started(pass) => {
                let evidence = self
                    .mutation_evidence
                    .as_mut()
                    .filter(|evidence| evidence.pass == pass)
                    .ok_or_else(|| "composer evidence start lost its owner".to_owned())?;
                evidence.awaiting_host = false;
            }
            ComposerHostMutationEvidenceOutcome::PageAccepted(pass) => {
                let evidence = self
                    .mutation_evidence
                    .as_mut()
                    .filter(|evidence| evidence.pass == pass)
                    .ok_or_else(|| {
                        "composer evidence acknowledgement changed its pass".to_owned()
                    })?;
                let acknowledgement = evidence.acknowledgement.take().ok_or_else(|| {
                    "composer evidence acknowledgement has no pending page".to_owned()
                })?;
                let accepted = self.input.update(cx, |input, _| {
                    input.acknowledge_mutation_evidence_page(acknowledgement)
                });
                if let Err(error) = accepted {
                    if !matches!(error, RangeTextInputError::Stale) {
                        return Err(format!(
                            "composer evidence acknowledgement was rejected: {error}"
                        ));
                    }
                    evidence.cancelled = true;
                }
                evidence.awaiting_host = false;
            }
            ComposerHostMutationEvidenceOutcome::Pending(key) => {
                if !self
                    .mutation_evidence
                    .as_ref()
                    .is_some_and(|evidence| evidence.pass.key() == key)
                {
                    return Err("composer evidence continuation lost its owner".to_owned());
                }
            }
            ComposerHostMutationEvidenceOutcome::Began(key) => {
                let accepted = self
                    .input
                    .update(cx, |input, cx| input.accept_mutation_preflight(key, cx));
                if matches!(accepted, Err(RangeTextInputError::Stale)) {
                    self.mutation_evidence = None;
                    return Ok(());
                }
                accepted.map_err(|error| {
                    format!("composer evidenced mutation preflight was rejected: {error}")
                })?;
                let pass = self
                    .input
                    .update(cx, |input, _| input.mutation_restart(key))
                    .map_err(|error| format!("composer mutation restart was rejected: {error}"))?;
                if let Some(cut) = self.propagated_cut.as_mut().filter(|cut| cut.key() == key) {
                    cut.restart(pass)?;
                }
                self.input
                    .update(cx, |input, cx| input.acknowledge_mutation_restart(pass, cx))
                    .map_err(|error| {
                        format!("composer mutation restart acknowledgement was rejected: {error}")
                    })?;
                self.mutation_evidence = None;
            }
            ComposerHostMutationEvidenceOutcome::Refused { key, failure } => {
                let cancelled = self
                    .mutation_evidence
                    .as_ref()
                    .is_some_and(|evidence| evidence.cancelled);
                let result = self
                    .input
                    .update(cx, |input, cx| input.reject_mutation_preflight(key, cx));
                if !cancelled && let Err(error) = result {
                    return Err(format!("composer evidence refusal was rejected: {error}"));
                }
                self.clear_propagated_cut(key);
                self.record_mutation_feedback(key, &failure, false, cx);
                self.last_mutation_admission_failure = Some(failure);
                self.finish_marker_removal_noncommit(key, window, cx)?;
                cx.notify();
            }
            ComposerHostMutationEvidenceOutcome::Unavailable { key, failure } => {
                let evidence = self
                    .mutation_evidence
                    .as_mut()
                    .filter(|evidence| evidence.pass.key() == key)
                    .ok_or_else(|| "composer unavailable evidence lost its owner".to_owned())?;
                evidence.terminally_unavailable = true;
                let message = failure.to_string();
                self.record_mutation_feedback(key, &failure, true, cx);
                self.last_mutation_admission_failure = Some(failure);
                return Err(message);
            }
        }
        Ok(())
    }

    pub(in crate::main_window::conversation_composer_owner) fn record_mutation_feedback(
        &mut self,
        key: gpui_text_input::MutationKey,
        failure: &crate::composer_host::ComposerHostMutationAdmissionFailure,
        terminally_unavailable: bool,
        cx: &mut Context<Self>,
    ) {
        use super::super::MainWindowComposerMutationFeedbackKind as Kind;
        use crate::composer_host::{
            ComposerHostMutationAdmissionFailure as Failure, ComposerHostMutationOutcome,
        };

        let kind = if terminally_unavailable {
            match failure {
                Failure::CommittedUnavailable { .. } => Kind::AdmittedWorkUnavailable,
                Failure::TerminalCleanup {
                    outcome: ComposerHostMutationOutcome::Committed { .. },
                    ..
                } => Kind::CommittedUnavailable,
                _ => Kind::Unavailable,
            }
        } else {
            match failure {
                Failure::Cancelled => return,
                Failure::OperationTooLarge => Kind::OperationTooLarge,
                Failure::CapacityUnavailable => Kind::CapacityUnavailable,
                Failure::Storage(_)
                | Failure::CommittedUnavailable { .. }
                | Failure::Source(
                    syndic_storage::DraftMarkerReadinessSourceErrorV1::Read(_)
                    | syndic_storage::DraftMarkerReadinessSourceErrorV1::PreflightRead(_),
                )
                | Failure::Assignment(syndic_storage::DraftMarkerLabelAssignmentErrorV1::Read(_))
                | Failure::Staging(syndic_storage::DraftMutationStagingErrorV1::Read(_)) => {
                    Kind::Storage
                }
                _ => Kind::Refused,
            }
        };
        self.mutation_feedback = Some(super::super::MainWindowComposerMutationFeedback {
            selection: self.selection,
            key,
            kind,
        });
        cx.notify();
    }

    pub(in crate::main_window::conversation_composer_owner) fn clear_mutation_evidence(
        &mut self,
        key: gpui_text_input::MutationKey,
    ) {
        if self
            .mutation_evidence
            .as_ref()
            .is_some_and(|evidence| evidence.pass.key() == key)
        {
            self.mutation_evidence = None;
        }
        if self
            .pending_marker_metadata
            .as_ref()
            .is_some_and(|(retained, _)| *retained == key)
        {
            self.pending_marker_metadata = None;
        }
    }

    pub fn retry_mutation_admission(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.mutation_evidence.is_none() || self.active_flight.is_some() || !self.is_live() {
            return Err("composer has no idle retained admission to retry".to_owned());
        }
        if self
            .mutation_evidence
            .as_ref()
            .is_some_and(|evidence| evidence.terminally_unavailable)
        {
            return Err("the exact composer request is terminally unavailable".to_owned());
        }
        self.last_error = None;
        self.schedule_pump(window, cx);
        Ok(())
    }
}
