use super::RunningProcessOwner;
use gpui::App;
use std::{cell::RefCell, rc::Rc, time::Duration};

pub(super) struct ObservedHomeFailure {
    pub(super) generation: beryl_home_store::HomeGeneration,
    pub(super) condition: crate::main_window::NoticeConditionId,
    pub(super) focus: Vec<(
        gpui::WindowHandle<crate::main_window::MainWindowShellRoot>,
        Option<gpui::FocusHandle>,
    )>,
}

impl RunningProcessOwner {
    fn project_exit_availability(owner: &Rc<RefCell<Self>>, app: &mut App) {
        Self::project_observed_home_failure(owner, app);
        Self::observe_running_home_failure(owner, app);
        Self::project_running_home_recovery_notices(owner, app);
        let (reason, windows) = {
            let owner = owner.borrow();
            (
                owner.process.commands.disabled_reason(),
                owner
                    .process
                    .windows
                    .shells()
                    .iter()
                    .map(|shell| shell.window())
                    .collect::<Vec<_>>(),
            )
        };
        for window in windows {
            let _ = window.update(app, |root, _, cx| {
                root.set_exit_disabled_reason(reason, cx);
            });
        }
    }

    fn project_observed_home_failure(owner: &Rc<RefCell<Self>>, app: &mut App) {
        let observed = {
            let retained = owner.borrow();
            let Some(graph) = retained
                .process
                .services
                .as_ref()
                .and_then(|services| services.graph())
            else {
                return;
            };
            let health = graph.home().health();
            if health.state() != beryl_home_store::HomeHealthState::Failed {
                return;
            }
            let Some(generation) = health.generation() else {
                return;
            };
            let windows = retained
                .process
                .windows
                .shells()
                .iter()
                .map(|shell| shell.window())
                .collect::<Vec<_>>();
            (generation, windows)
        };
        let (condition, owned) = {
            let mut retained = owner.borrow_mut();
            if retained
                .observed_home_failure
                .as_ref()
                .is_none_or(|failure| failure.generation != observed.0)
            {
                retained.observed_home_failure = Some(ObservedHomeFailure {
                    generation: observed.0,
                    condition: crate::main_window::NoticeConditionId::new(),
                    focus: Vec::new(),
                });
            }
            let failure = retained.observed_home_failure.as_mut().unwrap();
            failure
                .focus
                .retain(|(window, _)| observed.1.contains(window));
            for handle in &observed.1 {
                if !failure.focus.iter().any(|(window, _)| window == handle) {
                    let focus = handle
                        .update(app, |root, window, cx| {
                            root.capture_running_recovery_focus(window, cx)
                        })
                        .ok()
                        .flatten();
                    failure.focus.push((*handle, focus));
                }
            }
            let condition = failure.condition.clone();
            (
                condition,
                retained.ordinary_recovery_observed_generation() == Some(observed.0),
            )
        };
        if owned {
            return;
        }
        for handle in observed.1 {
            let _ = handle.update(app, |root, window, cx| {
                root.project_running_home_recovery_notice(
                    &condition,
                    crate::main_window::MainWindowHomeRecoveryNoticeState::Failed,
                    window,
                    cx,
                );
            });
        }
    }

    pub(super) fn observe_exit_availability(owner: &Rc<RefCell<Self>>, app: &mut App) {
        Self::project_exit_availability(owner, app);
        let weak = Rc::downgrade(owner);
        let task = app.spawn(async move |cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                if !cx
                    .update(|app| {
                        let Some(owner) = weak.upgrade() else {
                            return false;
                        };
                        Self::project_exit_availability(&owner, app);
                        true
                    })
                    .unwrap_or(false)
                {
                    break;
                }
            }
        });
        owner.borrow_mut().exit_availability = Some(task);
    }
}
