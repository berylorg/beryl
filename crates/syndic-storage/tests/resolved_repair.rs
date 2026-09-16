#![cfg(feature = "test-faults")]

use beryl_model::{CasThreadId, CasTurnId, InputGateRevision, SyndicTurnId};
use syndic_storage::test_faults::{
    decode_turn_state_for_test as decode, turn_state_codec_bytes as encode,
};
use syndic_storage::*;

fn target(request: RepairRequestDisposition) -> RepairRequiredTarget {
    RepairRequiredTarget::new(
        SyndicTurnId::from_bytes([17; 16]),
        CasTurnSource::new(CasThreadId::new("T").unwrap(), CasTurnId::new("t").unwrap()),
        RepairCaptureGap::new(
            RepairSourceEventWitness::new(
                SourceEventSequence::new(3).unwrap(),
                RepairSourceEventDigest::from_bytes([31; 32]),
            ),
            TurnEndStatus::new(TurnTerminalOutcome::Interrupted, None).unwrap(),
            RepairCaptureGapReason::ForcedAbortOrderingUnproven,
            None,
        )
        .unwrap(),
        request,
    )
}

fn state(target: &RepairRequiredTarget, reason: TurnIncompleteReason) -> TurnStateRecord {
    TurnStateRecord::with_capture_frontiers(
        target.turn_id(),
        TurnStateRevision::FIRST,
        target.gap().status().lifecycle(),
        target.gap().terminal().sequence().get(),
        2,
        0,
        1,
        1,
        Some(TurnEndStatus::new(target.gap().status().outcome(), Some(reason)).unwrap()),
        SyndicTimestamp::from_unix_millis(9),
        TurnDispatchProvenance::Unattempted,
    )
    .unwrap()
}

fn requests() -> [RepairRequestDisposition; 2] {
    [
        RepairRequestDisposition::Available,
        RepairRequestDisposition::Consumed(
            ConsumedRepairRequest::new(
                RepairRequestAttemptNonce::from_bytes([27; 16]),
                InputGateRevision::new(8).unwrap(),
                InputGateRevision::new(9).unwrap(),
            )
            .unwrap(),
        ),
    ]
}

#[test]
fn resolved_state_round_trips_exact_disposition_and_distinct_original_status() {
    for request in requests() {
        for reason in [
            TurnIncompleteReason::AuthorityLost,
            TurnIncompleteReason::UnsupportedHistory(UnsupportedHistoryReason::UnknownPublicItem),
        ] {
            let target = target(request);
            let resolved =
                ResolvedRepair::new(target.clone(), RepairResolution::Incomplete(reason));
            let value = state(&target, reason)
                .with_resolved_repair(resolved.clone())
                .unwrap();
            let decoded = decode(&encode(&value)).unwrap();
            assert_eq!(decoded, value);
            assert_eq!(decoded.resolved_repair(), Some(&resolved));
            assert_eq!(
                decoded.resolved_repair().unwrap().target().request(),
                request
            );
            assert_eq!(target.gap().status().incomplete_reason(), None);
            assert_eq!(decoded.incomplete_reason(), Some(reason));
        }
    }
}

#[test]
fn resolution_encoding_is_exact_and_rejects_all_truncations_tags_and_trailing_bytes() {
    for request in requests() {
        let target = target(request);
        let plain = state(&target, TurnIncompleteReason::AuthorityLost);
        let mut expected = encode(&plain);
        assert_eq!(expected.pop(), Some(0));
        let optional_offset = expected.len();
        expected.push(1);
        expected.extend(test_faults::encode_repair_target_for_test(&target));
        let resolution_offset = expected.len();
        expected.extend([0, 1]);
        let value = plain
            .with_resolved_repair(ResolvedRepair::new(
                target,
                RepairResolution::Incomplete(TurnIncompleteReason::AuthorityLost),
            ))
            .unwrap();
        let bytes = encode(&value);
        assert_eq!(bytes, expected);
        for end in 0..bytes.len() {
            assert!(decode(&bytes[..end]).is_none(), "prefix {end}");
        }
        for (offset, bad) in [
            (optional_offset, 2),
            (resolution_offset, 1),
            (resolution_offset + 1, 255),
        ] {
            let mut malformed = bytes.clone();
            malformed[offset] = bad;
            assert!(decode(&malformed).is_none(), "tag at {offset}");
        }
        let mut trailing = bytes;
        trailing.push(0);
        assert!(decode(&trailing).is_none());
    }
}

#[test]
fn containing_identity_frontier_and_reason_cannot_be_substituted() {
    let target = target(RepairRequestDisposition::Available);
    let resolved = ResolvedRepair::new(
        target.clone(),
        RepairResolution::Incomplete(TurnIncompleteReason::AuthorityLost),
    );
    assert!(
        state(&target, TurnIncompleteReason::StreamLost)
            .with_resolved_repair(resolved.clone())
            .is_err()
    );
    let value = state(&target, TurnIncompleteReason::AuthorityLost)
        .with_resolved_repair(resolved)
        .unwrap();
    let bytes = encode(&value);
    for offset in [0, 24, 32] {
        let mut changed = bytes.clone();
        changed[offset] ^= 1;
        assert!(
            decode(&changed).is_none(),
            "identity/lifecycle/frontier byte {offset}"
        );
    }
}

#[test]
fn resolved_target_cannot_be_replaced_or_claim_reset() {
    let target = target(requests()[1]);
    let resolved = ResolvedRepair::new(
        target.clone(),
        RepairResolution::Incomplete(TurnIncompleteReason::AuthorityLost),
    );
    let value = state(&target, TurnIncompleteReason::AuthorityLost)
        .with_resolved_repair(resolved.clone())
        .unwrap();
    assert_eq!(value.clone().with_resolved_repair(resolved).unwrap(), value);
    let reset = RepairRequiredTarget::new(
        target.turn_id(),
        target.source().clone(),
        target.gap(),
        RepairRequestDisposition::Available,
    );
    assert!(
        value
            .with_resolved_repair(ResolvedRepair::new(
                reset,
                RepairResolution::Incomplete(TurnIncompleteReason::AuthorityLost)
            ))
            .is_err()
    );
}

#[test]
fn absent_resolution_is_explicit_and_old_payload_is_rejected() {
    let value = state(
        &target(RepairRequestDisposition::Available),
        TurnIncompleteReason::StreamLost,
    );
    let mut bytes = encode(&value);
    assert_eq!(decode(&bytes), Some(value));
    assert_eq!(bytes.pop(), Some(0));
    assert!(decode(&bytes).is_none());
}
