use super::{continuation_support::*, restart::observed_build, *};
use syndic_storage::test_faults::{
    draft_marker_program_snapshot_for_test, inject_coordinated_draft_marker_program_byte_for_test,
    inject_draft_marker_program_byte_for_test,
};

#[test]
fn pending_root_leaf_option_and_tag_substitutions_fail_closed_after_reopen() {
    for fault in 0..7 {
        let (home, store, storage, thread) = fixture("marker-program-corruption", 101 + fault);
        let session = open_session(
            &storage,
            &store,
            &current(&storage, &store, thread),
            111,
            112,
        );
        let (prepared, fragments) = stage_fragments(
            &storage,
            &store,
            &session,
            113,
            vec![insertion(point(0), marker(114, 7, 1))],
            DraftCompositePositionV1::new(0, DraftCompositeGapWitnessV1::BeforeAll),
        );
        let build = loop {
            let build = observed_build(&storage, &store, &prepared, &fragments);
            if draft_marker_program_snapshot_for_test(&build).is_some_and(|program| {
                program.pending == if (2..=4).contains(&fault) { 6 } else { 7 }
            }) {
                break build;
            }
            let advance = storage
                .prepare_draft_piece_build_advance(
                    &store,
                    session.draft_id(),
                    session.session_id(),
                    prepared.header().operation_id(),
                )
                .unwrap()
                .unwrap();
            committed(execute(&store, storage.advance_draft_piece_edit(advance)));
        };
        let program = draft_marker_program_snapshot_for_test(&build).unwrap();
        let (offset, value) = match fault {
            0 => (0, 2),
            1 => (program.pending_offset, 8),
            2 => (
                program.pending_offset + 2,
                program.bytes[program.pending_offset + 2] ^ 1,
            ),
            3 => (
                program.bytes.len() - 48,
                program.bytes[program.bytes.len() - 48] ^ 1,
            ),
            4 => (
                program.bytes.len() - 32,
                program.bytes[program.bytes.len() - 32] ^ 1,
            ),
            5 => (
                program.pending_offset + 1 + 122 + 1,
                program.bytes[program.pending_offset + 1 + 122 + 1] ^ 1,
            ),
            _ => (26, program.bytes[26] ^ 1),
        };
        if fault < 2 || fault == 6 {
            assert!(
                syndic_storage::test_faults::draft_marker_program_codec_rejects_for_test(
                    &build, offset, value
                )
            );
            inject_draft_marker_program_byte_for_test(
                &store,
                &storage,
                syndic_storage::DraftPieceSettlementKeyV1::new(
                    build.draft_id(),
                    build.session_id(),
                    build.operation_id(),
                ),
                offset,
                value,
            );
        } else {
            inject_coordinated_draft_marker_program_byte_for_test(
                &store,
                &storage,
                syndic_storage::DraftPieceSettlementKeyV1::new(
                    build.draft_id(),
                    build.session_id(),
                    build.operation_id(),
                ),
                offset,
                value,
            );
        }
        drop(store);
        let mut store =
            HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
        let storage = SyndicStorage::register(&mut store).unwrap();
        assert!(
            storage
                .prepare_draft_piece_build_advance(
                    &store,
                    session.draft_id(),
                    session.session_id(),
                    prepared.header().operation_id()
                )
                .is_err(),
            "fault {fault} was accepted"
        );
        assert!(
            storage
                .draft_piece_operation_status_page(&store, &prepared, 1, &fragments)
                .is_err(),
            "fault {fault} authenticated"
        );
    }
}

#[test]
fn cancelling_each_partial_insert_boundary_preserves_the_candidate_and_terminal_custody() {
    for target in [1, 5, 6, 7, 0] {
        let (home, store, storage, thread) = fixture("marker-program-cancel", 121 + target);
        let session = open_session(
            &storage,
            &store,
            &current(&storage, &store, thread),
            131,
            132,
        );
        let (prepared, fragments) = stage_fragments(
            &storage,
            &store,
            &session,
            133,
            vec![insertion(point(0), marker(134, 7, 1))],
            DraftCompositePositionV1::new(0, DraftCompositeGapWitnessV1::BeforeAll),
        );
        loop {
            let build = observed_build(&storage, &store, &prepared, &fragments);
            if draft_marker_program_snapshot_for_test(&build).is_some_and(|program| {
                program.pending == target && (target != 0 || program.phase == 3)
            }) {
                break;
            }
            let advance = storage
                .prepare_draft_piece_build_advance(
                    &store,
                    session.draft_id(),
                    session.session_id(),
                    prepared.header().operation_id(),
                )
                .unwrap()
                .unwrap();
            committed(execute(&store, storage.advance_draft_piece_edit(advance)));
        }
        committed(execute(
            &store,
            storage.cancel_draft_piece_edit(storage.revision(&store).unwrap(), prepared.clone()),
        ));
        drop(store);
        let mut store =
            HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
        let storage = SyndicStorage::register(&mut store).unwrap();
        assert_eq!(
            active_session(&storage, &store, session.draft_id(), session.session_id())
                .newest_root(),
            session.newest_root()
        );
        assert!(
            matches!(storage.draft_piece_operation_status_page(&store, &prepared, 1, &fragments).unwrap(),
            DraftPieceOperationVerificationV1::Status(DraftPieceOperationStatusV1::Settled(settlement))
                if settlement.outcome() == &syndic_storage::DraftPieceSettlementOutcomeV1::Cancelled)
        );
        assert!(matches!(
            storage.prepare_draft_piece_build_advance(
                &store,
                session.draft_id(),
                session.session_id(),
                prepared.header().operation_id()
            ),
            Ok(None) | Err(DraftPiecePrepareErrorV1::InvalidRoot)
        ));
        assert_eq!(
            current(&storage, &store, thread)
                .draft()
                .piece_root()
                .summary()
                .marker_count(),
            0
        );
    }
}
