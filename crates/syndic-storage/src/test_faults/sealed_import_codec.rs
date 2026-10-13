use crate::{
    codec::{Family, parts::Encoder},
    draft_piece::*,
};

fn unique_offset(bytes: &[u8], needle: &[u8]) -> usize {
    let mut offsets = bytes
        .windows(needle.len())
        .enumerate()
        .filter_map(|(offset, window)| (window == needle).then_some(offset));
    let offset = offsets.next().expect("canonical fixture field exists");
    assert!(
        offsets.next().is_none(),
        "canonical fixture field is unique"
    );
    offset
}

fn rejects<F: Family>(bytes: &[u8]) -> bool {
    F::decode_value(bytes).is_err()
}

pub fn sealed_import_build_codec_rejections_for_test(
    build: &DraftPieceBuildRecordV1,
) -> Vec<(&'static str, bool)> {
    let bytes = DraftPieceBuildsFamily::encode_value(build).unwrap();
    assert_eq!(
        DraftPieceBuildsFamily::encode_value(
            &DraftPieceBuildsFamily::decode_value(&bytes).unwrap()
        )
        .unwrap(),
        bytes
    );
    let header = build.canonical_header();
    let header_start = unique_offset(&bytes, header);
    let domain = b"syndic/draft-sealed-composer-import/v1";
    let suffix = header_start + unique_offset(header, domain) - 5;
    assert_eq!(bytes[suffix], 1);
    let mut unknown_kind = bytes.clone();
    unknown_kind[suffix] = 2;
    let mut absent_kind = bytes.clone();
    absent_kind.drain(suffix..header_start + header.len());
    absent_kind[header_start - 4..header_start]
        .copy_from_slice(&u32::try_from(suffix - header_start).unwrap().to_be_bytes());
    let DraftPieceBuildFrontierV1::SealedImport(frontier) = build.frontier() else {
        panic!("fixture requires explicit sealed import")
    };
    let mut e = Encoder::new();
    e.u8(7);
    enc_import_frontier(&mut e, frontier);
    let encoded_frontier = e.finish();
    let frontier_start = unique_offset(&bytes, &encoded_frontier);
    let mut e = Encoder::new();
    enc_import_basis(&mut e, frontier.basis);
    let phase = frontier_start + 1 + e.finish().len();
    let decoder = phase + 1 + 4 * 8;
    let next_piece = decoder + 1 + 8;
    let mut unknown_frontier = bytes.clone();
    unknown_frontier[frontier_start] = 8;
    let mut unknown_phase = bytes.clone();
    unknown_phase[phase] = 3;
    let mut unknown_decoder = bytes.clone();
    unknown_decoder[decoder] = 4;
    let mut zero_piece = bytes.clone();
    zero_piece[next_piece..next_piece + 8].fill(0);
    vec![
        (
            "unknown header kind",
            rejects::<DraftPieceBuildsFamily>(&unknown_kind),
        ),
        (
            "absent import header kind",
            rejects::<DraftPieceBuildsFamily>(&absent_kind),
        ),
        (
            "unknown frontier kind",
            rejects::<DraftPieceBuildsFamily>(&unknown_frontier),
        ),
        (
            "unknown source phase",
            rejects::<DraftPieceBuildsFamily>(&unknown_phase),
        ),
        (
            "unknown source decoder",
            rejects::<DraftPieceBuildsFamily>(&unknown_decoder),
        ),
        (
            "zero source piece cursor",
            rejects::<DraftPieceBuildsFamily>(&zero_piece),
        ),
    ]
}

pub fn sealed_import_terminal_codec_rejections_for_test(
    settlement: &DraftPieceSettlementV1,
    cancellation: &DraftReplacementCancellationReceiptV1,
) -> Vec<(&'static str, bool)> {
    let bytes = DraftPieceSettlementsFamily::encode_value(settlement).unwrap();
    let DraftPieceSettlementClosureV1::SealedImport(selection) = settlement.closure() else {
        panic!("fixture requires sealed import closure")
    };
    let mut e = Encoder::new();
    enc_import_selection(&mut e, selection).unwrap();
    let closure = unique_offset(&bytes, &e.finish()) - 1;
    assert_eq!(bytes[closure], 2);
    let mut unknown_closure = bytes;
    unknown_closure[closure] = 3;
    let baseline = DraftEditHistoryFrontiersFamily::encode_value(selection.baseline()).unwrap();
    let mut baseline_key =
        DraftEditHistoryFrontiersFamily::encode_key(&selection.baseline().reference().key())
            .unwrap();
    assert_eq!(baseline[16], 3);
    assert_eq!(baseline_key[16], 3);
    let mut unknown_baseline = baseline;
    unknown_baseline[16] = 4;
    baseline_key[16] = 4;
    let record = DraftEditorCandidateSessionRecordV1::OpenReceipt(
        DraftEditorCandidateSessionOpenReceiptV1::from_replacement_cancellation(
            cancellation.clone(),
        ),
    );
    let mut cancel = DraftEditorCandidateSessionsFamily::encode_value(&record).unwrap();
    let mut cancel_key =
        DraftEditorCandidateSessionsFamily::encode_key(&cancellation.key()).unwrap();
    assert_eq!(cancel[0], 4);
    assert_eq!(cancel_key[32], 4);
    let canonical_cancel = DraftEditorCandidateSessionsFamily::encode_value(
        &DraftEditorCandidateSessionsFamily::decode_value(&cancel).unwrap(),
    )
    .unwrap()
        == cancel;
    let canonical_cancel_key = DraftEditorCandidateSessionsFamily::encode_key(
        &DraftEditorCandidateSessionsFamily::decode_key(&cancel_key).unwrap(),
    )
    .unwrap()
        == cancel_key;
    let mut trailing_cancel = cancel.clone();
    trailing_cancel.push(0);
    let truncated_cancel_rejected =
        rejects::<DraftEditorCandidateSessionsFamily>(&cancel[..cancel.len() - 1]);
    let mut trailing_cancel_key = cancel_key.clone();
    trailing_cancel_key.push(0);
    let truncated_cancel_key_rejected =
        DraftEditorCandidateSessionsFamily::decode_key(&cancel_key[..cancel_key.len() - 1])
            .is_err();
    cancel[0] = 5;
    cancel_key[32] = 5;
    vec![
        ("canonical cancellation value roundtrip", canonical_cancel),
        ("canonical cancellation key roundtrip", canonical_cancel_key),
        (
            "cancellation value trailing bytes",
            rejects::<DraftEditorCandidateSessionsFamily>(&trailing_cancel),
        ),
        ("cancellation value truncation", truncated_cancel_rejected),
        (
            "cancellation key trailing bytes",
            DraftEditorCandidateSessionsFamily::decode_key(&trailing_cancel_key).is_err(),
        ),
        ("cancellation key truncation", truncated_cancel_key_rejected),
        (
            "unknown import settlement closure",
            rejects::<DraftPieceSettlementsFamily>(&unknown_closure),
        ),
        (
            "unknown baseline history variant",
            rejects::<DraftEditHistoryFrontiersFamily>(&unknown_baseline),
        ),
        (
            "unknown baseline history key",
            DraftEditHistoryFrontiersFamily::decode_key(&baseline_key).is_err(),
        ),
        (
            "unknown cancellation record",
            rejects::<DraftEditorCandidateSessionsFamily>(&cancel),
        ),
        (
            "unknown cancellation key",
            DraftEditorCandidateSessionsFamily::decode_key(&cancel_key).is_err(),
        ),
    ]
}
