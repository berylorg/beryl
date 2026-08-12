use std::sync::{Arc, mpsc};

use super::{TerminalServiceShutdownError, TerminalServiceStartError, slot::RunningServiceSlot};
use crate::cas_projection::{
    PersistentFailureNotificationStatus, ProjectionConnectionServiceCloseError,
    ProjectionConnectionServiceCloseOutcome,
};

pub(super) struct TerminalWorkerStart {
    pub(super) slot: Arc<RunningServiceSlot>,
    pub(super) receiver: mpsc::Receiver<()>,
}

pub(super) struct TerminalWorkerExit {
    service_error: Option<ProjectionConnectionServiceCloseError>,
    terminal_unavailable: bool,
}

enum TerminalCutFailure {
    CurrentNotification,
    FailureNotification(PersistentFailureNotificationStatus),
    ServiceWithdrawal,
    ServiceOwnership,
    ServiceClosed,
    ServiceClose(ProjectionConnectionServiceCloseError),
}

impl std::fmt::Display for TerminalCutFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CurrentNotification => formatter.write_str("current notification lookup"),
            Self::FailureNotification(status) => {
                write!(formatter, "failure notification returned {status:?}")
            }
            Self::ServiceWithdrawal => formatter.write_str("service withdrawal"),
            Self::ServiceOwnership => formatter.write_str("service ownership recovery failed"),
            Self::ServiceClosed => formatter.write_str("service was already ordinarily closed"),
            Self::ServiceClose(error) => write!(formatter, "failed-service disposal: {error}"),
        }
    }
}

impl TerminalWorkerStart {
    pub(super) fn spawn(
        self,
    ) -> Result<std::thread::JoinHandle<TerminalWorkerExit>, TerminalServiceStartError> {
        let Self { slot, receiver } = self;
        let thread_slot = Arc::clone(&slot);
        let handle = std::thread::Builder::new()
            .name("beryl-terminal-service-disposal".to_owned())
            .spawn(move || run(thread_slot, receiver));
        match handle {
            Ok(handle) => Ok(handle),
            Err(error) => {
                slot.begin_shutdown();
                let service_error = settle_current_service(&slot).err();
                let suffix = service_error
                    .map(|failure| format!("; initial service cleanup failed: {failure}"))
                    .unwrap_or_default();
                Err(TerminalServiceStartError::WorkerSpawn(format!(
                    "{error}{suffix}"
                )))
            }
        }
    }
}

fn run(slot: Arc<RunningServiceSlot>, receiver: mpsc::Receiver<()>) -> TerminalWorkerExit {
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
                    Err(TerminalCutFailure::ServiceClose(error)) => {
                        service_error = Some(error);
                        terminal_unavailable = true;
                    }
                    Err(reason) => {
                        #[cfg(test)]
                        eprintln!(
                            "the service became unavailable while disposing the failed service: {reason}"
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
    if terminal_unavailable {
        slot.mark_terminal_settled();
    }
    TerminalWorkerExit {
        service_error,
        terminal_unavailable,
    }
}

fn cut_and_dispose_current_failed_service(
    slot: &Arc<RunningServiceSlot>,
) -> Result<(), TerminalCutFailure> {
    let (_home_generation, service_generation, notification) = slot
        .current_notification()
        .map_err(|_| TerminalCutFailure::CurrentNotification)?;
    let notification_status = notification.notify();
    if !matches!(
        notification_status,
        PersistentFailureNotificationStatus::Signaled | PersistentFailureNotificationStatus::Joined
    ) {
        return Err(TerminalCutFailure::FailureNotification(notification_status));
    }
    let publication = slot
        .withdraw(service_generation)
        .map_err(|_| TerminalCutFailure::ServiceWithdrawal)?;
    slot.wait_until_unleased();
    let (service, _state) = publication
        .into_parts()
        .map_err(|_| TerminalCutFailure::ServiceOwnership)?;
    match service.close() {
        Ok(ProjectionConnectionServiceCloseOutcome::PersistentFailure(_evidence)) => Ok(()),
        Ok(ProjectionConnectionServiceCloseOutcome::Closed) => {
            Err(TerminalCutFailure::ServiceClosed)
        }
        Err(error) => Err(TerminalCutFailure::ServiceClose(error)),
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

impl TerminalWorkerExit {
    pub(super) fn into_result(self) -> Result<(), TerminalServiceShutdownError> {
        let Self {
            service_error,
            terminal_unavailable,
        } = self;
        if let Some(error) = service_error {
            Err(TerminalServiceShutdownError::Service(error))
        } else if terminal_unavailable {
            Err(TerminalServiceShutdownError::TerminalUnavailable)
        } else {
            Ok(())
        }
    }
}
