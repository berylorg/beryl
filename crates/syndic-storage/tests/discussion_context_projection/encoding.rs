use super::*;
use beryl_model::{RecoveryItemSequenceDigest, ThreadRevision};
use sha2::{Digest, Sha256};
use test_faults::{decode_lineage_fixture, encode_lineage_fixture};

fn prefix_bytes(prefix: CasRepresentedPrefixProof) -> Vec<u8> {
    let mut bytes = Vec::new();
    match prefix.tail() {
        Some(turn) => {
            bytes.push(1);
            bytes.extend_from_slice(turn.as_bytes());
        }
        None => bytes.push(0),
    }
    bytes.extend_from_slice(&prefix.source_thread_revision().get().to_be_bytes());
    bytes.extend_from_slice(prefix.digest().as_bytes());
    bytes
}

#[test]
fn ordinary_native_and_recovery_v1_keep_exact_wire_bytes() {
    let prefix = CasRepresentedPrefixProof::new(
        Some(beryl_model::SyndicTurnId::from_bytes([71; 16])),
        ThreadRevision::new(9).unwrap(),
        root_turn_chain_digest(beryl_model::SyndicTurnId::from_bytes([71; 16])),
    );
    let native = CasLineageProof::native(NativeCasLineage::Fork, prefix).unwrap();
    let mut expected = vec![0, 3];
    expected.extend(prefix_bytes(prefix));
    assert_eq!(encode_lineage_fixture(native), expected);
    assert_eq!(decode_lineage_fixture(&expected).unwrap(), native);
    let digest = RecoveryItemSequenceDigest::from_bytes([0x6a; 32]);
    let recovered = CasLineageProof::recovered(
        RecoveredInjectionProof::new(
            RecoveryProjectionVersion::V1,
            prefix,
            digest,
            RecoveryItemCount::new(2).unwrap(),
            RecoveryUtf8ByteCount::new(17).unwrap(),
            timestamp(100),
            generation(),
        )
        .unwrap(),
    );
    let mut expected = vec![1, 1];
    expected.extend(prefix_bytes(prefix));
    expected.extend_from_slice(&[0x6a; 32]);
    expected.extend_from_slice(&2u32.to_be_bytes());
    expected.extend_from_slice(&17u64.to_be_bytes());
    expected.extend_from_slice(&100u64.to_be_bytes());
    expected.extend_from_slice(&20u64.to_be_bytes());
    expected.extend_from_slice(&21u64.to_be_bytes());
    assert_eq!(encode_lineage_fixture(recovered), expected);
    assert_eq!(decode_lineage_fixture(&expected).unwrap(), recovered);
}

#[test]
fn context_seed_and_replay_wire_fields_have_exact_widths_and_closed_decoding() {
    let (_home, store, storage) = seeded("context-lineage-encoding");
    let (basis, seed, _) = native_seed(&store, &storage);
    let lineage = seed
        .completed_lineage(timestamp(100), generation())
        .unwrap();
    let bytes = encode_lineage_fixture(lineage);
    let invented = ContextInjectionObservation::new(
        RecoveryItemCount::new(1).unwrap(),
        RecoveryUtf8ByteCount::new(470).unwrap(),
        RecoveryItemSequenceDigest::from_bytes([0x44; 32]),
        timestamp(100),
    )
    .unwrap();
    assert!(
        basis
            .context()
            .unwrap()
            .complete_native(NativeCasLineage::Fork, generation(), Some(invented))
            .is_err()
    );
    assert_eq!(bytes.len(), 397);
    assert_eq!(&bytes[..3], &[2, 1, 0]);
    assert_eq!(&bytes[3..19], store.home_id().as_bytes());
    assert_eq!(&bytes[19..76], prefix_bytes(basis.represented_prefix()));
    assert_eq!(&bytes[76..84], &1u64.to_be_bytes());
    assert_eq!(&bytes[84..92], &470u64.to_be_bytes());
    assert_eq!(&bytes[156..164], &100u64.to_be_bytes());
    assert_eq!(&bytes[341..349], &1u64.to_be_bytes());
    assert_eq!(&bytes[349..357], &470u64.to_be_bytes());
    assert_eq!(&bytes[389..397], &100u64.to_be_bytes());
    let owner = storage
        .thread(&store, id(210), point_limit())
        .unwrap()
        .unwrap()
        .context_owner_id()
        .unwrap();
    let beryl_model::DiscussionContextOwnerId::SubmittedTurn(owner_turn) = owner else {
        panic!("submitted owner");
    };
    let record = storage
        .context_envelope(&store, owner, point_limit())
        .unwrap()
        .unwrap();
    let source = record.envelope().descriptor().source();
    let hex = |value: &[u8]| {
        value
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    };
    let selected_digest = Sha256::digest(b"assistant");
    let frame = format!(
        "[Beryl discussion context v1]\nsource_thread={}\nsource_turn={}\nsource_item={}\nsource_projection={}\nprojection_revision={:016x}\nrange_start=0000000000000000\nrange_end=0000000000000009\nselected_utf8_bytes=0000000000000009\nselected_sha256={}\n\nassistant\n[/Beryl discussion context v1]",
        "1e".repeat(16),
        "20".repeat(16),
        "21".repeat(16),
        hex(source.projection_id().as_bytes()),
        source.projection_revision().get(),
        hex(&selected_digest)
    );
    assert_eq!(replay(&store, &storage, seed, 65_536)[0].1, frame);
    let mut frame_digest = Sha256::new();
    frame_digest.update(b"syndic/context-frame/v1\0");
    frame_digest.update(frame.as_bytes());
    let mut entry = Sha256::new();
    entry.update(b"syndic/context-entry/v1\0");
    entry.update(store.home_id().as_bytes());
    entry.update(id(210).as_bytes());
    entry.update(owner_turn.as_bytes());
    entry.update(record.revision().get().to_be_bytes());
    entry.update([1]);
    entry.update(source.thread_id().as_bytes());
    entry.update(source.turn_id().as_bytes());
    entry.update(source.item_id().as_bytes());
    entry.update(source.projection_id().as_bytes());
    entry.update(source.projection_revision().get().to_be_bytes());
    entry.update(0u64.to_be_bytes());
    entry.update(9u64.to_be_bytes());
    entry.update(selected_digest);
    entry.update(5u64.to_be_bytes());
    entry.update(470u64.to_be_bytes());
    entry.update(frame_digest.finalize());
    let entry = entry.finalize();
    assert_eq!(&bytes[309..341], entry.as_slice());
    let mut summary = Sha256::new();
    summary.update(b"syndic/context-sequence/v1/step\0");
    summary.update(Sha256::digest(b"syndic/context-sequence/v1/empty\0"));
    summary.update(1u64.to_be_bytes());
    summary.update(entry);
    summary.update(470u64.to_be_bytes());
    assert_eq!(&bytes[92..124], summary.finalize().as_slice());
    assert_eq!(decode_lineage_fixture(&bytes).unwrap(), lineage);
    for (offset, value) in [(0, 3), (1, 2), (2, 3)] {
        let mut invalid = bytes.clone();
        invalid[offset] = value;
        assert!(decode_lineage_fixture(&invalid).is_err());
    }
    let mut invalid = bytes.clone();
    invalid[76..84].fill(0);
    assert!(decode_lineage_fixture(&invalid).is_err());
    let mut invalid = bytes.clone();
    invalid[156..164].copy_from_slice(&99u64.to_be_bytes());
    assert!(decode_lineage_fixture(&invalid).is_err());
    let mut invalid = bytes.clone();
    invalid.push(0);
    assert!(decode_lineage_fixture(&invalid).is_err());
    assert!(decode_lineage_fixture(&bytes[..396]).is_err());
    let target = request(&store, &storage);
    let RecoveryAssembly::Ready(replay) = storage
        .prepare_recovery_projection(
            &store,
            RecoveryProjectionRequest::for_pending_selected_turn_parent(
                id(210),
                target.selected_path(),
                Some(1_000_000),
            )
            .with_tool_profile(target.tool_profile()),
        )
        .unwrap()
    else {
        panic!("context replay");
    };
    let lineage = replay
        .completed_lineage(timestamp(101), generation())
        .unwrap();
    let bytes = encode_lineage_fixture(lineage);
    assert_eq!(bytes.len(), 237);
    assert_eq!(&bytes[..3], &[2, 1, 2]);
    assert_eq!(bytes[180], 2);
    assert_eq!(decode_lineage_fixture(&bytes).unwrap(), lineage);
    store.close().unwrap();
}
