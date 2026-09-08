use crate::draft_piece::build_mapping::{codec, fixture::BuildMappingForTest, model::MapRoot};
use crate::{SyndicStorage, draft_piece::*};
use beryl_home_store::HomeStore;

pub fn seed_fragmented_copy_alignment_for_test(
    storage: &SyndicStorage,
    store: &HomeStore,
    source: &DraftPieceBuildRecordV1,
) -> DraftPieceBuildRecordV1 {
    let mapping = source.mapping().unwrap();
    assert!(matches!(
        mapping.mapping_stage,
        DraftPieceMappingStageV1::TextMapStart { .. }
    ));
    assert_eq!(mapping.current_map, MapRoot::Identity(512));
    assert_eq!(
        source
            .working_roots()
            .sequence_summary()
            .logical_utf8_bytes(),
        512
    );
    assert_eq!(source.working_roots().sequence_summary().piece_count(), 1);
    assert_eq!(source.next_record_ordinal(), 1);
    assert!(source.marker_effect_continuation().active().is_none());
    let selected = storage
        .point::<DraftPieceBuildProgressFamily>(
            store,
            source.progress_receipt().key(),
            point_limit(),
        )
        .unwrap()
        .unwrap();
    let prior = storage
        .point::<DraftPieceBuildProgressFamily>(
            store,
            selected.previous().unwrap().key(),
            point_limit(),
        )
        .unwrap()
        .unwrap();
    assert!(matches!(
        prior.mapping().unwrap().mapping_stage,
        DraftPieceMappingStageV1::TextSourceEnd { .. }
    ));
    let mut owner = [0u8; 48];
    owner[..16].copy_from_slice(source.draft_id().as_bytes());
    owner[16..32].copy_from_slice(source.session_id().as_bytes());
    owner[32..].copy_from_slice(source.operation_id().as_bytes());
    let mut seeded = BuildMappingForTest::new(512);
    seeded.set_owner(owner);
    seeded.set_ordinal(1);
    seeded.seed_runs(storage, store, &[(0, 8); 64]);
    assert_eq!(seeded.height(), 2);
    assert_eq!(seeded.totals(), (512, 512));
    let current_map = codec::decode_root(&seeded.root_bytes()).unwrap();
    let (_, next_prior) = authenticated_build_transition(
        with_endpoint(source, &prior, current_map),
        prior.previous(),
        prior.fragment_endpoint(),
    )
    .unwrap();
    let (next, receipt) = authenticated_build_transition(
        with_endpoint(source, &selected, current_map),
        Some(next_prior.reference()),
        selected.fragment_endpoint(),
    )
    .unwrap();
    assert_eq!(next_prior.key(), prior.key());
    assert_eq!(receipt.key(), selected.key());
    let DraftPieceBuildFrontierV1::Planning { fragment_ordinal } = source.frontier() else {
        panic!("planning");
    };
    let fragment_key = DraftPieceBuildFragmentKeyV1::new(
        source.draft_id(),
        source.session_id(),
        source.operation_id(),
        fragment_ordinal,
    );
    let fragment = storage
        .point::<DraftPieceBuildFragmentsFamily>(store, fragment_key, point_limit())
        .unwrap()
        .unwrap();
    assert!(progress_receipt_is_exact(&next_prior));
    assert!(progress_receipt_is_exact(&receipt));
    assert!(progress_receipt_matches_build(&receipt, &next));
    assert!(marker_effect_progress_transition_is_exact(
        &next_prior,
        &receipt,
        Some(&fragment)
    ));
    let key = DraftPieceSettlementKeyV1::new(
        source.draft_id(),
        source.session_id(),
        source.operation_id(),
    );
    let session_key =
        DraftEditorCandidateSessionRecordKeyV1::head(source.draft_id(), source.session_id());
    let DraftEditorCandidateSessionRecordV1::Head(session) = storage
        .point::<DraftEditorCandidateSessionsFamily>(store, session_key, point_limit())
        .unwrap()
        .unwrap()
    else {
        panic!("head");
    };
    let custody = session.active_operation().copied().unwrap();
    let target = DraftEditorActiveOperationV1::building(
        custody.operation_id(),
        custody.proposal_digest().unwrap(),
        custody.predecessor_candidate_generation(),
        custody.predecessor_root(),
        custody.predecessor_history(),
        next.progress_receipt(),
    );
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
        Some(target),
    );
    super::put_mapping_fixture_record::<DraftPieceBuildProgressFamily>(
        storage,
        store,
        &next_prior.key(),
        &next_prior,
    );
    super::put_mapping_fixture_record::<DraftPieceBuildProgressFamily>(
        storage,
        store,
        &receipt.key(),
        &receipt,
    );
    super::put_mapping_fixture_record::<DraftPieceBuildsFamily>(storage, store, &key, &next);
    super::put_mapping_fixture_record::<DraftEditorCandidateSessionsFamily>(
        storage,
        store,
        &session_key,
        &DraftEditorCandidateSessionRecordV1::Head(session),
    );
    next
}

fn with_endpoint(
    source: &DraftPieceBuildRecordV1,
    endpoint: &DraftPieceBuildProgressReceiptV1,
    root: MapRoot,
) -> DraftPieceBuildRecordV1 {
    let mut mapping = endpoint.mapping().unwrap();
    mapping.current_map = root;
    DraftPieceBuildRecordV1::new(
        source.draft_id(),
        source.session_id(),
        source.predecessor_candidate_generation(),
        source.predecessor_root(),
        source.predecessor_history(),
        source.operation_id(),
        source.predecessor_caret(),
        source.predecessor_selection(),
        source.caret(),
        source.selection(),
        source.fragment_count(),
        source.fragment_chain(),
        source.canonical_header().to_vec(),
        source.staged_fragment_count(),
        source.staged_fragment_chain(),
        source.proposal_digest(),
        endpoint.working_roots(),
        endpoint.base_frontier(),
        endpoint.successor_frontier(),
        6,
        endpoint.frontier(),
        endpoint.reference(),
        endpoint.successor(),
        endpoint.build_digest(),
        endpoint.lifecycle(),
    )
    .with_durable_continuation(endpoint.durable_continuation())
    .with_marker_effect_continuation(endpoint.marker_effect_continuation())
    .with_mapping(Some(mapping))
    .with_writer_admission(endpoint.writer_admission())
}
