use super::*;

#[test]
fn reopen_rejects_a_promoted_leaf_whose_successor_witness_was_removed() {
    let fixture = promotion_fixture(93, id(93));
    let (home, store, storage) = seed("phase58-promotion-missing-witness", fixture.records.clone());
    let request = promotion(
        &store,
        storage,
        SyndicTurnId::from_bytes([150; 16]),
        SyndicItemId::from_bytes([151; 16]),
    );
    execute_promotion(&store, storage, request).unwrap();
    store.validate_registered_domains().unwrap();

    let initial_generation = fixture
        .records
        .iter()
        .find_map(|record| match record {
            FixtureRecord::AcceptedRouteGeneration(generation)
                if generation.thread_id() == fixture.thread =>
            {
                Some(generation.clone())
            }
            _ => None,
        })
        .unwrap();
    let page = storage
        .accepted_route_page(
            &store,
            fixture.thread,
            initial_generation.generation(),
            initial_generation.revision().checked_next().unwrap(),
            None,
        )
        .unwrap();
    let promoted = page
        .records()
        .iter()
        .find(|entry| entry.input().id() == fixture.accepted_input)
        .unwrap()
        .leaf();
    assert!(promoted.promotion().is_some());
    assert!(promoted.last_transition().is_none());
    let corrupt = AcceptedRouteLeafRecord::new(
        promoted.input_id(),
        promoted.thread_id(),
        promoted.generation(),
        promoted.ordinal(),
        promoted.revision(),
        promoted.state(),
        promoted.lifecycle(),
    );
    commit(
        &store,
        storage,
        batch([FixtureRecord::AcceptedRouteLeaf(corrupt)]),
    );

    let error = store.validate_registered_domains().unwrap_err();
    assert!(
        error
            .to_string()
            .contains("promoted accepted-route leaf is missing its successor witness"),
        "unexpected validation error: {error}",
    );
    store.close().unwrap();

    let mut reopened = open(home.path());
    let error = match SyndicStorage::register(&mut reopened) {
        Ok(_) => panic!("promoted leaf without its successor witness reopened successfully"),
        Err(error) => error,
    };
    assert!(
        error
            .to_string()
            .contains("promoted accepted-route leaf is missing its successor witness"),
        "unexpected reopen error: {error}",
    );
    reopened.close().unwrap();
}
