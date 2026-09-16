#![cfg(feature = "test-faults")]

use beryl_model::{CasThreadId, CasTurnId, SyndicTurnId};
use syndic_storage::{
    CasTurnSource, RepairCaptureGap, RepairCaptureGapReason, RepairRequestDisposition,
    RepairRequiredTarget, RepairSourceEventDigest, RepairSourceEventWitness, SourceEventPayload,
    SourceEventRecord, SourceEventSequence, TurnEndStatus, TurnIncompleteReason,
    TurnTerminalOutcome, test_faults::repair_source_events_match_for_test as authenticate,
};

fn source(thread: &str, turn: &str) -> CasTurnSource {
    CasTurnSource::new(
        CasThreadId::new(thread).unwrap(),
        CasTurnId::new(turn).unwrap(),
    )
}

fn terminal() -> SourceEventRecord {
    SourceEventRecord::new(
        SyndicTurnId::from_bytes([1; 16]),
        SourceEventSequence::new(9).unwrap(),
        Some(source("thread", "turn")),
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(
                TurnTerminalOutcome::Interrupted,
                Some(TurnIncompleteReason::StreamLost),
            )
            .unwrap(),
        ),
    )
    .unwrap()
}

fn target(
    witness: RepairSourceEventWitness,
    reason: RepairCaptureGapReason,
    issue: Option<RepairSourceEventWitness>,
) -> RepairRequiredTarget {
    let SourceEventPayload::TurnEnded(status) = terminal().payload().clone() else {
        unreachable!()
    };
    RepairRequiredTarget::new(
        terminal().turn_id(),
        terminal().source().unwrap().clone(),
        RepairCaptureGap::new(witness, status, reason, issue).unwrap(),
        RepairRequestDisposition::Available,
    )
}

#[test]
fn exact_terminal_uses_one_scoped_read_for_each_gap_without_issue() {
    let event = terminal();
    for reason in [
        RepairCaptureGapReason::TerminalCaptureIncomplete,
        RepairCaptureGapReason::ForcedAbortOrderingUnproven,
    ] {
        let target = target(event.repair_witness(), reason, None);
        let mut reads = Vec::new();
        assert_eq!(
            authenticate(&target, |turn, sequence| {
                reads.push((turn, sequence));
                Ok::<_, ()>(Some(event.clone()))
            }),
            Ok(true)
        );
        assert_eq!(reads, vec![(event.turn_id(), event.sequence())]);
    }
}

#[test]
fn missing_terminal_and_read_failure_never_follow_issue() {
    let event = terminal();
    let issue = RepairSourceEventWitness::new(
        SourceEventSequence::FIRST,
        RepairSourceEventDigest::from_bytes([7; 32]),
    );
    let target = target(
        event.repair_witness(),
        RepairCaptureGapReason::ProviderObservationIssue,
        Some(issue),
    );
    for fail in [false, true] {
        let mut calls = 0;
        let result = authenticate(&target, |turn, sequence| {
            calls += 1;
            assert_eq!((turn, sequence), (event.turn_id(), event.sequence()));
            if fail { Err("read failed") } else { Ok(None) }
        });
        assert_eq!(result, if fail { Err("read failed") } else { Ok(false) });
        assert_eq!(calls, 1);
    }
}

#[test]
fn recomputed_digest_does_not_authorize_structural_substitutions() {
    let original = terminal();
    let variants = [
        SourceEventRecord::new(
            SyndicTurnId::from_bytes([2; 16]),
            original.sequence(),
            original.source().cloned(),
            original.payload().clone(),
        )
        .unwrap(),
        SourceEventRecord::new(
            original.turn_id(),
            original.sequence(),
            Some(source("Thread", "turn")),
            original.payload().clone(),
        )
        .unwrap(),
        SourceEventRecord::new(
            original.turn_id(),
            original.sequence(),
            Some(source("thread", "Turn")),
            original.payload().clone(),
        )
        .unwrap(),
        SourceEventRecord::new(
            original.turn_id(),
            original.sequence(),
            None,
            original.payload().clone(),
        )
        .unwrap(),
        SourceEventRecord::new(
            original.turn_id(),
            original.sequence(),
            original.source().cloned(),
            SourceEventPayload::TurnActivated,
        )
        .unwrap(),
        SourceEventRecord::new(
            original.turn_id(),
            original.sequence(),
            original.source().cloned(),
            SourceEventPayload::TurnEnded(TurnEndStatus::complete()),
        )
        .unwrap(),
        SourceEventRecord::new(
            original.turn_id(),
            original.sequence(),
            original.source().cloned(),
            SourceEventPayload::TurnEnded(
                TurnEndStatus::new(TurnTerminalOutcome::Interrupted, None).unwrap(),
            ),
        )
        .unwrap(),
    ];
    for event in variants {
        for witness in [original.repair_witness(), event.repair_witness()] {
            let target = target(
                witness,
                RepairCaptureGapReason::ForcedAbortOrderingUnproven,
                None,
            );
            assert_eq!(
                authenticate(&target, |_, _| Ok::<_, ()>(Some(event.clone()))),
                Ok(false)
            );
        }
    }
}

#[test]
fn exact_sequence_and_digest_are_both_required() {
    let event = terminal();
    for witness in [
        RepairSourceEventWitness::new(
            event.sequence(),
            RepairSourceEventDigest::from_bytes([0; 32]),
        ),
        RepairSourceEventWitness::new(SourceEventSequence::FIRST, event.repair_witness().digest()),
    ] {
        let target = target(
            witness,
            RepairCaptureGapReason::ForcedAbortOrderingUnproven,
            None,
        );
        assert_eq!(
            authenticate(&target, |_, _| Ok::<_, ()>(Some(event.clone()))),
            Ok(false)
        );
    }
}
