use crate::{
    SyndicPointReadLimit, SyndicStorage,
    codec::{ExactCodec, Family},
    domain::SyndicDomain,
    draft_piece::*,
};
use beryl_home_store::{HomeStore, RecordCodec};

mod endpoint;
pub use endpoint::*;
mod records;
use records::write_endpoint;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftMarkerProgramSnapshotForTest {
    pub phase: u8,
    pub pending: u8,
    pub proof: Option<(u8, u8)>,
    pub bytes: Vec<u8>,
    pub pending_offset: usize,
}

pub fn draft_marker_program_snapshot_for_test(
    build: &DraftPieceBuildRecordV1,
) -> Option<DraftMarkerProgramSnapshotForTest> {
    let active = build.marker_effect_continuation().active()?;
    let bytes = canonical_marker_program_bytes(active);
    let mut pending_offset = 1 + usize::from(active.removal_site().is_some()) * 16;
    pending_offset += 1;
    if let Some(planning) = active.planning() {
        pending_offset += 2
            + usize::from(planning.source_boundary.is_some()) * 16
            + usize::from(planning.previous_start.is_some()) * 16;
    }
    pending_offset += 1 + usize::from(active.insertion_site().is_some()) * 24;
    let proof = match active.pending() {
        DraftPieceMarkerPendingV1::Proof {
            purpose, component, ..
        } => Some((purpose as u8, component as u8)),
        _ => None,
    };
    Some(DraftMarkerProgramSnapshotForTest {
        phase: match active.phase() {
            DraftPieceActiveMarkerPhaseV1::Removing => 0,
            DraftPieceActiveMarkerPhaseV1::DerivingInsertionGap => 1,
            DraftPieceActiveMarkerPhaseV1::Inserting => 2,
            DraftPieceActiveMarkerPhaseV1::Publishing => 3,
        },
        pending: bytes[pending_offset],
        proof,
        bytes,
        pending_offset,
    })
}

pub fn draft_marker_program_codec_rejects_for_test(
    build: &DraftPieceBuildRecordV1,
    offset: usize,
    replacement: u8,
) -> bool {
    let mut encoded = DraftPieceBuildsFamily::encode_value(build).unwrap();
    let program = draft_marker_program_snapshot_for_test(build).unwrap();
    let start = program_start(&encoded, build, &program.bytes);
    encoded[start + offset] = replacement;
    DraftPieceBuildsFamily::decode_value(&encoded).is_err()
}

pub fn draft_marker_program_roundtrip_for_test(build: &DraftPieceBuildRecordV1) -> bool {
    let bytes = DraftPieceBuildsFamily::encode_value(build).unwrap();
    let decoded = DraftPieceBuildsFamily::decode_value(&bytes).unwrap();
    DraftPieceBuildsFamily::encode_value(&decoded).unwrap() == bytes
        && (0..bytes.len()).all(|len| DraftPieceBuildsFamily::decode_value(&bytes[..len]).is_err())
        && {
            let mut trailing = bytes;
            trailing.push(0);
            DraftPieceBuildsFamily::decode_value(&trailing).is_err()
        }
}

pub fn inject_draft_marker_program_byte_for_test(
    store: &HomeStore,
    storage: &SyndicStorage,
    key: DraftPieceSettlementKeyV1,
    offset: usize,
    replacement: u8,
) {
    let build = storage
        .point::<DraftPieceBuildsFamily>(
            store,
            key,
            SyndicPointReadLimit::new(DraftPieceBuildsFamily::MAX_VALUE_BYTES).unwrap(),
        )
        .unwrap()
        .unwrap();
    let program = draft_marker_program_snapshot_for_test(&build).unwrap();
    let mut encoded =
        <ExactCodec<DraftPieceBuildsFamily> as RecordCodec<SyndicDomain>>::encode_value(&build)
            .unwrap();
    let start = program_start(&encoded, &build, &program.bytes);
    encoded[start + offset] = replacement;
    let mut stored = DraftPieceBuildsFamily::RECORD_VERSION
        .get()
        .to_be_bytes()
        .to_vec();
    stored.extend_from_slice(&encoded);
    let key = <ExactCodec<DraftPieceBuildsFamily> as RecordCodec<SyndicDomain>>::encode_key(&key)
        .unwrap();
    store
        .inject_persisted_corrupt_record::<SyndicDomain, ExactCodec<DraftPieceBuildsFamily>>(
            &storage.handle,
            &key,
            &stored,
        )
        .unwrap();
}

fn program_start(encoded: &[u8], build: &DraftPieceBuildRecordV1, program: &[u8]) -> usize {
    let active =
        canonical_active_marker_bytes(build.marker_effect_continuation().active().unwrap());
    let starts: Vec<_> = encoded
        .windows(active.len())
        .enumerate()
        .filter_map(|(index, candidate)| (candidate == active).then_some(index))
        .collect();
    assert_eq!(
        starts.len(),
        1,
        "fixture program must have a unique encoded location"
    );
    starts[0] + active.len() - program.len()
}

pub fn inject_coordinated_draft_marker_program_byte_for_test(
    store: &HomeStore,
    storage: &SyndicStorage,
    key: DraftPieceSettlementKeyV1,
    offset: usize,
    replacement: u8,
) {
    let build = storage
        .point::<DraftPieceBuildsFamily>(
            store,
            key,
            SyndicPointReadLimit::new(DraftPieceBuildsFamily::MAX_VALUE_BYTES).unwrap(),
        )
        .unwrap()
        .unwrap();
    let receipt = storage
        .point::<DraftPieceBuildProgressFamily>(
            store,
            build.progress_receipt().key(),
            SyndicPointReadLimit::new(DraftPieceBuildProgressFamily::MAX_VALUE_BYTES).unwrap(),
        )
        .unwrap()
        .unwrap();
    let prior_key = receipt.previous().unwrap().key();
    let prior = storage
        .point::<DraftPieceBuildProgressFamily>(
            store,
            prior_key,
            SyndicPointReadLimit::new(DraftPieceBuildProgressFamily::MAX_VALUE_BYTES).unwrap(),
        )
        .unwrap()
        .unwrap();
    let program = draft_marker_program_snapshot_for_test(&build).unwrap();
    let mut encoded = DraftPieceBuildsFamily::encode_value(&build).unwrap();
    let start = program_start(&encoded, &build, &program.bytes);
    encoded[start + offset] = replacement;
    let substituted = DraftPieceBuildsFamily::decode_value(&encoded)
        .expect("coordinated substitution must satisfy local byte grammar");
    install_coordinated_program(store, storage, key, substituted, receipt, prior);
}

fn install_coordinated_program(
    store: &HomeStore,
    storage: &SyndicStorage,
    key: DraftPieceSettlementKeyV1,
    substituted: DraftPieceBuildRecordV1,
    receipt: DraftPieceBuildProgressReceiptV1,
    prior: DraftPieceBuildProgressReceiptV1,
) {
    let prior_key = prior.key();
    let (substituted, replacement_receipt) = authenticated_build_transition(
        substituted,
        receipt.previous(),
        receipt.fragment_endpoint(),
    )
    .unwrap();
    assert_eq!(replacement_receipt.key(), receipt.key());
    assert!(progress_receipt_is_exact(&replacement_receipt));
    assert!(progress_receipt_matches_build(
        &replacement_receipt,
        &substituted
    ));
    let session_key = DraftEditorCandidateSessionRecordKeyV1::head(
        substituted.draft_id(),
        substituted.session_id(),
    );
    let DraftEditorCandidateSessionRecordV1::Head(session) = storage
        .point::<DraftEditorCandidateSessionsFamily>(
            store,
            session_key,
            SyndicPointReadLimit::new(DraftEditorCandidateSessionsFamily::MAX_VALUE_BYTES).unwrap(),
        )
        .unwrap()
        .unwrap()
    else {
        panic!("fixture requires session head");
    };
    let custody = session.active_operation().copied().unwrap();
    let session = DraftEditorCandidateSessionV1::from_parts(
        session.thread_id(),
        session.draft_id(),
        session.session_id(),
        session.open_operation_id(),
        session.session_generation(),
        session.durable_base_selector_revision(),
        session.durable_base_root(),
        session.durable_base_history(),
        session.published_candidate_generation(),
        session.published_selector_revision(),
        session.published_root(),
        session.published_history(),
        session.newest_candidate_generation(),
        session.newest_root(),
        session.newest_history(),
        session.dirty_generation(),
        session.logical_extent(),
        session.lifecycle(),
        Some(DraftEditorActiveOperationV1::building(
            custody.operation_id(),
            custody.proposal_digest().unwrap(),
            custody.predecessor_candidate_generation(),
            custody.predecessor_root(),
            custody.predecessor_history(),
            substituted.progress_receipt(),
        )),
    );
    assert_eq!(
        key,
        DraftPieceSettlementKeyV1::new(
            substituted.draft_id(),
            substituted.session_id(),
            substituted.operation_id()
        )
    );
    write_endpoint(
        store,
        storage,
        substituted,
        Some(replacement_receipt),
        None,
        DraftEditorCandidateSessionRecordV1::Head(session),
    );
    let unchanged = storage
        .point::<DraftPieceBuildProgressFamily>(
            store,
            prior_key,
            SyndicPointReadLimit::new(DraftPieceBuildProgressFamily::MAX_VALUE_BYTES).unwrap(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(unchanged, prior);
}
