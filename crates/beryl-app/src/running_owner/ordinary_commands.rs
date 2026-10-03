use super::*;
use crate::{
    cas_projection::ProjectionCancellationToken,
    startup_owner::{RunningExitGate, RunningExitRequest},
};
use gpui::{App, AsyncApp};
use std::time::Duration;

mod nonfinal;
mod pre_native;
mod publication;
mod recovery;

struct MountedRunningOwner(std::rc::Weak<RefCell<RunningProcessOwner>>);
impl gpui::Global for MountedRunningOwner {}

impl RunningProcessOwner {
    pub(crate) fn status_stop_worker(
        &self,
    ) -> Option<crate::app_services::PublishedExactStopWorker> {
        self.process
            .services
            .as_ref()?
            .published_exact_stop_worker()
    }
    #[cfg(test)]
    pub(crate) fn test_order_shells_by_descending_window_id(&mut self, app: &App) {
        self.process
            .windows
            .shells_mut()
            .sort_by_key(|shell| std::cmp::Reverse(shell.retained_window_id(app).unwrap()));
    }
    #[cfg(test)]
    pub(crate) fn test_before_next_ordinary_command_admission(
        &mut self,
        hook: impl FnOnce(&beryl_home_store::HomeStore) + Send + 'static,
    ) {
        self.before_ordinary_command_admission = Some(Box::new(hook));
    }
    #[cfg(test)]
    pub(crate) fn test_before_next_ordinary_session_removal(
        &mut self,
        hook: impl FnOnce(&beryl_home_store::HomeStore) + Send + 'static,
    ) {
        self.before_ordinary_session_removal = Some(Box::new(hook));
    }

    #[cfg(test)]
    pub(crate) fn test_before_next_ordinary_draft_prepare(
        &mut self,
        hook: impl FnOnce(&beryl_home_store::HomeStore) + 'static,
    ) {
        self.before_ordinary_draft_prepare = Some(Box::new(hook));
    }
    #[cfg(test)]
    pub(crate) fn test_ordinary_command_status(
        &self,
    ) -> (usize, bool, bool, Option<beryl_home_store::HomeGeneration>) {
        (
            self.process.windows.shells().len(),
            self.exit_requested(),
            self.interrupted_exit.is_some(),
            self.process
                .services
                .as_ref()
                .and_then(|s| s.graph())
                .and_then(|g| g.home().health().generation()),
        )
    }
    pub(crate) fn mounted_owner(app: &App) -> Option<std::rc::Weak<RefCell<Self>>> {
        app.try_global::<MountedRunningOwner>()
            .map(|owner| owner.0.clone())
    }

    pub(crate) fn publish_created_window(
        owner: &Rc<RefCell<Self>>,
        shell: crate::main_window::MainWindowShell,
        app: &mut App,
    ) -> Result<(), crate::main_window::MainWindowShell> {
        let window = shell.window();
        let id = match window
            .read(app)
            .ok()
            .and_then(|root| root.controller())
            .map(|c| c.window_id())
        {
            Some(id) => id,
            None => return Err(shell),
        };
        let command = {
            let retained = owner.borrow();
            if !retained.ordinary_commands_mounted
                || retained.exit_requested()
                || retained.shutdown.is_some()
                || retained.process.commands.disabled_reason().is_some()
            {
                return Err(shell);
            }
            retained.process.commands.window_command(id)
        };
        let stop_worker = owner.borrow().status_stop_worker();
        if window
            .update(app, |root, window, cx| {
                root.mount_running_command(command, window, cx);
                if let Some(worker) = stop_worker {
                    root.mount_exact_status_worker(worker, window, cx);
                }
            })
            .is_err()
        {
            return Err(shell);
        }
        owner
            .borrow_mut()
            .process
            .windows
            .publish_created_window(shell, app)
    }

    pub(crate) fn mount_ordinary_commands(
        owner: &Rc<RefCell<Self>>,
        app: &mut App,
    ) -> Result<(), String> {
        let commands = {
            let retained = owner.borrow();
            if retained.ordinary_commands_mounted
                || retained.waiting_for_exit
                || retained
                    .process
                    .services
                    .as_ref()
                    .and_then(|s| s.graph())
                    .is_none()
            {
                return Err("ordinary command mounting is unavailable".into());
            }
            retained
                .process
                .commands
                .set_gate(RunningExitGate::Unavailable, true);
            retained
                .process
                .windows
                .shells()
                .iter()
                .map(|shell| {
                    let window = shell.window();
                    let id = window
                        .read(app)
                        .map_err(|e| e.to_string())?
                        .controller()
                        .ok_or("ordinary command shell controller is unavailable")?
                        .window_id();
                    Ok((window, retained.window_exit_command(id, app)?))
                })
                .collect::<Result<Vec<_>, String>>()?
        };
        if commands.is_empty() {
            return Err("ordinary commands require published windows".into());
        }
        let stop_worker = owner.borrow().status_stop_worker();
        for (window, command) in commands {
            window
                .update(app, |root, window, cx| {
                    root.mount_running_command(command, window, cx);
                    if let Some(worker) = &stop_worker {
                        root.mount_exact_status_worker(worker.clone(), window, cx);
                    }
                })
                .map_err(|e| e.to_string())?;
        }
        Self::arm_ordinary_commands(owner, app)?;
        owner.borrow_mut().ordinary_commands_mounted = true;
        app.set_global(MountedRunningOwner(Rc::downgrade(owner)));
        crate::main_window::MainWindowCreationOwner::bind_running_process(
            Rc::downgrade(owner),
            app,
        );
        owner
            .borrow()
            .process
            .commands
            .set_gate(RunningExitGate::Unavailable, false);
        Ok(())
    }

    fn arm_ordinary_commands(owner: &Rc<RefCell<Self>>, app: &mut App) -> Result<(), String> {
        Self::wait_for_exit_attempt(
            owner,
            ProjectionCancellationToken::new(),
            app,
            |owner, completion, app| {
                if matches!(completion, RunningExitCompletion::Attempt(_)) {
                    if let Err(error) = Self::arm_ordinary_commands(owner, app) {
                        owner
                            .borrow()
                            .process
                            .commands
                            .set_gate(RunningExitGate::Unavailable, true);
                        Self::report_ordinary_command_failure(owner, None, &error, app);
                    }
                }
            },
        )
    }

    pub(super) fn report_ordinary_command_failure(
        owner: &Rc<RefCell<Self>>,
        invoking: Option<beryl_model::WindowId>,
        error: &str,
        app: &mut App,
    ) {
        let windows = owner
            .borrow()
            .process
            .windows
            .shells()
            .iter()
            .map(|s| s.window())
            .collect::<Vec<_>>();
        for window in windows {
            let Ok(Some((id, ingress))) = window.update(app, |root, window, cx| {
                let id = root.controller().unwrap().window_id();
                if invoking.is_some_and(|invoking| invoking != id) {
                    return None;
                }
                Some((id, root.notice_ingress(window, cx)))
            }) else {
                continue;
            };
            let content = crate::main_window::NoticeContent::new(
                crate::main_window::NoticeVariant::Error,
                crate::main_window::NoticeDismissal::Dismissible,
                "Beryl couldn't close this window",
                error,
            );
            let _ = ingress.admit(
                crate::main_window::NoticeRecord {
                    window_id: id,
                    condition: crate::main_window::NoticeConditionId::new(),
                    revision: 1,
                    kind: crate::main_window::NoticeKind::Error,
                    content,
                },
                app,
            );
        }
    }
}
