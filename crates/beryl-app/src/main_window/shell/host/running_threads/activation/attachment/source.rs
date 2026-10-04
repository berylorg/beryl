use super::*;

impl ActivationSource {
    pub(super) fn irreversible(&self) -> bool {
        self.outcome.is_some() || self.committed.is_some() || self.publication.is_some()
    }
    pub(super) fn fail(&mut self, error: String) {
        self.error = Some(error);
        if !self.irreversible() && !self.retired {
            self.stage = if self.receipt.is_some() {
                Stage::Retire
            } else {
                Stage::Finished
            };
        }
    }
    pub(super) fn receipt(&self) -> Result<Receipt, String> {
        self.receipt.ok_or("Activation receipt is missing".into())
    }
    pub(super) fn run_source(&mut self) -> Result<(), String> {
        if !self.reader.current() {
            return Err("Running threads source retired; activation custody is retained".into());
        }
        if self
            .home
            .as_ref()
            .is_some_and(|home| home.health().state() != beryl_home_store::HomeHealthState::Healthy)
        {
            return Err("Activation Home is unavailable; admitted custody is retained".into());
        }
        if self.cancellation.is_cancelled()
            && !self.irreversible()
            && self.flush.is_none()
            && !matches!(self.stage, Stage::Retire | Stage::RestoreAutosave)
        {
            self.fail("Thread activation cancelled".into());
            return Ok(());
        }
        match self.stage {
            Stage::Begin => {
                let (home, state, syndic) = self
                    .reader
                    .activation_sources()
                    .ok_or("Running threads source retired")?;
                self.home = Some(home);
                self.state = Some(state);
                self.syndic = Some(syndic);
                let (request, retirement) =
                    crate::bootstrap::thread_activation_request(self.target.thread_id())?;
                let admission = self.service.begin_activation(
                    self.target,
                    request,
                    retirement,
                    &self.cancellation,
                );
                let admission = match admission {
                    Ok(value) => value,
                    Err(error) => {
                        self.receipt = self.service.pending_receipt();
                        return Err(error);
                    }
                };
                match admission {
                    ActivationAdvance::Ready(receipt) => self.receipt = Some(receipt),
                    ActivationAdvance::RetirementPending(receipt)
                    | ActivationAdvance::FailureRetirementPending { receipt, .. } => {
                        self.receipt = Some(receipt);
                        return Err("Target composer preparation requires retirement".into());
                    }
                    _ => return Err("Target composer preparation was refused".into()),
                }
                let prepared = self
                    .service
                    .prepare_claim_presentation_source(self.receipt()?)?;
                self.pending_selection = Some(prepared.selection());
                self.request.activation = prepared.selection().binding().host_generation().get();
                self.presentation = Some(prepared);
                if self.provider.is_none() {
                    self.provider = Some(
                        self.reader
                            .transcript_provider()
                            .map_err(|error| error.to_string())?,
                    );
                }
                self.transcript = Some(
                    self.provider
                        .as_ref()
                        .unwrap()
                        .prepare_attachment(self.request.clone(), &self.transcript_cancel)
                        .map_err(|error| error.to_string())?,
                );
                self.stage = Stage::Install;
            }
            Stage::PrepareFence => {
                self.expected = self.service.publish_preflight(self.receipt()?)?;
                self.stage = Stage::Fence;
            }
            Stage::Flush => {
                let receipt = self.receipt()?;
                if self.flush.is_none() {
                    self.flush = Some(self.service.begin_claim_publication_save(receipt)?);
                }
                match self.flush.unwrap() {
                    FlushAdmission::Started {
                        ticket,
                        state: FlushState::CaptureRequired,
                    }
                    | FlushAdmission::Joined {
                        ticket,
                        state: FlushState::CaptureRequired,
                    } => {
                        #[cfg(all(test, feature = "test-faults"))]
                        {
                            if let Some(hook) = self.before_save.take() {
                                hook(&self.cancellation);
                            }
                        }
                        let authority = match self.service.autosave_capture_requirement(self.expected)? {
                            crate::main_window::MainWindowComposerAutosaveCaptureRequirement::ChangedMarkers => Some(fresh_marker_authority()?),
                            _ => None,
                        };
                        let captured = self.service.capture_flush_publication(
                            self.expected,
                            ticket,
                            self.assets.clone(),
                            &self.marker_seals,
                            fresh_piece_operation_id()?,
                            authority,
                            current_timestamp()?,
                            &self.cancellation,
                        )?;
                        if matches!(captured, FlushCapture::Unsatisfied(_)) {
                            self.error = Some("Prior composer flush was refused".into());
                            self.stage = Stage::Retire;
                            return Ok(());
                        }
                        if matches!(captured, FlushCapture::Stale) {
                            self.terminal_release_failure = true;
                            return Err("Prior composer save source was stale; settlement custody is retained".into());
                        }
                        self.flush = Some(FlushAdmission::Joined {
                            ticket,
                            state: FlushState::PublicationPending,
                        });
                    }
                    FlushAdmission::Started {
                        state: FlushState::DisposalRequired,
                        ..
                    }
                    | FlushAdmission::Joined {
                        state: FlushState::DisposalRequired,
                        ..
                    } => return Err("Prior composer flush unexpectedly required disposal".into()),
                    _ => {}
                }
                let advance = self.service.advance_claim_publication_source(receipt)?;
                if matches!(advance.advance, PublishAdvance::ReconciliationPending) {
                    self.error = Some("Prior editor save is awaiting reconciliation".into());
                }
                if let PublishAdvance::Progress(state) = advance.advance {
                    match self.flush.unwrap() {
                        FlushAdmission::Started { ticket, .. }
                        | FlushAdmission::Joined { ticket, .. } => {
                            self.flush = Some(FlushAdmission::Joined { ticket, state });
                        }
                        FlushAdmission::Satisfied(_) => {}
                    }
                }
                self.advance = Some(advance);
                self.stage = Stage::AcceptFlush;
            }
            Stage::Commit => {
                #[cfg(all(test, feature = "test-faults"))]
                if let Some(hook) = self.before_commit.take() {
                    hook(&self.cancellation);
                }
                if self.lease.invoking() != self.expected.window_id() {
                    return Err("Selection lease belongs to another window".into());
                }
                let home = self.home.as_ref().ok_or("Activation Home is missing")?;
                let state = self.state.as_ref().ok_or("Activation State is missing")?;
                let syndic = self.syndic.as_ref().ok_or("Activation Syndic is missing")?;
                let lease = self.lease.clone();
                let outcome = lease
                    .admit_commit(|| {
                        self.owner.take().unwrap().commit(
                            home,
                            state,
                            syndic,
                            self.cancellation.clone(),
                        )
                    })
                    .map_err(|error| error.to_string())?;
                self.accept_outcome(outcome);
            }
            Stage::Reconcile => {
                let Some(Outcome::Pending(pending)) = self.outcome.take() else {
                    return Err("Activation reconciliation custody is missing".into());
                };
                let outcome =
                    pending.reconcile(self.home.as_ref().unwrap(), self.state.as_ref().unwrap());
                self.accept_outcome(outcome);
            }
            Stage::BeginFinal => {
                self.service
                    .begin_final_publish(self.receipt()?, self.expected)?;
                self.stage = Stage::Release;
            }
            Stage::DisposePrior => {
                #[cfg(all(test, feature = "test-faults"))]
                if let Some(hook) = self.before_disposal.take() {
                    hook(&self.cancellation);
                }
                self.advance = Some(self.service.advance_committed_claim_disposal_source(
                    self.receipt()?,
                    self.expected,
                    fresh_piece_operation_id()?,
                )?);
                self.stage = Stage::AcceptDisposal;
            }
            Stage::Complete => {
                let work = self
                    .widget_work
                    .take()
                    .ok_or("Widget release custody is missing")?;
                let completion = self
                    .service
                    .complete_claim_publication_source(self.receipt()?, work);
                let completion = match completion {
                    Ok(completion) => completion,
                    Err((work, error)) => {
                        self.widget_work = Some(work);
                        self.terminal_release_failure = true;
                        return Err(error);
                    }
                };
                match completion {
                    Completion::Pending(release) => {
                        self.release = Some(release);
                        self.widget_work = Some(WidgetWork::Released(release));
                        self.stage = Stage::AcceptRelease;
                    }
                    Completion::Published {
                        release,
                        publication,
                    } => {
                        self.release = Some(release);
                        self.publication = Some(publication);
                        self.stage = Stage::AcceptRelease;
                    }
                    Completion::RetainedFailure { release, error } => {
                        self.release = Some(release);
                        self.widget_work = Some(WidgetWork::Released(release));
                        self.stage = Stage::AcceptRelease;
                        self.error = Some(error);
                    }
                }
            }
            Stage::PrepareAutosave => {
                let selected = self
                    .publication
                    .as_ref()
                    .ok_or("Composer publication custody is missing")?
                    .selection();
                self.autosave = Some(
                    self.service
                        .prepare_claim_autosave_source(selected, self.settings)?,
                );
                self.stage = if self.gui_published {
                    Stage::Finalize
                } else {
                    Stage::Revalidate
                };
            }
            Stage::Revalidate => {
                let provider = self
                    .provider
                    .as_ref()
                    .ok_or("Transcript provider is missing")?;
                let prepared = self.transcript.take();
                let revalidated = match prepared {
                    Some(prepared) => provider.revalidate_after_claim(
                        prepared,
                        &self.request,
                        &self.transcript_cancel,
                    ),
                    None => {
                        provider.prepare_attachment(self.request.clone(), &self.transcript_cancel)
                    }
                };
                match revalidated {
                    Ok(prepared) => {
                        self.transcript = Some(prepared);
                        self.stage = Stage::Publish;
                    }
                    Err(error) => {
                        self.transcript_retries = self.transcript_retries.saturating_add(1);
                        if matches!(
                            error,
                            crate::transcript_provider::TranscriptAttachmentError::Stale
                                | crate::transcript_provider::TranscriptAttachmentError::Busy
                        ) {
                            return Ok(());
                        }
                        return Err(format!(
                            "Target transcript preparation is unavailable: {error}"
                        ));
                    }
                }
            }
            Stage::Retire => {
                self.transcript = None;
                let advance = if self.flush_settled {
                    self.service
                        .abort_claim_publication_before_release(self.receipt()?, self.expected)?
                } else {
                    self.service.release_failed_pending(self.receipt()?)?
                };
                match advance {
                    RetirementAdvance::Retired => {
                        self.retired = true;
                        self.stage = Stage::Detach;
                    }
                    RetirementAdvance::Pending | RetirementAdvance::DepartedFreshBoundary => {}
                }
            }
            Stage::RestoreAutosave => {
                let selected = self
                    .service
                    .selected_identity()
                    .ok_or("Prior composer selection is missing")?;
                if selected.window_id() != self.prior.window_id()
                    || selected.claim() != self.prior.claim()
                {
                    return Err("Prior composer claim changed during retirement".into());
                }
                self.expected = selected;
                self.autosave = Some(
                    self.service
                        .prepare_claim_autosave_source(self.expected, self.settings)?,
                );
                self.stage = Stage::Restore;
            }
            _ => {}
        }
        Ok(())
    }
    pub(super) fn accept_outcome(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Settled(commit) => {
                self.committed = Some(commit);
                self.stage = Stage::DisposePrior;
            }
            Outcome::Pending(pending) => {
                self.error = Some(format!(
                    "Thread selection is awaiting reconciliation: {}",
                    pending.problem()
                ));
                self.outcome = Some(Outcome::Pending(pending));
                self.stage = Stage::Reconcile;
            }
            Outcome::NotCommitted(error) => {
                self.error = Some(error.to_string());
                self.stage = Stage::Retire;
            }
        }
    }
    pub(super) fn source_stage(&self) -> bool {
        matches!(
            self.stage,
            Stage::Begin
                | Stage::PrepareFence
                | Stage::Flush
                | Stage::Commit
                | Stage::Reconcile
                | Stage::DisposePrior
                | Stage::BeginFinal
                | Stage::Complete
                | Stage::PrepareAutosave
                | Stage::Revalidate
                | Stage::Retire
                | Stage::RestoreAutosave
        )
    }
}
