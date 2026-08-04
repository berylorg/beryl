use std::sync::{Arc, Mutex, mpsc};

use beryl_home_store::{HomeHealthState, HomeStore};

use super::{
    RunningSessionRecoveryShutdownError, RunningSessionRecoveryStartError,
    provider::ProviderFactoryOwner,
    recovery::{RecoveryBuildOutcome, build_converged_replacement},
    slot::RunningServiceSlot,
};
use crate::cas_projection::{
    PersistentFailureNotificationStatus, ProjectionConnectionServiceCloseOutcome,
    ProjectionServiceConfig,
};

pub(super) struct RecoveryWorkerStart {
    pub(super) home: Arc<HomeStore>,
    pub(super) config: ProjectionServiceConfig,
    pub(super) slot: Arc<RunningServiceSlot>,
    pub(super) signal: mpsc::SyncSender<()>,
    pub(super) receiver: mpsc::Receiver<()>,
    pub(super) provider_factory: ProviderFactoryOwner,
}

pub(super) struct RecoveryWorkerExit {
    service_error: Option<crate::cas_projection::ProjectionConnectionServiceCloseError>,
    terminal_recovery: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RecoveryWorkerAction {
    Continue,
    Shutdown,
    Terminal,
}

enum RecoveryCutFailure {
    CurrentNotification,
    FailureNotification(PersistentFailureNotificationStatus),
    ServiceWithdrawal,
    EpochOwnership,
    ServiceClosed,
    ServiceClose,
}

impl std::fmt::Display for RecoveryCutFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CurrentNotification => formatter.write_str("current notification lookup"),
            Self::FailureNotification(status) => {
                write!(formatter, "failure notification returned {status:?}")
            }
            Self::ServiceWithdrawal => formatter.write_str("service withdrawal"),
            Self::EpochOwnership => formatter.write_str("service-epoch ownership recovery"),
            Self::ServiceClosed => formatter.write_str("service was already closed"),
            Self::ServiceClose => formatter.write_str("persistent-failure service close"),
        }
    }
}

impl RecoveryWorkerStart {
    pub(super) fn spawn(
        self,
    ) -> Result<std::thread::JoinHandle<RecoveryWorkerExit>, RunningSessionRecoveryStartError> {
        let Self {
            home,
            config,
            slot,
            signal,
            receiver,
            provider_factory,
        } = self;
        let factory = Arc::new(Mutex::new(Some(provider_factory)));
        let thread_factory = Arc::clone(&factory);
        let thread_slot = Arc::clone(&slot);
        let handle = std::thread::Builder::new()
            .name("beryl-running-session-recovery".to_owned())
            .spawn(move || {
                let provider_factory = thread_factory
                    .lock()
                    .unwrap_or_else(|poison| poison.into_inner())
                    .take()
                    .expect("the recovery worker takes its provider factory once");
                run(
                    home,
                    config,
                    thread_slot,
                    signal,
                    receiver,
                    provider_factory,
                )
            });
        match handle {
            Ok(handle) => Ok(handle),
            Err(error) => {
                slot.begin_shutdown();
                let service_error = settle_current_service(&slot).err();
                let mut provider_factory = factory
                    .lock()
                    .unwrap_or_else(|poison| poison.into_inner())
                    .take()
                    .expect("a failed worker spawn leaves the provider factory in escrow");
                provider_factory.shutdown();
                let suffix = service_error
                    .map(|failure| format!("; initial service cleanup failed: {failure}"))
                    .unwrap_or_default();
                Err(RunningSessionRecoveryStartError::WorkerSpawn(format!(
                    "{error}{suffix}"
                )))
            }
        }
    }
}

fn run(
    home: Arc<HomeStore>,
    config: ProjectionServiceConfig,
    slot: Arc<RunningServiceSlot>,
    signal: mpsc::SyncSender<()>,
    receiver: mpsc::Receiver<()>,
    mut provider_factory: ProviderFactoryOwner,
) -> RecoveryWorkerExit {
    let mut terminal_recovery = false;
    while receiver.recv().is_ok() {
        if slot.is_shutting_down() {
            break;
        }
        let Ok((flight_home_generation, flight_service_generation, flight_notification)) =
            slot.current_notification()
        else {
            terminal_recovery = true;
            break;
        };
        let mut completed_verification = None;
        let action = match home.health().state() {
            HomeHealthState::Healthy | HomeHealthState::Opening | HomeHealthState::Reopening => {
                RecoveryWorkerAction::Continue
            }
            HomeHealthState::Verifying => match home.verify_health() {
                Ok(health)
                    if health.state() == HomeHealthState::Healthy
                        && health.generation() == Some(flight_home_generation) =>
                {
                    if let Some(completed) = slot.complete_same_generation_verification(
                        &home,
                        flight_home_generation,
                        flight_service_generation,
                        &flight_notification,
                    ) {
                        completed_verification = Some(completed);
                        RecoveryWorkerAction::Continue
                    } else if slot.is_shutting_down() {
                        RecoveryWorkerAction::Shutdown
                    } else {
                        RecoveryWorkerAction::Terminal
                    }
                }
                Ok(_) | Err(_) => recover_current_service(
                    &home,
                    config,
                    &slot,
                    &signal,
                    &receiver,
                    &mut provider_factory,
                ),
            },
            HomeHealthState::Failed => recover_current_service(
                &home,
                config,
                &slot,
                &signal,
                &receiver,
                &mut provider_factory,
            ),
        };
        match action {
            RecoveryWorkerAction::Continue => {
                flight_notification.finish_completed_recovery_supervisor_flight(
                    completed_verification,
                    !slot.is_shutting_down(),
                );
            }
            RecoveryWorkerAction::Shutdown => break,
            RecoveryWorkerAction::Terminal => {
                terminal_recovery = true;
                slot.mark_terminal();
                break;
            }
        }
    }
    slot.begin_shutdown();
    drop(home);
    let service_error = settle_current_service(&slot).err();
    provider_factory.shutdown();
    RecoveryWorkerExit {
        service_error,
        terminal_recovery,
    }
}

fn recover_current_service(
    home: &Arc<HomeStore>,
    config: ProjectionServiceConfig,
    slot: &Arc<RunningServiceSlot>,
    signal: &mpsc::SyncSender<()>,
    receiver: &mpsc::Receiver<()>,
    provider_factory: &mut ProviderFactoryOwner,
) -> RecoveryWorkerAction {
    let handoff = match cut_current_failed_service(slot) {
        Ok(handoff) => handoff,
        Err(reason) => {
            #[cfg(test)]
            eprintln!(
                "same-home recovery entered a terminal state while cutting the current service: {reason}"
            );
            let _ = reason;
            return RecoveryWorkerAction::Terminal;
        }
    };
    match build_converged_replacement(
        handoff,
        home,
        config,
        signal,
        receiver,
        slot,
        provider_factory,
    ) {
        RecoveryBuildOutcome::Converged(converged) => {
            match converged.publish_recovered_service(slot) {
                Ok(_metadata) => RecoveryWorkerAction::Continue,
                Err(error) => {
                    let _ = error.dispose();
                    #[cfg(test)]
                    eprintln!(
                        "same-home recovery entered a terminal state during final publication"
                    );
                    RecoveryWorkerAction::Terminal
                }
            }
        }
        RecoveryBuildOutcome::Shutdown => RecoveryWorkerAction::Shutdown,
        RecoveryBuildOutcome::Terminal => RecoveryWorkerAction::Terminal,
    }
}

fn cut_current_failed_service(
    slot: &Arc<RunningServiceSlot>,
) -> Result<crate::cas_projection::PersistentFailureCutHandoff, RecoveryCutFailure> {
    let (_home_generation, service_generation, notification) = slot
        .current_notification()
        .map_err(|_| RecoveryCutFailure::CurrentNotification)?;
    let notification_status = notification.notify();
    if !matches!(
        notification_status,
        PersistentFailureNotificationStatus::Signaled | PersistentFailureNotificationStatus::Joined
    ) {
        return Err(RecoveryCutFailure::FailureNotification(notification_status));
    }
    let epoch = slot
        .withdraw(service_generation)
        .map_err(|_| RecoveryCutFailure::ServiceWithdrawal)?;
    slot.wait_until_unleased();
    let (service, _state) = epoch
        .into_parts()
        .map_err(|_| RecoveryCutFailure::EpochOwnership)?;
    match service.close() {
        Ok(ProjectionConnectionServiceCloseOutcome::PersistentFailure(handoff)) => Ok(handoff),
        Ok(ProjectionConnectionServiceCloseOutcome::Closed) => {
            Err(RecoveryCutFailure::ServiceClosed)
        }
        Err(_) => Err(RecoveryCutFailure::ServiceClose),
    }
}

fn settle_current_service(
    slot: &Arc<RunningServiceSlot>,
) -> Result<(), crate::cas_projection::ProjectionConnectionServiceCloseError> {
    let Some(epoch) = slot.take_for_shutdown() else {
        return Ok(());
    };
    slot.wait_until_unleased();
    let (service, _state) = epoch.into_parts().map_err(|_| {
        crate::cas_projection::ProjectionConnectionServiceCloseError::HomeOwnershipLeaked
    })?;
    match service.close()? {
        ProjectionConnectionServiceCloseOutcome::Closed => Ok(()),
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(_handoff) => Err(
            crate::cas_projection::ProjectionConnectionServiceCloseError::PersistentFailureWorkerShutdown,
        ),
    }
}

impl RecoveryWorkerExit {
    pub(super) fn into_result(self) -> Result<(), RunningSessionRecoveryShutdownError> {
        if let Some(error) = self.service_error {
            Err(RunningSessionRecoveryShutdownError::Service(error))
        } else if self.terminal_recovery {
            Err(RunningSessionRecoveryShutdownError::TerminalRecovery)
        } else {
            Ok(())
        }
    }
}
