use super::*;

impl SameWindowThreadAcquisition {
    pub fn future_selection(&self) -> WindowClaimSelection {
        self.replacement.future_selection()
    }
    pub fn future_window(&self) -> &SessionWindowRecord {
        self.replacement.future_window()
    }
    pub fn draft_id(&self) -> SyndicDraftId {
        self.draft
    }
    pub fn disposition(&self) -> SameWindowThreadDisposition {
        self.disposition
    }
    pub fn commit(self, store: &HomeStore, state: &BerylState) -> SameWindowThreadOutcome {
        if store.home_id() != self.home || store.generation_identity().ok() != Some(self.generation)
        {
            return SameWindowThreadOutcome::NotCommitted(SameWindowThreadError::SourceChanged);
        }
        let outcome = store.execute(self.command);
        let mut pending = SameWindowThreadPending {
            replacement: self.replacement,
            draft: self.draft,
            disposition: self.disposition,
            rows: self.rows,
            handle: None,
            receipt: None,
            later_failure: None,
            local_finalization: None,
            problem: SameWindowThreadError::Collision,
            unavailable: false,
        };
        match outcome {
            CommandOutcome::NotCommitted { evidence } => SameWindowThreadOutcome::NotCommitted(
                RunningThreadActivationError::from(evidence).into(),
            ),
            CommandOutcome::Committed {
                receipt,
                later_failure,
                local_finalization,
            } => {
                pending.receipt = Some(receipt);
                pending.later_failure = later_failure;
                pending.local_finalization = local_finalization;
                pending.audit(store, state)
            }
            CommandOutcome::Indeterminate {
                failure,
                reconciliation,
            } => {
                pending.handle = Some(reconciliation.install_and_handle());
                pending.problem = RunningThreadActivationError::from(failure).into();
                SameWindowThreadOutcome::Pending(pending)
            }
        }
    }
}

impl SameWindowThreadPending {
    pub fn problem(&self) -> &SameWindowThreadError {
        &self.problem
    }
    pub fn future_selection(&self) -> WindowClaimSelection {
        self.replacement.future_selection()
    }
    pub fn receipt(&self) -> Option<&CommitReceipt> {
        self.receipt.as_ref()
    }
    pub fn reconcile(mut self, store: &HomeStore, state: &BerylState) -> SameWindowThreadOutcome {
        if self.unavailable {
            return SameWindowThreadOutcome::Unavailable(self);
        }
        if let Some(handle) = &self.handle {
            match store.reconcile(handle) {
                Ok(ReconciliationResolution::ExactOld) => {
                    match state
                        .session()
                        .classify_window_claim_replacement(store, &self.replacement)
                    {
                        Ok(WindowClaimReplacementState::Original) => {
                            return SameWindowThreadOutcome::NotCommitted(
                                SameWindowThreadError::ReconciledOld,
                            );
                        }
                        Ok(_) => return self.collision(),
                        Err(error) => {
                            self.problem = RunningThreadActivationError::from(error).into();
                            return SameWindowThreadOutcome::Pending(self);
                        }
                    }
                }
                Ok(ReconciliationResolution::ExactNew { receipt }) => self.receipt = Some(receipt),
                Ok(
                    ReconciliationResolution::ExactSuccessor { .. }
                    | ReconciliationResolution::Collision,
                ) => return self.collision(),
                Err(error) => {
                    self.problem = RunningThreadActivationError::from(error).into();
                    return SameWindowThreadOutcome::Pending(self);
                }
            }
        }
        self.audit(store, state)
    }
    fn collision(mut self) -> SameWindowThreadOutcome {
        self.problem = SameWindowThreadError::Collision;
        self.unavailable = true;
        SameWindowThreadOutcome::Unavailable(self)
    }
    fn audit(mut self, store: &HomeStore, state: &BerylState) -> SameWindowThreadOutcome {
        let audit = (|| -> Result<bool, SameWindowThreadError> {
            let revision = store
                .home_revision()
                .map_err(RunningThreadActivationError::from)?;
            let mut exact = self.receipt.is_some()
                && state
                    .session()
                    .classify_window_claim_replacement(store, &self.replacement)
                    .map_err(RunningThreadActivationError::from)?
                    == WindowClaimReplacementState::Committed;
            for thread in self.rows.thread_ids() {
                let row = state
                    .catalog()
                    .current_row_source(store, thread, CatalogPointReadLimit::schema_maximum())
                    .map_err(RunningThreadActivationError::from)?;
                exact &= row.is_some_and(|row| self.rows.matches_row(row.row()));
            }
            if store
                .home_revision()
                .map_err(RunningThreadActivationError::from)?
                != revision
            {
                return Err(SameWindowThreadError::SourceChanged);
            }
            Ok(exact)
        })();
        match audit {
            Ok(false) => self.collision(),
            Err(error) => {
                self.problem = error;
                SameWindowThreadOutcome::Pending(self)
            }
            Ok(true) => SameWindowThreadOutcome::Settled(SameWindowThreadCommit {
                window: self.replacement.future_window().clone(),
                selection: self.replacement.future_selection(),
                claim: self.replacement.future_claim(),
                draft: self.draft,
                disposition: self.disposition,
                receipt: self.receipt.take().unwrap(),
                later_failure: self.later_failure,
                local_finalization: self.local_finalization,
            }),
        }
    }
}
