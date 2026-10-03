use super::RunningProcessOwner;
use crate::startup_owner::{RunningExitGate, RunningExitRequest, RunningWindowExit};
use beryl_model::WindowId;
use gpui::App;
use std::{cell::RefCell, rc::Rc};

impl RunningProcessOwner {
    #[cfg(test)]
    pub(crate) fn test_window_command_unchecked(&self, window: WindowId) -> RunningWindowExit {
        self.process.commands.window_command(window)
    }

    pub(crate) fn set_exit_gate(&self, gate: RunningExitGate, blocked: bool) {
        self.process.commands.set_gate(gate, blocked);
    }

    pub(crate) fn window_exit_command(
        &self,
        invoking: WindowId,
        app: &App,
    ) -> Result<RunningWindowExit, String> {
        self.require_exit_window(invoking, app)?;
        Ok(self.process.commands.window_command(invoking))
    }

    pub(crate) fn resolve_exit_window(
        &self,
        request: &mut RunningExitRequest,
        app: &App,
    ) -> Result<WindowId, String> {
        let invoking = self
            .process
            .commands
            .bind_invoking_window(request, self.process.windows.window_ids().first().copied())
            .ok_or("the Exit request is not active here or has no published main window")?;
        self.require_exit_window(invoking, app)?;
        Ok(invoking)
    }

    fn require_exit_window(&self, invoking: WindowId, app: &App) -> Result<(), String> {
        if self.process.windows.window_ids().contains(&invoking)
            && self.process.windows.shells().iter().any(|shell| {
                shell
                    .window()
                    .read(app)
                    .ok()
                    .and_then(|root| root.controller())
                    .is_some_and(|controller| controller.window_id() == invoking)
            })
        {
            Ok(())
        } else {
            Err("the invoking main window is unavailable".into())
        }
    }

    pub(crate) fn wait_for_exit(
        owner: &Rc<RefCell<Self>>,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, RunningExitRequest, &mut App) + 'static,
    ) -> Result<(), String> {
        {
            let mut owner = owner.borrow_mut();
            if owner.waiting_for_exit {
                return Err("a running Exit wait is already pending".to_owned());
            }
            owner.waiting_for_exit = true;
        }
        #[cfg(test)]
        let (stop, mut stop_receiver) = futures_channel::oneshot::channel();
        #[cfg(test)]
        {
            owner.borrow_mut().exit_wait_stop = Some(stop);
        }
        let retained = owner.clone();
        app.spawn(async move |cx| {
            #[cfg(test)]
            let mut stopped = None;
            let request = std::future::poll_fn(|cx| {
                #[cfg(test)]
                if let std::task::Poll::Ready(Ok(acknowledged)) =
                    std::future::Future::poll(std::pin::Pin::new(&mut stop_receiver), cx)
                {
                    stopped = Some(acknowledged);
                    return std::task::Poll::Ready(None);
                }
                retained
                    .borrow_mut()
                    .process
                    .commands
                    .poll_exit(cx)
                    .map(Some)
            })
            .await;
            retained.borrow_mut().waiting_for_exit = false;
            #[cfg(test)]
            {
                retained.borrow_mut().exit_wait_stop = None;
            }
            if let Some(request) = request {
                let _ = cx.update(|app| completed(&retained, request, app));
            } else {
                drop(completed);
                drop(retained);
                #[cfg(test)]
                if let Some(acknowledged) = stopped {
                    let _ = acknowledged.send(());
                }
            }
        })
        .detach();
        Ok(())
    }

    #[cfg(test)]
    pub(crate) async fn test_stop_running_observers(
        owner: &Rc<RefCell<Self>>,
        cx: &mut gpui::AsyncApp,
    ) -> Result<(), String> {
        let stop = cx
            .update(|_| {
                let mut retained = owner.borrow_mut();
                if retained.observing_initial_work
                    || retained.confirmation.is_some()
                    || retained.progress.is_some()
                    || retained.process.services.is_none()
                    || retained.process.commands.lifecycle_admitted()
                    || retained
                        .automatic_recovery_outcome()
                        .is_some_and(|outcome| {
                            matches!(*outcome, super::InterruptedExitRecoveryOutcome::Running)
                        })
                {
                    return Err("running observer stop requires settled owner work".to_string());
                }
                retained.exit_availability.take();
                Ok(retained.exit_wait_stop.take())
            })
            .map_err(|e| e.to_string())??;
        if let Some(stop) = stop {
            let (acknowledged, completed) = futures_channel::oneshot::channel();
            stop.send(acknowledged)
                .map_err(|_| "running command wait stop is unavailable")?;
            completed
                .await
                .map_err(|_| "running command wait stop did not settle")?;
        }
        Ok(())
    }

    pub(crate) fn finish_exit(owner: &Rc<RefCell<Self>>, request: &RunningExitRequest) -> bool {
        let result = {
            let mut owner = owner.borrow_mut();
            if owner.observing_initial_work
                || owner.confirmation.is_some()
                || owner.shutdown.is_some()
                || owner.progress.is_some()
                || owner.process.services.is_none()
                || owner.interrupted_exit.is_some()
                || owner.final_teardown.is_some()
            {
                return false;
            }
            owner.process.commands.finish_exit_deferred_wake(request)
        };
        let Ok(wake) = result else {
            return false;
        };
        if let Some(wake) = wake {
            wake.wake();
        }
        true
    }
}
