#![cfg(feature = "test-faults")]

use beryl_model::{CasThreadId, CasTurnId, InputGateRevision, SyndicTurnId};
use syndic_storage::{
    CasTurnSource, ConsumedRepairRequest, RepairCaptureGap, RepairCaptureGapReason,
    RepairRequestAttemptNonce, RepairRequestDisposition, RepairRequiredTarget,
    RepairSourceEventDigest, RepairSourceEventWitness, SourceEventSequence, TurnEndStatus,
    TurnIncompleteReason, TurnTerminalOutcome,
    test_faults::{
        decode_repair_target_for_test as decode, encode_repair_target_for_test as encode,
    },
};

fn witness(sequence: u64, byte: u8) -> RepairSourceEventWitness {
    RepairSourceEventWitness::new(
        SourceEventSequence::new(sequence).unwrap(),
        RepairSourceEventDigest::from_bytes([byte; 32]),
    )
}

fn target(reason: RepairCaptureGapReason, issue: bool, consumed: bool) -> RepairRequiredTarget {
    let status = match reason {
        RepairCaptureGapReason::TerminalCaptureIncomplete => {
            TurnEndStatus::incomplete(TurnIncompleteReason::StreamLost)
        }
        RepairCaptureGapReason::ForcedAbortOrderingUnproven => {
            TurnEndStatus::new(TurnTerminalOutcome::Interrupted, None).unwrap()
        }
        RepairCaptureGapReason::ProviderObservationIssue => TurnEndStatus::complete(),
    };
    RepairRequiredTarget::new(
        SyndicTurnId::from_bytes([0x11; 16]),
        CasTurnSource::new(CasThreadId::new("T").unwrap(), CasTurnId::new("ü").unwrap()),
        RepairCaptureGap::new(
            witness(9, 0x22),
            status,
            reason,
            issue.then(|| witness(8, 0x33)),
        )
        .unwrap(),
        if consumed {
            RepairRequestDisposition::Consumed(
                ConsumedRepairRequest::new(
                    RepairRequestAttemptNonce::from_bytes([0x44; 16]),
                    InputGateRevision::new(7).unwrap(),
                    InputGateRevision::new(8).unwrap(),
                )
                .unwrap(),
            )
        } else {
            RepairRequestDisposition::Available
        },
    )
}

#[test]
fn fixed_available_and_consumed_payload_bytes() {
    let value = target(
        RepairCaptureGapReason::ForcedAbortOrderingUnproven,
        false,
        false,
    );
    let mut expected = vec![0x11; 16];
    expected.extend_from_slice(&[0, 0, 0, 1, b'T', 0, 0, 0, 2, 0xc3, 0xbc]);
    expected.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 9]);
    expected.extend_from_slice(&[0x22; 32]);
    expected.extend_from_slice(&[1, 0, 1, 0, 0]);
    assert_eq!(encode(&value), expected);
    assert_eq!(decode(&expected).unwrap(), value);

    let consumed = target(
        RepairCaptureGapReason::ForcedAbortOrderingUnproven,
        true,
        true,
    );
    expected.truncate(70);
    expected.push(1);
    expected.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 8]);
    expected.extend_from_slice(&[0x33; 32]);
    expected.push(1);
    expected.extend_from_slice(&[0x44; 16]);
    expected.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 7, 0, 0, 0, 0, 0, 0, 0, 8]);
    assert_eq!(encode(&consumed), expected);
    assert_eq!(decode(&expected).unwrap(), consumed);
}

#[test]
fn closed_reasons_and_dispositions_roundtrip_canonically() {
    for reason in [
        RepairCaptureGapReason::TerminalCaptureIncomplete,
        RepairCaptureGapReason::ForcedAbortOrderingUnproven,
        RepairCaptureGapReason::ProviderObservationIssue,
    ] {
        for issue in [false, true] {
            if reason == RepairCaptureGapReason::ProviderObservationIssue && !issue {
                continue;
            }
            for consumed in [false, true] {
                let value = target(reason, issue, consumed);
                let bytes = encode(&value);
                let decoded = decode(&bytes).unwrap();
                assert_eq!(decoded, value);
                assert_eq!(encode(&decoded), bytes);
                for end in 0..bytes.len() {
                    assert!(decode(&bytes[..end]).is_err(), "prefix {end}");
                }
                let mut trailing = bytes;
                trailing.push(0);
                assert!(decode(&trailing).is_err());
            }
        }
    }
}

#[test]
fn invalid_tags_and_inconsistent_gap_evidence_are_rejected() {
    let bytes = encode(&target(
        RepairCaptureGapReason::ForcedAbortOrderingUnproven,
        false,
        false,
    ));
    for (offset, replacement) in [
        (67, 255),
        (68, 2),
        (69, 255),
        (70, 2),
        (71, 2),
        (67, 4),
        (67, 0),
        (69, 0),
        (69, 2),
    ] {
        let mut bad = bytes.clone();
        bad[offset] = replacement;
        assert!(
            decode(&bad).is_err(),
            "offset {offset}, replacement {replacement}"
        );
    }
    let mut zero = bytes.clone();
    zero[27..35].fill(0);
    assert!(decode(&zero).is_err());
    let bytes = encode(&target(
        RepairCaptureGapReason::ProviderObservationIssue,
        true,
        false,
    ));
    for sequence in [0, 9, 10, u64::MAX] {
        let mut bad = bytes.clone();
        bad[71..79].copy_from_slice(&sequence.to_be_bytes());
        assert!(decode(&bad).is_err());
    }
}

#[test]
fn consumed_revision_zero_nonadjacency_and_overflow_are_rejected() {
    let bytes = encode(&target(
        RepairCaptureGapReason::ForcedAbortOrderingUnproven,
        false,
        true,
    ));
    for (source, successor) in [(0u64, 1u64), (7, 0), (7, 7), (7, 9), (8, 7), (u64::MAX, 1)] {
        let mut bad = bytes.clone();
        bad[88..96].copy_from_slice(&source.to_be_bytes());
        bad[96..104].copy_from_slice(&successor.to_be_bytes());
        assert!(decode(&bad).is_err(), "{source} -> {successor}");
    }
}

#[test]
fn maximum_bounded_identities_and_integer_values_roundtrip() {
    let base = target(
        RepairCaptureGapReason::ForcedAbortOrderingUnproven,
        false,
        false,
    );
    let value = RepairRequiredTarget::new(
        base.turn_id(),
        CasTurnSource::new(
            CasThreadId::new("é".repeat(128)).unwrap(),
            CasTurnId::new("界".repeat(85) + "x").unwrap(),
        ),
        RepairCaptureGap::new(
            witness(u64::MAX, 255),
            base.gap().status(),
            base.gap().reason(),
            Some(witness(u64::MAX - 1, 0)),
        )
        .unwrap(),
        RepairRequestDisposition::Consumed(
            ConsumedRepairRequest::new(
                RepairRequestAttemptNonce::from_bytes([255; 16]),
                InputGateRevision::new(u64::MAX - 1).unwrap(),
                InputGateRevision::new(u64::MAX).unwrap(),
            )
            .unwrap(),
        ),
    );
    assert_eq!(decode(&encode(&value)).unwrap(), value);
    assert_eq!(encode(&value).len(), 653);
}

#[test]
fn malformed_unbounded_and_absent_correlation_are_rejected() {
    let bytes = encode(&target(
        RepairCaptureGapReason::ForcedAbortOrderingUnproven,
        false,
        false,
    ));
    for (range, replacement) in [
        (16..20, vec![0, 0, 0, 0]),
        (16..21, vec![0, 0, 0, 0]),
        (16..20, u32::MAX.to_be_bytes().to_vec()),
        (21..27, vec![0, 0, 0, 0]),
        (25..27, vec![0xff, 0xff]),
    ] {
        let mut bad = bytes.clone();
        bad.splice(range, replacement);
        assert!(decode(&bad).is_err());
    }
    for field in [16..21, 21..27] {
        let mut replacement = 257u32.to_be_bytes().to_vec();
        replacement.extend_from_slice(&[b'x'; 257]);
        let mut bad = bytes.clone();
        bad.splice(field, replacement);
        assert!(decode(&bad).is_err());
    }
}
