use super::*;

pub(super) fn assert_verification(work: syndic_storage::StagedDraftPieceVerificationWorkV1) {
    assert_eq!(
        work.charged_encoded_value_bytes,
        work.attempted_reads * 65_536
    );
    assert!(work.attempted_reads <= 128, "{work:?}");
    assert!(work.charged_encoded_value_bytes <= 8_388_608, "{work:?}");
}

pub(super) fn complete(
    mut flight: StagedDraftPieceOutcomeFlightV1,
    store: &HomeStore,
) -> StagedDraftPieceCommandCompletionV1 {
    for _ in 0..32 {
        assert_verification(flight.verification_work());
        match flight.into_completion() {
            Ok(completion) => {
                assert_verification(completion.verification);
                return completion;
            }
            Err(pending) => {
                assert!(
                    !matches!(pending.state(), State::Unavailable | State::NotCommitted),
                    "{pending:?}"
                );
                flight = pending.resume(store);
            }
        }
    }
    panic!("mapping command did not complete: {flight:?}");
}

pub(super) fn stage_text(
    storage: &SyndicStorage,
    store: &HomeStore,
    session: &DraftEditorCandidateSessionV1,
    operation: u8,
    length: usize,
) -> DraftMutationStagingIdentityV1 {
    let (_, identity, _) = stage_replacement(
        storage,
        store,
        session,
        operation,
        DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("x".repeat(length))],
        ),
        DraftLogicalExtentV1::new(length as u64, 1),
    );
    identity
}

pub(super) fn advance_until(
    storage: &SyndicStorage,
    store: &HomeStore,
    identity: DraftMutationStagingIdentityV1,
    mut predicate: impl FnMut(&DraftPieceBuildRecordV1, DraftBuildMappingSnapshotForTest) -> bool,
) -> DraftPieceBuildRecordV1 {
    for _ in 0..512 {
        let build = staged_outcome_build_for_test(storage, store, identity);
        let mapping = draft_build_mapping_snapshot(&build).unwrap();
        if predicate(&build, mapping) {
            return build;
        }
        let command = storage
            .prepare_staged_draft_piece_advance(store, identity, build.progress_receipt())
            .unwrap_or_else(|error| {
                panic!(
                    "mapping stage {mapping:?}, frontier {:?}: {error:?}",
                    build.frontier()
                )
            })
            .expect("mapping stage was not reached before completion");
        let completion = complete(command.submit(store), store);
        assert!(matches!(
            completion.result,
            DraftPieceReconciledCommandV1::Pending(_)
        ));
    }
    panic!("mapping stage was not reached within the fixture command bound");
}

pub(super) fn advance_once(
    storage: &SyndicStorage,
    store: &HomeStore,
    identity: DraftMutationStagingIdentityV1,
    build: &DraftPieceBuildRecordV1,
) -> StagedDraftPieceCommandCompletionV1 {
    let command = storage
        .prepare_staged_draft_piece_advance(store, identity, build.progress_receipt())
        .unwrap()
        .unwrap();
    complete(command.submit(store), store)
}

pub(super) fn assert_unadopted(
    storage: &SyndicStorage,
    store: &HomeStore,
    session: &DraftEditorCandidateSessionV1,
) {
    assert_eq!(
        syndic_storage::test_faults::draft_build_mapping_candidate_pair_for_test(
            storage, store, session
        ),
        (session.newest_root(), session.newest_history()),
    );
}
