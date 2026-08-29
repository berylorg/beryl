use super::*;

#[test]
fn divergent_nonempty_prefix_selects_the_exact_inclusive_ancestor() {
    let home = TestHome::new("phase10-native-divergent-prefix");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    seed_populated(&store, storage.clone());
    let child = id(114);
    let parent = support::populated::source_turn();
    fixtures::seed_child_at_tail(
        &store,
        &storage,
        id(30),
        child,
        SyndicDraftId::from_bytes([115; 16]),
    );
    let fixture = fixtures::append_pending(
        &store,
        &storage,
        child,
        SyndicTurnId::from_bytes([116; 16]),
        parent,
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .expect("pending fork target fixture must be scrub-valid");
    let advanced = fixtures::advance_source_to_divergent_prefix(&store, &storage);
    assert_ne!(advanced.turn, parent);
    assert_ne!(advanced.selected.digest(), fixture.selected.digest());
    fixtures::finish_current_transcript(&store, &storage, id(30));
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .expect("divergent native source fixture must be scrub-valid");
    store.close().unwrap();

    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let before_revision = storage.revision(&store).unwrap();

    let NativeProjectionPlan::Fork {
        basis,
        source,
        through_turn,
        native_turn_count,
    } = storage
        .prepare_native_projection(
            &store,
            &NativeProjectionRequest::new(
                fixture.thread,
                fixture.selected,
                fixture.execution,
                fixture.tool_profile,
            ),
            point_limit(),
        )
        .unwrap()
    else {
        panic!("divergent source prefix must fork through the exact earlier ancestor")
    };
    assert_eq!(basis.represented_prefix().tail(), Some(parent));
    assert_eq!(source.thread_id(), id(30));
    assert_eq!(source.binding_revision(), advanced.binding_revision);
    assert_eq!(source.selected_path(), advanced.selected);
    assert_eq!(
        source.binding().represented_prefix().tail(),
        Some(advanced.turn)
    );
    assert_eq!(
        source.binding().native_turn_count(),
        advanced.native_turn_count
    );
    assert_eq!(advanced.native_turn_count, CasNativeTurnCount::new(2));
    assert_eq!(
        through_turn,
        Some(CasTurnId::new("source-history-turn").unwrap())
    );
    assert_eq!(native_turn_count, CasNativeTurnCount::new(1));
    assert_eq!(storage.revision(&store).unwrap(), before_revision);
    store.close().unwrap();
}
