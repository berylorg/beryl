use super::{ExitAttemptError, RunningProcessOwner, exit_attempt::ExitAttemptOutcome};
use crate::{main_window::*, startup_owner::RunningExitRequest};
use gpui::App;
use std::{cell::RefCell, fmt, rc::Rc};

impl RunningProcessOwner {
    pub(super) fn report_exit_delivery_failure(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        outcome: &ExitAttemptOutcome,
        app: &mut App,
    ) {
        let Some(error) = delivery_error(outcome) else {
            return;
        };
        let Some(invoking) = request.invoking_window() else {
            return;
        };
        let window = {
            let owner = owner.borrow();
            owner.process.windows.shells().iter().find_map(|shell| {
                let window = shell.window();
                window
                    .read(app)
                    .ok()
                    .and_then(|root| root.controller())
                    .is_some_and(|controller| controller.window_id() == invoking)
                    .then_some(window)
            })
        };
        let Some(window) = window else { return };
        let Ok(ingress) = window.update(app, |root, window, cx| root.notice_ingress(window, cx))
        else {
            return;
        };
        let _ = ingress.admit(
            NoticeRecord {
                window_id: invoking,
                condition: NoticeConditionId::new(),
                revision: 1,
                kind: NoticeKind::Error,
                content: failure_content(error),
            },
            app,
        );
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
        Self::report_exit_delivery_failure(owner, request, &outcome, app);
    }
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
