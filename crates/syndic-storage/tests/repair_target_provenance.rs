use beryl_model::{CasThreadId, CasTurnId, InputGateRevision, SyndicTurnId};
use syndic_storage::{
    CasTurnSource, ConsumedRepairRequest, RepairCaptureGap, RepairCaptureGapReason,
    RepairProvenanceError, RepairRequestAttemptNonce, RepairRequestDisposition,
    RepairRequiredTarget, RepairSourceEventDigest, RepairSourceEventWitness, SourceEventSequence,
    TurnEndStatus, TurnIncompleteReason, TurnTerminalOutcome, UnsupportedHistoryReason,
};

fn witness(sequence: u64, byte: u8) -> RepairSourceEventWitness {
    RepairSourceEventWitness::new(
        SourceEventSequence::new(sequence).unwrap(),
        RepairSourceEventDigest::from_bytes([byte; 32]),
    )
}

fn source(thread: &str, turn: &str) -> CasTurnSource {
    CasTurnSource::new(
        CasThreadId::new(thread).unwrap(),
        CasTurnId::new(turn).unwrap(),
    )
}

fn gap() -> RepairCaptureGap {
    RepairCaptureGap::new(
        witness(3, 30),
        TurnEndStatus::new(
            TurnTerminalOutcome::Complete,
            Some(TurnIncompleteReason::CompletionMismatch),
        )
        .unwrap(),
        RepairCaptureGapReason::TerminalCaptureIncomplete,
        Some(witness(2, 20)),
    )
    .unwrap()
}

#[test]
fn target_preserves_exact_correlation_gap_and_consumed_request() {
    let request = RepairRequestDisposition::Consumed(
        ConsumedRepairRequest::new(
            RepairRequestAttemptNonce::from_bytes([255; 16]),
            InputGateRevision::new(6).unwrap(),
            InputGateRevision::new(7).unwrap(),
        )
        .unwrap(),
    );
    let turn = SyndicTurnId::from_bytes([8; 16]);
    let correlation = source("Case-Sensitive/Thread", "Exact:Turn");
    let target = RepairRequiredTarget::new(turn, correlation.clone(), gap(), request);
    assert_eq!(target.turn_id(), turn);
    assert_eq!(target.source(), &correlation);
    assert_eq!(
        target.source().thread_id().as_str(),
        "Case-Sensitive/Thread"
    );
    assert_eq!(target.source().turn_id().as_str(), "Exact:Turn");
    assert_eq!(target.gap(), gap());
    assert_eq!(target.request(), request);
    assert_eq!(target.clone(), target);
    assert_eq!(target.gap().terminal(), witness(3, 30));
    assert_eq!(target.gap().issue(), Some(witness(2, 20)));
    assert_eq!(
        target.gap().status().outcome(),
        TurnTerminalOutcome::Complete
    );
    assert_eq!(
        target.gap().reason(),
        RepairCaptureGapReason::TerminalCaptureIncomplete,
    );
}

#[test]
fn witness_retains_all_digest_bytes_and_full_sequence_domain() {
    let bytes = std::array::from_fn(|index| (index * 7) as u8);
    let digest = RepairSourceEventDigest::from_bytes(bytes);
    let maximum =
        RepairSourceEventWitness::new(SourceEventSequence::new(u64::MAX).unwrap(), digest);
    assert_eq!(maximum.sequence().get(), u64::MAX);
    assert_eq!(maximum.digest().as_bytes(), &bytes);
    assert_ne!(maximum, witness(u64::MAX, 0));
    assert_ne!(witness(1, 0), witness(2, 0));
    assert_ne!(witness(1, 0), witness(1, 255));
    assert_eq!(witness(1, 0).digest().as_bytes(), &[0; 32]);
    assert_eq!(witness(1, 255).digest().as_bytes(), &[255; 32]);
    assert!(SourceEventSequence::new(0).is_err());
}

#[test]
fn correlation_accepts_exact_byte_limit_and_rejects_invalid_external_ids() {
    let text = "é".repeat(128);
    let target = RepairRequiredTarget::new(
        SyndicTurnId::from_bytes([1; 16]),
        source(&text, &text),
        gap(),
        RepairRequestDisposition::Available,
    );
    assert_eq!(target.source().thread_id().as_str(), text);
    assert_eq!(target.source().turn_id().as_str(), text);
    for invalid in [
        String::new(),
        format!("{text}x"),
        " padded".into(),
        "a\0b".into(),
    ] {
        assert!(CasThreadId::new(&invalid).is_err());
        assert!(CasTurnId::new(&invalid).is_err());
    }
}

#[test]
fn unknown_terminal_cannot_be_repaired_even_with_gap_evidence() {
    let status = TurnEndStatus::new(
        TurnTerminalOutcome::UnknownTerminal,
        Some(TurnIncompleteReason::StreamLost),
    )
    .unwrap();
    for reason in [
        RepairCaptureGapReason::TerminalCaptureIncomplete,
        RepairCaptureGapReason::ForcedAbortOrderingUnproven,
        RepairCaptureGapReason::ProviderObservationIssue,
    ] {
        assert_eq!(
            RepairCaptureGap::new(witness(2, 2), status, reason, Some(witness(1, 1))),
            Err(RepairProvenanceError::UnknownTerminal),
        );
    }
}

#[test]
fn capture_gap_requires_and_preserves_closed_terminal_reason() {
    assert_eq!(
        RepairCaptureGap::new(
            witness(1, 1),
            TurnEndStatus::complete(),
            RepairCaptureGapReason::TerminalCaptureIncomplete,
            None,
        ),
        Err(RepairProvenanceError::MissingIncompleteReason),
    );
    for reason in [
        TurnIncompleteReason::StreamLost,
        TurnIncompleteReason::AuthorityLost,
        TurnIncompleteReason::WorkerStopped,
        TurnIncompleteReason::CompletionMismatch,
        TurnIncompleteReason::ItemAuditFailed,
        TurnIncompleteReason::UnsupportedHistory(UnsupportedHistoryReason::HostedImageGeneration),
    ] {
        let status = TurnEndStatus::incomplete(reason);
        let gap = RepairCaptureGap::new(
            witness(1, 1),
            status,
            RepairCaptureGapReason::TerminalCaptureIncomplete,
            None,
        )
        .unwrap();
        assert_eq!(gap.status(), status);
        assert_eq!(gap.status().incomplete_reason(), Some(reason));
        assert_eq!(gap.issue(), None);
    }
}

#[test]
fn forced_abort_gap_requires_interrupted_terminal() {
    for outcome in [
        TurnTerminalOutcome::Complete,
        TurnTerminalOutcome::Failed,
        TurnTerminalOutcome::Incomplete,
        TurnTerminalOutcome::Interrupted,
    ] {
        let status =
            TurnEndStatus::new(outcome, Some(TurnIncompleteReason::ItemAuditFailed)).unwrap();
        let result = RepairCaptureGap::new(
            witness(1, 0),
            status,
            RepairCaptureGapReason::ForcedAbortOrderingUnproven,
            None,
        );
        if outcome == TurnTerminalOutcome::Interrupted {
            assert_eq!(result.unwrap().status(), status);
        } else {
            assert_eq!(
                result,
                Err(RepairProvenanceError::RequiresInterruptedTerminal)
            );
        }
    }
}

#[test]
fn observation_gap_requires_a_strictly_preceding_issue() {
    assert_eq!(
        RepairCaptureGap::new(
            witness(2, 2),
            TurnEndStatus::complete(),
            RepairCaptureGapReason::ProviderObservationIssue,
            None,
        ),
        Err(RepairProvenanceError::MissingObservationIssue),
    );
    for (terminal, issue) in [(1, 1), (1, 2), (u64::MAX, u64::MAX)] {
        for reason in [
            RepairCaptureGapReason::ProviderObservationIssue,
            RepairCaptureGapReason::TerminalCaptureIncomplete,
        ] {
            assert_eq!(
                RepairCaptureGap::new(
                    witness(terminal, 0),
                    gap().status(),
                    reason,
                    Some(witness(issue, 1)),
                ),
                Err(RepairProvenanceError::InvalidIssueOrder),
            );
        }
    }
    let issue = witness(u64::MAX - 1, 255);
    let gap = RepairCaptureGap::new(
        witness(u64::MAX, 0),
        TurnEndStatus::complete(),
        RepairCaptureGapReason::ProviderObservationIssue,
        Some(issue),
    )
    .unwrap();
    assert_eq!(gap.issue(), Some(issue));
}

#[test]
fn target_equality_does_not_replace_identity_with_a_shared_digest() {
    let request = RepairRequestDisposition::Available;
    let turn = SyndicTurnId::from_bytes([1; 16]);
    let original = RepairRequiredTarget::new(turn, source("thread", "turn"), gap(), request);
    for other in [
        RepairRequiredTarget::new(
            SyndicTurnId::from_bytes([2; 16]),
            source("thread", "turn"),
            gap(),
            request,
        ),
        RepairRequiredTarget::new(turn, source("other-thread", "turn"), gap(), request),
        RepairRequiredTarget::new(turn, source("thread", "other-turn"), gap(), request),
    ] {
        assert_eq!(
            original.gap().terminal().digest(),
            other.gap().terminal().digest()
        );
        assert_ne!(original, other);
    }
}
