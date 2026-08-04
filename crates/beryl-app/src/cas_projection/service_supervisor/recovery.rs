use std::{sync::mpsc, time::Instant};

use beryl_home_store::{HomeStore, RecoveryRetrySchedule};
use beryl_state::BerylState;
use syndic_storage::SyndicStorage;

use super::{provider::ProviderFactoryOwner, slot::RunningServiceSlot};
use crate::cas_projection::{
    CandidateSetConvergedAdoptedProjectionConnectionService, PersistentFailureCutHandoff,
    ProjectionCandidateLedgerSealFailure, ProjectionCandidateReauthenticationStatus,
    ProjectionServiceConfig, ScheduledOrdinaryProviderEpochContext,
    TerminalAdoptedProjectionConnectionService, UnpublishedProjectionConnectionService,
};

pub(super) enum RecoveryBuildOutcome {
    Converged(CandidateSetConvergedAdoptedProjectionConnectionService),
    Shutdown,
    Terminal,
}

pub(super) fn build_converged_replacement(
    handoff: PersistentFailureCutHandoff,
    home: &std::sync::Arc<HomeStore>,
    config: ProjectionServiceConfig,
    recovery_signal: &mpsc::SyncSender<()>,
    receiver: &mpsc::Receiver<()>,
    slot: &RunningServiceSlot,
    provider_factory: &mut ProviderFactoryOwner,
) -> RecoveryBuildOutcome {
    let inventory = match handoff.into_recovery_inventory() {
        Ok(inventory) => inventory,
        Err(_owning_error) => return terminal_outcome("recovery inventory"),
    };
    let quarantine = match inventory.into_pending_projection_quarantine() {
        Ok(quarantine) => quarantine,
        Err(_owning_error) => return terminal_outcome("pending-projection quarantine"),
    };
    let mut schedule = RecoveryRetrySchedule::default();
    loop {
        if slot.is_shutting_down() {
            return dispose_retry_shutdown(quarantine);
        }
        match home.recover_same_home() {
            Ok(_) => break,
            Err(_) => {
                if wait_retry_delay(receiver, slot, schedule.next_delay()) {
                    return dispose_retry_shutdown(quarantine);
                }
            }
        }
    }

    let state = match BerylState::reacquire(home) {
        Ok(state) => state,
        Err(_) => return terminal_outcome("Beryl state reacquisition"),
    };
    let storage = match SyndicStorage::reacquire(home) {
        Ok(storage) => storage,
        Err(_) => return terminal_outcome("Syndic storage reacquisition"),
    };
    let health = home.health();
    let Some(home_generation) = health.generation() else {
        return terminal_outcome("recovered home generation");
    };
    let provider = match provider_factory.create_epoch(ScheduledOrdinaryProviderEpochContext::new(
        home.home_id(),
        home_generation,
        state,
    )) {
        Ok(provider) => provider,
        Err(_) => return terminal_outcome("provider epoch construction"),
    };
    let replacement = match UnpublishedProjectionConnectionService::from_recovered_handles(
        std::sync::Arc::clone(home),
        state,
        storage,
        config,
        provider,
    ) {
        Ok(replacement) => replacement,
        Err(_) => return terminal_outcome("replacement service construction"),
    };
    if replacement
        .attach_recovery_supervisor(recovery_signal.clone())
        .is_err()
    {
        return terminal_outcome("replacement notification attachment");
    }
    let mut adopted = match quarantine.adopt_unpublished_service(replacement) {
        Ok(adopted) => adopted,
        Err(error) => {
            let _ = error.dispose();
            return terminal_outcome("service-epoch adoption");
        }
    };
    if adopted.converge_recovered_startup().is_err() {
        let _ = adopted.dispose_after_recovery_failure();
        return terminal_outcome("startup convergence");
    }
    converge_candidates(adopted)
}

fn dispose_retry_shutdown(
    quarantine: crate::cas_projection::PersistentFailurePendingProjectionQuarantine,
) -> RecoveryBuildOutcome {
    if quarantine.dispose_for_supervisor_shutdown().is_ok() {
        RecoveryBuildOutcome::Shutdown
    } else {
        terminal_outcome("retry-shutdown disposition")
    }
}

fn wait_retry_delay(
    receiver: &mpsc::Receiver<()>,
    slot: &RunningServiceSlot,
    delay: std::time::Duration,
) -> bool {
    let deadline = Instant::now() + delay;
    loop {
        if slot.is_shutting_down() {
            return true;
        }
        let now = Instant::now();
        if now >= deadline {
            return false;
        }
        match receiver.recv_timeout(deadline.saturating_duration_since(now)) {
            Ok(()) if slot.is_shutting_down() => return true,
            Ok(()) => {}
            Err(mpsc::RecvTimeoutError::Timeout) => return false,
            Err(mpsc::RecvTimeoutError::Disconnected) => return true,
        }
    }
}

fn converge_candidates(
    adopted: crate::cas_projection::AdoptedUnpublishedProjectionConnectionService,
) -> RecoveryBuildOutcome {
    let mut ledger = adopted.begin_candidate_reauthentication();
    loop {
        if ledger.metadata().terminal_reason().is_some() {
            let terminal = ledger
                .into_terminal_service()
                .expect("a terminal ledger yields its whole-attempt owner");
            dispose_terminal(terminal);
            return terminal_outcome("candidate-ledger terminal state");
        }
        while let Some(candidate_id) = next_unresolved_candidate(&ledger) {
            let outcome = match ledger.reauthenticate_candidate(candidate_id) {
                Ok(outcome) => outcome,
                Err(_) => {
                    let _ = ledger.dispose_after_recovery_failure();
                    return terminal_outcome("candidate reauthentication");
                }
            };
            if outcome.status() == ProjectionCandidateReauthenticationStatus::Rejected
                && ledger.dispose_candidate(candidate_id).is_err()
            {
                let _ = ledger.dispose_after_recovery_failure();
                return terminal_outcome("candidate disposition");
            }
        }
        match ledger.seal() {
            Ok(converged) => return RecoveryBuildOutcome::Converged(converged),
            Err(ProjectionCandidateLedgerSealFailure::Terminal(terminal)) => {
                dispose_terminal(terminal);
                return terminal_outcome("candidate-ledger seal terminal");
            }
            Err(ProjectionCandidateLedgerSealFailure::Retryable(error)) => {
                ledger = error.into_ledger();
                if ledger.metadata().terminal_reason().is_some() {
                    continue;
                }
                if ledger.metadata().unprocessed_count() == 0
                    && ledger.metadata().rejected_count() == 0
                {
                    let _ = ledger.dispose_after_recovery_failure();
                    return terminal_outcome("candidate-ledger seal stalled");
                }
            }
        }
    }
}

fn next_unresolved_candidate(
    ledger: &crate::cas_projection::AdoptedProjectionCandidateReauthenticationLedger,
) -> Option<crate::cas_projection::ProjectionCandidateId> {
    ledger
        .candidates()
        .find(|candidate| {
            matches!(
                candidate.status(),
                ProjectionCandidateReauthenticationStatus::Unprocessed
                    | ProjectionCandidateReauthenticationStatus::Rejected
            )
        })
        .map(|candidate| candidate.candidate_id())
}

fn dispose_terminal(terminal: TerminalAdoptedProjectionConnectionService) {
    let _ = terminal.dispose();
}

fn terminal_outcome(stage: &'static str) -> RecoveryBuildOutcome {
    #[cfg(test)]
    eprintln!("same-home recovery entered a terminal state during {stage}");
    let _ = stage;
    RecoveryBuildOutcome::Terminal
}
