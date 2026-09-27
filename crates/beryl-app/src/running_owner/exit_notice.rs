use super::{ExitAttemptError, RunningProcessOwner, exit_attempt::ExitAttemptOutcome};
use crate::{app_services::AppServiceShutdownProgress, cas_projection::ShutdownFailure};
use crate::{main_window::*, startup_owner::RunningExitRequest};
use beryl_model::{SyndicThreadId, WindowId};
use gpui::App;
use std::{cell::RefCell, fmt, rc::Rc};

impl RunningProcessOwner {
    pub(super) fn report_exit_failure(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        outcome: &ExitAttemptOutcome,
        app: &mut App,
    ) {
        if matches!(&outcome.result, Err(ExitAttemptError::SessionPublication(_))) {
            owner.borrow_mut().retain_reported_exit_failure(request);
        }
        let (thread, content) = if let Some(error) = delivery_error(outcome) {
            (None, failure_content(error))
        } else if let Some((thread, reason)) = coordinator_failure(outcome) {
            let step = match reason {
                ShutdownFailure::SourceUnavailable => "Shutdown work observation is unavailable",
                ShutdownFailure::StopFailed => "Shutdown could not stop work",
                ShutdownFailure::CleanupFailed => "Shutdown could not complete work cleanup",
                ShutdownFailure::UnprovenExecution { .. }
                | ShutdownFailure::UnprovenCompaction { .. } => {
                    "Shutdown could not prove that work settled"
                }
                ShutdownFailure::Cancelled => unreachable!(),
            };
            (thread, failure_content(&format_args!("{step}: {reason:?}")))
        } else {
            return;
        };
        let windows = {
            let owner = owner.borrow();
            owner
                .process
                .windows
                .shells()
                .iter()
                .filter_map(|shell| {
                    let window = shell.window();
                    let controller = window.read(app).ok()?.controller()?;
                    let selected = controller
                        .composer_mount()
                        .and_then(|mount| mount.read(app).contribution())
                        .map(|composer| {
                            composer.read(app).selection_identity().claim().thread_id()
                        });
                    Some((controller.window_id(), selected, window))
                })
                .collect()
        };
        for (destination, window) in
            failure_destinations(windows, thread, request.invoking_window())
        {
            let Ok(ingress) =
                window.update(app, |root, window, cx| root.notice_ingress(window, cx))
            else {
                continue;
            };
            let _ = ingress.admit(
                NoticeRecord {
                    window_id: destination,
                    condition: NoticeConditionId::new(),
                    revision: 1,
                    kind: NoticeKind::Error,
                    content: content.clone(),
                },
                app,
            );
        }
    }

    #[cfg(test)]
    pub(crate) fn test_report_exit_delivery_failure(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        error: ExitAttemptError,
        command_completed: bool,
        app: &mut App,
    ) {
        let outcome = ExitAttemptOutcome {
            result: Err(error),
            command_completed,
        };
        Self::report_exit_failure(owner, request, &outcome, app);
    }

    #[cfg(test)]
    pub(crate) fn test_report_exit_work_failure(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        reason: ShutdownFailure,
        reopened: bool,
        app: &mut App,
    ) {
        let outcome = ExitAttemptOutcome {
            result: Ok(super::ExitAttemptCompletion::Progress(
                AppServiceShutdownProgress::Failed { reason, reopened },
            )),
            command_completed: false,
        };
        Self::report_exit_failure(owner, request, &outcome, app);
    }
}

fn coordinator_failure(
    outcome: &ExitAttemptOutcome,
) -> Option<(Option<SyndicThreadId>, &ShutdownFailure)> {
    let Ok(super::ExitAttemptCompletion::Progress(AppServiceShutdownProgress::Failed {
        reason,
        ..
    })) = &outcome.result
    else {
        return None;
    };
    let thread = match reason {
        ShutdownFailure::UnprovenExecution { thread, .. } => Some(*thread),
        ShutdownFailure::UnprovenCompaction { operation } => Some(operation.thread_id()),
        ShutdownFailure::SourceUnavailable
        | ShutdownFailure::StopFailed
        | ShutdownFailure::CleanupFailed => None,
        ShutdownFailure::Cancelled => return None,
    };
    Some((thread, reason))
}

fn failure_destinations<T>(
    windows: Vec<(WindowId, Option<SyndicThreadId>, T)>,
    thread: Option<SyndicThreadId>,
    invoking: Option<WindowId>,
) -> Vec<(WindowId, T)> {
    let viewed = thread.is_some() && windows.iter().any(|(_, selected, _)| *selected == thread);
    windows
        .into_iter()
        .filter_map(|(id, selected, window)| {
            let affected = if viewed {
                selected == thread
            } else {
                Some(id) == invoking
            };
            affected.then_some((id, window))
        })
        .collect()
}

fn delivery_error(outcome: &ExitAttemptOutcome) -> Option<&ExitAttemptError> {
    outcome.result.as_ref().err()
}

fn failure_content(error: &impl fmt::Display) -> NoticeContent {
    struct Detail(String);
    impl fmt::Write for Detail {
        fn write_str(&mut self, value: &str) -> fmt::Result {
            if self.0.len() > NOTICE_DETAIL_BYTES {
                return Err(fmt::Error);
            }
            let remaining = NOTICE_DETAIL_BYTES - self.0.len();
            if value.len() <= remaining {
                self.0.push_str(value);
                return Ok(());
            }
            let mut end = remaining;
            while !value.is_char_boundary(end) {
                end -= 1;
            }
            self.0.push_str(&value[..end]);
            // Force the notice's canonical truncation, including its explicit truncation flag.
            self.0.push_str("…...");
            Err(fmt::Error)
        }
    }
    let mut detail = Detail(String::with_capacity(NOTICE_DETAIL_BYTES + 6));
    let _ = fmt::write(&mut detail, format_args!("{error}"));
    NoticeContent::new(
        NoticeVariant::Error,
        NoticeDismissal::Dismissible,
        "Couldn't exit Beryl",
        &detail.0,
    )
}

#[cfg(test)]
mod tests {
    include!("../../tests/unit/app_services/exit_failure_notice.rs");
}
