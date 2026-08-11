use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) enum TerminalResponseReconciliation {
    AwaitRouterTerminal,
    RetireConnection,
    InvariantFailure,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) enum TerminalResponseDisposition {
    Accepted,
    CompletionUnknown,
    Rejected,
    ProvenNondispatch,
}

impl From<&CompactThreadDisposition> for TerminalResponseDisposition {
    fn from(disposition: &CompactThreadDisposition) -> Self {
        match disposition {
            CompactThreadDisposition::RequestAccepted => Self::Accepted,
            CompactThreadDisposition::CompletionUnknown { .. } => Self::CompletionUnknown,
            CompactThreadDisposition::ExactRejection { .. } => Self::Rejected,
            CompactThreadDisposition::ProvenNotDispatched { .. } => Self::ProvenNondispatch,
        }
    }
}

pub(in crate::cas_projection) fn terminal_response_reconciliation(
    attempt_matches: bool,
    disposition: TerminalResponseDisposition,
    observation: CompactionRequestTransitionStatus,
    unbind_failed: bool,
) -> TerminalResponseReconciliation {
    if !attempt_matches {
        return TerminalResponseReconciliation::InvariantFailure;
    }
    match (disposition, observation) {
        (
            TerminalResponseDisposition::Accepted,
            CompactionRequestTransitionStatus::Exact
            | CompactionRequestTransitionStatus::TerminalAlreadySettled,
        ) if !unbind_failed => TerminalResponseReconciliation::AwaitRouterTerminal,
        (
            TerminalResponseDisposition::Accepted | TerminalResponseDisposition::CompletionUnknown,
            CompactionRequestTransitionStatus::Exact
            | CompactionRequestTransitionStatus::TerminalAlreadySettled,
        ) => TerminalResponseReconciliation::RetireConnection,
        (
            TerminalResponseDisposition::Rejected | TerminalResponseDisposition::ProvenNondispatch,
            _,
        )
        | (
            _,
            CompactionRequestTransitionStatus::Prior | CompactionRequestTransitionStatus::Collision,
        ) => TerminalResponseReconciliation::InvariantFailure,
    }
}

pub(super) fn run_worker(
    coordinator: Weak<ContextCompactionCoordinator>,
    receiver: Arc<Mutex<mpsc::Receiver<CompactionWork>>>,
) {
    loop {
        let work = receiver
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .recv();
        let Ok(work) = work else {
            break;
        };
        let Some(coordinator) = coordinator.upgrade() else {
            break;
        };
        coordinator.queued_current.fetch_sub(1, Ordering::AcqRel);
        let active = coordinator.workers_current.fetch_add(1, Ordering::AcqRel) + 1;
        update_high_water(&coordinator.workers_high_water, active);
        let _active = ActiveWorkerGuard(&coordinator.workers_current);
        coordinator.drive(work);
    }
}

struct ActiveWorkerGuard<'a>(&'a AtomicUsize);

impl Drop for ActiveWorkerGuard<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

impl ContextCompactionCoordinator {
    fn drive(&self, work: CompactionWork) {
        #[cfg(feature = "test-faults")]
        match work {
            CompactionWork::Hold(gate) => {
                gate.wait(&self.closing);
                return;
            }
            CompactionWork::Probe => return,
            CompactionWork::Operation { local, target } => {
                self.drive_operation(local, target);
                return;
            }
        }
        #[cfg(not(feature = "test-faults"))]
        let CompactionWork::Operation { local, target } = work;
        #[cfg(not(feature = "test-faults"))]
        self.drive_operation(local, target);
    }

    fn drive_operation(&self, local: Arc<LocalCompaction>, mut target: LiveEventTarget) {
        if !local.command_is_current() {
            self.fail_local(&local);
            drop(target);
            return;
        }
        if self.closing.load(Ordering::Acquire) {
            let _ = self.settle(&local, CompactionSettlement::CancelledBeforeDispatch);
            self.fail_local(&local);
            let _ = target.into_context_compaction_nondispatch_projection();
            return;
        }
        if self.claim_dispatch(&local).is_err() {
            let _ = self.settle(&local, CompactionSettlement::CancelledBeforeDispatch);
            self.fail_local(&local);
            let _ = target.into_context_compaction_nondispatch_projection();
            return;
        }
        let dispatch = target.dispatch_context_compaction(
            CompactionAttemptCorrelation::from_bytes(*local.attempt.as_bytes()),
            COMPACTION_REQUEST_TIMEOUT,
        );
        let dispatch = match dispatch {
            Ok(Ok(command)) => command.into_parts().0,
            Ok(Err(_)) | Err(_) => {
                let _ = self.abandon(&local, CompactionAbandonmentReason::TargetAuthorityLost);
                self.fail_local(&local);
                target.retire_context_compaction_connection();
                return;
            }
        };
        self.reconcile_dispatch(local, target, dispatch);
    }

    fn claim_dispatch(&self, local: &LocalCompaction) -> Result<(), ContextCompactionError> {
        self.ensure_local_current(local)?;
        let _mutation = local
            .mutation
            .lock()
            .map_err(|_| ContextCompactionError::AuthorityMismatch)?;
        let before = self.read_operation(local.operation_id)?;
        let request =
            ClaimCompactionDispatch::new(local.operation_id, before.revision(), local.attempt);
        require_committed_command(
            self.home
                .execute_current(self.storage.current_claim_compaction_dispatch(request)),
        )?;
        let after = self.read_operation(local.operation_id)?;
        if after.dispatch_claim().is_none()
            || after.attempt() != local.attempt
            || after.state() != &CompactionOperationState::DispatchClaimed
        {
            return Err(ContextCompactionError::Storage);
        }
        Ok(())
    }

    fn reconcile_dispatch(
        &self,
        local: Arc<LocalCompaction>,
        mut target: LiveEventTarget,
        dispatch: ExactContextCompactionDispatch,
    ) {
        if dispatch.before_dispatch_failure().is_some() {
            let _ =
                self.observe_request(&local, CompactionRequestDisposition::ProvenLocalNondispatch);
            let _ = self.settle(&local, CompactionSettlement::LocalNondispatch);
            self.fail_local(&local);
            let _ = target.into_context_compaction_nondispatch_projection();
            return;
        }
        let Some(outcome) = dispatch.outcome() else {
            let _ = self.abandon(&local, CompactionAbandonmentReason::TargetAuthorityLost);
            self.fail_local(&local);
            target.retire_context_compaction_connection();
            return;
        };
        let attempt_matches =
            outcome.request().attempt_correlation().as_bytes() == local.attempt.as_bytes();
        if !attempt_matches {
            if !local.is_finished() {
                let _ = self.abandon(&local, CompactionAbandonmentReason::TargetAuthorityLost);
                self.fail_local(&local);
            }
            target.retire_context_compaction_connection();
            return;
        }
        if local.is_finished() {
            let observation = match outcome.disposition() {
                CompactThreadDisposition::RequestAccepted => {
                    self.observe_request(&local, CompactionRequestDisposition::Accepted)
                }
                CompactThreadDisposition::CompletionUnknown { .. } => {
                    self.observe_request(&local, CompactionRequestDisposition::CompletionUnknown)
                }
                CompactThreadDisposition::ExactRejection { .. } => {
                    self.observe_request(&local, CompactionRequestDisposition::RejectedBeforeCore)
                }
                CompactThreadDisposition::ProvenNotDispatched { .. } => self
                    .observe_request(&local, CompactionRequestDisposition::ProvenLocalNondispatch),
            }
            .unwrap_or(CompactionRequestTransitionStatus::Collision);
            let reconciliation = terminal_response_reconciliation(
                attempt_matches,
                outcome.disposition().into(),
                observation,
                dispatch.unbind_failure().is_some(),
            );
            match reconciliation {
                TerminalResponseReconciliation::AwaitRouterTerminal => {
                    self.await_terminal(&local, target);
                }
                TerminalResponseReconciliation::RetireConnection
                | TerminalResponseReconciliation::InvariantFailure => {
                    target.retire_context_compaction_connection();
                }
            }
            return;
        }
        self.reconcile_live_dispatch(local, target, dispatch);
    }

    fn reconcile_live_dispatch(
        &self,
        local: Arc<LocalCompaction>,
        mut target: LiveEventTarget,
        dispatch: ExactContextCompactionDispatch,
    ) {
        let outcome = dispatch
            .outcome()
            .expect("live response reconciliation retains exact outcome");
        match outcome.disposition() {
            CompactThreadDisposition::RequestAccepted => {
                let observation =
                    match self.observe_request(&local, CompactionRequestDisposition::Accepted) {
                        Ok(observation) => observation,
                        Err(_) => {
                            self.fail_local(&local);
                            target.retire_context_compaction_connection();
                            return;
                        }
                    };
                if observation == CompactionRequestTransitionStatus::Exact {
                    local.mark_accepted();
                }
                if dispatch.unbind_failure().is_some() {
                    if !local.is_finished() {
                        let _ =
                            self.abandon(&local, CompactionAbandonmentReason::TargetAuthorityLost);
                        self.fail_local(&local);
                    }
                    target.retire_context_compaction_connection();
                    return;
                }
                self.await_terminal(&local, target);
            }
            CompactThreadDisposition::ExactRejection { .. } => {
                let observation =
                    self.observe_request(&local, CompactionRequestDisposition::RejectedBeforeCore);
                if observation.is_err() || local.is_finished() {
                    target.retire_context_compaction_connection();
                    return;
                }
                let _ = self.abandon(
                    &local,
                    CompactionAbandonmentReason::ProviderRejectedBeforeCore,
                );
                self.fail_local(&local);
                target.retire_context_compaction_connection();
            }
            CompactThreadDisposition::ProvenNotDispatched { .. } => {
                let _ = self
                    .observe_request(&local, CompactionRequestDisposition::ProvenLocalNondispatch);
                if local.is_finished() {
                    target.retire_context_compaction_connection();
                    return;
                }
                let _ = self.settle(&local, CompactionSettlement::LocalNondispatch);
                self.fail_local(&local);
                let _ = target.into_context_compaction_nondispatch_projection();
            }
            CompactThreadDisposition::CompletionUnknown { .. } => {
                let observation =
                    self.observe_request(&local, CompactionRequestDisposition::CompletionUnknown);
                if matches!(
                    observation,
                    Ok(CompactionRequestTransitionStatus::TerminalAlreadySettled)
                ) || local.is_finished()
                {
                    target.retire_context_compaction_connection();
                    return;
                }
                let _ = self.abandon(&local, CompactionAbandonmentReason::CompletionUnknown);
                self.fail_local(&local);
                target.retire_context_compaction_connection();
            }
        }
    }

    fn await_terminal(&self, local: &LocalCompaction, mut target: LiveEventTarget) {
        loop {
            if !local.command_is_current() {
                self.fail_local(local);
                drop(target);
                return;
            }
            match target.poll(Duration::from_millis(100)) {
                crate::cas_projection::LiveEventPoll::Quiet => {}
                crate::cas_projection::LiveEventPoll::ProvenTerminal(_) => {
                    if !local.is_finished()
                        && let Ok(operation) = self.read_operation(local.operation_id)
                    {
                        let _mutation = local
                            .mutation
                            .lock()
                            .unwrap_or_else(|poison| poison.into_inner());
                        let _ = self.finalize_terminal_locked(local, &operation);
                    }
                    let _ = target.into_proven_terminal_projection();
                    return;
                }
                crate::cas_projection::LiveEventPoll::Closed(_) => {
                    let _ = self.abandon(local, CompactionAbandonmentReason::TargetAuthorityLost);
                    if !local.is_finished() {
                        self.fail_local(local);
                    }
                    drop(target);
                    return;
                }
                crate::cas_projection::LiveEventPoll::Approval(_)
                | crate::cas_projection::LiveEventPoll::DynamicTool(_) => {
                    let _ =
                        self.abandon(local, CompactionAbandonmentReason::ProviderProtocolConflict);
                    self.fail_local(local);
                    target.retire_context_compaction_connection();
                    return;
                }
            }
        }
    }
}
