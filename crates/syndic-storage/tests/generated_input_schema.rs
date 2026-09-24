#![cfg(feature = "test-faults")]

use beryl_model::*;
use sha2::{Digest, Sha256};
use syndic_storage::{test_faults::*, *};
#[path = "generated_input_schema/history.rs"]
mod history;
mod support;

fn receipt(resolution: &str) -> DiscussionHandoffReceipt {
    DiscussionHandoffReceipt {
        parent_thread_revision: ThreadRevision::new(7).unwrap(),
        parent_gate_revision: InputGateRevision::new(9).unwrap(),
        child_thread_id: SyndicThreadId::from_bytes([2; 16]),
        intent_id: ResolutionIntentId::from_bytes([3; 16]),
        job_id: JobId::from_bytes([3; 16]),
        context_owner: DiscussionContextOwnerId::SubmittedTurn(SyndicTurnId::from_bytes([4; 16])),
        context_digest: DiscussionContextDigest::from_bytes([5; 32]),
        resolving_turn_id: SyndicTurnId::from_bytes([6; 16]),
        resolution_digest: Sha256::digest(resolution.as_bytes()).into(),
        parent_turn_id: SyndicTurnId::from_bytes([7; 16]),
        canonical_item_id: SyndicItemId::from_bytes([8; 16]),
    }
}

fn content(resolution: &str) -> ContentReference {
    PreparedContent::composer(
        &ComposerPayload::new(vec![
            ComposerAtom::text(format!("Discussion resolution:\n\n{resolution}")).unwrap(),
        ])
        .unwrap(),
    )
    .unwrap()
    .reference(ContentRevision::new(1).unwrap())
}

fn input(
    receipt: DiscussionHandoffReceipt,
    content: ContentReference,
) -> Result<AcceptedInputRecord, SyndicRecordError> {
    AcceptedInputRecord::new(
        SyndicAcceptedInputId::from_bytes([3; 16]),
        SyndicThreadId::from_bytes([1; 16]),
        AcceptedInputOrdinal::FIRST,
        AcceptedInputSource::DiscussionHandoff(receipt),
        content,
        None,
        SyndicTimestamp::from_unix_millis(100),
    )
}

#[test]
fn generated_receipt_has_closed_encoding_without_composer_authority() {
    let text = "literal [image:A] and <context> stay text";
    let record = input(receipt(text), content(text)).unwrap();
    assert_eq!(record.composer_admission(), None);
    assert_eq!(record.route_generation(), None);
    assert_eq!(record.asset_reference_set(), None);
    let bytes = accepted_input_codec_bytes(&record);
    assert_eq!(bytes[40], 1);
    assert_eq!(decode_accepted_input_for_test(&bytes), Some(record.clone()));
    for end in 0..bytes.len() {
        assert!(
            decode_accepted_input_for_test(&bytes[..end]).is_none(),
            "cut {end}"
        );
    }
    let mut bad = bytes.clone();
    bad[40] = 2;
    assert!(decode_accepted_input_for_test(&bad).is_none());
    let mut bad = bytes.clone();
    bad[0] ^= 1;
    assert!(decode_accepted_input_for_test(&bad).is_none());
    let mut bad = bytes;
    bad.push(0);
    assert!(decode_accepted_input_for_test(&bad).is_none());

    let order = AcceptedOrderIndexRecord::from_source(
        record.thread_id(),
        record.ordinal(),
        record.id(),
        record.source().order_source(),
    );
    let bytes = accepted_order_codec_bytes(&order);
    assert_eq!(bytes.len(), 41);
    assert_eq!(bytes[40], 1);
    assert_eq!(decode_accepted_order_for_test(&bytes), Some(order));
    for end in 0..bytes.len() {
        assert!(decode_accepted_order_for_test(&bytes[..end]).is_none());
    }
    let mut bad = bytes.clone();
    bad[40] = 2;
    assert!(decode_accepted_order_for_test(&bad).is_none());
    let mut bad = bytes;
    bad.push(0);
    assert!(decode_accepted_order_for_test(&bad).is_none());
}

#[test]
fn generated_item_roundtrip_preserves_distinct_provenance() {
    let proof = receipt("result");
    let record = input(proof, content("result")).unwrap();
    let item = CanonicalItemRecord::local_discussion_handoff(
        proof.canonical_item_id,
        proof.parent_turn_id,
        TurnItemOrdinal::FIRST,
        ProjectionRevision::new(1).unwrap(),
        record.content(),
        record.id(),
    );
    assert_eq!(item.kind(), CanonicalItemKind::DiscussionHandoff);
    assert_eq!(item.provider_kind(), ProviderItemKind::UserMessage);
    let bytes = canonical_item_codec_bytes(&item);
    assert_eq!(decode_canonical_item_for_test(&bytes), Some(item));
    for end in 0..bytes.len() {
        assert!(decode_canonical_item_for_test(&bytes[..end]).is_none());
    }
    let mut bad = bytes;
    bad.push(0);
    assert!(decode_canonical_item_for_test(&bad).is_none());
}

#[test]
fn generated_constructor_rejects_identity_aliases_and_out_of_bound_payloads() {
    let maximum = "🦀".repeat(65_536);
    let value = input(receipt(&maximum), content(&maximum)).unwrap();
    assert_eq!(value.content().summary().logical_utf8_bytes(), 262_168);
    assert!(input(receipt(""), content("")).is_err());
    let oversized = format!("{maximum}a");
    assert!(input(receipt(&oversized), content(&oversized)).is_err());
    for mutation in 0..4 {
        let mut proof = receipt("result");
        match mutation {
            0 => proof.job_id = JobId::from_bytes([9; 16]),
            1 => proof.intent_id = ResolutionIntentId::from_bytes([9; 16]),
            2 => proof.child_thread_id = SyndicThreadId::from_bytes([1; 16]),
            _ => proof.parent_turn_id = proof.resolving_turn_id,
        }
        assert_eq!(
            input(proof, content("result")),
            Err(SyndicRecordError::InvalidGeneratedHandoffInput)
        );
    }
}
