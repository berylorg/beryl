use crate::codec::Family;
use crate::draft_piece::{
    DraftEditorCandidateSessionDisposeReceiptV1, DraftEditorCandidateSessionOpenReceiptV1,
    DraftEditorCandidateSessionRecordV1, DraftEditorCandidateSessionsFamily,
    DraftPieceSettlementsFamily,
};
use crate::{ComposerAcceptedInputReferenceV1, DraftPieceSettlementV1, SyndicStorage};
use beryl_home_store::HomeStore;

pub fn composer_recall_record_sizes(
    store: &HomeStore,
    storage: &SyndicStorage,
    key: crate::DraftPieceSettlementKeyV1,
) -> (usize, usize) {
    use crate::draft_piece::{DraftPieceBuildProgressFamily, DraftPieceBuildsFamily};
    let limit = crate::SyndicPointReadLimit::new(75_000).unwrap();
    let build = storage
        .point::<DraftPieceBuildsFamily>(store, key, limit)
        .unwrap()
        .unwrap();
    let progress = storage
        .point::<DraftPieceBuildProgressFamily>(store, build.progress_receipt().key(), limit)
        .unwrap()
        .unwrap();
    let build_bytes = DraftPieceBuildsFamily::encode_value(&build).unwrap();
    let progress_bytes = DraftPieceBuildProgressFamily::encode_value(&progress).unwrap();
    assert!(build_bytes.len() <= DraftPieceBuildsFamily::MAX_VALUE_BYTES);
    assert!(progress_bytes.len() <= DraftPieceBuildProgressFamily::MAX_VALUE_BYTES);
    (build_bytes.len(), progress_bytes.len())
}

pub fn composer_recall_descriptor_substitutions(
    prepared: &crate::PreparedDraftComposerRecallV1,
) -> Vec<(&'static str, crate::PreparedDraftComposerRecallV1)> {
    use crate::draft_piece::recall::{
        DraftComposerRootReplacementPurposeV1, canonical_recall_header,
    };
    let mut output = Vec::new();
    for (case, name) in [
        "source key",
        "acceptance digest",
        "source thread",
        "source content",
        "source assets",
    ]
    .into_iter()
    .enumerate()
    {
        let mut p = prepared.clone();
        match case {
            0 => {
                p.basis.key = crate::DraftEditorCandidateSessionRecordKeyV1::disposal_receipt(
                    p.basis.key.draft_id(),
                    p.basis.key.session_id(),
                    crate::DraftPieceOperationIdV1::from_bytes([245; 16]),
                )
            }
            1 => p.basis.acceptance_digest[0] ^= 1,
            2 => p.basis.thread = beryl_model::SyndicThreadId::from_bytes([245; 16]),
            3 => {
                p.basis.content = crate::ContentReference::new(
                    beryl_model::SyndicContentId::from_bytes([245; 16]),
                    p.basis.content.revision(),
                    p.basis.content.encoding(),
                    p.basis.content.summary(),
                )
            }
            4 => {
                p.basis.assets = Some(
                    beryl_model::SealedAssetReferenceSetProof::new(
                        beryl_model::AssetReferenceSetId::from_bytes([245; 16]),
                        beryl_model::SequentialMarkerSummaryV1::new(
                            beryl_model::sequential_marker_digest_seed(),
                            0,
                            None,
                        )
                        .unwrap(),
                        beryl_model::OrderedMarkerAssetSummaryV1::new(
                            beryl_model::ordered_marker_asset_digest_seed(),
                            0,
                        ),
                        0,
                        beryl_model::AssetReferenceSetDigest::from_bytes([245; 32]),
                    )
                    .unwrap(),
                )
            }
            _ => unreachable!(),
        }
        p.canonical_header = canonical_recall_header(
            p.header,
            p.before.session_generation(),
            &DraftComposerRootReplacementPurposeV1::AcceptedRecall(p.basis.clone()),
        );
        output.push((name, p));
    }
    output
}

pub fn composer_acceptance_disposal_record_bytes(
    store: &HomeStore,
    storage: &SyndicStorage,
    source: &ComposerAcceptedInputReferenceV1,
) -> Vec<u8> {
    let record = storage
        .point::<DraftEditorCandidateSessionsFamily>(
            store,
            source.key(),
            crate::SyndicPointReadLimit::new(75_000).unwrap(),
        )
        .unwrap()
        .unwrap();
    DraftEditorCandidateSessionsFamily::encode_value(&record).unwrap()
}
pub fn composer_session_record_codec_roundtrip(bytes: &[u8]) -> Option<Vec<u8>> {
    let record = DraftEditorCandidateSessionsFamily::decode_value(bytes).ok()?;
    DraftEditorCandidateSessionsFamily::encode_value(&record).ok()
}
pub fn composer_ordinary_disposal_record_bytes(accepted: &[u8]) -> Vec<u8> {
    let record = DraftEditorCandidateSessionsFamily::decode_value(accepted).unwrap();
    let DraftEditorCandidateSessionRecordV1::OpenReceipt(receipt) = record else {
        panic!("expected receipt");
    };
    let disposal = receipt.disposal().unwrap();
    let ordinary = DraftEditorCandidateSessionDisposeReceiptV1::new(
        disposal.request_bytes().to_vec(),
        disposal.before_head().clone(),
        disposal.after_head().clone(),
        disposal.frontier().clone(),
    );
    DraftEditorCandidateSessionsFamily::encode_value(
        &DraftEditorCandidateSessionRecordV1::OpenReceipt(
            DraftEditorCandidateSessionOpenReceiptV1::from_disposal(ordinary),
        ),
    )
    .unwrap()
}
pub fn composer_root_replacement_settlement_bytes(settlement: &DraftPieceSettlementV1) -> Vec<u8> {
    DraftPieceSettlementsFamily::encode_value(settlement).unwrap()
}
pub fn composer_root_replacement_settlement_roundtrip(bytes: &[u8]) -> Option<Vec<u8>> {
    let settlement = DraftPieceSettlementsFamily::decode_value(bytes).ok()?;
    DraftPieceSettlementsFamily::encode_value(&settlement).ok()
}

pub fn composer_root_replacement_outer_substitutions(
    settlement: &DraftPieceSettlementV1,
) -> Vec<(&'static str, Vec<u8>)> {
    use crate::draft_piece::{
        DraftCompositeGapWitnessV1, DraftCompositePositionV1, DraftPieceBuildProgressReceiptKeyV1,
        DraftPieceBuildProgressReceiptReferenceV1, DraftPieceDigestV1, DraftPieceOperationIdV1,
        DraftPieceSettlementKeyV1,
    };
    let mut output = Vec::new();
    for (case, name) in [
        "operation identity",
        "predecessor generation",
        "before caret",
        "after caret",
        "build digest",
        "terminal receipt key",
    ]
    .into_iter()
    .enumerate()
    {
        let alternate = DraftPieceOperationIdV1::from_bytes([247; 16]);
        let key = if case == 0 {
            DraftPieceSettlementKeyV1::new(
                settlement.key().draft_id(),
                settlement.key().session_id(),
                alternate,
            )
        } else {
            settlement.key()
        };
        let receipt = if case == 5 {
            settlement.terminal_progress_receipt().map(|r| {
                DraftPieceBuildProgressReceiptReferenceV1::new(
                    DraftPieceBuildProgressReceiptKeyV1::new(
                        r.key().draft_id(),
                        r.key().session_id(),
                        alternate,
                        r.key().transition_ordinal(),
                    ),
                    r.digest(),
                )
            })
        } else {
            settlement.terminal_progress_receipt()
        };
        if case == 5 && receipt.is_none() {
            continue;
        }
        let changed = DraftCompositePositionV1::new(
            settlement
                .predecessor_caret()
                .utf8_offset()
                .checked_add(1)
                .unwrap(),
            DraftCompositeGapWitnessV1::Unambiguous,
        );
        let corrupted = DraftPieceSettlementV1::new_boxed(
            key,
            settlement.proposal_digest(),
            settlement.predecessor_candidate_generation() + u64::from(case == 1),
            settlement.predecessor_root(),
            settlement.predecessor_history(),
            settlement.fragment_count(),
            settlement.fragment_chain(),
            if case == 2 {
                changed
            } else {
                settlement.predecessor_caret()
            },
            settlement.predecessor_selection(),
            if case == 3 {
                changed
            } else {
                settlement.caret()
            },
            settlement.selection(),
            if case == 4 {
                Some(DraftPieceDigestV1::from_bytes([248; 32]))
            } else {
                settlement.build_digest()
            },
            settlement.canonical_header().to_vec(),
            settlement.terminal_source().cloned(),
            receipt,
            settlement.outcome().clone(),
            Box::new(settlement.closure().clone()),
        );
        output.push((
            name,
            DraftPieceSettlementsFamily::encode_value(&corrupted).unwrap(),
        ));
    }
    output
}
