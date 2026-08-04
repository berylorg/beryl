use super::*;

#[test]
fn populated_ordered_pages_preserve_cursor_continuation_and_index_getters() {
    let home = TestHome::new("populated-ordered-reads");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    commit(&store, storage, batch(populated_records()));
    let one = CursorReadLimits::new(1, 65_536).unwrap();

    let accepted = storage.accepted_order(&store, id(40), None, one).unwrap();
    assert_eq!(accepted.records()[0].input_id(), steering_input());
    assert!(accepted.has_more());
    assert!(accepted.stored_bytes() > 0);
    let accepted_tail = storage
        .accepted_order(&store, id(40), Some(accepted.records()[0].ordinal()), one)
        .unwrap();
    assert_eq!(accepted_tail.records()[0].input_id(), next_input());
    assert!(!accepted_tail.has_more());

    let routes = storage
        .accepted_route_page(
            &store,
            id(40),
            AcceptedRouteGeneration::FIRST,
            AcceptedRouteRevision::new(2).unwrap(),
            None,
        )
        .unwrap();
    assert_eq!(routes.records()[1].input().id(), next_input());
    assert!(matches!(
        routes.records()[1].effective_state(),
        AcceptedRouteEffectiveState::NextTurn(_)
    ));
    let events = storage
        .source_events(&store, active_turn(), None, one)
        .unwrap();
    assert_eq!(events.records()[0].sequence(), SourceEventSequence::FIRST);
    let items = storage
        .turn_items(&store, source_turn(), None, one)
        .unwrap();
    assert_eq!(items.records()[0].item_id(), source_item());
    let transcript = storage
        .transcript_entries(&store, id(30), TranscriptGeneration::FIRST, None, one)
        .unwrap();
    assert_eq!(transcript.records()[0].projection_id(), source_projection());
    let projections = storage
        .item_projections(
            &store,
            source_item(),
            ItemProjectionGeneration::FIRST,
            None,
            one,
        )
        .unwrap();
    assert_eq!(
        projections.records()[0].projection_id(),
        source_projection()
    );
    let resources = storage
        .projection_resources(&store, source_resource_projection(), None, one)
        .unwrap();
    assert_eq!(resources.records()[0].resource_id(), source_resource());

    let binding_one = BindingRevision::new(1).unwrap();
    let history = storage.binding_history(&store, id(40), None, one).unwrap();
    assert_eq!(history.records()[0].revision(), binding_one);
    assert!(history.has_more());
    let history_tail = storage
        .binding_history(&store, id(40), Some(binding_one), one)
        .unwrap();
    assert_eq!(history_tail.records()[0].revision().get(), 2);
    assert!(history_tail.has_more());
    let history_end = storage
        .binding_history(&store, id(40), Some(BindingRevision::new(2).unwrap()), one)
        .unwrap();
    assert_eq!(history_end.records()[0].revision().get(), 3);
    assert!(!history_end.has_more());

    let children = storage
        .turn_children(&store, SyndicTurnId::from_bytes([29; 16]), None, one)
        .unwrap();
    assert_eq!(children.records()[0].child_id(), source_turn());
    store.close().unwrap();
}

#[test]
fn successful_recovery_requires_old_handle_reacquisition() {
    let home = TestHome::new("reacquire");
    let faults = FaultController::new();
    let mut store = HomeStore::open_with_faults(
        HomeOpenOptions::new(home.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let old = SyndicStorage::register(&mut store).unwrap();
    commit(&store, old, batch(empty_thread_records(id(1), draft_id(2))));

    let replacement = HistorySummaryRecord::new(
        id(1),
        beryl_model::ProjectionRevision::new(1).unwrap(),
        ThreadRevision::new(1).unwrap(),
        None,
        syndic_storage::empty_selected_path_digest(),
        true,
        timestamp(1),
    );
    let mut fixture = FixtureBatch::new();
    fixture
        .put(FixtureRecord::HistorySummary(replacement))
        .unwrap();
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(old.fixture_contribution(old.revision(&store).unwrap(), fixture))
        .unwrap();
    faults.fail_next(FaultPoint::BeforeCommit);
    assert!(store.execute(command).is_err());
    assert_eq!(store.health().state(), HomeHealthState::Verifying);

    faults.fail_next(FaultPoint::BeforeVerification);
    assert!(store.verify_health().is_err());
    assert_eq!(store.health().state(), HomeHealthState::Failed);
    store.recover_same_home().unwrap();
    assert_eq!(store.health().state(), HomeHealthState::Healthy);

    assert!(matches!(
        old.thread(&store, id(1), SyndicPointReadLimit::new(1_024).unwrap()),
        Err(SyndicReadError::Read(ReadError::ForeignDomain {
            domain: "syndic"
        }))
    ));
    let current = SyndicStorage::reacquire(&store).unwrap();
    assert!(
        current
            .thread(&store, id(1), SyndicPointReadLimit::new(1_024).unwrap())
            .unwrap()
            .is_some()
    );
    store.close().unwrap();
}
