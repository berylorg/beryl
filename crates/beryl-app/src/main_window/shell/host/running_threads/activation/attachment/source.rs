mod initial_target;
mod prior_disposal;
mod prior_save;

use super::*;

impl ActivationSource {
    fn capture_completed_successor(&mut self) -> Result<(), String> {
        if self.completed_successor.is_none() {
            if let Some(target) = self
                .committed_selection()
                .or_else(|| self.creation.is_none().then_some(self.target))
            {
                self.completed_successor = self
                    .service
                    .take_completed_thread_successor_cleanup(target)?;
            }
        }
        Ok(())
    }
    pub(super) fn irreversible(&self) -> bool {
        self.outcome.is_some()
            || self.committed.is_some()
            || self.publication.is_some()
            || self
                .creation
                .as_ref()
                .is_some_and(|source| source.irreversible())
    }
    pub(super) fn fail(&mut self, error: String) {
        self.error = Some(error);
        if self.creation.is_some() {
            if !self.irreversible() && !self.terminal_release_failure {
                self.stage = if self
                    .creation
                    .as_ref()
                    .is_some_and(|creation| creation.has_save_custody())
                {
                    Stage::ThreadRelease
                } else if self.flush.is_some() {
                    self.terminal_release_failure = true;
                    self.stage
                } else {
                    Stage::RestoreAutosave
                };
            }
            return;
        }
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
            && self.creation.is_none()
            && !self.irreversible()
            && self.flush.is_none()
            && !matches!(self.stage, Stage::Retire | Stage::RestoreAutosave)
        {
            self.fail("Thread activation cancelled".into());
            return Ok(());
        }
        match self.stage {
            Stage::Begin => self.run_initial_target_source(),
            Stage::Flush => self.run_prior_save_source(),
            Stage::DisposePrior => self.run_prior_disposal_source(),
            _ => self.run_remaining_source(),
        }
    }

    #[inline(never)]
    fn run_remaining_source(&mut self) -> Result<(), String> {
        match self.stage {
            Stage::ThreadSave
            | Stage::ThreadPrepare
            | Stage::ThreadCommit
            | Stage::ThreadReconcile
            | Stage::ThreadAdopt
            | Stage::ThreadRelease => {
                return self.run_thread_creation_source();
            }
            Stage::PrepareFence => {
                self.expected = self.service.publish_preflight(self.receipt()?)?;
                self.stage = Stage::Fence;
            }
            Stage::Commit => {
                if self.ordinary_save.is_none() {
                    self.ordinary_save = Some(
                        self.service
                            .capture_ordinary_claim_save(self.receipt()?, self.expected)?,
                    );
                }
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
            Stage::Complete => {
                if self.completed_predecessor.is_none() {
                    self.completed_predecessor =
                        Some(self.service.take_completed_thread_predecessor_disposal(
                            self.receipt()?,
                            self.expected,
                        )?);
                }
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
                if let Some(creation) = &self.creation {
                    creation
                        .operation
                        .as_ref()
                        .ok_or("Original thread confirmation custody is missing")?
                        .validate_publication()?;
                }
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
                        self.capture_completed_successor()?;
                        self.retired = true;
                        self.stage = Stage::Detach;
                    }
                    RetirementAdvance::Pending | RetirementAdvance::DepartedFreshBoundary => {}
                }
            }
            Stage::RestoreAutosave => {
                if self.creation.is_none() {
                    self.capture_completed_successor()?;
                    if let Some(completed) = self.completed_successor.take() {
                        match self
                            .service
                            .settle_completed_thread_successor_cleanup(completed)
                        {
                            Ok(progress) => self.completed_successor_progress = Some(progress),
                            Err((completed, error)) => {
                                self.completed_successor = Some(completed);
                                return Err(error);
                            }
                        }
                    }
                }
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
                self.stage = if self.creation.is_some() {
                    Stage::ThreadRestore
                } else {
                    Stage::Restore
                };
            }
            _ => {}
        }
        Ok(())
    }
    pub(super) fn accept_outcome(&mut self, outcome: Outcome) {
        #[cfg(all(test, feature = "test-faults"))]
        if let Some(hook) = self.after_claim_outcome.take() {
            hook(&outcome);
        }
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
                self.outcome = Some(Outcome::NotCommitted(error));
                self.stage = Stage::Retire;
            }
        }
    }
    pub(super) fn source_stage(&self) -> bool {
        matches!(
            self.stage,
            Stage::ThreadSave
                | Stage::ThreadPrepare
                | Stage::ThreadCommit
                | Stage::ThreadReconcile
                | Stage::ThreadAdopt
                | Stage::ThreadRelease
                | Stage::Begin
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
