use super::*;

#[test]
fn progress_delivery_errors_preserve_diagnostics_without_claiming_reopening() {
    use crate::{app_services::AppServiceCloseError, running_owner::ExitProgressError};
    for command_completed in [false, true] {
        for error in [
            ExitProgressError::Request("original request".into()),
            ExitProgressError::Intent,
            ExitProgressError::Scheduling("original scheduling refusal".into()),
            ExitProgressError::Unavailable,
            ExitProgressError::Service(AppServiceCloseError::NotReady),
        ] {
            let expected = error.to_string();
            let outcome = ExitAttemptOutcome {
                result: Err(ExitAttemptError::Progress(error)),
                command_completed,
            };
            let report = delivery_error(&outcome).unwrap();
            let content = failure_content(report);
            assert_eq!(content.detail().as_str(), expected);
            assert_eq!(content.commands().count(), 0);
            assert_eq!(outcome.command_completed, command_completed);
        }
    }
}

#[test]
fn coordinator_outcomes_are_not_delivery_errors() {
    use crate::{
        app_services::AppServiceShutdownProgress as Progress, cas_projection::ShutdownFailure,
        running_owner::ExitAttemptCompletion as Completion,
    };
    for result in [
        Completion::Cancelled,
        Completion::ConfirmedObservationCancelled,
        Completion::Progress(Progress::Waiting),
        Completion::Progress(Progress::Ready),
        Completion::Progress(Progress::Failed {
            reason: ShutdownFailure::Cancelled,
            reopened: true,
        }),
        Completion::Progress(Progress::Failed {
            reason: ShutdownFailure::SourceUnavailable,
            reopened: false,
        }),
    ] {
        let outcome = ExitAttemptOutcome {
            result: Ok(result),
            command_completed: false,
        };
        assert!(delivery_error(&outcome).is_none());
    }
}

#[test]
fn diagnostic_projection_is_commandless_and_bounded_at_unicode_edges() {
    for length in [
        0,
        1,
        NOTICE_DETAIL_BYTES - 1,
        NOTICE_DETAIL_BYTES,
        NOTICE_DETAIL_BYTES + 1,
    ] {
        for suffix in ["", "é", "🦀"] {
            let source = format!("{}{suffix}", "x".repeat(length));
            let content = failure_content(&source);
            assert_eq!(content.title().as_str(), "Couldn't exit Beryl");
            assert_eq!(content.variant, NoticeVariant::Error);
            assert_eq!(content.dismissal, NoticeDismissal::Dismissible);
            assert_eq!(content.commands().count(), 0);
            assert!(content.detail().as_str().len() <= NOTICE_DETAIL_BYTES);
            assert_eq!(
                content.detail().is_truncated(),
                source.len() > NOTICE_DETAIL_BYTES
            );
            if source.len() <= NOTICE_DETAIL_BYTES {
                assert_eq!(content.detail().as_str(), source);
            } else {
                assert!(content.detail().as_str().ends_with('…'));
            }
        }
    }
}

#[test]
fn diagnostic_formatting_stops_at_the_display_budget() {
    struct LongDiagnostic(std::cell::Cell<usize>);
    impl fmt::Display for LongDiagnostic {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            loop {
                self.0.set(self.0.get() + 1);
                f.write_str("🦀")?;
                assert!(self.0.get() <= NOTICE_DETAIL_BYTES, "unbounded formatting");
            }
        }
    }
    let diagnostic = LongDiagnostic(std::cell::Cell::new(0));
    let content = failure_content(&diagnostic);
    assert!(content.detail().is_truncated());
    assert_eq!(diagnostic.0.get(), NOTICE_DETAIL_BYTES / 4 + 1);
}
