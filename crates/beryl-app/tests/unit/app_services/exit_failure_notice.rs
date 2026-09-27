use super::*;

#[test]
fn progress_delivery_errors_preserve_diagnostics_without_claiming_reopening() {
    use crate::{app_services::AppServiceCloseError, running_owner::ExitProgressError};
    for command_completed in [false, true] {
        for error in [
            ExitProgressError::Request("original request".into()),
            ExitProgressError::Intent,
            ExitProgressError::Scheduling("original scheduling refusal".into()),
            ExitProgressError::Interaction("original interaction refusal".into()),
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
            reason: ShutdownFailure::Cancelled,
            reopened: false,
        }),
    ] {
        let outcome = ExitAttemptOutcome {
            result: Ok(result),
            command_completed: false,
        };
        assert!(delivery_error(&outcome).is_none());
        assert!(coordinator_failure(&outcome).is_none());
    }
}

#[test]
fn work_failure_routes_only_to_viewing_windows_or_original_invoker() {
    use beryl_model::SyndicTurnId;
    use syndic_storage::{CompactionOperationId, CompactionOperationNonce};
    let thread = SyndicThreadId::from_bytes([41; 16]);
    let other = SyndicThreadId::from_bytes([42; 16]);
    let invoking = WindowId::from_bytes([43; 16]);
    let viewing = WindowId::from_bytes([44; 16]);
    let second_view = WindowId::from_bytes([45; 16]);
    for reopened in [false, true] {
        for reason in [
            ShutdownFailure::UnprovenExecution {
                thread,
                turn: SyndicTurnId::from_bytes([46; 16]),
            },
            ShutdownFailure::UnprovenCompaction {
                operation: CompactionOperationId::new(
                    thread,
                    CompactionOperationNonce::from_bytes([47; 16]),
                ),
            },
        ] {
            let outcome = ExitAttemptOutcome {
                result: Ok(super::super::ExitAttemptCompletion::Progress(
                    AppServiceShutdownProgress::Failed { reason, reopened },
                )),
                command_completed: reopened,
            };
            let (target, original) = coordinator_failure(&outcome).unwrap();
            assert_eq!(target, Some(thread));
            assert_eq!(*original, reason);
            let windows = vec![
                (invoking, Some(other), 1),
                (viewing, Some(thread), 2),
                (second_view, Some(thread), 3),
            ];
            assert_eq!(
                failure_destinations(windows.clone(), target, Some(invoking)),
                vec![(viewing, 2), (second_view, 3)]
            );
            assert_eq!(
                failure_destinations(windows, target, None),
                vec![(viewing, 2), (second_view, 3)]
            );
            let unviewed = vec![(invoking, None, 1), (viewing, Some(other), 2)];
            assert_eq!(
                failure_destinations(unviewed.clone(), target, Some(invoking)),
                vec![(invoking, 1)]
            );
            assert!(failure_destinations(unviewed.clone(), target, Some(second_view)).is_empty());
            assert!(failure_destinations(unviewed, target, None).is_empty());
            assert!(failure_destinations::<()>(vec![], target, Some(invoking)).is_empty());
            assert_eq!(outcome.command_completed, reopened);
        }
    }
}

#[test]
fn unattributed_failures_route_only_to_the_original_surviving_invoker() {
    let invoking = WindowId::from_bytes([51; 16]);
    let other = WindowId::from_bytes([52; 16]);
    let missing = WindowId::from_bytes([53; 16]);
    let thread = SyndicThreadId::from_bytes([54; 16]);
    for reopened in [false, true] {
        for reason in [
            ShutdownFailure::SourceUnavailable,
            ShutdownFailure::StopFailed,
            ShutdownFailure::CleanupFailed,
        ] {
            let outcome = ExitAttemptOutcome {
                result: Ok(super::super::ExitAttemptCompletion::Progress(
                    AppServiceShutdownProgress::Failed { reason, reopened },
                )),
                command_completed: reopened,
            };
            let (target, original) = coordinator_failure(&outcome).unwrap();
            assert_eq!(target, None);
            assert_eq!(*original, reason);
            assert!(delivery_error(&outcome).is_none());
            let windows = vec![(invoking, Some(thread), 1), (other, None, 2)];
            assert_eq!(
                failure_destinations(windows.clone(), target, Some(invoking)),
                vec![(invoking, 1)]
            );
            assert!(failure_destinations(windows.clone(), target, Some(missing)).is_empty());
            assert!(failure_destinations(windows, target, None).is_empty());
            assert!(failure_destinations::<()>(vec![], target, Some(invoking)).is_empty());
            assert!(matches!(
                outcome.result,
                Ok(super::super::ExitAttemptCompletion::Progress(
                    AppServiceShutdownProgress::Failed { reason: retained, reopened: retained_reopening }
                )) if retained == reason && retained_reopening == reopened
            ));
            assert_eq!(outcome.command_completed, reopened);
        }
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
