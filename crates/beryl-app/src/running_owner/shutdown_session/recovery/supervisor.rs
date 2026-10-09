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
    ordinary_notices: Option<(
        Vec<gpui::WindowHandle<crate::main_window::MainWindowShellRoot>>,
        crate::main_window::NoticeConditionId,
        crate::main_window::NoticeConditionId,
        beryl_home_store::HomeGeneration,
    )>,
    _task: Task<()>,
}

#[cfg(test)]
pub(super) struct OrdinaryContinuationObservation {
    request: Rc<()>,
    outcome: Rc<RefCell<InterruptedExitRecoveryOutcome>>,
}

impl Drop for AutomaticInterruptedExitRecovery {
    fn drop(&mut self) {
        self.cancellation.cancel();
    }
}

impl RunningProcessOwner {
    #[cfg(test)]
    pub(super) fn test_ordinary_continuation_observation(
        &self,
        request: &Rc<()>,
    ) -> Result<OrdinaryContinuationObservation, String> {
        let original = self
            .automatic_recovery
            .as_ref()
            .ok_or("original ordinary recovery outcome is unavailable")?;
        if !self.active_recovery_identity(request) || !Rc::ptr_eq(&original.request, request) {
            return Err("original ordinary recovery continuation identity changed".into());
        }
        Ok(OrdinaryContinuationObservation {
            request: original.request.clone(),
            outcome: original.outcome.clone(),
        })
    }

    #[cfg(test)]
    pub(super) fn test_complete_ordinary_continuation(
        &self,
        observed: OrdinaryContinuationObservation,
    ) -> Result<(), String> {
        let original = self
            .automatic_recovery
            .as_ref()
            .ok_or("original ordinary recovery outcome is unavailable")?;
        if self
            .interrupted_exit
            .as_ref()
            .is_some_and(|recovery| !Rc::ptr_eq(&recovery.request, &observed.request))
            || !Rc::ptr_eq(&original.request, &observed.request)
            || !Rc::ptr_eq(&original.outcome, &observed.outcome)
        {
            return Err("original ordinary recovery completion identity changed".into());
        }
        *observed.outcome.borrow_mut() = InterruptedExitRecoveryOutcome::Completed;
        Ok(())
    }

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
        Self::start_recovery_task(owner, request.retain_for_recovery(), app);
    }

    pub(super) fn start_recovery_task(
        owner: &Rc<RefCell<Self>>,
        request: impl RecoveryIdentity + 'static,
        app: &mut App,
    ) {
        let original_generation = owner.borrow().recovery_supervisor_generation(&request);
        let ordinary_notices = request
            .lifecycle()
            .is_none()
            .then(|| {
                let retained = owner.borrow();
                original_generation.as_ref().ok().map(|generation| {
                    (
                        retained
                            .process
                            .windows
                            .shells()
                            .iter()
                            .map(|shell| shell.window())
                            .collect(),
                        retained
                            .observed_home_failure
                            .as_ref()
                            .filter(|failure| failure.generation == *generation)
                            .map(|failure| failure.condition.clone())
                            .unwrap_or_default(),
                        crate::main_window::NoticeConditionId::new(),
                        *generation,
                    )
                })
            })
            .flatten();
        let configured = (|| -> Result<_, String> {
            let retained = owner.borrow();
            let mut configured = Vec::new();
            for shell in retained.process.windows.shells().iter().filter(|_| {
                retained
                    .process
                    .services
                    .as_ref()
                    .and_then(|services| services.graph())
                    .is_some()
                    && retained
                        .interrupted_exit
                        .as_ref()
                        .is_some_and(|recovery| recovery.publication.borrow().is_none())
            }) {
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
            let generation = *original_generation.as_ref().map_err(Clone::clone)?;
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
        let task = app.spawn(async move |cx| {
            let retired = configured.as_ref().ok().map(|(_, generation)| *generation);
            let result = async {
                let (mut configured, _) = configured?;
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|_| "Recovery clock is before the Unix epoch")?;
                let millis =
                    u64::try_from(now.as_millis()).map_err(|_| "Recovery timestamp overflowed")?;
                if let Some(lifecycle) = request.lifecycle() {
                    weak.recovery_owner()?
                        .borrow_mut()
                        .retain_interrupted_exit_session(lifecycle)?;
                }
                Self::recover_owned_claim_operation(
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
            ordinary_notices,
            _task: task,
        });
    }

    pub(super) fn recovery_supervisor_generation(
        &self,
        request: &impl RecoveryIdentity,
    ) -> Result<beryl_home_store::HomeGeneration, String> {
        if !self.active_recovery_identity(&request.identity()) {
            return Err("Recovery request changed".into());
        }
        if request.lifecycle().is_none() {
            let recovery = self
                .interrupted_exit
                .as_ref()
                .ok_or("ordinary recovery capture is unavailable")?;
            return match recovery.session.borrow().as_ref() {
                Some(RunningShutdownSession::UnchangedRunning(capture)) => {
                    Ok(capture.captured_generation())
                }
                _ => Err("ordinary recovery original generation capture is unavailable".into()),
            };
        }
        self.process
            .services
            .as_ref()
            .and_then(|services| services.graph())
            .and_then(|graph| graph.home().health().generation())
            .ok_or_else(|| "Recovery home generation is unavailable".into())
    }

    pub(in crate::running_owner) fn project_running_home_recovery_notices(
        owner: &Rc<RefCell<Self>>,
        app: &mut App,
    ) {
        use crate::main_window::MainWindowHomeRecoveryNoticeState as State;
        let projection = {
            let retained = owner.borrow();
            let Some(recovery) = &retained.automatic_recovery else {
                return;
            };
            let Some((windows, failed, recovered, generation)) = &recovery.ordinary_notices else {
                return;
            };
            if retained
                .observed_home_failure
                .as_ref()
                .is_some_and(|failure| failure.generation != *generation)
            {
                return;
            }
            let state = match &*recovery.outcome.borrow() {
                InterruptedExitRecoveryOutcome::Running if recovery.failure.borrow().is_some() => {
                    State::Retrying
                }
                InterruptedExitRecoveryOutcome::Running => State::Recovering,
                InterruptedExitRecoveryOutcome::Completed => State::Recovered,
                InterruptedExitRecoveryOutcome::Cancelled => State::Cancelled,
                InterruptedExitRecoveryOutcome::Unavailable(_) => State::Unavailable,
            };
            (
                windows.clone(),
                if state == State::Recovered {
                    recovered.clone()
                } else {
                    failed.clone()
                },
                state,
            )
        };
        for window in projection.0 {
            let _ = window.update(app, |root, window, cx| {
                root.project_running_home_recovery_notice(&projection.1, projection.2, window, cx);
            });
        }
    }

    pub(in crate::running_owner) fn ordinary_recovery_observed_generation(
        &self,
    ) -> Option<beryl_home_store::HomeGeneration> {
        self.automatic_recovery
            .as_ref()
            .and_then(|recovery| recovery.ordinary_notices.as_ref().map(|notice| notice.3))
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
