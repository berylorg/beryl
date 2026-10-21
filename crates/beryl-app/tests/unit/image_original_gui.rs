use super::*;

fn admitted(copy: Result<(), OriginalImageFailure>) -> OriginalImageCommands {
    let mut commands = OriginalImageCommands::new(Weak::new(), None);
    commands.failure = None;
    commands.facts = Some(OriginalImageAvailability { copy, save: true });
    commands
}

#[test]
fn unsupported_clipboard_representation_preserves_original_save_availability() {
    let commands = admitted(Err(OriginalImageFailure::Unsupported));
    assert_eq!(
        commands.reason(OriginalImageRequest::Copy),
        Some("The original has no supported clipboard representation.")
    );
    assert_eq!(commands.reason(OriginalImageRequest::Save), None);
}

#[test]
fn clipboard_capacity_refusal_preserves_original_save_availability() {
    let commands = admitted(Err(OriginalImageFailure::Capacity));
    assert_eq!(
        commands.reason(OriginalImageRequest::Copy),
        Some("The complete original exceeds the clipboard capacity.")
    );
    assert_eq!(commands.reason(OriginalImageRequest::Save), None);
}

#[test]
fn checking_and_unreadable_originals_disable_both_commands_with_distinct_reasons() {
    let mut commands = OriginalImageCommands::new(Weak::new(), None);
    commands.failure = None;
    for request in [OriginalImageRequest::Copy, OriginalImageRequest::Save] {
        assert_eq!(
            commands.reason(request),
            Some("Checking the exact original image.")
        );
    }
    commands.failure = Some(OriginalImageFailure::Unavailable);
    for request in [OriginalImageRequest::Copy, OriginalImageRequest::Save] {
        assert_eq!(
            commands.reason(request),
            Some("The original image is locally unavailable.")
        );
    }
}

#[test]
fn failed_external_effect_feedback_keeps_admitted_command_facts() {
    for operation in [OriginalImageRequest::Copy, OriginalImageRequest::Save] {
        for failure in [
            OriginalImageFailure::Clipboard,
            OriginalImageFailure::Destination,
            OriginalImageFailure::Write,
            OriginalImageFailure::Publish,
            OriginalImageFailure::Cleanup,
            OriginalImageFailure::Cancelled,
            OriginalImageFailure::Capacity,
            OriginalImageFailure::Unsupported,
        ] {
            let mut commands = admitted(Ok(()));
            commands.operation = operation;
            commands.complete(Err(failure));
            assert!(commands.facts.is_some(), "{operation:?}: {failure:?}");
            assert_eq!(commands.failure, None);
            assert_eq!(commands.reason(OriginalImageRequest::Copy), None);
            assert_eq!(commands.reason(OriginalImageRequest::Save), None);
        }
    }
    assert_eq!(
        outcome(
            OriginalImageRequest::Save,
            Err(OriginalImageFailure::Cleanup)
        ),
        "Save failed: temporary output cleanup failed."
    );
}

#[test]
fn terminal_source_failure_disables_both_commands_without_rechecking() {
    for operation in [OriginalImageRequest::Copy, OriginalImageRequest::Save] {
        for failure in [
            OriginalImageFailure::Unavailable,
            OriginalImageFailure::Integrity,
            OriginalImageFailure::Stale,
        ] {
            let mut commands = admitted(Ok(()));
            commands.operation = operation;
            commands.complete(Err(failure));
            assert_eq!(commands.facts, None);
            assert_eq!(commands.failure, Some(failure));
            for request in [OriginalImageRequest::Copy, OriginalImageRequest::Save] {
                assert_eq!(commands.reason(request), Some(failure_reason(failure)));
            }
            assert!(commands.active.is_none());
            assert!(commands.wait.is_none());
        }
    }
}
