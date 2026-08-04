use super::*;

#[test]
fn clean_normal_status_preserves_the_provider_outcome() {
    for (provider, durable) in [
        (
            NormalTurnTerminalStatus::Completed,
            TurnTerminalOutcome::Complete,
        ),
        (
            NormalTurnTerminalStatus::Failed,
            TurnTerminalOutcome::Failed,
        ),
        (
            NormalTurnTerminalStatus::Interrupted,
            TurnTerminalOutcome::Interrupted,
        ),
    ] {
        let status = terminal_status(provider, None);
        assert_eq!(status.outcome(), durable);
        assert_eq!(status.incomplete_reason(), None);
    }
}

#[test]
fn completion_mismatch_has_priority_and_only_narrative_mismatch_reacquires() {
    let issue = TerminalAudit {
        provider_issue: true,
        first_unsupported: Some(UnsupportedHistoryReason::UnsupportedRequiredPayload),
        unresolved_item: true,
        ..TerminalAudit::default()
    }
    .classify();
    assert_eq!(
        issue.incomplete_reason,
        Some(TurnIncompleteReason::CompletionMismatch)
    );
    assert!(!issue.same_native_reacquisition_required);

    let narrative = TerminalAudit {
        narrative_mismatch: true,
        first_unsupported: Some(UnsupportedHistoryReason::UnsupportedRequiredPayload),
        unresolved_item: true,
        ..TerminalAudit::default()
    }
    .classify();
    assert_eq!(
        narrative.incomplete_reason,
        Some(TurnIncompleteReason::CompletionMismatch)
    );
    assert!(narrative.same_native_reacquisition_required);
}

#[test]
fn unsupported_history_precedes_an_unresolved_item() {
    let unsupported = TerminalAudit {
        first_unsupported: Some(UnsupportedHistoryReason::ImpossibleLifecycle),
        unresolved_item: true,
        ..TerminalAudit::default()
    }
    .classify();
    assert_eq!(
        unsupported.incomplete_reason,
        Some(TurnIncompleteReason::UnsupportedHistory(
            UnsupportedHistoryReason::ImpossibleLifecycle,
        ))
    );
    assert!(!unsupported.same_native_reacquisition_required);

    let unresolved = TerminalAudit {
        unresolved_item: true,
        ..TerminalAudit::default()
    }
    .classify();
    assert_eq!(
        unresolved.incomplete_reason,
        Some(TurnIncompleteReason::ItemAuditFailed)
    );
    assert!(!unresolved.same_native_reacquisition_required);
}
