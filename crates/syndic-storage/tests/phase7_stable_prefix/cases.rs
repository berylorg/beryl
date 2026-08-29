use super::*;

#[test]
fn live_closed_prefix_survives_resume_supersession_and_eof_promotion() {
    let home = TestHome::new("phase7-stable-prefix");
    let mut store = open(home.path());
    let mut storage = SyndicStorage::register(&mut store).unwrap();
    let initial = "stable paragraph\n\nopen suffix";
    let fixture = seed_live_assistant(&store, &storage, initial);

    let generation_1 = ItemProjectionGeneration::FIRST;
    start_build(&store, &storage, fixture.item, generation_1);
    advance_build_once(&store, &storage, fixture.item, generation_1);
    let interrupted = storage
        .item_projection_build(&store, fixture.item, generation_1, point_limit())
        .unwrap()
        .unwrap()
        .clone();
    assert!(matches!(
        interrupted.phase(),
        ItemProjectionBuildPhase::Parsing(_)
    ));
    assert_eq!(interrupted.projection_count(), 1);

    store.close().unwrap();
    store = open(home.path());
    storage = SyndicStorage::register(&mut store).unwrap();
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    assert_eq!(
        storage
            .item_projection_build(&store, fixture.item, generation_1, point_limit())
            .unwrap()
            .unwrap(),
        interrupted
    );

    let set_1 = finish_build(&store, &storage, fixture.item, generation_1);
    assert_eq!(set_1.stable_projection_count(), 1);
    assert_eq!(set_1.projection_count(), 2);
    assert!(!set_1.stable_eof_resolved());
    assert_eq!(
        set_1.resume_checkpoint().consumed_source_bytes(),
        initial.len() as u64
    );
    assert_eq!(
        set_1.resume_checkpoint().closed_source_bytes(),
        "stable paragraph\n\n".len() as u64
    );
    let generation_1_records = read_generation(&store, &storage, fixture.item, generation_1);

    append_text(&store, &storage, &fixture, " continued", 7);
    let stale_head = storage
        .item_projection_head(&store, fixture.item, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(stale_head.generation(), generation_1);
    assert_eq!(stale_head.lifecycle(), ProjectionLifecycle::Stale);
    assert_eq!(
        storage
            .item_projection_set(&store, fixture.item, generation_1, point_limit())
            .unwrap()
            .unwrap(),
        set_1
    );

    let generation_2 = generation_1.checked_next().unwrap();
    start_build(&store, &storage, fixture.item, generation_2);
    let resumed = storage
        .item_projection_build(&store, fixture.item, generation_2, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(resumed.projection_count(), 1);
    assert_eq!(
        resumed.phase().clone(),
        ItemProjectionBuildPhase::Parsing(set_1.resume_checkpoint().clone())
    );
    advance_build_once(&store, &storage, fixture.item, generation_2);
    let resumed_after_append = storage
        .item_projection_build(&store, fixture.item, generation_2, point_limit())
        .unwrap()
        .unwrap()
        .clone();
    assert_eq!(resumed_after_append.projection_count(), 1);

    store.close().unwrap();
    store = open(home.path());
    storage = SyndicStorage::register(&mut store).unwrap();
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    assert_eq!(
        storage
            .item_projection_build(&store, fixture.item, generation_2, point_limit())
            .unwrap()
            .unwrap(),
        resumed_after_append
    );

    append_text(&store, &storage, &fixture, " again", 8);
    let superseded = storage
        .item_projection_build(&store, fixture.item, generation_2, point_limit())
        .unwrap()
        .unwrap()
        .clone();
    assert!(matches!(
        superseded.phase(),
        ItemProjectionBuildPhase::Superseded(_)
    ));
    assert_eq!(superseded.projection_count(), 1);

    let generation_3 = generation_2.checked_next().unwrap();
    start_build(&store, &storage, fixture.item, generation_3);
    let replacement = storage
        .item_projection_build(&store, fixture.item, generation_3, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(replacement.projection_count(), 1);
    assert_eq!(replacement.output_digest(), superseded.output_digest());
    let set_3 = finish_build(&store, &storage, fixture.item, generation_3);
    assert_eq!(set_3.stable_projection_count(), 1);
    assert_eq!(set_3.projection_count(), 2);
    assert!(!set_3.stable_eof_resolved());
    let generation_3_records = read_generation(&store, &storage, fixture.item, generation_3);
    assert_eq!(
        generation_3_records[0].projection_id(),
        generation_1_records[0].projection_id()
    );
    assert_ne!(
        generation_3_records[1].projection_id(),
        generation_1_records[1].projection_id()
    );

    complete_item(
        &store,
        &storage,
        &fixture,
        &format!("{initial} continued again"),
        9,
    );
    assert_eq!(
        storage
            .turn_state(&store, fixture.turn, point_limit())
            .unwrap()
            .unwrap()
            .lifecycle(),
        TurnLifecycle::Active
    );
    let completed_head = storage
        .item_projection_head(&store, fixture.item, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(completed_head.generation(), generation_3);
    assert_eq!(completed_head.lifecycle(), ProjectionLifecycle::Stale);

    let generation_4 = generation_3.checked_next().unwrap();
    start_build(&store, &storage, fixture.item, generation_4);
    let set_4 = finish_build(&store, &storage, fixture.item, generation_4);
    assert_eq!(set_4.stable_projection_count(), 2);
    assert_eq!(set_4.projection_count(), 2);
    assert!(set_4.stable_eof_resolved());
    assert_eq!(set_4.stable_digest(), set_4.digest());
    assert_eq!(
        set_4.resume_checkpoint().closed_source_bytes(),
        set_4.source_bytes()
    );
    let generation_4_records = read_generation(&store, &storage, fixture.item, generation_4);
    assert_eq!(
        projection_ids(&generation_4_records),
        projection_ids(&generation_3_records)
    );

    store.close().unwrap();
    let mut reopened = open(home.path());
    let reopened_storage = SyndicStorage::register(&mut reopened).unwrap();
    reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    for generation in [generation_1, generation_3, generation_4] {
        read_generation(&reopened, &reopened_storage, fixture.item, generation);
    }
    let retained_superseded = reopened_storage
        .item_projection_build(&reopened, fixture.item, generation_2, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(retained_superseded, superseded);
    reopened.close().unwrap();
}
