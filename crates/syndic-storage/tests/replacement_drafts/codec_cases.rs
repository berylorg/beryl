use super::*;
use syndic_storage::test_faults::{
    draft_canonical_encoding_for_test, draft_canonical_payload_reencode_for_test,
    sealed_import_build_codec_rejections_for_test, sealed_import_checkpoint_for_test,
    sealed_import_terminal_codec_rejections_for_test,
};

fn key(f: &Fixture, p: &PreparedDraftReplacementImportV1) -> DraftPieceSettlementKeyV1 {
    DraftPieceSettlementKeyV1::new(f.session.draft_id(), p.session_id(), p.operation_id())
}

#[test]
fn import_requires_explicit_header_and_closed_source_frontier_with_valid_cursor() {
    let f = Fixture::new(
        "import-codec-source",
        ComposerPayload::new(vec![ComposerAtom::text("exact\r\n🙂").unwrap()]).unwrap(),
        vec![],
        65_536,
    );
    let p = f.prepare();
    begin(&f, &p);
    let checkpoint = sealed_import_checkpoint_for_test(&f.store, &f.storage, key(&f, &p));
    for (name, rejected) in sealed_import_build_codec_rejections_for_test(&checkpoint.build) {
        assert!(rejected, "{name}");
    }
    assert!(f.head().active_operation().is_some());
    advance_all(&f, &p);
    start(&f, &p);
}

#[test]
fn import_baseline_settlement_and_cancellation_are_closed_and_consume_exact_bytes() {
    let f = Fixture::new(
        "import-codec-terminal",
        ComposerPayload::new(vec![ComposerAtom::text("copy\r\n🙂").unwrap()]).unwrap(),
        vec![],
        65_536,
    );
    let p = f.prepare();
    begin(&f, &p);
    advance_all(&f, &p);
    let started = start(&f, &p);
    let encoding = draft_canonical_encoding_for_test(&f.store, &f.storage, key(&f, &p), &f.head());
    assert_eq!(encoding.versions, [6, 6, 6, 1, 2]);
    for (family, payload) in encoding.payloads.iter().enumerate() {
        let payload = payload
            .as_ref()
            .expect("all committed import records exist");
        assert_eq!(
            draft_canonical_payload_reencode_for_test(family, payload),
            Some(payload.clone())
        );
        let mut trailing = payload.clone();
        trailing.push(0);
        assert!(
            draft_canonical_payload_reencode_for_test(family, &trailing).is_none(),
            "family {family} trailing bytes"
        );
        assert!(
            draft_canonical_payload_reencode_for_test(family, &payload[..payload.len() - 1])
                .is_none(),
            "family {family} truncation"
        );
    }
    let cancellation = cancel(&f, &f.head(), 73);
    for (name, rejected) in sealed_import_terminal_codec_rejections_for_test(
        started.settlement(),
        cancellation.receipt(),
    ) {
        assert!(rejected, "{name}");
    }
}
