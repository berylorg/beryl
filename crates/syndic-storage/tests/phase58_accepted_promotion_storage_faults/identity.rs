use super::*;

#[test]
fn every_successor_identity_namespace_collision_is_rejected_without_partial_promotion() {
    let fixture = promotion_fixture(91, id(91));
    let (_home, store, storage) = seed(
        "phase58-promotion-identity-collision-matrix",
        fixture.records.clone(),
    );
    let collision_thread = id(200);
    let mut create = HomeCommand::new(store.home_revision().unwrap());
    create
        .add(storage.create_thread(
            storage.revision(&store).unwrap(),
            CreateThread::ordinary(
                collision_thread,
                beryl_model::SyndicDraftId::from_bytes([201; 16]),
                crate::support::exact_cas::execution_binding(),
                timestamp(1),
                DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ))
        .unwrap();
    match store.execute(create) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!(
            "expected collision fixture creation to commit without later failure, got {outcome:?}"
        ),
    }
    let existing_item = SyndicItemId::from_bytes([203; 16]);
    support::exact_cas::submit_current_draft(
        &store,
        storage.clone(),
        collision_thread,
        beryl_model::SyndicDraftId::from_bytes([202; 16]),
        existing_item,
        "canonical collision owner",
        timestamp(10),
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    let parent = storage
        .thread(&store, fixture.thread, limit())
        .unwrap()
        .unwrap()
        .committed_tail()
        .unwrap();
    let turn_collisions = [
        (parent, SyndicItemId::from_bytes([130; 16]), "existing turn"),
        (
            SyndicTurnId::from_bytes(*fixture.current_draft.as_bytes()),
            SyndicItemId::from_bytes([131; 16]),
            "raw draft namespace",
        ),
        (
            SyndicTurnId::from_bytes(*fixture.accepted_input.as_bytes()),
            SyndicItemId::from_bytes([132; 16]),
            "raw accepted-input namespace",
        ),
    ];

    for (turn, item, namespace) in turn_collisions {
        let request = promotion(&store, &storage, turn, item);
        assert_eq!(
            storage
                .accepted_input_promotion_status(&store, &request, limit())
                .unwrap(),
            AcceptedInputPromotionStatus::Collision,
            "{namespace} collision must not classify as Prior",
        );
        let error = match execute_promotion(&store, &storage, request) {
            CommandOutcome::NotCommitted { evidence } => evidence,
            outcome => panic!("expected definitive identity collision, got {outcome:?}"),
        };
        assert!(
            matches!(
                mutation_error(&error),
                SyndicMutationError::AdmissionIdentityCollision
            ),
            "{namespace} collision returned {error}",
        );
    }

    let item_collision = promotion(
        &store,
        &storage,
        SyndicTurnId::from_bytes([135; 16]),
        existing_item,
    );
    assert_eq!(
        storage
            .accepted_input_promotion_status(&store, &item_collision, limit())
            .unwrap(),
        AcceptedInputPromotionStatus::Collision,
    );
    let error = match execute_promotion(&store, &storage, item_collision) {
        CommandOutcome::NotCommitted { evidence } => evidence,
        outcome => panic!("expected definitive item collision, got {outcome:?}"),
    };
    assert!(matches!(
        mutation_error(&error),
        SyndicMutationError::AdmissionIdentityCollision
    ));

    let fresh = promotion(
        &store,
        &storage,
        SyndicTurnId::from_bytes([133; 16]),
        SyndicItemId::from_bytes([134; 16]),
    );
    assert_eq!(
        storage
            .accepted_input_promotion_status(&store, &fresh, limit())
            .unwrap(),
        AcceptedInputPromotionStatus::Prior,
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();
}
