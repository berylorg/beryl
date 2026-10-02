use super::*;
use crate::{
    cas_projection::ProjectionCancellationToken,
    main_window::{
        NoticeConditionId, NoticeContent, NoticeDismissal, NoticeKind, NoticeRecord, NoticeVariant,
    },
    startup_owner::RunningExitRequest,
};
use beryl_model::WindowId;
use gpui::AsyncApp;
use std::time::Duration;

mod quit;

#[derive(Debug)]
pub(crate) enum RunningExitCompletion {
    Attempt(exit_attempt::ExitAttemptOutcome),
    Finished,
    Blocked,
}

pub(super) enum FinalTeardownStatus {
    Running,
    Blocked(String),
    Finished,
}

pub(super) struct FinalTeardown {
    identity: Rc<()>,
    request: RunningExitRequest,
    status: FinalTeardownStatus,
    condition: NoticeConditionId,
    quit: Option<quit::BlockedQuitConfirmation>,
    termination_admitted: bool,
    #[cfg(test)]
    termination: Option<Box<dyn FnOnce()>>,
    #[cfg(test)]
    native_failure: bool,
    #[cfg(test)]
    native_receipt_gate: Option<futures_channel::oneshot::Receiver<()>>,
}

impl RunningProcessOwner {
    pub(crate) fn wait_for_exit_attempt(
        owner: &Rc<RefCell<Self>>,
        cancellation: ProjectionCancellationToken,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, RunningExitCompletion, &mut App) + 'static,
    ) -> Result<(), String> {
        if owner.borrow().final_teardown.is_some() {
            return Err("final teardown already owns the running Exit".into());
        }
        Self::wait_for_exit_result(
            owner,
            cancellation,
            app,
            move |owner, request, outcome, app| {
                if !matches!(outcome.result, Ok(ExitAttemptCompletion::SessionReady)) {
                    completed(owner, RunningExitCompletion::Attempt(outcome), app);
                    return;
                }
                let delivery = Rc::new(RefCell::new(Some(completed)));
                let settled = delivery.clone();
                if let Err((request, error)) =
                    Self::finish_ready_exit(owner, request, app, move |owner, result, app| {
                        let completed = settled.borrow_mut().take().unwrap();
                        completed(owner, result, app);
                    })
                {
                    let outcome = exit_attempt::ExitAttemptOutcome {
                        result: Err(ExitAttemptError::SessionPublication(error)),
                        command_completed: false,
                    };
                    Self::report_exit_failure(owner, &request, &outcome, app);
                    let completed = delivery.borrow_mut().take().unwrap();
                    completed(owner, RunningExitCompletion::Attempt(outcome), app);
                }
            },
        )
    }

    pub(crate) fn finish_ready_exit(
        owner: &Rc<RefCell<Self>>,
        mut request: RunningExitRequest,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, RunningExitCompletion, &mut App) + 'static,
    ) -> Result<(), (RunningExitRequest, String)> {
        let ready = (|| {
            let owner = owner.borrow();
            if owner.final_teardown.is_some()
                || owner.confirmation.is_some()
                || owner.progress.is_some()
                || owner.observing_initial_work
            {
                return Err("final teardown admission is unavailable".to_owned());
            }
            let invoking = owner.resolve_exit_window(&mut request, app)?;
            if owner.shutdown_status()
                != Some((
                    invoking,
                    ShutdownIntent::ApplicationExit,
                    RunningShutdownStatus::WorkReady,
                ))
            {
                return Err("final teardown requires the original work-ready Exit".into());
            }
            owner.require_shutdown_session_ready()?;
            let attempt = owner.shutdown.as_ref().unwrap();
            attempt
                .lease
                .validate()
                .map_err(|error| error.to_string())?;
            if !attempt
                .drafts
                .as_ref()
                .is_some_and(|drafts| drafts.borrow().ready())
            {
                return Err("final teardown requires prepared resident drafts".into());
            }
            attempt
                .drafts
                .as_ref()
                .unwrap()
                .borrow_mut()
                .install_detached_sources(app)?;
            Ok(())
        })();
        if let Err(error) = ready {
            return Err((request, error));
        }
        let identity = Rc::new(());
        {
            let mut owner = owner.borrow_mut();
            owner.exit_availability.take();
            owner.final_teardown = Some(FinalTeardown {
                identity: identity.clone(),
                request,
                status: FinalTeardownStatus::Running,
                condition: NoticeConditionId::new(),
                quit: None,
                termination_admitted: false,
                #[cfg(test)]
                termination: None,
                #[cfg(test)]
                native_failure: false,
                #[cfg(test)]
                native_receipt_gate: None,
            });
        }
        let retained = owner.clone();
        app.spawn(async move |cx| {
            let result = Self::complete_final_teardown(&retained, cx).await;
            cx.update(|app| {
                let current = retained
                    .borrow()
                    .final_teardown
                    .as_ref()
                    .is_some_and(|attempt| Rc::ptr_eq(&attempt.identity, &identity));
                if !current {
                    return;
                }
                let outcome = match result {
                    Ok(()) => {
                        retained
                            .borrow_mut()
                            .final_teardown
                            .as_mut()
                            .unwrap()
                            .status = FinalTeardownStatus::Finished;
                        RunningExitCompletion::Finished
                    }
                    Err(error) => {
                        Self::block_final_teardown(&retained, error, app);
                        RunningExitCompletion::Blocked
                    }
                };
                let quit = matches!(outcome, RunningExitCompletion::Finished);
                completed(&retained, outcome, app);
                if quit {
                    app.quit();
                }
            })
            .expect("final teardown retains its GUI completion executor");
        })
        .detach();
        Ok(())
    }

    async fn complete_final_teardown(
        owner: &Rc<RefCell<Self>>,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let drafts = owner
            .borrow()
            .shutdown
            .as_ref()
            .unwrap()
            .drafts
            .as_ref()
            .unwrap()
            .clone();
        loop {
            let ready = cx
                .update(|app| drafts.borrow_mut().retire_final_residents(app))
                .map_err(|error| error.to_string())??;
            if ready {
                break;
            }
            cx.background_executor()
                .timer(Duration::from_millis(10))
                .await;
        }
        let mut services = owner
            .borrow_mut()
            .process
            .services
            .take()
            .expect("admitted final teardown owns returned services");
        let (returned, result) = cx
            .background_executor()
            .spawn(async move {
                let result = services.finish_shutdown();
                (services, result)
            })
            .await;
        owner.borrow_mut().process.services = Some(returned);
        result.map_err(|error| format!("Final service cleanup failed: {error}"))?;

        loop {
            match &owner.borrow().startup_cleanup {
                StartupCleanup::Settled => break,
                StartupCleanup::Failed(error) => {
                    return Err(format!("Auxiliary cleanup failed: {error}"));
                }
                StartupCleanup::Pending => {}
            }
            cx.background_executor()
                .timer(Duration::from_millis(10))
                .await;
        }
        let count = owner.borrow().process.windows.shells().len();
        for index in 0..count {
            loop {
                let drained = cx
                    .update(|app| {
                        owner.borrow().process.windows.shells()[index]
                            .window()
                            .update(app, |root, _, cx| root.drain_detached_shutdown_reads(cx))
                            .map_err(|error| error.to_string())
                    })
                    .map_err(|error| error.to_string())??;
                if drained {
                    break;
                }
                cx.background_executor()
                    .timer(Duration::from_millis(10))
                    .await;
            }
            #[cfg(test)]
            if owner
                .borrow()
                .final_teardown
                .as_ref()
                .unwrap()
                .native_failure
            {
                return Err("injected published native cleanup failure after read drain".into());
            }
            let receipt = cx
                .update(|app| {
                    owner.borrow_mut().process.windows.shells_mut()[index]
                        .begin_final_native_cleanup(app)
                })
                .map_err(|error| error.to_string())??;
            receipt
                .await
                .map_err(|error| format!("Published native destruction failed: {error}"))?;
            #[cfg(test)]
            {
                let gate = owner
                    .borrow_mut()
                    .final_teardown
                    .as_mut()
                    .unwrap()
                    .native_receipt_gate
                    .take();
                if let Some(gate) = gate {
                    gate.await.map_err(|_| {
                        "native destruction receipt delivery was abandoned".to_owned()
                    })?;
                }
            }
            cx.update(|app| {
                owner.borrow_mut().process.windows.shells_mut()[index]
                    .settle_final_native_cleanup(app)
            })
            .map_err(|error| error.to_string())??;
        }
        cx.update(|app| {
            let mut owner = owner.borrow_mut();
            owner
                .process
                .appearance
                .update(app, |appearance, _| appearance.retire());
            owner.process.windows.release_retired_shells();
            Ok::<(), String>(())
        })
        .map_err(|error| error.to_string())??;
        Ok(())
    }

    fn block_final_teardown(owner: &Rc<RefCell<Self>>, error: String, app: &mut App) {
        let detail = bounded_detail(&error);
        let (condition, windows) = {
            let mut owner = owner.borrow_mut();
            let attempt = owner.final_teardown.as_mut().unwrap();
            attempt.status = FinalTeardownStatus::Blocked(detail.clone());
            (
                attempt.condition.clone(),
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
            let result = window.update(app, |root, window, cx| {
                root.resume_detached_shutdown_reads(window, cx);
                root.mount_blocked_shutdown(Rc::downgrade(owner), cx);
                (
                    root.controller().unwrap().window_id(),
                    root.notice_ingress(window, cx),
                )
            });
            if let Ok((window_id, ingress)) = result {
                let _ = ingress.admit(
                    NoticeRecord {
                        window_id,
                        condition: condition.clone(),
                        revision: 1,
                        kind: NoticeKind::Error,
                        content: NoticeContent::new(
                            NoticeVariant::Error,
                            NoticeDismissal::Dismissible,
                            "Beryl couldn't finish shutting down",
                            &detail,
                        ),
                    },
                    app,
                );
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn test_final_teardown_detail(&self) -> Option<&str> {
        match &self.final_teardown.as_ref()?.status {
            FinalTeardownStatus::Blocked(detail) => Some(detail),
            _ => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn test_fail_final_native_cleanup(&mut self) {
        self.final_teardown.as_mut().unwrap().native_failure = true;
    }

    #[cfg(test)]
    pub(crate) fn test_defer_final_native_receipt(
        &mut self,
    ) -> futures_channel::oneshot::Sender<()> {
        let (sender, receiver) = futures_channel::oneshot::channel();
        self.final_teardown.as_mut().unwrap().native_receipt_gate = Some(receiver);
        sender
    }

    #[cfg(test)]
    pub(crate) fn test_final_exit_identity(&self) -> Option<Rc<()>> {
        self.final_teardown
            .as_ref()
            .map(|attempt| attempt.request.identity())
    }
}

fn bounded_detail(detail: &str) -> String {
    let mut end = detail.len().min(crate::notice_limits::NOTICE_DETAIL_BYTES);
    while !detail.is_char_boundary(end) {
        end -= 1;
    }
    detail[..end].to_owned()
}
