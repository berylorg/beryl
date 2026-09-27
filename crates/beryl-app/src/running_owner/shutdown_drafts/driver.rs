use super::*;
use std::time::Duration;

#[derive(Clone, Copy)]
pub(crate) enum RunningShutdownDraftAction {
    Prepare,
    Release,
}

impl RunningProcessOwner {
    pub(crate) fn drive_shutdown_drafts(
        owner: &Rc<RefCell<Self>>,
        action: RunningShutdownDraftAction,
        app: &mut App,
        completed: impl FnOnce(
            &Rc<RefCell<Self>>,
            Result<RunningShutdownDraftProgress, String>,
            &mut App,
        ) + 'static,
    ) -> Result<(), String> {
        Self::drive_shutdown_drafts_with(owner, action, app, completed, move |owner, app| {
            Self::poll_driven_shutdown_drafts(owner, action, app)
        })
    }

    fn poll_driven_shutdown_drafts(
        owner: &Rc<RefCell<Self>>,
        action: RunningShutdownDraftAction,
        app: &mut App,
    ) -> Result<RunningShutdownDraftProgress, String> {
        match action {
            RunningShutdownDraftAction::Prepare => {
                Self::advance_shutdown_drafts_inner(owner, app, true)
            }
            RunningShutdownDraftAction::Release => {
                Self::release_shutdown_drafts_inner(owner, app, true)
            }
        }
    }

    fn drive_shutdown_drafts_with(
        owner: &Rc<RefCell<Self>>,
        action: RunningShutdownDraftAction,
        app: &mut App,
        completed: impl FnOnce(
            &Rc<RefCell<Self>>,
            Result<RunningShutdownDraftProgress, String>,
            &mut App,
        ) + 'static,
        mut poll: impl FnMut(
            &Rc<RefCell<Self>>,
            &mut App,
        ) -> Result<RunningShutdownDraftProgress, String>
        + 'static,
    ) -> Result<(), String> {
        let drafts =
            Self::shutdown_drafts(owner, matches!(action, RunningShutdownDraftAction::Prepare))?;
        {
            let mut state = drafts
                .try_borrow_mut()
                .map_err(|_| "shutdown drafts are busy")?;
            if state.driving {
                return Err("shutdown draft driver is already active".into());
            }
            if state.releasing && matches!(action, RunningShutdownDraftAction::Prepare) {
                return Err("shutdown draft recovery has already started".into());
            }
            state.driving = true;
        }
        let owner = owner.clone();
        app.spawn(async move |cx| {
            loop {
                let result = cx.update(|app| {
                    let current = Self::shutdown_drafts(&owner, false)?;
                    if !Rc::ptr_eq(&current, &drafts) {
                        return Err("shutdown draft attempt changed".into());
                    }
                    poll(&owner, app)
                });
                let Ok(result) = result else { return };
                if matches!(result, Ok(RunningShutdownDraftProgress::Pending)) {
                    cx.background_executor()
                        .timer(Duration::from_millis(50))
                        .await;
                    continue;
                }
                drafts.borrow_mut().driving = false;
                let _ = cx.update(|app| completed(&owner, result, app));
                return;
            }
        })
        .detach();
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn test_drive_delayed_shutdown_drafts(
        owner: &Rc<RefCell<Self>>,
        app: &mut App,
        polls: Rc<std::cell::Cell<usize>>,
        completed: impl FnOnce(
            &Rc<RefCell<Self>>,
            Result<RunningShutdownDraftProgress, String>,
            &mut App,
        ) + 'static,
    ) -> Result<(), String> {
        Self::drive_shutdown_drafts_with(
            owner,
            RunningShutdownDraftAction::Prepare,
            app,
            completed,
            move |owner, app| {
                let pass = polls.get();
                polls.set(pass + 1);
                if pass < 2 {
                    Ok(RunningShutdownDraftProgress::Pending)
                } else {
                    Self::poll_driven_shutdown_drafts(
                        owner,
                        RunningShutdownDraftAction::Prepare,
                        app,
                    )
                }
            },
        )
    }
}
