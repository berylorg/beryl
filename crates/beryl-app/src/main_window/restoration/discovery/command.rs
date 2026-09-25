use super::probe::DiscoverySnapshot;
use beryl_home_store::{
    CommandCancellation, CommandError, CommandOutcome, CommitReceipt, CommittedLocalFinalization,
    HomeCommand, HomeServiceReference, ReconciliationFailure, ReconciliationHandle,
    ReconciliationResolution,
};
use beryl_model::{SessionRevision, WindowId, WindowPlacement};
use beryl_state::{
    BeginSessionRestore, BerylState, InitializeThreadlessWindow, RecordRevision, SessionExitIntent,
    ThreadClaimState,
};

pub(super) enum ExpectedSuccessor {
    Restore(DiscoverySnapshot),
    Initialize {
        before: DiscoverySnapshot,
        window: WindowId,
        placement: WindowPlacement,
    },
}

pub(super) struct CommandFlight {
    pub(super) expected: ExpectedSuccessor,
    receipt: Option<CommitReceipt>,
    failure: Option<CommandError>,
    reconciliation: Option<ReconciliationHandle>,
    reconciliation_failure: Option<ReconciliationFailure>,
    reconciled: bool,
    local_finalization: Option<CommittedLocalFinalization>,
}

impl CommandFlight {
    pub(super) fn retained_command(
        &self,
    ) -> Option<crate::main_window::RestoreSetRetainedCommand<'_>> {
        use crate::main_window::{RestoreSetRetainedCommand, RestoreSetRetainedReason};
        let local_finalization = self.local_finalization.as_ref()?;
        Some(RestoreSetRetainedCommand {
            reason: match &self.expected {
                ExpectedSuccessor::Restore(_) => {
                    RestoreSetRetainedReason::BeginRestoreLocalFinalization
                }
                ExpectedSuccessor::Initialize { .. } => {
                    RestoreSetRetainedReason::ThreadlessInitializationLocalFinalization
                }
            },
            receipt: self
                .receipt
                .as_ref()
                .expect("committed finalization retains its receipt"),
            failure: self.failure.as_ref(),
            local_finalization,
        })
    }

    pub(super) fn execute(
        store: &HomeServiceReference,
        command: HomeCommand,
        expected: ExpectedSuccessor,
    ) -> Self {
        let mut flight = Self {
            expected,
            receipt: None,
            failure: None,
            reconciliation: None,
            reconciliation_failure: None,
            reconciled: false,
            local_finalization: None,
        };
        match store.execute(command) {
            CommandOutcome::NotCommitted { evidence } => flight.failure = Some(evidence),
            CommandOutcome::Committed {
                receipt,
                later_failure,
                local_finalization,
            } => {
                flight.receipt = Some(receipt);
                flight.failure = later_failure;
                flight.local_finalization = local_finalization;
            }
            CommandOutcome::Indeterminate {
                failure,
                reconciliation,
            } => {
                flight.failure = Some(failure);
                flight.reconciliation = Some(reconciliation.install_and_handle());
            }
        }
        flight
    }

    pub(super) fn committed(&self) -> bool {
        self.receipt.is_some()
    }

    pub(super) fn settle(&mut self, store: &HomeServiceReference) -> Result<bool, String> {
        if !self.reconciled {
            if let Some(handle) = &self.reconciliation {
                match store.retry_reconciliation(handle) {
                    Err(error) => {
                        self.reconciliation_failure = Some(error);
                        return Ok(false);
                    }
                    Ok(ReconciliationResolution::ExactOld) => self.reconciled = true,
                    Ok(ReconciliationResolution::ExactNew { receipt }) => {
                        self.receipt = Some(receipt);
                        self.reconciled = true;
                    }
                    Ok(
                        ReconciliationResolution::ExactSuccessor { .. }
                        | ReconciliationResolution::Collision,
                    ) => {
                        return Err(
                            "restore command reconciliation collision retains original custody"
                                .to_owned(),
                        );
                    }
                }
                self.reconciliation_failure = None;
            }
        }
        Ok(self.local_finalization.is_none())
    }
}

impl ExpectedSuccessor {
    fn before(&self) -> &DiscoverySnapshot {
        match self {
            Self::Restore(before) | Self::Initialize { before, .. } => before,
        }
    }

    pub(super) fn command(
        &self,
        state: &BerylState,
        cancellation: &CommandCancellation,
    ) -> Result<HomeCommand, String> {
        let before = self.before();
        let sessions = state.session();
        let contribution = match self {
            Self::Restore(_) => sessions.begin_restore(
                before.domain_revision,
                BeginSessionRestore::new(before.session.as_ref().unwrap().header().revision()),
            ),
            Self::Initialize {
                window, placement, ..
            } => sessions.initialize_threadless(
                before.domain_revision,
                match &before.session {
                    Some(session) => InitializeThreadlessWindow::for_empty_session(
                        session.header().revision(),
                        *window,
                        placement.clone(),
                    ),
                    None => InitializeThreadlessWindow::new(*window, placement.clone()),
                },
            ),
        };
        let mut command =
            HomeCommand::new(before.home_revision).with_cancellation(cancellation.clone());
        command.add(contribution).map_err(|e| e.to_string())?;
        Ok(command)
    }

    pub(super) fn validate(&self, current: &DiscoverySnapshot) -> Result<(), String> {
        let before = self.before();
        let expected_revision = match &before.session {
            Some(session) => session
                .header()
                .revision()
                .checked_next()
                .map_err(|e| e.to_string())?,
            None => SessionRevision::new(1).unwrap(),
        };
        let session = current
            .session
            .as_ref()
            .ok_or_else(|| "restore command successor is missing".to_owned())?;
        if session.header().revision() != expected_revision
            || session.header().exit_intent() != SessionExitIntent::Running
            || current.no_runtimes != before.no_runtimes
            || current.domain_revision
                != before
                    .domain_revision
                    .checked_next()
                    .map_err(|e| e.to_string())?
        {
            return Err("restore command successor identity changed".to_owned());
        }
        match self {
            Self::Initialize {
                window, placement, ..
            } => {
                if session.windows().len() != 1
                    || session.header().fallback().is_some()
                    || !current.no_runtimes
                {
                    return Err("threadless initialization successor changed".to_owned());
                }
                let actual = &session.windows()[0];
                if actual.window_id() != *window
                    || actual.placement() != placement
                    || actual.revision() != RecordRevision::INITIAL
                    || actual.selected_thread().is_some()
                    || actual.remembered_target().is_some()
                {
                    return Err("threadless initialization member changed".to_owned());
                }
            }
            Self::Restore(before) => {
                let original = before.session.as_ref().unwrap();
                if session.windows().len() != original.windows().len()
                    || session.header().fallback() != original.header().fallback()
                {
                    return Err("restore command changed the saved member set".to_owned());
                }
                for (index, (old, new)) in
                    original.windows().iter().zip(session.windows()).enumerate()
                {
                    let old_claim = before.claims[index];
                    let new_claim = current.claims[index];
                    if old.window_id() != new.window_id()
                        || old.placement() != new.placement()
                        || old.remembered_target() != new.remembered_target()
                    {
                        return Err("restore command changed a saved member".to_owned());
                    }
                    if let Some(claim) =
                        old_claim.filter(|claim| claim.state() == ThreadClaimState::Active)
                    {
                        let changed = new_claim
                            .ok_or_else(|| "restoring successor claim is missing".to_owned())?;
                        if changed.thread_id() != claim.thread_id()
                            || changed.state() != ThreadClaimState::Restoring
                            || changed.generation() != expected_revision
                            || changed.revision()
                                != claim.revision().checked_next().map_err(|e| e.to_string())?
                            || Some(new.revision().get()) != old.revision().get().checked_add(1)
                        {
                            return Err("restoring successor claim differs from command".to_owned());
                        }
                    } else if old != new || old_claim != new_claim {
                        return Err(
                            "restore changed an already restoring or threadless member".to_owned()
                        );
                    }
                }
            }
        }
        Ok(())
    }
}
