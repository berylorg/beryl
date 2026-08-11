use std::sync::{Arc, Mutex, mpsc};

use super::{
    RunningSessionRecoveryShutdownError, RunningSessionRecoveryStartError,
    provider::ProviderFactoryOwner, slot::RunningServiceSlot,
};
use crate::cas_projection::{
    PersistentFailureNotificationStatus, ProjectionConnectionServiceCloseError,
    ProjectionConnectionServiceCloseOutcome,
};

pub(super) struct RecoveryWorkerStart {
    pub(super) slot: Arc<RunningServiceSlot>,
    pub(super) receiver: mpsc::Receiver<()>,
    pub(super) provider_factory: ProviderFactoryOwner,
}

pub(super) struct RecoveryWorkerExit {
    service_error: Option<ProjectionConnectionServiceCloseError>,
    terminal_unavailable: bool,
    provider_factory: Option<ProviderFactoryOwner>,
}

enum RecoveryCutFailure {
    CurrentNotification,
    FailureNotification(PersistentFailureNotificationStatus),
    ServiceWithdrawal,
    ServiceOwnership,
    ServiceClosed,
    ServiceClose(ProjectionConnectionServiceCloseError),
}

impl std::fmt::Display for RecoveryCutFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CurrentNotification => formatter.write_str("current notification lookup"),
            Self::FailureNotification(status) => {
                write!(formatter, "failure notification returned {status:?}")
            }
            Self::ServiceWithdrawal => formatter.write_str("service withdrawal"),
            Self::ServiceOwnership => formatter.write_str("service ownership recovery"),
            Self::ServiceClosed => formatter.write_str("service was already ordinarily closed"),
            Self::ServiceClose(error) => write!(formatter, "failed-service disposal: {error}"),
        }
    }
}

impl RecoveryWorkerStart {
    pub(super) fn spawn(
        self,
    ) -> Result<std::thread::JoinHandle<RecoveryWorkerExit>, RunningSessionRecoveryStartError> {
        let Self {
            slot,
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
                run(thread_slot, receiver, provider_factory)
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
                    .expect("a failed worker spawn leaves the provider factory owned");
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
    slot: Arc<RunningServiceSlot>,
    receiver: mpsc::Receiver<()>,
    mut provider_factory: ProviderFactoryOwner,
) -> RecoveryWorkerExit {
    let mut terminal_unavailable = false;
    let mut service_error = None;
    while receiver.recv().is_ok() {
        if slot.is_shutting_down() {
            break;
        }
        let Ok((_home_generation, _service_generation, notification)) = slot.current_notification()
        else {
            terminal_unavailable = true;
            break;
        };
        match notification.notify() {
            PersistentFailureNotificationStatus::NotFailed => {}
            PersistentFailureNotificationStatus::Unavailable => terminal_unavailable = true,
            PersistentFailureNotificationStatus::Signaled
            | PersistentFailureNotificationStatus::Joined => {
                match cut_and_dispose_current_failed_service(&slot) {
                    Ok(()) => terminal_unavailable = true,
                    Err(RecoveryCutFailure::ServiceClose(error)) => {
                        service_error = Some(error);
                        terminal_unavailable = true;
                    }
                    Err(reason) => {
                        #[cfg(test)]
                        eprintln!(
                            "running-session recovery became unavailable while disposing the failed service: {reason}"
                        );
                        let _ = reason;
                        terminal_unavailable = true;
                    }
                }
            }
        }
        if terminal_unavailable {
            slot.mark_terminal();
            break;
        }
    }
    slot.begin_shutdown();
    if service_error.is_none() {
        service_error = settle_current_service(&slot).err();
    }
    provider_factory.shutdown();
    if terminal_unavailable {
        slot.mark_terminal_settled();
    }
    RecoveryWorkerExit {
        service_error,
        terminal_unavailable,
        provider_factory: Some(provider_factory),
    }
}

fn cut_and_dispose_current_failed_service(
    slot: &Arc<RunningServiceSlot>,
) -> Result<(), RecoveryCutFailure> {
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
    let publication = slot
        .withdraw(service_generation)
        .map_err(|_| RecoveryCutFailure::ServiceWithdrawal)?;
    slot.wait_until_unleased();
    let (service, _state) = publication
        .into_parts()
        .map_err(|_| RecoveryCutFailure::ServiceOwnership)?;
    match service.close() {
        Ok(ProjectionConnectionServiceCloseOutcome::PersistentFailure(_evidence)) => Ok(()),
        Ok(ProjectionConnectionServiceCloseOutcome::Closed) => {
            Err(RecoveryCutFailure::ServiceClosed)
        }
        Err(error) => Err(RecoveryCutFailure::ServiceClose(error)),
    }
}

fn settle_current_service(
    slot: &Arc<RunningServiceSlot>,
) -> Result<(), ProjectionConnectionServiceCloseError> {
    let Some(publication) = slot.take_for_shutdown() else {
        return Ok(());
    };
    slot.wait_until_unleased();
    let (service, _state) = publication
        .into_parts()
        .map_err(|_| ProjectionConnectionServiceCloseError::HomeOwnershipLeaked)?;
    service.close().map(|_terminal_outcome| ())
}

impl RecoveryWorkerExit {
    pub(super) fn into_result(self) -> Result<(), RunningSessionRecoveryShutdownError> {
        let Self {
            service_error,
            terminal_unavailable,
            mut provider_factory,
        } = self;
        if let Some(provider_factory) = provider_factory.as_mut() {
            provider_factory.shutdown();
        }
        if let Some(error) = service_error {
            Err(RunningSessionRecoveryShutdownError::Service(error))
        } else if terminal_unavailable {
            Err(RunningSessionRecoveryShutdownError::TerminalRecovery)
        } else {
            Ok(())
        }
    }
}
