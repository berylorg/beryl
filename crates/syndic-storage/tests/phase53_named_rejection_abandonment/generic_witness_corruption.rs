use super::*;

#[test]
fn generic_abandonment_reopen_rejects_nonprior_gate_witness() {
    let home = TestHome::new("phase53-generic-abandonment-corrupt-gate-witness");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let records = mixed_abandonment_records();
    let source = records
        .iter()
        .find_map(|record| match record {
            FixtureRecord::AcceptedRouteGeneration(route) if route.thread_id() == id(40) => {
                Some(route.clone())
            }
            _ => None,
        })
        .unwrap();
    commit(&store, storage, batch(records));
    let request = abandonment_request(&store, storage);
    store
        .execute_current(storage.current_abandon_active_binding(request.clone()))
        .unwrap();

    let prior = request.target().clone();
    let snapshot = prior.pending().snapshot_id();
    let cas_thread = prior.pending().cas_thread_id().clone();
    let bad_gate = storage
        .input_gate(&store, request.thread_id(), limit())
        .unwrap()
        .unwrap()
        .revision();
    let lost = AcceptedRouteProjectionLostProof::new(
        prior,
        AcceptedRouteAbandonmentProof::new(
            request.expected_binding_revision(),
            bad_gate,
            AcceptedRouteHeadProof::new(source.generation(), source.revision()),
            AcceptedRouteAbandonmentKind::Generic,
        ),
        request.expected_binding_revision().checked_next().unwrap(),
        snapshot,
        cas_thread,
    );
    let corrupt = AcceptedRouteGenerationRecord::new(
        source.thread_id(),
        source.generation(),
        source.revision().checked_next().unwrap(),
        AcceptedRouteTarget::ProjectionLost(lost),
        source.first_ordinal(),
        source.last_ordinal(),
        source.input_count(),
        0,
        0,
        source.next_turn_count() + source.ready_retryable_count(),
        source.terminal_count() + source.delivering_count(),
        source.live_logical_utf8_bytes() - source.delivering_logical_utf8_bytes(),
        0,
    )
    .unwrap();
    commit(
        &store,
        storage,
        batch([FixtureRecord::AcceptedRouteGeneration(corrupt)]),
    );
    assert!(store.validate_registered_domains().is_err());
    store.close().unwrap();

    let mut reopened = open(home.path());
    match SyndicStorage::register(&mut reopened) {
        Ok(_) => panic!("generic abandonment with a non-prior gate witness reopened"),
        Err(error) => assert!(error.to_string().contains("abandonment proof disagrees")),
    }
    reopened.close().unwrap();
}
