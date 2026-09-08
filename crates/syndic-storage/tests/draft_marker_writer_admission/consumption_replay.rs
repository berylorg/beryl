use super::*;

pub(super) fn consume_target_with_stale_advances(
    storage: &SyndicStorage,
    store: &HomeStore,
    admission: DraftMarkerAdmissionOwnerV1,
    identity: DraftMutationStagingIdentityV1,
) {
    loop {
        let advance = storage
            .prepare_draft_piece_build_advance(
                store,
                identity.draft_id(),
                identity.session_id(),
                identity.operation_id().as_piece_operation(),
            )
            .unwrap()
            .expect("unfinished admitted build produces a quantum");
        let replay = advance.clone();
        committed(execute(store, storage.advance_draft_piece_edit(advance)));
        let once = snapshot(storage, store, admission);
        let once_head = once.head().unwrap();
        let consumed = once_head.target_root().count() == 0;
        let digest = once_head.digest();
        let capacity = once.capacity().unwrap().digest();
        let replay_outcome = execute(store, storage.advance_draft_piece_edit(replay));
        assert!(matches!(
            replay_outcome,
            CommandOutcome::NotCommitted {
                evidence: CommandError::Conflict { .. }
            }
        ));
        let replayed = snapshot(storage, store, admission);
        assert_eq!(replayed.head().unwrap().digest(), digest);
        assert_eq!(replayed.capacity().unwrap().digest(), capacity);
        if consumed {
            break;
        }
    }
}
