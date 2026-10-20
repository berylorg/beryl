use super::*;
use beryl_model::{CasNativeTurnCount, SyndicExecutionSnapshotId};

fn canonical_discussion(name: &str) -> (TestHome, HomeStore, SyndicStorage) {
    canonical_discussion_with_projection(name, false)
}

fn canonical_discussion_with_projection(
    name: &str,
    replay: bool,
) -> (TestHome, HomeStore, SyndicStorage) {
    let home = TestHome::new(name);
    let mut candidate = open(home.path());
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    support::seed_canonical_empty_thread(&store, storage.clone(), id(30), draft_id(31));
    support::exact_cas::submit_current_draft(
        &store,
        storage.clone(),
        id(30),
        draft_id(32),
        SyndicItemId::from_bytes([33; 16]),
        "source question",
        timestamp(2),
    );
    let target = request_for(&store, &storage, id(30));
    let NativeProjectionPlan::Fresh { basis } = storage
        .prepare_native_projection(&store, &target, point_limit())
        .unwrap()
    else {
        panic!("empty source starts native fresh");
    };
    let lineage =
        CasLineageProof::native(NativeCasLineage::Fresh, basis.represented_prefix()).unwrap();
    execution::committed(store.execute_current(storage.current_publish_valid_binding(
        PublishValidBinding::from_native(
            basis,
            target.execution().clone(),
            CasThreadId::new("canonical-context-source").unwrap(),
            CasNativeTurnCount::ZERO,
            lineage,
        ),
    )));
    execution::complete_pending(
        &store,
        &storage,
        id(30),
        SyndicItemId::from_bytes([33; 16]),
        Some((SyndicItemId::from_bytes([34; 16]), "source answer")),
        10,
    );
    execution::create_discussion_at_answer(
        &store,
        &storage,
        id(30),
        target.selected_path().tail().unwrap(),
        SyndicItemId::from_bytes([34; 16]),
        "source answer",
        210,
        211,
    );
    support::exact_cas::submit_current_draft(
        &store,
        storage.clone(),
        id(210),
        draft_id(212),
        SyndicItemId::from_bytes([213; 16]),
        "discussion user",
        timestamp(210),
    );
    let target = request(&store, &storage);
    let NativeProjectionPlan::Fork {
        basis,
        native_turn_count,
        ..
    } = storage
        .prepare_native_projection(&store, &target, point_limit())
        .unwrap()
    else {
        panic!("canonical discussion fork");
    };
    let seed = if replay {
        let RecoveryAssembly::Ready(projection) = storage
            .prepare_recovery_projection(
                &store,
                RecoveryProjectionRequest::for_pending_selected_turn_parent(
                    id(210),
                    target.selected_path(),
                    Some(1_000_000),
                )
                .with_tool_profile(target.tool_profile()),
            )
            .unwrap()
        else {
            panic!("canonical complete replay");
        };
        projection
    } else {
        storage
            .prepare_native_context_seed(&store, basis, Some(1_000_000))
            .unwrap()
    };
    let lineage = seed
        .completed_lineage(timestamp(300), generation())
        .unwrap();
    let publication = if replay {
        PublishValidBinding::from_recovery(
            seed,
            basis,
            target.execution().clone(),
            CasThreadId::new("canonical-context-target").unwrap(),
            CasNativeTurnCount::ZERO,
            lineage,
        )
    } else {
        PublishValidBinding::from_native(
            basis,
            target.execution().clone(),
            CasThreadId::new("canonical-context-target").unwrap(),
            native_turn_count,
            lineage,
        )
    };
    execution::committed(store.execute_current(storage.current_publish_valid_binding(publication)));
    (home, store, storage)
}

#[test]
fn context_proof_reopens_exactly_and_old_preparation_loses_generation_authority() {
    let (home, store, storage) = canonical_discussion("context-canonical-reopen");
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    let current = storage
        .current_binding(&store, id(210), point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Valid(usable) = current.binding().state() else {
        panic!("canonical usable proof");
    };
    let expected = usable.clone();
    let old = storage
        .prepare_native_projection(&store, &request(&store, &storage), point_limit())
        .unwrap()
        .basis();
    store.close().unwrap();
    let mut candidate = open(home.path());
    let storage = SyndicStorage::register_with_schema_validation(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    let reopened = storage
        .current_binding(&store, id(210), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        reopened.binding().state(),
        &BindingState::Valid(expected.clone())
    );
    storage
        .validate_context_binding(&store, id(210), &expected)
        .unwrap();
    let old_request = PublishValidBinding::from_native(
        old,
        expected.execution().clone(),
        expected.cas_thread_id().clone(),
        expected.native_turn_count(),
        expected.lineage(),
    );
    assert!(matches!(
        store.execute_current(storage.current_publish_valid_binding(old_request)),
        CommandOutcome::NotCommitted { .. }
    ));
    let new = storage
        .prepare_native_projection(&store, &request(&store, &storage), point_limit())
        .unwrap()
        .basis();
    execution::committed(store.execute_current(storage.current_publish_valid_binding(
        PublishValidBinding::from_native(
            new,
            expected.execution().clone(),
            expected.cas_thread_id().clone(),
            expected.native_turn_count(),
            expected.lineage(),
        ),
    )));
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();
}

#[test]
fn original_context_ordering_floor_gates_activation_and_cancelled_snapshot_remains_exact() {
    let (_home, store, storage) = canonical_discussion("context-activation-ordering");
    let current = storage
        .current_binding(&store, id(210), point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Valid(usable) = current.binding().state() else {
        panic!("canonical usable proof");
    };
    let target = request(&store, &storage);
    let turn = target.selected_path().tail().unwrap();
    let state = storage
        .turn_state(&store, turn, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(&store, id(210), point_limit())
        .unwrap()
        .unwrap();
    let classification = storage
        .prepare_context_binding_cancellable(&store, id(210), usable, &|| false)
        .unwrap();
    let make = |started| {
        ActivateBinding::new(
            id(210),
            current.binding().revision(),
            gate.revision(),
            state.revision(),
            target.selected_path(),
            SyndicExecutionSnapshotId::from_bytes([245; 16]),
            turn,
            generation(),
            timestamp(started),
        )
        .with_context_classification(classification)
    };
    support::enroll_fixture_activity(&store, &storage, id(210), turn);
    assert!(matches!(
        store.execute_current(storage.current_activate_binding(make(299))),
        CommandOutcome::NotCommitted { .. }
    ));
    execution::committed(store.execute_current(storage.current_activate_binding(make(300))));
    let active = storage
        .current_binding(&store, id(210), point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(&store, id(210), point_limit())
        .unwrap()
        .unwrap();
    let state = storage
        .turn_state(&store, turn, point_limit())
        .unwrap()
        .unwrap();
    execution::committed(
        store.execute_current(storage.current_cancel_binding_activation(
            CancelBindingActivation::new(
                id(210),
                active.binding().revision(),
                gate.revision(),
                state.revision(),
                target.selected_path(),
                SyndicExecutionSnapshotId::from_bytes([245; 16]),
                turn,
            ),
        )),
    );
    let snapshot = storage
        .execution_snapshot(
            &store,
            SyndicExecutionSnapshotId::from_bytes([245; 16]),
            point_limit(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(snapshot.lineage(), usable.lineage());
    assert_eq!(snapshot.started_at(), timestamp(300));
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();
}

#[test]
fn structurally_decodable_context_digest_substitution_fails_whole_home_validation() {
    let (_home, store, storage) = canonical_discussion("context-semantic-corruption");
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    let current = storage
        .current_binding(&store, id(210), point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Valid(usable) = current.binding().state() else {
        panic!("canonical usable proof");
    };
    let mut bytes = test_faults::encode_lineage_fixture(usable.lineage());
    bytes[124] ^= 1;
    let substituted = test_faults::decode_lineage_fixture(&bytes).unwrap();
    let replacement = UsableCasBinding::new(
        usable.execution().clone(),
        usable.cas_thread_id().clone(),
        usable.represented_prefix(),
        usable.native_turn_count(),
        usable.tool_profile(),
        substituted,
    );
    support::commit(
        &store,
        storage.clone(),
        support::batch([test_faults::FixtureRecord::Binding(BindingRecord::new(
            id(210),
            current.binding().revision(),
            current.binding().selected_path(),
            BindingState::Valid(replacement),
        ))]),
    );
    assert!(
        store
            .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
            .is_err()
    );
    let _ = store.close();
}

#[test]
fn consistently_rehashed_foreign_home_context_cannot_pass_activation_or_whole_home_validation() {
    let (_home, store, storage) = canonical_discussion("context-foreign-home-owner");
    let current = storage
        .current_binding(&store, id(210), point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Valid(usable) = current.binding().state() else {
        panic!("canonical usable proof");
    };
    let mut foreign = *store.home_id().as_bytes();
    foreign[0] ^= 1;
    let foreign = beryl_model::BerylHomeId::from_bytes(foreign);
    let lineage =
        test_faults::foreign_home_seed_lineage_fixture(&store, &storage, id(210), usable, foreign)
            .unwrap();
    let CasLineageProof::ContextAware(foreign_proof) = lineage else {
        panic!("foreign seed");
    };
    let CasLineageProof::ContextAware(original) = usable.lineage() else {
        panic!("original seed");
    };
    assert_ne!(foreign_proof.summary(), original.summary());
    assert_ne!(
        foreign_proof.projection_digest(),
        original.projection_digest()
    );
    assert_eq!(foreign_proof.injection(), original.injection());
    assert_eq!(
        test_faults::decode_lineage_fixture(&test_faults::encode_lineage_fixture(lineage)).unwrap(),
        lineage
    );
    let replacement = UsableCasBinding::new(
        usable.execution().clone(),
        usable.cas_thread_id().clone(),
        usable.represented_prefix(),
        usable.native_turn_count(),
        usable.tool_profile(),
        lineage,
    );
    support::commit(
        &store,
        storage.clone(),
        support::batch([test_faults::FixtureRecord::Binding(BindingRecord::new(
            id(210),
            current.binding().revision(),
            current.binding().selected_path(),
            BindingState::Valid(replacement.clone()),
        ))]),
    );
    assert!(
        storage
            .validate_context_binding(&store, id(210), &replacement)
            .is_err()
    );
    let target = request(&store, &storage);
    let turn = target.selected_path().tail().unwrap();
    let state = storage
        .turn_state(&store, turn, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(&store, id(210), point_limit())
        .unwrap()
        .unwrap();
    let activation = ActivateBinding::new(
        id(210),
        current.binding().revision(),
        gate.revision(),
        state.revision(),
        target.selected_path(),
        SyndicExecutionSnapshotId::from_bytes([246; 16]),
        turn,
        generation(),
        timestamp(301),
    );
    assert!(matches!(
        store.execute_current(storage.current_activate_binding(activation)),
        CommandOutcome::NotCommitted { .. }
    ));
    assert!(
        store
            .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
            .is_err()
    );
    let _ = store.close();
}

#[test]
fn replay_observation_digest_and_totals_are_checked_against_the_complete_canonical_sequence() {
    for field in [0, 1, 2] {
        let (_home, store, storage) = canonical_discussion_with_projection(
            &format!("context-replay-observation-{field}"),
            true,
        );
        store
            .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
            .unwrap();
        let current = storage
            .current_binding(&store, id(210), point_limit())
            .unwrap()
            .unwrap();
        let BindingState::Valid(usable) = current.binding().state() else {
            panic!("canonical replay binding");
        };
        let mut bytes = test_faults::encode_lineage_fixture(usable.lineage());
        assert_eq!(bytes.len(), 237);
        match field {
            0 => bytes[197] ^= 1,
            1 => bytes[181..189].copy_from_slice(&4u64.to_be_bytes()),
            2 => {
                let actual = u64::from_be_bytes(bytes[189..197].try_into().unwrap());
                bytes[189..197].copy_from_slice(&(actual + 1).to_be_bytes());
            }
            _ => unreachable!(),
        }
        let changed = test_faults::decode_lineage_fixture(&bytes).unwrap();
        let replacement = UsableCasBinding::new(
            usable.execution().clone(),
            usable.cas_thread_id().clone(),
            usable.represented_prefix(),
            usable.native_turn_count(),
            usable.tool_profile(),
            changed,
        );
        support::commit(
            &store,
            storage.clone(),
            support::batch([test_faults::FixtureRecord::Binding(BindingRecord::new(
                id(210),
                current.binding().revision(),
                current.binding().selected_path(),
                BindingState::Valid(replacement),
            ))]),
        );
        assert!(
            store
                .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
                .is_err()
        );
        let _ = store.close();
    }
}
