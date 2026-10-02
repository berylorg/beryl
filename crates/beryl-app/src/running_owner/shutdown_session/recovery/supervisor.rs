use super::*;
use beryl_home_store::CommandCancellation;
use gpui::{App, Task};
use syndic_storage::SyndicTimestamp;

pub(crate) enum InterruptedExitRecoveryOutcome {
    Running,
    Completed,
    Cancelled,
    Unavailable(String),
}

pub(in crate::running_owner) struct AutomaticInterruptedExitRecovery {
    request: Rc<()>,
    cancellation: CommandCancellation,
    outcome: Rc<RefCell<InterruptedExitRecoveryOutcome>>,
    failure: Rc<RefCell<Option<RecoveryPreparationFailure>>>,
    _task: Task<()>,
}

impl Drop for AutomaticInterruptedExitRecovery {
    fn drop(&mut self) {
        self.cancellation.cancel();
    }
}

impl RunningProcessOwner {
    pub(in crate::running_owner) fn start_reported_exit_recovery(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        app: &mut App,
    ) {
        {
            let retained = owner.borrow();
            #[cfg(test)]
            if retained.disable_automatic_recovery {
                return;
            }
            if !retained.process.commands.is_active(request)
                || !retained
                    .interrupted_exit
                    .as_ref()
                    .is_some_and(|recovery| Rc::ptr_eq(&recovery.request, &request.identity()))
                || retained
                    .automatic_recovery
                    .as_ref()
                    .is_some_and(|recovery| Rc::ptr_eq(&recovery.request, &request.identity()))
            {
                return;
            }
        }
        let configured = (|| -> Result<_, String> {
            let retained = owner.borrow();
            let mut configured = Vec::new();
            for shell in retained.process.windows.shells() {
                let window = shell.window();
                let root = window.read(app).map_err(|error| error.to_string())?;
                let controller = root
                    .controller()
                    .ok_or("Interrupted Exit window controller is unavailable")?;
                if let Some(mount) = controller.composer_mount() {
                    let resident = mount
                        .read(app)
                        .contribution()
                        .ok_or("Interrupted Exit resident is unavailable")?;
                    configured.push((
                        window,
                        resident.update(app, |resident, cx| {
                            resident.interrupted_exit_configurator(cx)
                        })?,
                    ));
                }
            }
            let generation = retained
                .process
                .services
                .as_ref()
                .ok_or("Recovery service owner is unavailable")?
                .graph()
                .ok_or("Recovery service graph is unavailable")?
                .home()
                .health()
                .generation()
                .ok_or("Recovery home generation is unavailable")?;
            Ok((configured, generation))
        })();
        let cancellation = CommandCancellation::new();
        let outcome = Rc::new(RefCell::new(InterruptedExitRecoveryOutcome::Running));
        let failure = Rc::new(RefCell::new(None));
        let weak = Rc::downgrade(owner);
        let retained_outcome = outcome.clone();
        let retained_failure = failure.clone();
        let task_cancellation = cancellation.clone();
        let identity = request.identity();
        let request = request.retain_for_recovery();
        let task = app.spawn(async move |cx| {
            let retired = configured.as_ref().ok().map(|(_, generation)| *generation);
            let result = async {
                let (mut configured, _) = configured?;
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|_| "Recovery clock is before the Unix epoch")?;
                let millis =
                    u64::try_from(now.as_millis()).map_err(|_| "Recovery timestamp overflowed")?;
                weak.recovery_owner()?
                    .borrow_mut()
                    .retain_interrupted_exit_session(&request)?;
                Self::recover_interrupted_exit(
                    &weak,
                    &request,
                    SyndicTimestamp::from_unix_millis(millis),
                    task_cancellation.clone(),
                    move |window| {
                        let index = configured
                            .iter()
                            .position(|(captured, _)| *captured == window)
                            .ok_or("Interrupted Exit composer settings are unavailable")?;
                        Ok(configured.swap_remove(index).1)
                    },
                    move |reported| *retained_failure.borrow_mut() = Some(reported),
                    cx,
                )
                .await
            }
            .await;
            let cleanup = if result.is_err() && task_cancellation.is_cancelled() {
                if let Some(retired) = retired {
                    Self::settle_automatic_interrupted_exit_cancellation(
                        &weak, &request, retired, cx,
                    )
                    .await
                } else {
                    Ok(())
                }
            } else {
                Ok(())
            };
            *retained_outcome.borrow_mut() = match (result, cleanup) {
                (Ok(()), _) => InterruptedExitRecoveryOutcome::Completed,
                (Err(error), Err(cleanup)) => InterruptedExitRecoveryOutcome::Unavailable(format!(
                    "{error}; cancellation cleanup: {cleanup}"
                )),
                (Err(_), Ok(())) if task_cancellation.is_cancelled() => {
                    InterruptedExitRecoveryOutcome::Cancelled
                }
                (Err(error), Ok(())) => InterruptedExitRecoveryOutcome::Unavailable(error),
            };
        });
        owner.borrow_mut().automatic_recovery = Some(AutomaticInterruptedExitRecovery {
            request: identity,
            cancellation,
            outcome,
            failure,
            _task: task,
        });
    }

    #[cfg(test)]
    pub(crate) fn test_disable_automatic_recovery(&mut self) {
        self.disable_automatic_recovery = true;
    }

    #[cfg(test)]
    pub(crate) fn test_cancel_recovery_before_publication_validation(&mut self) {
        self.cancel_recovery_before_publication_validation = true;
    }

    #[cfg(test)]
    pub(crate) fn test_cancel_recovery_after_publication(&mut self) {
        self.cancel_recovery_after_publication = true;
    }

    #[cfg(test)]
    pub(crate) fn test_cancel_recovery_after_resident_admission(&mut self) {
        self.cancel_recovery_after_resident_admission = true;
    }

    pub(crate) fn automatic_recovery_outcome(
        &self,
    ) -> Option<std::cell::Ref<'_, InterruptedExitRecoveryOutcome>> {
        self.automatic_recovery
            .as_ref()
            .map(|recovery| recovery.outcome.borrow())
    }

    pub(crate) fn automatic_recovery_failure(
        &self,
    ) -> Option<std::cell::Ref<'_, Option<RecoveryPreparationFailure>>> {
        self.automatic_recovery
            .as_ref()
            .map(|recovery| recovery.failure.borrow())
    }

    pub(crate) fn cancel_automatic_recovery(&self) {
        if let Some(recovery) = &self.automatic_recovery {
            recovery.cancellation.cancel();
        }
    }
}
