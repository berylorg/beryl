use super::*;
use beryl_home_store::{
    CommandError, CommandOutcome, CommittedLocalFinalization, HomeCommand, ReconciliationFailure,
    ReconciliationResolution,
};
use beryl_state::ActivateRestoringClaim;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RestoredClaimActivationProgress {
    Retry,
    Pending,
    Active,
}

pub(super) struct RestoredClaimActivation {
    expected_session: SessionRevision,
    reconciliation: Option<ReconciliationHandle>,
    receipt: Option<CommitReceipt>,
    failure: Option<CommandError>,
    reconciliation_failure: Option<ReconciliationFailure>,
    local_finalization: Option<CommittedLocalFinalization>,
    applied: bool,
}

impl RestoredWindowComposer {
    pub(in crate::main_window) fn claim_activation_local_finalization(
        &self,
    ) -> Option<&CommittedLocalFinalization> {
        self.claim_activation.as_ref()?.local_finalization.as_ref()
    }

    #[cfg(feature = "test-faults")]
    pub fn test_arm_before_claim_activation(&mut self, fault: impl FnOnce() + Send + 'static) {
        self.before_claim_activation = Some(Box::new(fault));
    }

    pub fn claim_activation_receipt(&self) -> Option<&CommitReceipt> {
        self.claim_activation.as_ref()?.receipt.as_ref()
    }

    pub fn claim_activation_failure(&self) -> Option<&CommandError> {
        self.claim_activation.as_ref()?.failure.as_ref()
    }

    pub fn claim_activation_reconciliation_failure(&self) -> Option<&ReconciliationFailure> {
        self.claim_activation
            .as_ref()?
            .reconciliation_failure
            .as_ref()
    }

    pub fn take_claim_activation_local_finalization(
        &mut self,
    ) -> Option<CommittedLocalFinalization> {
        self.claim_activation.as_mut()?.local_finalization.take()
    }

    pub fn activate_claim(
        &mut self,
        attempt: &RestoredWindowPreparationAttempt,
        cancellation: &CommandCancellation,
    ) -> Result<RestoredClaimActivationProgress, String> {
        if !Arc::ptr_eq(&self.source.attempt, &attempt.identity)
            || !Arc::ptr_eq(&self.source.store, &attempt.store)
        {
            return Err("restored claim belongs to another startup attempt".to_owned());
        }
        attempt.validate_lifetime()?;
        if self.candidate.retirement_started || self.candidate.preparation_started {
            return Err("restored editor already transferred or retired".to_owned());
        }
        if let Some(activation) = self.claim_activation.as_ref() {
            if activation.applied {
                self.source.validate()?;
                return Ok(RestoredClaimActivationProgress::Active);
            }
            if activation.reconciliation.is_some() {
                if !self.reconcile_claim()? {
                    return Ok(RestoredClaimActivationProgress::Pending);
                }
                if self.claim_activation.as_ref().unwrap().receipt.is_none() {
                    return Ok(RestoredClaimActivationProgress::Retry);
                }
            }
            if self.claim_activation.as_ref().unwrap().receipt.is_some() {
                self.adopt_activated_claim()?;
                return Ok(RestoredClaimActivationProgress::Active);
            }
        }
        if cancellation.is_cancelled() {
            return Err("restored claim activation cancelled".to_owned());
        }
        self.source.validate()?;
        if !self.candidate.activated || self.source.claim.state() != ThreadClaimState::Restoring {
            return Err("restored claim activation requires a ready restoring editor".to_owned());
        }
        let expected_session;
        let command;
        {
            let mut session = self
                .source
                .attempt
                .session
                .lock()
                .map_err(|_| "restore session fence is poisoned".to_owned())?;
            if session.activation.is_some() {
                return Err("another restored claim activation remains unsettled".to_owned());
            }
            expected_session = session
                .revision
                .ok_or_else(|| "restore session revision is missing".to_owned())?;
            let mut prepared = HomeCommand::new(
                self.source
                    .store
                    .home_revision()
                    .map_err(|error| error.to_string())?,
            )
            .with_cancellation(cancellation.clone());
            prepared
                .add(
                    self.source.session.activate_restoring_claim(
                        self.source
                            .session
                            .revision(&self.source.store)
                            .map_err(|error| error.to_string())?,
                        ActivateRestoringClaim::new(
                            expected_session,
                            self.source.window.window_id(),
                            self.source.window.revision(),
                            self.source.window.selected_thread().unwrap(),
                        ),
                    ),
                )
                .map_err(|error| error.to_string())?;
            session.activation = Some(self.window_id());
            command = prepared;
        }
        let mut activation = RestoredClaimActivation {
            expected_session,
            reconciliation: None,
            receipt: None,
            failure: None,
            reconciliation_failure: None,
            local_finalization: None,
            applied: false,
        };
        #[cfg(feature = "test-faults")]
        if let Some(fault) = self.before_claim_activation.take() {
            fault();
        }
        let progress = match self.source.store.execute(command) {
            CommandOutcome::NotCommitted { evidence } => {
                activation.failure = Some(evidence);
                RestoredClaimActivationProgress::Retry
            }
            CommandOutcome::Indeterminate {
                failure,
                reconciliation,
            } => {
                activation.failure = Some(failure);
                activation.reconciliation = Some(reconciliation.install_and_handle());
                RestoredClaimActivationProgress::Pending
            }
            CommandOutcome::Committed {
                receipt,
                later_failure,
                local_finalization,
            } => {
                activation.receipt = Some(receipt);
                activation.failure = later_failure;
                activation.local_finalization = local_finalization;
                RestoredClaimActivationProgress::Active
            }
        };
        self.claim_activation = Some(activation);
        match progress {
            RestoredClaimActivationProgress::Retry => self.finish_claim_flight(false)?,
            RestoredClaimActivationProgress::Active => self.adopt_activated_claim()?,
            RestoredClaimActivationProgress::Pending => {}
        }
        Ok(progress)
    }

    fn reconcile_claim(&mut self) -> Result<bool, String> {
        let activation = self.claim_activation.as_mut().unwrap();
        let Some(handle) = activation.reconciliation.as_ref() else {
            return Ok(true);
        };
        match self.source.store.retry_reconciliation(handle) {
            Err(failure) => {
                activation.reconciliation_failure = Some(failure);
                return Ok(false);
            }
            Ok(ReconciliationResolution::ExactOld) => {
                activation.reconciliation = None;
                activation.reconciliation_failure = None;
                self.finish_claim_flight(false)?;
            }
            Ok(ReconciliationResolution::ExactNew { receipt }) => {
                activation.reconciliation = None;
                activation.reconciliation_failure = None;
                activation.receipt = Some(receipt);
            }
            Ok(
                ReconciliationResolution::ExactSuccessor { .. }
                | ReconciliationResolution::Collision,
            ) => {
                return Err(
                    "restored claim reconciliation collision retains original custody".to_owned(),
                );
            }
        }
        Ok(true)
    }

    fn finish_claim_flight(&self, committed: bool) -> Result<(), String> {
        let activation = self.claim_activation.as_ref().unwrap();
        let mut session = self
            .source
            .attempt
            .session
            .lock()
            .map_err(|_| "restore session fence is poisoned".to_owned())?;
        if session.activation != Some(self.window_id())
            || session.revision != Some(activation.expected_session)
        {
            return Err("restored claim flight no longer owns its session expectation".to_owned());
        }
        if committed {
            session.revision = Some(
                activation
                    .expected_session
                    .checked_next()
                    .map_err(|error| error.to_string())?,
            );
        }
        session.activation = None;
        Ok(())
    }

    fn adopt_activated_claim(&mut self) -> Result<(), String> {
        let activation = self.claim_activation.as_ref().unwrap();
        if activation.receipt.is_none() {
            return Err("restored claim has no exact committed receipt".to_owned());
        }
        if activation.local_finalization.is_some() {
            return Err("restored claim retains committed local-finalization custody".to_owned());
        }
        let revision = activation
            .expected_session
            .checked_next()
            .map_err(|error| error.to_string())?;
        let before = self
            .source
            .store
            .home_revision()
            .map_err(|error| error.to_string())?;
        let bootstrap = self
            .source
            .session
            .minimal_bootstrap(&self.source.store)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "activated restore session is missing".to_owned())?;
        let window = bootstrap
            .windows()
            .iter()
            .find(|window| window.window_id() == self.window_id())
            .ok_or_else(|| "activated restore member is missing".to_owned())?
            .clone();
        let selection = window
            .selected_thread()
            .ok_or_else(|| "activated restore selection is missing".to_owned())?;
        let claim = self
            .source
            .session
            .thread_claim_catalog_source(&self.source.store, selection.thread_id())
            .map_err(|error| error.to_string())?
            .claim()
            .ok_or_else(|| "activated restore claim is missing".to_owned())?;
        if bootstrap.header().revision() != revision
            || Some(window.revision().get()) != self.source.window.revision().get().checked_add(1)
            || window.remembered_target() != self.source.window.remembered_target()
            || window.placement() != self.source.window.placement()
            || selection.thread_id() != self.source.claim.thread_id()
            || selection.generation() != revision
            || selection.revision()
                != self
                    .source
                    .claim
                    .revision()
                    .checked_next()
                    .map_err(|error| error.to_string())?
            || claim.window_id() != self.window_id()
            || claim.state() != ThreadClaimState::Active
            || claim.generation() != selection.generation()
            || claim.revision() != selection.revision()
            || self
                .source
                .store
                .home_revision()
                .map_err(|error| error.to_string())?
                != before
        {
            return Err(
                "activated restore member differs from the exact command outcome".to_owned(),
            );
        }
        self.finish_claim_flight(true)?;
        self.source.session_revision = revision;
        self.source.window = window;
        self.source.claim = claim;
        self.candidate.claim = selection;
        self.claim_activation.as_mut().unwrap().applied = true;
        self.source.validate()
    }

    pub(super) fn settle_claim_for_retirement(&mut self) -> Result<bool, String> {
        let Some(activation) = self.claim_activation.as_ref() else {
            return Ok(true);
        };
        if activation.applied {
            return Ok(true);
        }
        if activation.reconciliation.is_some() && !self.reconcile_claim()? {
            return Ok(false);
        }
        let activation = self.claim_activation.as_ref().unwrap();
        if activation.local_finalization.is_some() {
            return Ok(false);
        }
        if activation.receipt.is_some() {
            self.finish_claim_flight(true)?;
            self.claim_activation.as_mut().unwrap().applied = true;
        }
        Ok(true)
    }
}
