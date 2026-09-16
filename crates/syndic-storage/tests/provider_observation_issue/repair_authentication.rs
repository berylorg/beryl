use super::*;
use syndic_storage::test_faults::repair_source_events_match_for_test as authenticate;

fn events(fixture: &Fixture) -> (SourceEventRecord, SourceEventRecord) {
    let issue = inspect_agent_start(fixture, 71)
        .into_issue(ProviderObservationIssueReason::DuplicateItemStart);
    let issue = SourceEventRecord::new(
        fixture.turn,
        SourceEventSequence::new(3).unwrap(),
        Some(fixture.source.clone()),
        SourceEventPayload::ProviderObservationIssue(Box::new(issue)),
    )
    .unwrap();
    let terminal = SourceEventRecord::new(
        fixture.turn,
        SourceEventSequence::new(4).unwrap(),
        Some(fixture.source.clone()),
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(
                TurnTerminalOutcome::Interrupted,
                Some(TurnIncompleteReason::CompletionMismatch),
            )
            .unwrap(),
        ),
    )
    .unwrap();
    (terminal, issue)
}

fn target(
    terminal: &SourceEventRecord,
    issue: &SourceEventRecord,
    reason: RepairCaptureGapReason,
) -> RepairRequiredTarget {
    let SourceEventPayload::TurnEnded(status) = terminal.payload() else {
        unreachable!()
    };
    RepairRequiredTarget::new(
        terminal.turn_id(),
        terminal.source().unwrap().clone(),
        RepairCaptureGap::new(
            terminal.repair_witness(),
            *status,
            reason,
            Some(issue.repair_witness()),
        )
        .unwrap(),
        RepairRequestDisposition::Available,
    )
}

#[test]
fn exact_issue_is_authenticated_for_every_gap_reason_with_two_scoped_reads() {
    let fixture = setup("repair-source-issue-match");
    let (terminal, issue) = events(&fixture);
    for reason in [
        RepairCaptureGapReason::TerminalCaptureIncomplete,
        RepairCaptureGapReason::ForcedAbortOrderingUnproven,
        RepairCaptureGapReason::ProviderObservationIssue,
    ] {
        let target = target(&terminal, &issue, reason);
        let mut reads = Vec::new();
        assert_eq!(
            authenticate(&target, |turn, sequence| {
                reads.push((turn, sequence));
                Ok::<_, ()>(Some(if sequence == terminal.sequence() {
                    terminal.clone()
                } else {
                    issue.clone()
                }))
            }),
            Ok(true)
        );
        assert_eq!(
            reads,
            vec![
                (fixture.turn, terminal.sequence()),
                (fixture.turn, issue.sequence())
            ]
        );
    }
}

#[test]
fn missing_issue_and_issue_read_error_are_not_accepted() {
    let fixture = setup("repair-source-issue-missing");
    let (terminal, issue) = events(&fixture);
    let target = target(
        &terminal,
        &issue,
        RepairCaptureGapReason::ProviderObservationIssue,
    );
    for fail in [false, true] {
        let mut reads = 0;
        assert_eq!(
            authenticate(&target, |_, sequence| {
                reads += 1;
                if sequence == terminal.sequence() {
                    Ok(Some(terminal.clone()))
                } else if fail {
                    Err("issue read failed")
                } else {
                    Ok(None)
                }
            }),
            if fail {
                Err("issue read failed")
            } else {
                Ok(false)
            }
        );
        assert_eq!(reads, 2);
    }
}

#[test]
fn issue_witness_commits_sequence_and_sealed_observation_identity() {
    let fixture = setup("repair-source-issue-witness");
    let (terminal, issue) = events(&fixture);
    let target = target(
        &terminal,
        &issue,
        RepairCaptureGapReason::ProviderObservationIssue,
    );
    let replacement = inspect_agent_start(&fixture, 72)
        .into_issue(ProviderObservationIssueReason::DuplicateItemStart);
    for substituted in [
        SourceEventRecord::new(
            issue.turn_id(),
            SourceEventSequence::FIRST,
            issue.source().cloned(),
            issue.payload().clone(),
        )
        .unwrap(),
        SourceEventRecord::new(
            issue.turn_id(),
            issue.sequence(),
            issue.source().cloned(),
            SourceEventPayload::ProviderObservationIssue(Box::new(replacement)),
        )
        .unwrap(),
    ] {
        assert_eq!(
            authenticate(&target, |_, sequence| Ok::<_, ()>(Some(
                if sequence == terminal.sequence() {
                    terminal.clone()
                } else {
                    substituted.clone()
                }
            ))),
            Ok(false)
        );
    }
}

#[test]
fn issue_substitution_fails_even_with_recomputed_digest() {
    let fixture = setup("repair-source-issue-substitution");
    let (terminal, issue) = events(&fixture);
    let other = CasTurnSource::new(
        beryl_model::CasThreadId::new("other-thread").unwrap(),
        beryl_model::CasTurnId::new("other-turn").unwrap(),
    );
    let variants = [
        SourceEventRecord::new(
            SyndicTurnId::from_bytes([99; 16]),
            issue.sequence(),
            issue.source().cloned(),
            issue.payload().clone(),
        )
        .unwrap(),
        SourceEventRecord::new(
            issue.turn_id(),
            issue.sequence(),
            Some(other.clone()),
            issue.payload().clone(),
        )
        .unwrap(),
        SourceEventRecord::new(
            issue.turn_id(),
            issue.sequence(),
            issue.source().cloned(),
            SourceEventPayload::TurnActivated,
        )
        .unwrap(),
        SourceEventRecord::new(
            issue.turn_id(),
            issue.sequence(),
            issue.source().cloned(),
            terminal.payload().clone(),
        )
        .unwrap(),
    ];
    for variant in variants {
        for witness_event in [&issue, &variant] {
            let target = target(
                &terminal,
                witness_event,
                RepairCaptureGapReason::ProviderObservationIssue,
            );
            assert_eq!(
                authenticate(&target, |_, sequence| Ok::<_, ()>(Some(
                    if sequence == terminal.sequence() {
                        terminal.clone()
                    } else {
                        variant.clone()
                    }
                ))),
                Ok(false)
            );
        }
    }
    // Outer routes and digests agree, but the issue still names the original route.
    let other_terminal = SourceEventRecord::new(
        terminal.turn_id(),
        terminal.sequence(),
        Some(other.clone()),
        terminal.payload().clone(),
    )
    .unwrap();
    let other_issue = SourceEventRecord::new(
        issue.turn_id(),
        issue.sequence(),
        Some(other),
        issue.payload().clone(),
    )
    .unwrap();
    let target = target(
        &other_terminal,
        &other_issue,
        RepairCaptureGapReason::ProviderObservationIssue,
    );
    assert_eq!(
        authenticate(&target, |_, sequence| Ok::<_, ()>(Some(
            if sequence == other_terminal.sequence() {
                other_terminal.clone()
            } else {
                other_issue.clone()
            }
        ))),
        Ok(false)
    );
}
