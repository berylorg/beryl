use super::*;
use beryl_model::{ImageLabelOrdinal, SyndicDraftMarkerId, SyndicTurnId};

#[test]
fn replaced_first_input_keeps_original_off_path_context_owner_and_parent_divergence() {
    let (_home, store, storage) = seeded("context-replaced-first-input");
    let (original_basis, original_seed, _) = native_seed(&store, &storage);
    let original_frame = replay(&store, &storage, original_seed, 31);
    let owner = storage
        .thread(&store, id(210), point_limit())
        .unwrap()
        .unwrap()
        .context_owner_id();
    let original_turn = request(&store, &storage).selected_path().tail().unwrap();
    let replacement = SyndicTurnId::from_bytes([222; 16]);
    native_fixtures::append_pending(
        &store,
        &storage,
        id(210),
        replacement,
        support::populated::source_turn(),
    );
    let (replacement_basis, replacement_seed, _) = native_seed(&store, &storage);
    assert_ne!(
        replacement_basis.selected_path().tail(),
        Some(original_turn)
    );
    assert_eq!(
        storage
            .thread(&store, id(210), point_limit())
            .unwrap()
            .unwrap()
            .context_owner_id(),
        owner
    );
    assert_eq!(
        replacement_basis.context().unwrap().summary(),
        original_basis.context().unwrap().summary()
    );
    assert_eq!(
        replay(&store, &storage, replacement_seed, 23),
        original_frame
    );
    let selected = request(&store, &storage);
    let RecoveryAssembly::Ready(recovery) = storage
        .prepare_recovery_projection(
            &store,
            RecoveryProjectionRequest::for_pending_selected_turn_parent(
                id(210),
                selected.selected_path(),
                Some(1_000_000),
            )
            .with_tool_profile(selected.tool_profile()),
        )
        .unwrap()
    else {
        panic!("replacement context replay");
    };
    assert_eq!(
        recovery.context().unwrap().summary(),
        original_basis.context().unwrap().summary()
    );
    assert_eq!(
        replay(&store, &storage, recovery, 47).last().unwrap(),
        &original_frame[0]
    );
    native_fixtures::advance_source_to_divergent_prefix(&store, &storage);
    let (_, after, _) = native_seed(&store, &storage);
    assert_eq!(replay(&store, &storage, after, 29), original_frame);
    store.close().unwrap();
}

#[test]
fn selected_earlier_parent_boundary_excludes_parent_context_at_the_same_boundary() {
    let (_home, store, storage) = seeded("context-excluded-parent-context");
    support::exact_cas::project_item_if_needed(
        &store,
        storage.clone(),
        SyndicItemId::from_bytes([213; 16]),
    );
    support::discussion_creation::create_child(&store, &storage, id(210), 220, 221);
    support::exact_cas::submit_current_draft(
        &store,
        storage.clone(),
        id(220),
        draft_id(223),
        SyndicItemId::from_bytes([224; 16]),
        "nested discussion",
        timestamp(20),
    );
    let target = request_for(&store, &storage, id(220));
    let RecoveryAssembly::Ready(projection) = storage
        .prepare_recovery_projection(
            &store,
            RecoveryProjectionRequest::for_pending_selected_turn_parent(
                id(220),
                target.selected_path(),
                Some(1_000_000),
            )
            .with_tool_profile(target.tool_profile()),
        )
        .unwrap()
    else {
        panic!("nested context replay");
    };
    assert_eq!(projection.context().unwrap().summary().count(), 1);
    let items = replay(&store, &storage, projection, 41);
    assert_eq!(items.len(), 3);
    assert_eq!(items[0].1, "source question");
    assert_eq!(items[1].1, "assistant");
    assert!(
        items[2]
            .1
            .contains(&format!("source_thread={}\n", "d2".repeat(16)))
    );
    assert!(!items.iter().any(|item| item.1 == "discuss this exactly"));
    store.close().unwrap();
}

#[test]
fn rich_native_prefix_needs_only_seed_budget_but_complete_replay_refuses_media() {
    let (_home, store, storage) = seeded("context-rich-native-prefix");
    let root = SyndicTurnId::from_bytes([29; 16]);
    let item = SyndicItemId::from_bytes([209; 16]);
    let media = ComposerPayload::new(vec![ComposerAtom::image_marker(
        SyndicDraftMarkerId::from_bytes([225; 16]),
        ImageLabelOrdinal::FIRST,
    )])
    .unwrap();
    let (content, mut records) = support::composer_content_records(&media);
    records.push(test_faults::FixtureRecord::CanonicalItem(
        CanonicalItemRecord::local_user_input(
            item,
            root,
            TurnItemOrdinal::FIRST,
            beryl_model::ProjectionRevision::new(1).unwrap(),
            content,
            None,
        ),
    ));
    support::commit(&store, storage.clone(), support::batch(records));
    let (basis, seed, _) = native_seed(&store, &storage);
    assert_eq!(seed.utf8_bytes().get(), 470);
    assert!(
        storage
            .prepare_native_context_seed(&store, basis, Some(940))
            .is_ok()
    );
    let target = request(&store, &storage);
    assert!(matches!(
        storage.prepare_recovery_projection(
            &store,
            RecoveryProjectionRequest::for_pending_selected_turn_parent(
                id(210),
                target.selected_path(),
                Some(1_000_000)
            )
            .with_tool_profile(target.tool_profile())
        ),
        Err(RecoveryProjectionError::MediaHistory { .. })
    ));
    store.close().unwrap();
}

#[test]
fn complete_replay_charges_frame_bytes_at_the_model_boundary() {
    let (_home, store, storage) = seeded("context-expanded-model-budget");
    let target = request(&store, &storage);
    let make = |tokens| {
        RecoveryProjectionRequest::for_pending_selected_turn_parent(
            id(210),
            target.selected_path(),
            Some(tokens),
        )
        .with_tool_profile(target.tool_profile())
    };
    let total = 15 + 9 + 470;
    assert!(matches!(
        storage.prepare_recovery_projection(&store, make((total - 1) * 2)),
        Err(RecoveryProjectionError::BudgetOverflow { .. })
    ));
    let RecoveryAssembly::Ready(proof) = storage
        .prepare_recovery_projection(&store, make(total * 2))
        .unwrap()
    else {
        panic!("exact expanded budget");
    };
    assert_eq!(proof.utf8_bytes().get(), total);
    assert_eq!(proof.item_count().get(), 3);
    store.close().unwrap();
}
